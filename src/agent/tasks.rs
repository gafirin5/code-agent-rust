#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{
    Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

// ============================================================================
// 0. Core Identifiers
// ============================================================================

/// Canonical identifier alias for managed background tasks.
pub type TaskId = String;

// ============================================================================
// 1. TaskStatus Enum & Lifecycle FSM
// ============================================================================

/// Represents the lifecycle state of a background task / subagent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    /// Returns the canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// True if the task has reached a final immutable state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// True if the task is actively queued or running.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    /// True if cancellation is allowed (i.e. not already in a terminal state).
    pub fn can_cancel(&self) -> bool {
        !self.is_terminal()
    }

    /// Enforces the valid transition matrix of the lifecycle state machine.
    pub fn can_transition_to(&self, next: TaskStatus) -> bool {
        match (self, next) {
            (Self::Queued, Self::Running) => true,
            (Self::Queued, Self::Cancelled) => true,
            (Self::Queued, Self::Failed) => true,
            (Self::Running, Self::Completed) => true,
            (Self::Running, Self::Failed) => true,
            (Self::Running, Self::Cancelled) => true,
            _ => false,
        }
    }

    /// Produces an ANSI-colored status badge for interactive REPL displays.
    pub fn badge(&self) -> String {
        match self {
            Self::Queued => "\x1B[33m[QUEUED]\x1B[0m".to_string(),
            Self::Running => "\x1B[34m[RUNNING]\x1B[0m".to_string(),
            Self::Completed => "\x1B[32m[COMPLETED]\x1B[0m".to_string(),
            Self::Failed => "\x1B[31m[FAILED]\x1B[0m".to_string(),
            Self::Cancelled => "\x1B[35m[CANCELLED]\x1B[0m".to_string(),
        }
    }

    /// Plain text badge without ANSI escape codes for file/log output.
    pub fn plain_badge(&self) -> &'static str {
        match self {
            Self::Queued => "[QUEUED]",
            Self::Running => "[RUNNING]",
            Self::Completed => "[COMPLETED]",
            Self::Failed => "[FAILED]",
            Self::Cancelled => "[CANCELLED]",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" | "canceled" => Ok(Self::Cancelled),
            other => Err(format!("Unknown task status: '{}'", other)),
        }
    }
}

// ============================================================================
// 2. Cooperative CancellationToken
// ============================================================================

/// Atomic, thread-safe cancellation handle allowing callers to signal and
/// inspect cooperative cancellation across threads and turn boundaries.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signals cancellation to all holders of this token or its clones.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Wait-free non-blocking query to inspect if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Returns `Err(TaskError::Cancelled)` if cancellation was requested.
    pub fn check(&self) -> Result<(), TaskError> {
        if self.is_cancelled() {
            Err(TaskError::Cancelled(
                "Operation cancelled by user or coordinator".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Provides access to the underlying atomic bool.
    pub fn as_raw(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

// ============================================================================
// 3. Domain Error Types
// ============================================================================

/// Domain errors for task management, lifecycle transitions, and synchronization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    NotFound(String),
    AlreadyTerminal(TaskStatus),
    InvalidTransition { from: TaskStatus, to: TaskStatus },
    Timeout(Duration),
    Cancelled(String),
    ExecutionFailed(String),
    SpawnFailed(String),
    CannotRemoveRunning(String),
    LockPoisoned(String),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "Task '{}' not found", id),
            Self::AlreadyTerminal(status) => {
                write!(f, "Task is already in terminal state '{}'", status)
            }
            Self::InvalidTransition { from, to } => {
                write!(f, "Invalid task state transition from '{}' to '{}'", from, to)
            }
            Self::Timeout(dur) => write!(f, "Task timed out after {:?}", dur),
            Self::Cancelled(reason) => write!(f, "Task cancelled: {}", reason),
            Self::ExecutionFailed(err) => write!(f, "Task execution failed: {}", err),
            Self::SpawnFailed(err) => write!(f, "Failed to spawn task worker thread: {}", err),
            Self::CannotRemoveRunning(id) => {
                write!(f, "Cannot remove task '{}' while it is still running", id)
            }
            Self::LockPoisoned(ctx) => write!(f, "Lock poisoned in {}", ctx),
        }
    }
}

impl std::error::Error for TaskError {}

// ============================================================================
// 4. Zero-Dependency Timestamp & Duration Formatting
// ============================================================================

/// Formats a `SystemTime` into an ISO-8601 UTC string (`YYYY-MM-DDTHH:MM:SSZ`)
/// using pure standard library arithmetic (Howard Hinnant civil calendar algorithm).
pub fn format_utc_timestamp(st: SystemTime) -> String {
    let duration = st
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();

    let days = (total_secs / 86400) as i64;
    let day_secs = total_secs % 86400;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    // Howard Hinnant's algorithm for converting civil date from day number
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let final_y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        final_y, m, d, hours, minutes, seconds
    )
}

/// Formats a `Duration` into a concise, human-readable elapsed string.
pub fn format_duration_human(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if total_secs == 0 {
        format!("{}ms", millis)
    } else if total_secs < 10 {
        format!("{}.{:01}s", total_secs, millis / 100)
    } else if total_secs < 60 {
        format!("{}s", total_secs)
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let rem_secs = total_secs % 60;
        format!("{}m {:02}s", mins, rem_secs)
    } else {
        let hours = total_secs / 3600;
        let rem_mins = (total_secs % 3600) / 60;
        format!("{}h {:02}m", hours, rem_mins)
    }
}

// ============================================================================
// 5. TaskLogBuffer & Output Isolation
// ============================================================================

/// Thread-safe in-memory log buffer capturing subagent log output, preventing
/// interlaced stdout corruption in concurrent multi-subagent scenarios.
#[derive(Clone, Debug, Default)]
pub struct TaskLogBuffer {
    lines: Arc<RwLock<Vec<String>>>,
}

impl TaskLogBuffer {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Appends a log line to the buffer.
    pub fn push(&self, line: impl Into<String>) {
        let mut guard = self.lines.write().unwrap_or_else(|e| e.into_inner());
        guard.push(line.into());
    }

    /// Alias for push.
    pub fn log(&self, line: impl Into<String>) {
        self.push(line);
    }

    /// Returns a cloned snapshot of all recorded lines.
    pub fn lines(&self) -> Vec<String> {
        self.lines.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Alias for lines.
    pub fn all_lines(&self) -> Vec<String> {
        self.lines()
    }

    /// Returns a paginated slice of lines with boundary clamping.
    pub fn get_lines(&self, offset: usize, limit: usize) -> Vec<String> {
        let guard = self.lines.read().unwrap_or_else(|e| e.into_inner());
        let total = guard.len();
        if offset >= total {
            return Vec::new();
        }
        let end = offset.saturating_add(limit).min(total);
        guard[offset..end].to_vec()
    }

    /// Returns all lines joined by newlines.
    pub fn text(&self) -> String {
        self.lines
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .join("\n")
    }

    /// Alias for text.
    pub fn to_string_lossy(&self) -> String {
        self.text()
    }

    /// Number of lines currently in the buffer.
    pub fn len(&self) -> usize {
        self.lines.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// True if the buffer contains zero lines.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all recorded lines.
    pub fn clear(&self) {
        let mut guard = self.lines.write().unwrap_or_else(|e| e.into_inner());
        guard.clear();
    }

    /// Returns the last `n` lines from the buffer.
    pub fn tail(&self, n: usize) -> Vec<String> {
        let guard = self.lines.read().unwrap_or_else(|e| e.into_inner());
        let total = guard.len();
        let start = total.saturating_sub(n);
        guard[start..].to_vec()
    }

    /// Returns all lines joined by newlines (alias for text).
    pub fn formatted(&self) -> String {
        self.text()
    }
}

/// Output routing destination supporting terminal stdout or buffered isolation.
#[derive(Clone, Debug)]
pub enum OutputSink {
    Terminal,
    Buffered(Arc<TaskLogBuffer>),
}

impl OutputSink {
    /// Creates a new buffered output sink with the given log buffer.
    pub fn buffered(buffer: Arc<TaskLogBuffer>) -> Self {
        Self::Buffered(buffer)
    }

    /// Emits a line of text to the target sink.
    pub fn emit(&self, text: &str) {
        match self {
            Self::Terminal => {
                println!("{}", text);
            }
            Self::Buffered(buffer) => {
                buffer.push(text);
            }
        }
    }

    /// Convenience alias for `emit`.
    pub fn log(&self, text: impl Into<String>) {
        let s = text.into();
        self.emit(&s);
    }

    /// Returns true if this sink writes to an isolated buffer without touching stdout.
    pub fn is_silent(&self) -> bool {
        matches!(self, Self::Buffered(_))
    }

    /// Returns the underlying buffer if buffered.
    pub fn buffer(&self) -> Option<Arc<TaskLogBuffer>> {
        match self {
            Self::Buffered(buf) => Some(buf.clone()),
            Self::Terminal => None,
        }
    }

    /// Emits an ephemeral spinner line only if the sink is interactive (Terminal).
    /// Suppressed completely when running with an isolated Buffered sink.
    pub fn emit_spinner(&self, text: &str) {
        if !self.is_silent() {
            use std::io::Write;
            print!("{}", text);
            let _ = std::io::stdout().flush();
        }
    }

    /// Clears an ephemeral spinner line only if the sink is interactive (Terminal).
    /// Suppressed completely when running with an isolated Buffered sink to avoid
    /// clearing user typing lines via `\r\x1B[K`.
    pub fn clear_spinner(&self) {
        if !self.is_silent() {
            use std::io::Write;
            print!("\r\x1B[K");
            let _ = std::io::stdout().flush();
        }
    }
}

// ============================================================================
// 6. TaskSnapshot (Public DTO) & TaskRecord (Internal State)
// ============================================================================

/// Lock-free, cloneable, serializable Data Transfer Object for public inspection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub elapsed_secs: f64,
    pub elapsed_human: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub notified: bool,
}

impl TaskSnapshot {
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn is_completed(&self) -> bool {
        self.status == TaskStatus::Completed
    }

