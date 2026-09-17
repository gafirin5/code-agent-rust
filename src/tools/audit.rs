//! Thread-safe, append-only audit logging subsystem for tool invocations.
//!
//! Writes single-line structured JSONL records to `.ctrl/audit.log`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

thread_local! {
    static CUSTOM_AUDIT_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

static GLOBAL_FILE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
static GLOBAL_AUDIT_LOGGER: OnceLock<AuditLogger> = OnceLock::new();

/// Structured audit record capturing tool execution details.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AuditRecord {
    pub timestamp: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    pub tool: String,
    pub parameters: serde_json::Value,
    pub status: String,
    pub duration_ms: u64,
}

/// Thread-safe logger for append-only audit trail.
#[derive(Clone, Debug)]
pub struct AuditLogger {
    log_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AuditLogger {
    /// Creates a new `AuditLogger` rooted at `workspace_root/.ctrl/audit.log`.
    pub fn new(workspace_root: &Path) -> Self {
        let ctrl_dir = workspace_root.join(".ctrl");
        let _ = fs::create_dir_all(&ctrl_dir);
        let log_path = ctrl_dir.join("audit.log");
        Self {
            log_path,
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Creates an `AuditLogger` with an explicit file path.
    pub fn from_path(log_path: PathBuf) -> Self {
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self {
            log_path,
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Returns the global default `AuditLogger` singleton for the current workspace.
    pub fn global() -> &'static AuditLogger {
        GLOBAL_AUDIT_LOGGER.get_or_init(|| {
            let root = crate::tools::filesystem::get_workspace_root();
            AuditLogger::new(&root)
        })
    }

    /// Appends an `AuditRecord` as a single-line JSON record.
    pub fn log(&self, record: &AuditRecord) -> Result<()> {
        let _global = GLOBAL_FILE_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(parent) = self.log_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create audit log directory: {:?}", parent))?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .with_context(|| format!("Failed to open audit log at {:?}", self.log_path))?;

        let json_line = serde_json::to_string(record)
            .context("Failed to serialize audit record to JSON")?;

        writeln!(file, "{}", json_line)?;
        file.flush().context("Failed to flush audit log file")?;
        Ok(())
    }

    /// Reads and parses all audit records from the log file.
    pub fn read_all(&self) -> Result<Vec<AuditRecord>> {
        let _global = GLOBAL_FILE_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());

        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.log_path)
            .with_context(|| format!("Failed to read audit log at {:?}", self.log_path))?;

        let mut records = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let rec: AuditRecord = serde_json::from_str(trimmed).with_context(|| {
                    format!("Failed to parse audit record at line {}: {}", idx + 1, trimmed)
                })?;
                records.push(rec);
            }
        }
        Ok(records)
    }

    /// Returns the path to the audit log.
    pub fn path(&self) -> &Path {
        &self.log_path
    }

    /// Clears the audit log file.
    pub fn clear(&self) -> Result<()> {
        let _global = GLOBAL_FILE_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());

        if self.log_path.exists() {
            let _ = fs::remove_file(&self.log_path);
        }
        Ok(())
    }
}

/// Logs a tool invocation to the active audit log.
pub fn log_tool_invocation(record: &AuditRecord) -> Result<()> {
    if let Some(dir) = CUSTOM_AUDIT_DIR.with(|c| c.borrow().clone()) {
        let logger = AuditLogger::new(&dir);
        return logger.log(record);
    }
    if let Ok(env_dir) = std::env::var("CTRL_AUDIT_DIR") {
        if !env_dir.trim().is_empty() {
            let logger = AuditLogger::new(Path::new(&env_dir));
            return logger.log(record);
        }
    }
    AuditLogger::global().log(record)
}

struct AuditDirGuard(Option<PathBuf>);

impl Drop for AuditDirGuard {
    fn drop(&mut self) {
        CUSTOM_AUDIT_DIR.with(|c| *c.borrow_mut() = self.0.take());
    }
}