    pub fn is_failed(&self) -> bool {
        self.status == TaskStatus::Failed
    }

    pub fn is_cancelled(&self) -> bool {
        self.status == TaskStatus::Cancelled
    }

    /// Formats a standardized completion notification banner for REPL display.
    /// When `color` is true, uses ANSI escape styling.
    /// When `color` is false, outputs pure ASCII text for non-TTY environments.
    pub fn format_notification(&self, color: bool) -> String {
        if color {
            match self.status {
                TaskStatus::Completed => {
                    let mut out = format!(
                        "\x1B[1;32m🔔 [COMPLETED]\x1B[0m \x1B[1;37m{}\x1B[0m (\x1B[36m{}\x1B[0m) in \x1B[33m{}\x1B[0m",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   \x1B[90mDescription:\x1B[0m {}", self.description));
                    }
                    out
                }
                TaskStatus::Failed => {
                    let err_msg = self.error.as_deref().unwrap_or("Unknown error");
                    let err_preview = if err_msg.len() > 120 {
                        format!("{}...", &err_msg[..117])
                    } else {
                        err_msg.to_string()
                    };
                    let mut out = format!(
                        "\x1B[1;31m🔔 [FAILED]\x1B[0m \x1B[1;37m{}\x1B[0m (\x1B[36m{}\x1B[0m) after \x1B[33m{}\x1B[0m",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   \x1B[90mDescription:\x1B[0m {}", self.description));
                    }
                    out.push_str(&format!("\n   \x1B[90mError:\x1B[0m \x1B[31m{}\x1B[0m", err_preview));
                    out
                }
                TaskStatus::Cancelled => {
                    let mut out = format!(
                        "\x1B[1;35m🔔 [CANCELLED]\x1B[0m \x1B[1;37m{}\x1B[0m (\x1B[36m{}\x1B[0m) after \x1B[33m{}\x1B[0m",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   \x1B[90mDescription:\x1B[0m {}", self.description));
                    }
                    out
                }
                other => {
                    format!(
                        "🔔 [{}] {} ({}) in {}",
                        other.as_str().to_uppercase(),
                        self.id,
                        self.name,
                        self.elapsed_human
                    )
                }
            }
        } else {
            match self.status {
                TaskStatus::Completed => {
                    let mut out = format!(
                        "🔔 [COMPLETED] {} ({}) in {}",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   Description: {}", self.description));
                    }
                    out
                }
                TaskStatus::Failed => {
                    let err_msg = self.error.as_deref().unwrap_or("Unknown error");
                    let err_preview = if err_msg.len() > 120 {
                        format!("{}...", &err_msg[..117])
                    } else {
                        err_msg.to_string()
                    };
                    let mut out = format!(
                        "🔔 [FAILED] {} ({}) after {}",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   Description: {}", self.description));
                    }
                    out.push_str(&format!("\n   Error: {}", err_preview));
                    out
                }
                TaskStatus::Cancelled => {
                    let mut out = format!(
                        "🔔 [CANCELLED] {} ({}) after {}",
                        self.id, self.name, self.elapsed_human
                    );
                    if !self.description.is_empty() {
                        out.push_str(&format!("\n   Description: {}", self.description));
                    }
                    out
                }
                other => {
                    format!(
                        "🔔 [{}] {} ({}) in {}",
                        other.as_str().to_uppercase(),
                        self.id,
                        self.name,
                        self.elapsed_human
                    )
                }
            }
        }
    }
}

/// Internal mutable synchronized task state.
#[derive(Debug)]
pub struct TaskRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub started_instant: Option<Instant>,
    pub duration: Option<Duration>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub cancellation_token: CancellationToken,
    pub logs: Arc<TaskLogBuffer>,
    pub notified: bool,
}

impl TaskRecord {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        cancellation_token: CancellationToken,
        logs: Arc<TaskLogBuffer>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            status: TaskStatus::Queued,
            created_at: SystemTime::now(),
            started_at: None,
            finished_at: None,
            started_instant: None,
            duration: None,
            result: None,
            error: None,
            cancellation_token,
            logs,
            notified: false,
        }
    }

    /// Enforces state transition from Queued to Running.
    pub fn mark_running(&mut self) -> Result<(), TaskError> {
        if self.cancellation_token.is_cancelled() {
            let _ = self.mark_cancelled(Some("Task cancelled before starting".into()));
            return Err(TaskError::Cancelled(
                "Task cancelled before starting".into(),
            ));
        }
        if !self.status.can_transition_to(TaskStatus::Running) {
            return Err(TaskError::InvalidTransition {
                from: self.status,
                to: TaskStatus::Running,
            });
        }
        self.status = TaskStatus::Running;
        let now = SystemTime::now();
        self.started_at = Some(now);
        self.started_instant = Some(Instant::now());
        Ok(())
    }

    /// Enforces transition to Completed.
    pub fn mark_completed(&mut self, output: String) -> Result<(), TaskError> {
        if !self.status.can_transition_to(TaskStatus::Completed) {
            if self.status.is_terminal() {
                return Err(TaskError::AlreadyTerminal(self.status));
            }
            return Err(TaskError::InvalidTransition {
                from: self.status,
                to: TaskStatus::Completed,
            });
        }
        self.status = TaskStatus::Completed;
        let now = SystemTime::now();
        self.finished_at = Some(now);
        self.duration = Some(
            self.started_instant
                .map(|inst| inst.elapsed())
                .unwrap_or_else(|| now.duration_since(self.created_at).unwrap_or_default()),
        );
        self.result = Some(output);
        Ok(())
    }

    /// Enforces transition to Failed.
    pub fn mark_failed(&mut self, err: String) -> Result<(), TaskError> {
        if !self.status.can_transition_to(TaskStatus::Failed) {
            if self.status.is_terminal() {
                return Err(TaskError::AlreadyTerminal(self.status));
            }
            return Err(TaskError::InvalidTransition {
                from: self.status,
                to: TaskStatus::Failed,
            });
        }
        self.status = TaskStatus::Failed;
        let now = SystemTime::now();
        self.finished_at = Some(now);
        self.duration = Some(
            self.started_instant
                .map(|inst| inst.elapsed())
                .unwrap_or_else(|| now.duration_since(self.created_at).unwrap_or_default()),
        );
        self.error = Some(err);
        Ok(())
    }

    /// Enforces transition to Cancelled.
    pub fn mark_cancelled(&mut self, reason: Option<String>) -> Result<(), TaskError> {
        if self.status.is_terminal() {
            return Err(TaskError::AlreadyTerminal(self.status));
        }
        self.status = TaskStatus::Cancelled;
        let now = SystemTime::now();
        self.finished_at = Some(now);
        self.duration = Some(
            self.started_instant
                .map(|inst| inst.elapsed())
                .unwrap_or_else(|| now.duration_since(self.created_at).unwrap_or_default()),
        );
        self.cancellation_token.cancel();
        if let Some(r) = reason {
            self.error = Some(format!("Cancelled: {}", r));
        }
        Ok(())
    }

    /// Computes accurate elapsed duration using monotonic clocks when active.
    pub fn elapsed_duration(&self) -> Duration {
        if let Some(d) = self.duration {
            return d;
        }
        if let Some(inst) = self.started_instant {
            return inst.elapsed();
        }
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or_default()
    }

    /// Creates an immutable lock-free snapshot.
    pub fn snapshot(&self) -> TaskSnapshot {
        let elapsed = self.elapsed_duration();
        let elapsed_secs = elapsed.as_secs_f64();
        let elapsed_human = format_duration_human(elapsed);
        let duration_ms = self
            .duration
            .map(|d| d.as_millis() as u64)
            .or_else(|| self.started_instant.map(|i| i.elapsed().as_millis() as u64))
            .or_else(|| {
                if self.status.is_terminal() {
                    Some(elapsed.as_millis() as u64)
                } else {
                    None
                }
            });

        TaskSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            status: self.status,
            created_at: format_utc_timestamp(self.created_at),
            started_at: self.started_at.map(format_utc_timestamp),
            finished_at: self.finished_at.map(format_utc_timestamp),
            elapsed_secs,
            elapsed_human,
            duration_ms,
            result: self.result.clone(),
            error: self.error.clone(),
            notified: self.notified,
        }
    }
}

// ============================================================================
// 7. TaskInner & TaskManager Registry
// ============================================================================

/// Granular per-task container holding synchronized state, Condvar, and logs.
#[derive(Debug)]
pub struct TaskInner {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cancellation_token: CancellationToken,
    pub record: Mutex<TaskRecord>,
    pub notify: Condvar,
    pub log_buffer: Arc<TaskLogBuffer>,
}

impl TaskInner {
    /// Acquires the per-task record lock with automatic poison recovery.
    pub fn lock_record(&self) -> MutexGuard<'_, TaskRecord> {
        self.record.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static GLOBAL_TASK_MANAGER: OnceLock<TaskManager> = OnceLock::new();

/// Thread-safe task manager providing non-blocking subagent spawning,
/// condvar-based awaiting with timeout handling, cancellation, and pruning.
#[derive(Debug)]
pub struct TaskManager {
    counter: AtomicUsize,
    tasks: RwLock<HashMap<String, Arc<TaskInner>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    /// Creates an isolated TaskManager instance (ideal for independent testing).
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(1),
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Accesses the global singleton instance across the CLI process.
    pub fn global() -> &'static TaskManager {
        GLOBAL_TASK_MANAGER.get_or_init(TaskManager::new)
    }

    fn read_tasks(&self) -> RwLockReadGuard<'_, HashMap<String, Arc<TaskInner>>> {
        self.tasks.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_tasks(&self) -> RwLockWriteGuard<'_, HashMap<String, Arc<TaskInner>>> {
        self.tasks.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Retrieves an inner task container, dropping the registry lock immediately.
    pub fn get_inner(&self, id: &str) -> Option<Arc<TaskInner>> {
        let tasks = self.read_tasks();
        tasks.get(id).cloned()
    }

    /// Spawns a new background task providing both `CancellationToken` and dedicated
    /// `Arc<TaskLogBuffer>` to the runner closure.
    ///
    /// Returns the assigned `TaskId`, `CancellationToken`, and `Arc<TaskLogBuffer>`.
    pub fn spawn_task_with_sink<F>(
        &self,
        name: String,
        description: String,
        runner: F,
    ) -> Result<(TaskId, CancellationToken, Arc<TaskLogBuffer>), TaskError>
    where
        F: FnOnce(CancellationToken, Arc<TaskLogBuffer>) -> anyhow::Result<String> + Send + 'static,
    {
        let id_num = self.counter.fetch_add(1, Ordering::SeqCst);
        let id = format!("task-{}", id_num);
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());

        let record = TaskRecord::new(
            id.clone(),
            name.clone(),
            description.clone(),
            token.clone(),
            logs.clone(),
        );

        let task_inner = Arc::new(TaskInner {
            id: id.clone(),
            name,
            description,
            cancellation_token: token.clone(),
            record: Mutex::new(record),
            notify: Condvar::new(),
            log_buffer: logs.clone(),
        });

        // Insert into registry (lock released immediately)
        {
            let mut tasks = self.write_tasks();
            tasks.insert(id.clone(), task_inner.clone());
        }

        let worker_task = task_inner.clone();
        let worker_token = token.clone();
        let worker_logs = logs.clone();
        let thread_name = format!("task-worker-{}", id);

        let spawn_res = thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                Self::run_worker_thread_with_sink(worker_task, worker_token, worker_logs, runner);
            });