/// Sets a thread-local workspace directory for audit logging during testing.
pub fn with_audit_dir<F, R>(dir: PathBuf, f: F) -> R
where
    F: FnOnce() -> R,
{
    let prev = CUSTOM_AUDIT_DIR.with(|c| c.borrow_mut().replace(dir));
    let _guard = AuditDirGuard(prev);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::thread;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ctrl_audit_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let _ = fs::create_dir_all(&path);
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_audit_record_task_id_serialization() {
        let rec_without_task = AuditRecord {
            timestamp: "2026-09-17T03:50:01Z".into(),
            session_id: "sess-01".into(),
            task_id: None,
            tool: "read_file".into(),
            parameters: json!({"path": "src/main.rs"}),
            status: "success".into(),
            duration_ms: 10,
        };

        let json_str = serde_json::to_string(&rec_without_task).unwrap();
        assert!(!json_str.contains("\"task_id\""));

        let deserialized: AuditRecord = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.task_id, None);
        assert_eq!(deserialized, rec_without_task);

        let rec_with_task = AuditRecord {
            timestamp: "2026-09-17T03:50:02Z".into(),
            session_id: "sess-01".into(),
            task_id: Some("task-01".into()),
            tool: "shell".into(),
            parameters: json!({"command": "cargo test"}),
            status: "success".into(),
            duration_ms: 25,
        };

        let json_str2 = serde_json::to_string(&rec_with_task).unwrap();
        assert!(json_str2.contains("\"task_id\":\"task-01\""));

        let deserialized2: AuditRecord = serde_json::from_str(&json_str2).unwrap();
        assert_eq!(deserialized2.task_id, Some("task-01".into()));
        assert_eq!(deserialized2, rec_with_task);
    }

    #[test]
    fn test_audit_logger_lifecycle() {
        let temp_dir = TempDirGuard::new("lifecycle");
        let logger = AuditLogger::new(temp_dir.path());

        // Initial state before write
        assert_eq!(logger.read_all().unwrap(), Vec::new());

        let rec1 = AuditRecord {
            timestamp: "2026-09-17T03:50:01Z".into(),
            session_id: "sess-01".into(),
            task_id: Some("task-01".into()),
            tool: "read_file".into(),
            parameters: json!({"path": "Cargo.toml"}),
            status: "success".into(),
            duration_ms: 12,
        };
        logger.log(&rec1).unwrap();

        let rec2 = AuditRecord {
            timestamp: "2026-09-17T03:50:02Z".into(),
            session_id: "sess-01".into(),
            task_id: None,
            tool: "shell".into(),
            parameters: json!({"command": "rm -rf build"}),
            status: "blocked".into(),
            duration_ms: 2,
        };
        logger.log(&rec2).unwrap();

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], rec1);
        assert_eq!(records[1], rec2);

        // Test clear
        logger.clear().unwrap();
        assert_eq!(logger.read_all().unwrap().len(), 0);
    }

    #[test]
    fn test_audit_logger_concurrency_and_special_chars() {
        let temp_dir = TempDirGuard::new("concurrency");
        let logger = Arc::new(AuditLogger::new(temp_dir.path()));

        let mut handles = Vec::new();
        for thread_idx in 0..20 {
            let logger_clone = logger.clone();
            handles.push(thread::spawn(move || {
                let rec = AuditRecord {
                    timestamp: format!("2026-09-17T03:50:{:02}Z", thread_idx),
                    session_id: format!("sess-{}", thread_idx),
                    task_id: Some(format!("task-{}", thread_idx)),
                    tool: "shell".into(),
                    parameters: json!({
                        "command": "echo \"quotes \\\" and newlines \n test\"",
                        "unicode": "🦀 Rust Agent 🚀",
                        "nested": { "array": [1, 2, 3], "escaped": "tab\there" }
                    }),
                    status: "success".into(),
                    duration_ms: thread_idx as u64 * 5,
                };
                logger_clone.log(&rec).expect("Concurrent audit write");
            }));
        }

        for h in handles {
            h.join().expect("Worker thread finished");
        }

        let records = logger.read_all().expect("Read concurrent audit records");
        assert_eq!(records.len(), 20, "All 20 concurrent records must be preserved");

        for rec in &records {
            assert_eq!(rec.parameters["unicode"], "🦀 Rust Agent 🚀");
            assert_eq!(
                rec.parameters["command"].as_str().unwrap(),
                "echo \"quotes \\\" and newlines \n test\""
            );
        }
    }

    #[test]
    fn test_with_audit_dir_isolation() {
        let temp_dir = TempDirGuard::new("with_dir");
        let dir = temp_dir.path().to_path_buf();

        with_audit_dir(dir.clone(), || {
            let rec = AuditRecord {
                timestamp: "2026-09-17T03:50:00Z".into(),
                session_id: "sess-iso".into(),
                task_id: None,
                tool: "test_tool".into(),
                parameters: json!({}),
                status: "success".into(),
                duration_ms: 5,
            };
            log_tool_invocation(&rec).unwrap();
        });

        let isolated_logger = AuditLogger::new(&dir);
        let records = isolated_logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool, "test_tool");
    }

    #[test]
    fn test_empty_lines_resilience() {
        let temp_dir = TempDirGuard::new("empty_lines");
        let logger = AuditLogger::new(temp_dir.path());

        let rec = AuditRecord {
            timestamp: "2026-09-17T03:50:00Z".into(),
            session_id: "sess-1".into(),
            task_id: None,
            tool: "tool1".into(),
            parameters: json!({}),
            status: "success".into(),
            duration_ms: 1,
        };
        logger.log(&rec).unwrap();

        // Inject empty and whitespace lines
        {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(logger.path())
                .unwrap();
            writeln!(file, "   ").unwrap();
            writeln!(file).unwrap();
        }

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tool, "tool1");
    }

    #[test]
    fn test_poison_recovery() {
        let temp_dir = TempDirGuard::new("poison");
        let logger = AuditLogger::new(temp_dir.path());

        // Intentionally poison the lock in a child thread
        let lock_clone = logger.lock.clone();
        let _ = thread::spawn(move || {
            let _guard = lock_clone.lock().unwrap();
            panic!("Intentional panic to poison lock");
        })
        .join();

        // Despite poisoning, logger.log and logger.read_all must succeed via poison resilience
        let rec = AuditRecord {
            timestamp: "2026-09-17T03:50:00Z".into(),
            session_id: "sess-poison".into(),
            task_id: None,
            tool: "poison_test".into(),
            parameters: json!({}),
            status: "success".into(),
            duration_ms: 1,
        };
        assert!(logger.log(&rec).is_ok());
        let records = logger.read_all().expect("read_all on poisoned lock");
        assert_eq!(records.len(), 1);
    }
}