        match spawn_res {
            Ok(_handle) => Ok((id, token, logs)),
            Err(e) => {
                let mut rec = task_inner.lock_record();
                let _ = rec.mark_failed(format!("Failed to spawn worker thread: {}", e));
                drop(rec);
                task_inner.notify.notify_all();
                Err(TaskError::SpawnFailed(e.to_string()))
            }
        }
    }

    /// Backward-compatible wrapper around `spawn_task_with_sink`.
    /// Preserves existing signature: `FnOnce(CancellationToken) -> anyhow::Result<String>`.
    pub fn spawn_task<F>(
        &self,
        name: String,
        description: String,
        runner: F,
    ) -> Result<(TaskId, CancellationToken), TaskError>
    where
        F: FnOnce(CancellationToken) -> anyhow::Result<String> + Send + 'static,
    {
        self.spawn_task_with_sink(name, description, move |token, _logs| runner(token))
            .map(|(id, token, _logs)| (id, token))
    }

    /// Worker thread routine executing the runner closure with `catch_unwind`.
    fn run_worker_thread_with_sink<F>(
        task: Arc<TaskInner>,
        token: CancellationToken,
        logs: Arc<TaskLogBuffer>,
        runner: F,
    ) where
        F: FnOnce(CancellationToken, Arc<TaskLogBuffer>) -> anyhow::Result<String> + Send + 'static,
    {
        // 1. Transition Queued -> Running
        {
            let mut rec = task.lock_record();
            if token.is_cancelled() || rec.status == TaskStatus::Cancelled {
                let _ = rec.mark_cancelled(Some("Cancelled before starting".into()));
                drop(rec);
                task.notify.notify_all();
                return;
            }
            if rec.mark_running().is_err() {
                drop(rec);
                task.notify.notify_all();
                return;
            }
            drop(rec);
            task.notify.notify_all();
        }

        // 2. Execute runner with catch_unwind (NO MUTEX HELD!)
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| runner(token.clone(), logs)));

        // 3. Re-acquire record lock and update terminal state
        {
            let mut rec = task.lock_record();
            match outcome {
                Ok(Ok(output)) => {
                    if token.is_cancelled() {
                        let _ = rec.mark_cancelled(Some("Cancelled during execution".into()));
                    } else {
                        let _ = rec.mark_completed(output);
                    }
                }
                Ok(Err(err)) => {
                    if token.is_cancelled() {
                        let _ = rec.mark_cancelled(Some(err.to_string()));
                    } else {
                        let _ = rec.mark_failed(err.to_string());
                    }
                }
                Err(panic_payload) => {
                    let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Worker thread panicked with unknown payload".to_string()
                    };
                    let _ = rec.mark_failed(format!("Panicked: {}", panic_msg));
                }
            }
        }

        // 4. Notify all waiters outside the mutex
        task.notify.notify_all();
    }

    /// Awaits task completion with optional timeout and spurious wakeup protection.
    pub fn await_task(
        &self,
        id: &str,
        timeout: Option<Duration>,
    ) -> Result<TaskSnapshot, TaskError> {
        let task = self
            .get_inner(id)
            .ok_or_else(|| TaskError::NotFound(id.to_string()))?;

        let mut rec = task.lock_record();

        if rec.status.is_terminal() {
            return Ok(rec.snapshot());
        }

        match timeout {
            None => {
                while !rec.status.is_terminal() {
                    rec = task.notify.wait(rec).unwrap_or_else(|e| e.into_inner());
                }
                Ok(rec.snapshot())
            }
            Some(dur) => {
                let start = Instant::now();

                while !rec.status.is_terminal() {
                    let elapsed = start.elapsed();
                    if elapsed >= dur {
                        return Err(TaskError::Timeout(dur));
                    }
                    let remaining = dur - elapsed;

                    let (new_rec, wait_res) = task
                        .notify
                        .wait_timeout(rec, remaining)
                        .unwrap_or_else(|e| e.into_inner());

                    rec = new_rec;

                    if rec.status.is_terminal() {
                        return Ok(rec.snapshot());
                    }

                    if wait_res.timed_out() {
                        return Err(TaskError::Timeout(dur));
                    }
                }

                Ok(rec.snapshot())
            }
        }
    }

    /// Awaits until the task transitions from Queued to Running (or reaches a terminal state),
    /// with optional timeout. Enables zero-sleep, deterministic startup synchronization.
    pub fn await_running(
        &self,
        id: &str,
        timeout: Option<Duration>,
    ) -> Result<TaskSnapshot, TaskError> {
        let task = self
            .get_inner(id)
            .ok_or_else(|| TaskError::NotFound(id.to_string()))?;

        let mut rec = task.lock_record();
        if rec.status != TaskStatus::Queued {
            return Ok(rec.snapshot());
        }

        match timeout {
            None => {
                while rec.status == TaskStatus::Queued {
                    rec = task.notify.wait(rec).unwrap_or_else(|e| e.into_inner());
                }
                Ok(rec.snapshot())
            }
            Some(dur) => {
                let start = Instant::now();
                while rec.status == TaskStatus::Queued {
                    let elapsed = start.elapsed();
                    if elapsed >= dur {
                        return Err(TaskError::Timeout(dur));
                    }
                    let remaining = dur - elapsed;
                    let (new_rec, wait_res) = task
                        .notify
                        .wait_timeout(rec, remaining)
                        .unwrap_or_else(|e| e.into_inner());
                    rec = new_rec;
                    if rec.status != TaskStatus::Queued {
                        return Ok(rec.snapshot());
                    }
                    if wait_res.timed_out() {
                        return Err(TaskError::Timeout(dur));
                    }
                }
                Ok(rec.snapshot())
            }
        }
    }

    /// Signals cancellation. Queued tasks are transitioned immediately to Cancelled,
    /// while running tasks are flagged via CancellationToken.
    pub fn cancel_task(&self, id: &str) -> Result<(), TaskError> {
        let task = self
            .get_inner(id)
            .ok_or_else(|| TaskError::NotFound(id.to_string()))?;

        let mut rec = task.lock_record();
        if rec.status.is_terminal() {
            return Err(TaskError::AlreadyTerminal(rec.status));
        }

        task.cancellation_token.cancel();

        if rec.status == TaskStatus::Queued {
            let _ = rec.mark_cancelled(Some("Cancelled while queued".into()));
            drop(rec);
            task.notify.notify_all();
        }

        Ok(())
    }

    /// Convenience method matching PROJECT.md interface contract:
    /// returns true if cancellation succeeded, false if not found or already terminal.
    pub fn cancel(&self, id: &str) -> bool {
        self.cancel_task(id).is_ok()
    }

    /// Detailed variant alias for `cancel_task`.
    pub fn try_cancel_task(&self, id: &str) -> Result<(), TaskError> {
        self.cancel_task(id)
    }

    /// Inspects a task snapshot by ID.
    pub fn get_task(&self, id: &str) -> Option<TaskSnapshot> {
        let task = self.get_inner(id)?;
        let rec = task.lock_record();
        Some(rec.snapshot())
    }

    /// Lists snapshots of all tasks in the registry.
    pub fn list_tasks(&self) -> Vec<TaskSnapshot> {
        let tasks = self.read_tasks();
        let mut snapshots: Vec<TaskSnapshot> = tasks
            .values()
            .map(|t| {
                let rec = t.lock_record();
                rec.snapshot()
            })
            .collect();

        // Sort numerically by task ID if prefixed with "task-", otherwise lexicographically
        snapshots.sort_by(|a, b| {
            let num_a = a
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            let num_b = b
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            match (num_a, num_b) {
                (Some(na), Some(nb)) => na.cmp(&nb),
                _ => a.id.cmp(&b.id),
            }
        });

        snapshots
    }

    /// Retrieves log lines for a specific task.
    pub fn get_task_logs(&self, id: &str) -> Option<Vec<String>> {
        let task = self.get_inner(id)?;
        Some(task.log_buffer.lines())
    }

    /// Clears all terminal tasks from the registry. Returns number of tasks removed.
    pub fn clear_completed(&self) -> usize {
        let mut tasks = self.write_tasks();
        let initial_len = tasks.len();
        tasks.retain(|_, task| {
            let rec = task.lock_record();
            !rec.status.is_terminal()
        });
        initial_len - tasks.len()
    }

    /// Prunes completed tasks down to `max_retained`, removing oldest finished first.
    pub fn prune_tasks(&self, max_retained: usize) -> usize {
        let mut tasks = self.write_tasks();
        let mut completed: Vec<(String, SystemTime)> = tasks
            .iter()
            .filter_map(|(id, task)| {
                let rec = task.lock_record();
                if rec.status.is_terminal() {
                    Some((id.clone(), rec.finished_at.unwrap_or(rec.created_at)))
                } else {
                    None
                }
            })
            .collect();

        if completed.len() <= max_retained {
            return 0;
        }

        completed.sort_by_key(|(_, time)| *time);
        let to_remove = completed.len() - max_retained;

        for (id, _) in completed.into_iter().take(to_remove) {
            tasks.remove(&id);
        }

        to_remove
    }

    /// Removes a specific task if it has completed. Rejects removal of running tasks.
    pub fn remove_task(&self, id: &str) -> Result<bool, TaskError> {
        let mut tasks = self.write_tasks();
        if let Some(task) = tasks.get(id) {
            let rec = task.lock_record();
            if !rec.status.is_terminal() {
                return Err(TaskError::CannotRemoveRunning(id.to_string()));
            }
        } else {
            return Ok(false);
        }
        tasks.remove(id);
        Ok(true)
    }

    /// Atomically extracts all terminal tasks that have not yet been announced,
    /// marks them as notified, and returns their snapshots sorted deterministically by task ID.
    pub fn drain_unnotified_terminal_tasks(&self) -> Vec<TaskSnapshot> {
        let inners: Vec<Arc<TaskInner>> = {
            let tasks = self.read_tasks();
            tasks.values().cloned().collect()
        };

        let mut unnotified = Vec::new();
        for task in inners {
            let mut rec = task.lock_record();
            if rec.status.is_terminal() && !rec.notified {
                rec.notified = true;
                unnotified.push(rec.snapshot());
            }
        }

        // Sort numerically by task ID if prefixed with "task-", otherwise lexicographically
        unnotified.sort_by(|a, b| {
            let num_a = a
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            let num_b = b
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            match (num_a, num_b) {
                (Some(na), Some(nb)) => na.cmp(&nb),
                _ => a.id.cmp(&b.id),
            }
        });

        unnotified
    }

    /// Read-only inspection of unnotified terminal tasks without modifying notification state.
    pub fn get_unnotified_terminal_tasks(&self) -> Vec<TaskSnapshot> {
        let inners: Vec<Arc<TaskInner>> = {
            let tasks = self.read_tasks();
            tasks.values().cloned().collect()
        };

        let mut unnotified = Vec::new();
        for task in inners {
            let rec = task.lock_record();
            if rec.status.is_terminal() && !rec.notified {
                unnotified.push(rec.snapshot());
            }
        }

        unnotified.sort_by(|a, b| {
            let num_a = a
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            let num_b = b
                .id
                .strip_prefix("task-")
                .and_then(|s| s.parse::<usize>().ok());
            match (num_a, num_b) {
                (Some(na), Some(nb)) => na.cmp(&nb),
                _ => a.id.cmp(&b.id),
            }
        });

        unnotified
    }

    /// Explicitly marks a task as notified (e.g. after interactive `/tasks wait` or `/tasks cancel`).
    /// Returns true if the task was found and was transitioned from unnotified to notified.
    pub fn mark_task_notified(&self, id: &str) -> bool {
        if let Some(task) = self.get_inner(id) {
            let mut rec = task.lock_record();
            if !rec.notified {
                rec.notified = true;
                return true;
            }
        }
        false
    }

    /// Fast boolean check for whether any unnotified terminal tasks exist.
    pub fn has_unnotified_completions(&self) -> bool {
        let inners: Vec<Arc<TaskInner>> = {
            let tasks = self.read_tasks();
            tasks.values().cloned().collect()
        };

        for task in inners {
            let rec = task.lock_record();
            if rec.status.is_terminal() && !rec.notified {
                return true;
            }
        }
        false
    }
}

// ============================================================================
// 8. Comprehensive Unit & Concurrency Test Suite
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // SUITE 1: STATE MACHINE & LIFECYCLE TRANSITIONS
    // =========================================================================

    #[test]
    fn test_task_status_lifecycle_and_invariants() {
        // 1. Permitted transitions
        assert!(TaskStatus::Queued.can_transition_to(TaskStatus::Running));
        assert!(TaskStatus::Queued.can_transition_to(TaskStatus::Cancelled));
        assert!(TaskStatus::Queued.can_transition_to(TaskStatus::Failed));
        assert!(TaskStatus::Running.can_transition_to(TaskStatus::Completed));
        assert!(TaskStatus::Running.can_transition_to(TaskStatus::Failed));
        assert!(TaskStatus::Running.can_transition_to(TaskStatus::Cancelled));

        // 2. Prohibited transitions (terminal immutability & invalid jumps)
        assert!(!TaskStatus::Completed.can_transition_to(TaskStatus::Running));
        assert!(!TaskStatus::Completed.can_transition_to(TaskStatus::Queued));
        assert!(!TaskStatus::Failed.can_transition_to(TaskStatus::Running));
        assert!(!TaskStatus::Cancelled.can_transition_to(TaskStatus::Running));
        assert!(!TaskStatus::Queued.can_transition_to(TaskStatus::Completed));

        // 3. Predicates
        assert!(!TaskStatus::Queued.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());

        // 4. Serde roundtrip
        let json_str = serde_json::to_string(&TaskStatus::Completed).unwrap();
        assert_eq!(json_str, "\"completed\"");
        let parsed: TaskStatus = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, TaskStatus::Completed);

        // 5. FromStr parsing (supporting both US and UK spellings)
        assert_eq!("queued".parse::<TaskStatus>().unwrap(), TaskStatus::Queued);
        assert_eq!("running".parse::<TaskStatus>().unwrap(), TaskStatus::Running);
        assert_eq!("completed".parse::<TaskStatus>().unwrap(), TaskStatus::Completed);
        assert_eq!("failed".parse::<TaskStatus>().unwrap(), TaskStatus::Failed);
        assert_eq!("cancelled".parse::<TaskStatus>().unwrap(), TaskStatus::Cancelled);
        assert_eq!("canceled".parse::<TaskStatus>().unwrap(), TaskStatus::Cancelled);
        assert!("invalid_state".parse::<TaskStatus>().is_err());
    }

    #[test]
    fn test_lifecycle_queued_to_running_to_completed() {
        let manager = TaskManager::new();
        let (id, _token) = manager
            .spawn_task("test-task".into(), "test run".into(), |_token| {
                thread::sleep(Duration::from_millis(15));
                Ok("successful execution".to_string())
            })
            .expect("Task should spawn successfully");

        let snapshot = manager
            .await_task(&id, Some(Duration::from_millis(1000)))
            .expect("Task should complete within timeout");

        assert_eq!(snapshot.id, id);
        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert_eq!(snapshot.result.as_deref(), Some("successful execution"));
        assert!(snapshot.error.is_none());
        assert!(snapshot.started_at.is_some());
        assert!(snapshot.finished_at.is_some());
        assert!(snapshot.duration_ms.is_some());
        assert!(snapshot.duration_ms.unwrap() >= 10);
    }

    #[test]
    fn test_lifecycle_queued_to_running_to_failed() {
        let manager = TaskManager::new();
        let (id, _token) = manager
            .spawn_task("fail-task".into(), "expected error".into(), |_token| {
                Err(anyhow::anyhow!("synthetic file read failure"))
            })
            .expect("Task should spawn");

        let snapshot = manager
            .await_task(&id, Some(Duration::from_millis(1000)))
            .expect("Await should return terminal state");

        assert_eq!(snapshot.status, TaskStatus::Failed);
        assert!(snapshot.result.is_none());
        assert!(snapshot
            .error
            .as_ref()
            .unwrap()
            .contains("synthetic file read failure"));
    }

    #[test]
    fn test_lifecycle_timestamps_and_duration_monotonicity() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("time-task".into(), "timing test".into(), |_| {
                thread::sleep(Duration::from_millis(35));
                Ok("ok".into())
            })
            .unwrap();

        let snapshot = manager.await_task(&id, None).unwrap();
        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert!(snapshot.duration_ms.unwrap() >= 30);
        assert!(snapshot.started_at.is_some());
        assert!(snapshot.finished_at.is_some());
    }

    #[test]
    fn test_worker_panic_resilience_and_no_hang() {
        let manager = TaskManager::new();
        let (id, _token) = manager
            .spawn_task("panic-task".into(), "intentional crash".into(), |_| {
                panic!("Deliberate worker panic to test catch_unwind");
            })
            .expect("Task should spawn");

        let snapshot = manager
            .await_task(&id, Some(Duration::from_millis(1000)))
            .expect("Await must unblock on thread panic");

        assert_eq!(snapshot.status, TaskStatus::Failed);
        assert!(snapshot.result.is_none());
        let err_msg = snapshot.error.expect("Error must capture panic message");
        assert!(err_msg.contains("Panicked: Deliberate worker panic"));

        // Registry should still be fully operational
        let list = manager.list_tasks();
        assert_eq!(list.len(), 1);
    }

    // =========================================================================
    // SUITE 2: COOPERATIVE CANCELLATION TESTS
    // =========================================================================

    #[test]
    fn test_cancellation_token_atomic_sharing() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        let token_clone = token.clone();
        token.cancel();
        assert!(token.is_cancelled());
        assert!(token_clone.is_cancelled());

        // Idempotent cancel
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cooperative_cancellation_mid_execution() {
        let manager = TaskManager::new();
        let iterations = Arc::new(AtomicUsize::new(0));
        let iter_clone = iterations.clone();

        let (id, _token) = manager
            .spawn_task("loop-task".into(), "cancellable work".into(), move |token| {
                for _ in 0..100 {
                    iter_clone.fetch_add(1, Ordering::SeqCst);
                    if token.is_cancelled() {
                        return Err(anyhow::anyhow!("stopped via cancellation token"));
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Ok("finished full loop".to_string())
            })
            .unwrap();

        // Deterministically wait until task enters Running
        manager
            .await_running(&id, Some(Duration::from_secs(2)))
            .expect("Task must start running");

        // Wait until worker has executed at least 2 iterations
        let wait_start = Instant::now();
        while iterations.load(Ordering::SeqCst) < 2 && wait_start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            iterations.load(Ordering::SeqCst) >= 2,
            "Worker thread failed to execute at least 2 iterations within timeout"
        );

        let cancel_res = manager.cancel_task(&id);
        assert!(cancel_res.is_ok(), "Cancellation should succeed");

        let snapshot = manager
            .await_task(&id, Some(Duration::from_millis(3000)))
            .unwrap();
        assert_eq!(snapshot.status, TaskStatus::Cancelled);
        assert!(snapshot.result.is_none());
        assert!(snapshot.error.is_some());
        let iters = iterations.load(Ordering::SeqCst);
        assert!(
            iters < 100,
            "Task should have stopped early, but executed {} iterations",
            iters
        );
        assert!(
            snapshot.duration_ms.is_some(),
            "Cancelled task must have duration_ms populated"
        );
        let dur = snapshot.duration_ms.unwrap();
        assert!(
            dur < 800,
            "Task should terminate early, duration was {}ms (expected < 800ms)",
            dur
        );
    }

    #[test]
    fn test_cancellation_before_execution_start() {
        let manager = TaskManager::new();
        let (id, token) = manager
            .spawn_task("immediate-cancel".into(), "fast cancel".into(), |token| {
                if token.is_cancelled() {
                    return Err(anyhow::anyhow!("cancelled before work"));
                }
                thread::sleep(Duration::from_millis(50));
                Ok("done".into())
            })
            .unwrap();

        // Cancel token immediately
        token.cancel();

        let snapshot = manager
            .await_task(&id, Some(Duration::from_millis(1000)))
            .unwrap();
        assert_eq!(snapshot.status, TaskStatus::Cancelled);
    }

    #[test]
    fn test_cancel_terminal_task_rejected() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("quick".into(), "quick task".into(), |_| Ok("done".into()))
            .unwrap();

        let snapshot = manager.await_task(&id, None).unwrap();
        assert_eq!(snapshot.status, TaskStatus::Completed);

        // Attempting to cancel already completed task
        let cancel_res = manager.cancel_task(&id);
        assert!(cancel_res.is_err());
        match cancel_res.unwrap_err() {
            TaskError::AlreadyTerminal(status) => assert_eq!(status, TaskStatus::Completed),
            other => panic!("Expected AlreadyTerminal, got {:?}", other),
        }
    }

    // =========================================================================
    // SUITE 3: SYNCHRONIZATION & AWAIT MECHANICS
    // =========================================================================

    #[test]
    fn test_await_immediate_for_completed() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("fast".into(), "fast task".into(), |_| Ok("instant".into()))
            .unwrap();

        let _ = manager.await_task(&id, None).unwrap();

        // Subsequent await returns immediately
        let start = Instant::now();
        let snapshot = manager.await_task(&id, None).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert!(
            elapsed < Duration::from_millis(10),
            "Immediate return took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_await_blocking_until_finish() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("blocking".into(), "blocking task".into(), |_| {
                thread::sleep(Duration::from_millis(30));
                Ok("unblocked".into())
            })
            .unwrap();

        let start = Instant::now();
        let snapshot = manager.await_task(&id, None).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert!(
            elapsed >= Duration::from_millis(25),
            "Elapsed should be >= 25ms, was {:?}",
            elapsed
        );
    }

    #[test]
    fn test_await_timeout_expired_running() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("long".into(), "long running task".into(), |_| {
                thread::sleep(Duration::from_millis(1000));
                Ok("finally done".into())
            })
            .unwrap();

        // Deterministically wait until worker has entered Running
        manager
            .await_running(&id, Some(Duration::from_secs(2)))
            .expect("Task must start running");

        let start = Instant::now();
        let await_res = manager.await_task(&id, Some(Duration::from_millis(30)));
        let elapsed = start.elapsed();

        assert!(await_res.is_err());
        match await_res.unwrap_err() {
            TaskError::Timeout(dur) => assert_eq!(dur, Duration::from_millis(30)),
            other => panic!("Expected Timeout, got {:?}", other),
        }
        assert!(
            elapsed >= Duration::from_millis(10),
            "Timeout fired prematurely: {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(2000),
            "Timeout took excessively long: {:?}",
            elapsed
        );

        // Task should still be running
        let snap = manager.get_task(&id).expect("Task must still exist");
        assert_eq!(snap.status, TaskStatus::Running);

        // Awaiting with larger timeout completes normally
        let final_snap = manager
            .await_task(&id, Some(Duration::from_millis(3000)))
            .unwrap();
        assert_eq!(final_snap.status, TaskStatus::Completed);
    }

    #[test]
    fn test_cancellation_while_queued_duration_populated() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut rec = TaskRecord::new("queued-cancel-test", "test", "desc", token, logs);

        assert_eq!(rec.status, TaskStatus::Queued);
        assert!(rec.duration.is_none());

        rec.mark_cancelled(Some("Cancelled before dispatch".into()))
            .unwrap();
        assert_eq!(rec.status, TaskStatus::Cancelled);
        assert!(
            rec.duration.is_some(),
            "record.duration must be populated even when cancelled from Queued"
        );
        let snap = rec.snapshot();
        assert_eq!(snap.status, TaskStatus::Cancelled);
        assert!(
            snap.duration_ms.is_some(),
            "snapshot.duration_ms must be Some for terminal tasks"
        );
        assert!(snap.elapsed_secs >= 0.0);
    }

    #[test]
    fn test_queued_task_cancellation_via_manager_duration_guarantee() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("queued-cancel-mgr".into(), "cancel immediately".into(), |_| {
                thread::sleep(Duration::from_millis(200));
                Ok("ok".into())
            })
            .unwrap();

        // Immediately cancel via cancel helper
        assert!(manager.cancel(&id));

        let snap = manager
            .await_task(&id, Some(Duration::from_millis(500)))
            .unwrap();
        assert_eq!(snap.status, TaskStatus::Cancelled);
        assert!(
            snap.duration_ms.is_some(),
            "duration_ms must be populated for cancelled task"
        );
        assert!(snap.duration_ms.unwrap() < 200);
    }

    #[test]
    fn test_queued_failure_duration_populated() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut rec = TaskRecord::new("queued-fail-test", "test", "desc", token, logs);

        assert_eq!(rec.status, TaskStatus::Queued);
        assert!(rec.duration.is_none());

        rec.mark_failed("Immediate spawn failure".into()).unwrap();
        assert_eq!(rec.status, TaskStatus::Failed);
        assert!(
            rec.duration.is_some(),
            "record.duration must be populated even when failed from Queued"
        );
        let snap = rec.snapshot();
        assert_eq!(snap.status, TaskStatus::Failed);
        assert!(
            snap.duration_ms.is_some(),
            "snapshot.duration_ms must be Some for terminal tasks"
        );
    }

    #[test]
    fn test_await_running_synchronization() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("running-sync".into(), "sync test".into(), |_| {
                thread::sleep(Duration::from_millis(50));
                Ok("done".into())
            })
            .unwrap();

        let snap = manager
            .await_running(&id, Some(Duration::from_secs(1)))
            .unwrap();
        assert!(snap.status == TaskStatus::Running || snap.status.is_terminal());

        let final_snap = manager.await_task(&id, None).unwrap();
        assert_eq!(final_snap.status, TaskStatus::Completed);
    }

    #[test]
    fn test_cancel_convenience_method() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("cancel-conv".into(), "test cancel conv".into(), |_| {
                thread::sleep(Duration::from_millis(100));
                Ok("done".into())
            })
            .unwrap();

        assert!(manager.cancel(&id));
        // Second cancel should return false as it is already terminal
        let snap = manager.await_task(&id, None).unwrap();
        assert_eq!(snap.status, TaskStatus::Cancelled);
        assert!(!manager.cancel(&id));
        // Nonexistent task returns false
        assert!(!manager.cancel("nonexistent-task-id"));
    }

    #[test]
    fn test_await_multiple_concurrent_awaiters() {
        let manager = Arc::new(TaskManager::new());
        let (id, _) = manager
            .spawn_task("shared".into(), "shared task".into(), |_| {
                thread::sleep(Duration::from_millis(40));
                Ok("broadcast result".into())
            })
            .unwrap();

        let mut handles = Vec::new();
        for _ in 0..4 {
            let m = manager.clone();
            let tid = id.clone();
            handles.push(thread::spawn(move || {
                m.await_task(&tid, Some(Duration::from_millis(1000)))
                    .expect("All awaiters must successfully unblock")
            }));
        }

        for h in handles {
            let snap = h.join().unwrap();
            assert_eq!(snap.status, TaskStatus::Completed);
            assert_eq!(snap.result.as_deref(), Some("broadcast result"));
        }
    }

    #[test]
    fn test_await_not_found() {
        let manager = TaskManager::new();
        let res = manager.await_task("task-nonexistent-404", None);
        assert!(res.is_err());
        match res.unwrap_err() {
            TaskError::NotFound(id) => assert_eq!(id, "task-nonexistent-404"),
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    // =========================================================================
    // SUITE 4: HIGH-CONCURRENCY STRESS TESTING (10–20 CONCURRENT TASKS)
    // =========================================================================

    #[test]
    fn test_concurrency_stress_20_simultaneous_tasks() {
        let manager = Arc::new(TaskManager::new());
        const TASK_COUNT: usize = 20;

        // Atomic probes to deterministically prove concurrent execution without wall-clock fragility
        let active_count = Arc::new(AtomicUsize::new(0));
        let peak_count = Arc::new(AtomicUsize::new(0));

        let start_time = Instant::now();
        let mut task_ids = Vec::with_capacity(TASK_COUNT);

        // Spawn 20 tasks concurrently
        for i in 0..TASK_COUNT {
            let active = active_count.clone();
            let peak = peak_count.clone();
            let (id, _) = manager
                .spawn_task(
                    format!("stress-{}", i),
                    format!("concurrency task {}", i),
                    move |_| {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);

                        thread::sleep(Duration::from_millis(40));

                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok(format!("result from worker {}", i))
                    },
                )
                .unwrap();
            task_ids.push(id);
        }

        assert_eq!(task_ids.len(), TASK_COUNT);

        // Await all 20 tasks concurrently using worker threads
        let mut await_handles = Vec::with_capacity(TASK_COUNT);
        for tid in task_ids {
            let m = manager.clone();
            await_handles.push(thread::spawn(move || {
                m.await_task(&tid, Some(Duration::from_secs(10)))
                    .expect("Task awaiter must not time out under load")
            }));
        }

        for (i, h) in await_handles.into_iter().enumerate() {
            let snap = h.join().expect("Worker thread must join without panic");
            assert_eq!(snap.status, TaskStatus::Completed);
            assert!(snap.result.unwrap().contains(&format!("worker {}", i)));
        }

        // 1. Deterministic Concurrency Proof:
        // Peak simultaneous worker count must prove significant parallel overlap.
        // Serial execution would yield peak == 1. Peak >= 5 conclusively proves parallel execution.
        let measured_peak = peak_count.load(Ordering::SeqCst);
        assert!(
            measured_peak >= 5,
            "Expected concurrent overlap (peak >= 5), but measured peak was {}",
            measured_peak
        );

        // 2. Deadlock Guard:
        // Generous fail-safe ceiling to detect deadlocks or hangs without brittleness to OS scheduling.
        let total_elapsed = start_time.elapsed();
        assert!(
            total_elapsed < Duration::from_secs(10),
            "Execution exceeded deadlock fail-safe threshold, total elapsed: {:?}",
            total_elapsed
        );

        let all_tasks = manager.list_tasks();
        assert_eq!(all_tasks.len(), TASK_COUNT);
    }

    #[test]
    fn test_concurrency_peak_parallelism_verification() {
        let manager = Arc::new(TaskManager::new());
        const CONCURRENT_WORKERS: usize = 15;
        let active_workers = Arc::new(AtomicUsize::new(0));
        let peak_workers = Arc::new(AtomicUsize::new(0));

        let mut task_ids = Vec::new();
        for i in 0..CONCURRENT_WORKERS {
            let active = active_workers.clone();
            let peak = peak_workers.clone();
            let (id, _) = manager
                .spawn_task(
                    format!("parallel-{}", i),
                    "parallel probe".into(),
                    move |_| {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut current_peak = peak.load(Ordering::SeqCst);
                        while current > current_peak {
                            match peak.compare_exchange_weak(
                                current_peak,
                                current,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            ) {
                                Ok(_) => break,
                                Err(actual) => current_peak = actual,
                            }
                        }

                        thread::sleep(Duration::from_millis(30));
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok("done".into())
                    },
                )
                .unwrap();
            task_ids.push(id);
        }

        for tid in task_ids {
            let _ = manager
                .await_task(&tid, Some(Duration::from_millis(1500)))
                .unwrap();
        }

        let measured_peak = peak_workers.load(Ordering::SeqCst);
        assert!(
            measured_peak >= 2,
            "Measured peak concurrency was {}, expected >= 2",
            measured_peak
        );
    }

    #[test]
    fn test_concurrency_mixed_workload_stress() {
        let manager = Arc::new(TaskManager::new());
        let mut ids = Vec::new();

        // 6 Success tasks
        for i in 0..6 {
            let (id, _) = manager
                .spawn_task(format!("succ-{}", i), "succ".into(), move |_| {
                    thread::sleep(Duration::from_millis(15));
                    Ok(format!("ok-{}", i))
                })
                .unwrap();
            ids.push((id, "succ"));
        }

        // 4 Failing tasks
        for i in 0..4 {
            let (id, _) = manager
                .spawn_task(format!("fail-{}", i), "fail".into(), move |_| {
                    thread::sleep(Duration::from_millis(10));
                    Err(anyhow::anyhow!("err-{}", i))
                })
                .unwrap();
            ids.push((id, "fail"));
        }

        // 4 Cancelled tasks
        for i in 0..4 {
            let (id, _) = manager
                .spawn_task(format!("canc-{}", i), "canc".into(), |token| {
                    for _ in 0..15 {
                        if token.is_cancelled() {
                            return Err(anyhow::anyhow!("cancelled"));
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok("finished".into())
                })
                .unwrap();
            ids.push((id, "canc"));
        }

        // Cancel the 4 cancellable tasks after brief delay
        thread::sleep(Duration::from_millis(20));
        for (id, kind) in &ids {
            if *kind == "canc" {
                let _ = manager.cancel_task(id);
            }
        }

        // Spawn observer thread hammering list_tasks and get_task
        let observer_m = manager.clone();
        let stop_observer = Arc::new(AtomicBool::new(false));
        let stop_flag = stop_observer.clone();
        let observer_handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                let _ = observer_m.list_tasks();
                thread::sleep(Duration::from_millis(2));
            }
        });

        // Await all tasks
        for (id, kind) in &ids {
            let snap = manager
                .await_task(id, Some(Duration::from_millis(1500)))
                .unwrap();
            match *kind {
                "succ" => assert_eq!(snap.status, TaskStatus::Completed),
                "fail" => assert_eq!(snap.status, TaskStatus::Failed),
                "canc" => assert_eq!(snap.status, TaskStatus::Cancelled),
                _ => {}
            }
        }

        stop_observer.store(true, Ordering::Relaxed);
        observer_handle.join().unwrap();
    }

    // =========================================================================
    // SUITE 5: LOG BUFFER & OUTPUT ISOLATION
    // =========================================================================

    #[test]
    fn test_task_log_buffer_high_contention() {
        let buffer = Arc::new(TaskLogBuffer::new());
        let mut handles = Vec::new();

        for t in 0..10 {
            let buf = buffer.clone();
            handles.push(thread::spawn(move || {
                for i in 0..50 {
                    buf.log(format!("thread {} msg {}", t, i));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(buffer.len(), 500);
        let all = buffer.all_lines();
        assert_eq!(all.len(), 500);
    }

    #[test]
    fn test_task_log_buffer_pagination_boundaries() {
        let buffer = TaskLogBuffer::new();
        for i in 0..50 {
            buffer.log(format!("line {}", i));
        }

        let slice1 = buffer.get_lines(0, 10);
        assert_eq!(slice1.len(), 10);
        assert_eq!(slice1[0], "line 0");
        assert_eq!(slice1[9], "line 9");

        // Clamped slice near boundary
        let slice2 = buffer.get_lines(45, 10);
        assert_eq!(slice2.len(), 5);
        assert_eq!(slice2[0], "line 45");
        assert_eq!(slice2[4], "line 49");

        // Completely out-of-bounds slice
        let slice3 = buffer.get_lines(100, 10);
        assert!(slice3.is_empty());
    }

    // =========================================================================
    // SUITE 6: PRUNING & SINGLETON
    // =========================================================================

    #[test]
    fn test_task_manager_prune_and_clear_completed() {
        let manager = TaskManager::new();

        let (id1, _) = manager
            .spawn_task("s1".into(), "s1".into(), |_| Ok("ok".into()))
            .unwrap();
        let (id2, _) = manager
            .spawn_task("s2".into(), "s2".into(), |_| Ok("ok".into()))
            .unwrap();
        let (id3, _) = manager
            .spawn_task("f1".into(), "f1".into(), |_| Err(anyhow::anyhow!("err")))
            .unwrap();
        let (id4, token4) = manager
            .spawn_task("active".into(), "active".into(), |_| {
                thread::sleep(Duration::from_millis(300));
                Ok("done".into())
            })
            .unwrap();

        manager.await_task(&id1, None).unwrap();
        manager.await_task(&id2, None).unwrap();
        manager.await_task(&id3, None).unwrap();

        // Ensure task 4 has transitioned to Running
        for _ in 0..100 {
            if manager.get_task(&id4).map(|s| s.status) == Some(TaskStatus::Running) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        // Clear completed terminal tasks
        let pruned_count = manager.clear_completed();
        assert_eq!(pruned_count, 3);

        let remaining = manager.list_tasks();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id4);
        assert_eq!(remaining[0].status, TaskStatus::Running);

        token4.cancel();
    }

    // =========================================================================
    // SUITE 7: ADDITIONAL COVERAGE (DATA MODELS, POISON RECOVERY, SINGLETON)
    // =========================================================================

    #[test]
    fn test_task_status_display_and_parsing() {
        assert_eq!(TaskStatus::Queued.to_string(), "queued");
        assert_eq!(TaskStatus::Running.to_string(), "running");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Failed.to_string(), "failed");
        assert_eq!(TaskStatus::Cancelled.to_string(), "cancelled");

        assert_eq!(TaskStatus::from_str("queued").unwrap(), TaskStatus::Queued);
        assert_eq!(TaskStatus::from_str("RUNNING").unwrap(), TaskStatus::Running);
        assert_eq!(TaskStatus::from_str("completed").unwrap(), TaskStatus::Completed);
        assert_eq!(TaskStatus::from_str("failed").unwrap(), TaskStatus::Failed);
        assert_eq!(TaskStatus::from_str("cancelled").unwrap(), TaskStatus::Cancelled);
        assert_eq!(TaskStatus::from_str("canceled").unwrap(), TaskStatus::Cancelled);
        assert!(TaskStatus::from_str("unknown_status").is_err());
    }

    #[test]
    fn test_task_status_predicates_and_badges() {
        assert!(TaskStatus::Queued.is_active());
        assert!(!TaskStatus::Queued.is_terminal());
        assert!(TaskStatus::Queued.can_cancel());

        assert!(TaskStatus::Running.is_active());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Running.can_cancel());

        assert!(!TaskStatus::Completed.is_active());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(!TaskStatus::Completed.can_cancel());

        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());

        assert!(TaskStatus::Queued.badge().contains("[QUEUED]"));
        assert_eq!(TaskStatus::Running.plain_badge(), "[RUNNING]");
    }

    #[test]
    fn test_task_status_serde_roundtrip() {
        let serialized = serde_json::to_string(&TaskStatus::Running).unwrap();
        assert_eq!(serialized, "\"running\"");
        let deserialized: TaskStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, TaskStatus::Running);
    }

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());

        let cloned = token.clone();
        token.cancel();

        assert!(token.is_cancelled());
        assert!(cloned.is_cancelled());
        assert!(cloned.check().is_err());
    }

    #[test]
    fn test_task_lifecycle_transitions() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut record = TaskRecord::new("task_1", "test", "description", token, logs);

        assert_eq!(record.status, TaskStatus::Queued);
        assert!(record.started_at.is_none());

        // Queued -> Running
        record.mark_running().unwrap();
        assert_eq!(record.status, TaskStatus::Running);
        assert!(record.started_at.is_some());

        // Running -> Completed
        record.mark_completed("Done successfully".to_string()).unwrap();
        assert_eq!(record.status, TaskStatus::Completed);
        assert_eq!(record.result.as_deref(), Some("Done successfully"));
        assert!(record.finished_at.is_some());
        assert!(record.duration.is_some());

        // Completed -> Any should fail
        assert!(record.mark_running().is_err());
        assert!(record.mark_failed("Err".into()).is_err());
    }

    #[test]
    fn test_cancellation_while_queued() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut record =
            TaskRecord::new("task_2", "cancel_queued", "desc", token.clone(), logs);

        record.mark_cancelled(Some("User cancelled".into())).unwrap();
        assert_eq!(record.status, TaskStatus::Cancelled);
        assert!(token.is_cancelled());

        // Cannot start a cancelled task
        assert!(record.mark_running().is_err());
    }

    #[test]
    fn test_timestamp_formatting() {
        let epoch = SystemTime::UNIX_EPOCH;
        assert_eq!(format_utc_timestamp(epoch), "1970-01-01T00:00:00Z");

        // Human duration formatting
        assert_eq!(format_duration_human(Duration::from_millis(250)), "250ms");
        assert_eq!(format_duration_human(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration_human(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration_human(Duration::from_secs(125)), "2m 05s");
        assert_eq!(format_duration_human(Duration::from_secs(3720)), "1h 02m");
    }

    #[test]
    fn test_task_snapshot_serialization() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut record = TaskRecord::new("task_3", "snapshot_test", "desc", token, logs);
        record.mark_running().unwrap();
        record.mark_completed("Result payload".into()).unwrap();

        let snapshot = record.snapshot();
        assert_eq!(snapshot.id, "task_3");
        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert_eq!(snapshot.result.as_deref(), Some("Result payload"));
        assert!(snapshot.error.is_none());

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"status\":\"completed\""));
        assert!(json.contains("\"result\":\"Result payload\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_lock_poisoning_recovery() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("poison-test".into(), "test".into(), |_| Ok("ok".into()))
            .unwrap();
        let task = manager.get_inner(&id).unwrap();

        // Deliberately poison the per-task mutex
        let inner_record = task.clone();
        let _ = panic::catch_unwind(AssertUnwindSafe(move || {
            let _guard = inner_record.record.lock().unwrap();
            panic!("deliberate panic while holding mutex");
        }));

        // Verify lock_record recovers smoothly without panicking
        let rec = task.lock_record();
        assert_eq!(rec.id, id);
    }

    #[test]
    fn test_global_singleton_instance() {
        let global1 = TaskManager::global();
        let global2 = TaskManager::global();
        assert!(std::ptr::eq(global1, global2));
    }

    #[test]
    fn test_remove_running_task_rejected() {
        let manager = TaskManager::new();
        let (id, token) = manager
            .spawn_task("active-task".into(), "active".into(), |_| {
                thread::sleep(Duration::from_millis(200));
                Ok("ok".into())
            })
            .unwrap();

        // Attempting to remove active task
        let remove_res = manager.remove_task(&id);
        assert!(remove_res.is_err());
        match remove_res.unwrap_err() {
            TaskError::CannotRemoveRunning(target_id) => assert_eq!(target_id, id),
            other => panic!("Expected CannotRemoveRunning, got {:?}", other),
        }

        token.cancel();
        let _ = manager.await_task(&id, None);

        // Now removal should succeed
        let remove_res2 = manager.remove_task(&id);
        assert_eq!(remove_res2.unwrap(), true);
    }

    #[test]
    fn test_output_sink_buffered_and_terminal() {
        let buf = Arc::new(TaskLogBuffer::new());
        let sink = OutputSink::Buffered(buf.clone());
        sink.emit("test log line 1");
        sink.log("test log line 2");
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.lines(), vec!["test log line 1", "test log line 2"]);

        let term_sink = OutputSink::Terminal;
        term_sink.emit("terminal test line");
    }

    #[test]
    fn test_prune_tasks_retention() {
        let manager = TaskManager::new();

        // Spawn 5 tasks
        let mut ids = Vec::new();
        for i in 0..5 {
            let (id, _) = manager
                .spawn_task(format!("task-{}", i), "test".into(), move |_| {
                    thread::sleep(Duration::from_millis(5));
                    Ok("done".into())
                })
                .unwrap();
            ids.push(id);
        }

        for id in &ids {
            manager.await_task(id, None).unwrap();
        }

        assert_eq!(manager.list_tasks().len(), 5);

        // Retain 2 newest, prune 3
        let pruned = manager.prune_tasks(2);
        assert_eq!(pruned, 3);
        assert_eq!(manager.list_tasks().len(), 2);
    }

    // =========================================================================
    // SUITE: ONCE-ONLY NOTIFICATION TRACKING & STATE MACHINE
    // =========================================================================

    #[test]
    fn test_notification_flag_initial_and_terminal_lifecycle() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("notify-test".into(), "test run".into(), |_| {
                thread::sleep(Duration::from_millis(10));
                Ok("finished".into())
            })
            .unwrap();

        // 1. Initially unnotified in snapshot
        let snap_running = manager.get_task(&id).unwrap();
        assert!(!snap_running.notified);

        // 2. Active task cannot be drained
        let drained_early = manager.drain_unnotified_terminal_tasks();
        assert!(drained_early.is_empty());

        // 3. Await completion
        let snap_done = manager.await_task(&id, None).unwrap();
        assert_eq!(snap_done.status, TaskStatus::Completed);
        assert!(!snap_done.notified);

        // 4. First drain returns the task with notified = true
        let drained = manager.drain_unnotified_terminal_tasks();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, id);
        assert!(drained[0].notified);

        // 5. Second drain returns empty (deduplication guarantee)
        let drained_again = manager.drain_unnotified_terminal_tasks();
        assert!(drained_again.is_empty());

        // 6. Inspecting the task confirms notified = true
        let snap_final = manager.get_task(&id).unwrap();
        assert!(snap_final.notified);
    }

    #[test]
    fn test_notification_all_terminal_variants() {
        let manager = TaskManager::new();

        // Task 1: Completed
        let (id1, _) = manager
            .spawn_task("t1".into(), "will succeed".into(), |_| Ok("ok".into()))
            .unwrap();
        manager.await_task(&id1, None).unwrap();

        // Task 2: Failed
        let (id2, _) = manager
            .spawn_task("t2".into(), "will fail".into(), |_| {
                Err(anyhow::anyhow!("synthetic failure"))
            })
            .unwrap();
        manager.await_task(&id2, None).unwrap();

        // Task 3: Cancelled while queued
        let (id3, _) = manager
            .spawn_task("t3".into(), "will cancel".into(), |_| {
                thread::sleep(Duration::from_millis(100));
                Ok("ok".into())
            })
            .unwrap();
        manager.cancel_task(&id3).unwrap();
        manager.await_task(&id3, None).unwrap();

        // Drain all terminal tasks
        let drained = manager.drain_unnotified_terminal_tasks();
        assert_eq!(drained.len(), 3);

        let statuses: Vec<TaskStatus> = drained.iter().map(|s| s.status).collect();
        assert!(statuses.contains(&TaskStatus::Completed));
        assert!(statuses.contains(&TaskStatus::Failed));
        assert!(statuses.contains(&TaskStatus::Cancelled));

        // Ensure all are marked notified
        for snap in &drained {
            assert!(snap.notified);
        }

        // Subsequent drain is empty
        assert!(manager.drain_unnotified_terminal_tasks().is_empty());
    }

    #[test]
    fn test_mark_task_notified_manual_override() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("manual-notify".into(), "desc".into(), |_| Ok("ok".into()))
            .unwrap();
        manager.await_task(&id, None).unwrap();

        // Manually mark as notified (as happens in /tasks wait or /tasks cancel)
        assert!(manager.mark_task_notified(&id));
        // Calling again returns false since already marked
        assert!(!manager.mark_task_notified(&id));

        // Draining now returns nothing because it was already marked
        let drained = manager.drain_unnotified_terminal_tasks();
        assert!(drained.is_empty());
    }

    #[test]
    fn test_get_unnotified_terminal_tasks_is_read_only() {
        let manager = TaskManager::new();
        let (id, _) = manager
            .spawn_task("inspect-only".into(), "desc".into(), |_| Ok("ok".into()))
            .unwrap();
        manager.await_task(&id, None).unwrap();

        assert!(manager.has_unnotified_completions());

        // Calling get_unnotified_terminal_tasks does not mutate state
        let peek1 = manager.get_unnotified_terminal_tasks();
        assert_eq!(peek1.len(), 1);
        assert!(!peek1[0].notified);

        let peek2 = manager.get_unnotified_terminal_tasks();
        assert_eq!(peek2.len(), 1);

        // Actual drain mutates
        let drained = manager.drain_unnotified_terminal_tasks();
        assert_eq!(drained.len(), 1);
        assert!(drained[0].notified);

        assert!(!manager.has_unnotified_completions());
        assert!(manager.get_unnotified_terminal_tasks().is_empty());
    }

    #[test]
    fn test_format_notification_colored_and_plain() {
        let token = CancellationToken::new();
        let logs = Arc::new(TaskLogBuffer::new());
        let mut rec = TaskRecord::new("task-42", "subagent:test", "sample task", token, logs);
        rec.mark_running().unwrap();
        rec.mark_completed("output data".into()).unwrap();
        let snap = rec.snapshot();

        // 1. Colored format
        let colored = snap.format_notification(true);
        assert!(colored.contains("[COMPLETED]"));
        assert!(colored.contains("\x1B[1;32m"));
        assert!(colored.contains("task-42"));
        assert!(colored.contains("sample task"));

        // 2. Plain format
        let plain = snap.format_notification(false);
        assert!(plain.contains("[COMPLETED]"));
        assert!(!plain.contains("\x1B["));
        assert!(plain.contains("task-42"));
        assert!(plain.contains("sample task"));
    }

    #[test]
    fn test_spawn_task_with_sink_captures_logs() {
        let manager = TaskManager::new();
        let (id, _token, logs) = manager
            .spawn_task_with_sink("sink-test".into(), "capturing logs".into(), |_token, task_logs| {
                task_logs.push("Log entry 1: initialized");
                task_logs.push("Log entry 2: executing tool");
                task_logs.push("Log entry 3: success");
                Ok("done".into())
            })
            .unwrap();

        manager.await_task(&id, None).unwrap();

        // Verify logs via returned handle
        assert_eq!(logs.len(), 3);
        assert_eq!(logs.lines()[0], "Log entry 1: initialized");
        assert_eq!(logs.lines()[2], "Log entry 3: success");

        // Verify logs via TaskManager query
        let queried_logs = manager.get_task_logs(&id).unwrap();
        assert_eq!(queried_logs.len(), 3);
        assert_eq!(queried_logs[1], "Log entry 2: executing tool");
    }

    #[test]
    fn test_output_sink_is_silent_and_spinners() {
        let buf = Arc::new(TaskLogBuffer::new());
        let buffered_sink = OutputSink::Buffered(buf.clone());
        assert!(buffered_sink.is_silent());
        assert!(buffered_sink.buffer().is_some());

        // Ephemeral spinner methods must be silent no-ops for buffered sink
        buffered_sink.emit_spinner("Thinking...");
        buffered_sink.clear_spinner();
        assert_eq!(buf.len(), 0); // Spinners are ephemeral and not saved to persistent logs

        buffered_sink.emit("Permanent log line");
        assert_eq!(buf.len(), 1);

        let term_sink = OutputSink::Terminal;
        assert!(!term_sink.is_silent());
        assert!(term_sink.buffer().is_none());
    }

    #[test]
    fn test_drain_unnotified_ignores_active_tasks() {
        let manager = TaskManager::new();
        let (barrier_tx, barrier_rx) = std::sync::mpsc::channel::<()>();

        let (id, _) = manager
            .spawn_task("test-active".into(), "blocking".into(), move |_| {
                let _ = barrier_rx.recv();
                Ok("unblocked".into())
            })
            .unwrap();

        // While task is still active (Queued or Running), it must not be drained
        let drained = manager.drain_unnotified_terminal_tasks();
        assert_eq!(drained.len(), 0, "Active tasks must never be drained");

        // Unblock and await
        let _ = barrier_tx.send(());
        manager.await_task(&id, None).unwrap();

        // Now that it finished, drain must return it
        let drained = manager.drain_unnotified_terminal_tasks();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, id);
    }
}
