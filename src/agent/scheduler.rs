use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};

// ============================================================================
// 1. UTC Date/Time Extraction (Howard Hinnant Algorithm)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcDateTime {
    pub year: i32,
    pub month: u32,   // 1-12
    pub day: u32,     // 1-31
    pub hour: u32,    // 0-23
    pub minute: u32,  // 0-59
    pub second: u32,  // 0-59
    pub weekday: u32, // 0-6 (0 = Sunday, 1 = Monday, ..., 6 = Saturday)
}

impl UtcDateTime {
    pub fn from_system_time(st: SystemTime) -> Self {
        let duration = st
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let total_secs = duration.as_secs();

        let days = (total_secs / 86400) as i64;
        let day_secs = total_secs % 86400;
        let hour = (day_secs / 3600) as u32;
        let minute = ((day_secs % 3600) / 60) as u32;
        let second = (day_secs % 60) as u32;

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

        // 1970-01-01 was Thursday (day index 4, Sunday = 0)
        let weekday = (((days + 4) % 7 + 7) % 7) as u32;

        Self {
            year: final_y as i32,
            month: m,
            day: d,
            hour,
            minute,
            second,
            weekday,
        }
    }
}

// ============================================================================
// 2. Pure-Rust 5-Field Cron & Interval Parser
// ============================================================================

/// Parsed cron pattern fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronPattern {
    pub expr: String,
    pub minutes: HashSet<u32>,
    pub hours: HashSet<u32>,
    pub days_of_month: HashSet<u32>,
    pub months: HashSet<u32>,
    pub days_of_week: HashSet<u32>,
}

/// Parsed schedule specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronSchedule {
    Pattern(Box<CronPattern>),
    Interval {
        expr: String,
        duration: Duration,
    },
}

impl CronSchedule {
    /// Parses a 5-field cron expression, standard macro, or `@every <dur>` interval.
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();

        // 1. Macros
        if trimmed.starts_with('@') {
            return Self::parse_macro(trimmed);
        }

        // 2. 5-field standard cron: minute hour dom month dow
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(anyhow!(
                "Invalid cron expression: expected 5 fields (minute hour dom month dow), got {}",
                parts.len()
            ));
        }

        let minutes = parse_cron_field(parts[0], 0, 59, false)?;
        let hours = parse_cron_field(parts[1], 0, 23, false)?;
        let days_of_month = parse_cron_field(parts[2], 1, 31, false)?;
        let months = parse_cron_field(parts[3], 1, 12, false)?;
        let days_of_week = parse_cron_field(parts[4], 0, 6, true)?;

        Ok(CronSchedule::Pattern(Box::new(CronPattern {
            expr: trimmed.to_string(),
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
        })))
    }

    fn parse_macro(macro_str: &str) -> Result<Self> {
        let lower = macro_str.to_lowercase();
        if lower.starts_with("@every ") {
            let dur_str = lower.strip_prefix("@every ").unwrap().trim();
            let duration = parse_duration_str(dur_str)?;
            return Ok(CronSchedule::Interval {
                expr: macro_str.to_string(),
                duration,
            });
        }

        let pattern = match lower.as_str() {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" | "@annually" => "0 0 1 1 *",
            other => return Err(anyhow!("Unknown cron macro: '{}'", other)),
        };

        let mut sched = Self::parse(pattern)?;
        if let CronSchedule::Pattern(ref mut p) = sched {
            p.expr = macro_str.to_string();
        }
        Ok(sched)
    }

    /// Evaluates if schedule matches the specified time, preventing multiple executions
    /// during the same minute for Pattern schedules.
    pub fn matches(&self, time: SystemTime, last_run: Option<SystemTime>) -> bool {
        match self {
            CronSchedule::Interval { duration, .. } => match last_run {
                None => true,
                Some(prev) => time.duration_since(prev).unwrap_or_default() >= *duration,
            },
            CronSchedule::Pattern(p) => {
                let utc = UtcDateTime::from_system_time(time);

                if !p.minutes.contains(&utc.minute)
                    || !p.hours.contains(&utc.hour)
                    || !p.days_of_month.contains(&utc.day)
                    || !p.months.contains(&utc.month)
                    || !p.days_of_week.contains(&utc.weekday)
                {
                    return false;
                }

                // Prevent firing multiple times within the same UTC minute
                if let Some(prev) = last_run {
                    let prev_utc = UtcDateTime::from_system_time(prev);
                    if prev_utc.year == utc.year
                        && prev_utc.month == utc.month
                        && prev_utc.day == utc.day
                        && prev_utc.hour == utc.hour
                        && prev_utc.minute == utc.minute
                    {
                        return false;
                    }
                }
                true
            }
        }
    }

    pub fn expr(&self) -> &str {
        match self {
            CronSchedule::Pattern(p) => &p.expr,
            CronSchedule::Interval { expr, .. } => expr,
        }
    }
}

fn parse_duration_str(s: &str) -> Result<Duration> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix("ms") {
        let val: u64 = rest.trim().parse()?;
        return Ok(Duration::from_millis(val));
    }
    if let Some(rest) = s.strip_suffix('s') {
        let val: u64 = rest.trim().parse()?;
        return Ok(Duration::from_secs(val));
    }
    if let Some(rest) = s.strip_suffix('m') {
        let val: u64 = rest.trim().parse()?;
        return Ok(Duration::from_secs(val * 60));
    }
    if let Some(rest) = s.strip_suffix('h') {
        let val: u64 = rest.trim().parse()?;
        return Ok(Duration::from_secs(val * 3600));
    }
    if let Some(rest) = s.strip_suffix('d') {
        let val: u64 = rest.trim().parse()?;
        return Ok(Duration::from_secs(val * 86400));
    }
    Err(anyhow!(
        "Invalid duration format: '{}'. Expected '100ms', '30s', '5m', '1h', or '1d'.",
        s
    ))
}

fn parse_cron_field(
    field_str: &str,
    min: u32,
    max: u32,
    allow_seven_as_zero: bool,
) -> Result<HashSet<u32>> {
    let mut values = HashSet::new();

    for item in field_str.split(',') {
        let item = item.trim();
        if item == "*" {
            for v in min..=max {
                values.insert(v);
            }
        } else if let Some(step_str) = item.strip_prefix("*/") {
            let step: u32 = step_str.parse().map_err(|_| {
                anyhow!(
                    "Invalid step '{}' in field '{}'",
                    step_str,
                    field_str
                )
            })?;
            if step == 0 {
                return Err(anyhow!("Step cannot be zero in field '{}'", field_str));
            }
            let mut v = min;
            while v <= max {
                values.insert(v);
                v += step;
            }
        } else if item.contains('-') {
            let (range_part, step) = if item.contains('/') {
                let parts: Vec<&str> = item.split('/').collect();
                let step: u32 = parts[1].parse().map_err(|_| {
                    anyhow!("Invalid range step in '{}'", item)
                })?;
                (parts[0], step)
            } else {
                (item, 1)
            };

            let range_parts: Vec<&str> = range_part.split('-').collect();
            if range_parts.len() != 2 {
                return Err(anyhow!("Invalid range format in '{}'", item));
            }
            let start: u32 = range_parts[0].parse()?;
            let end: u32 = range_parts[1].parse()?;

            if start > end || start < min || end > max {
                return Err(anyhow!(
                    "Range '{}-{}' out of bounds [{}..{}]",
                    start,
                    end,
                    min,
                    max
                ));
            }

            let mut v = start;
            while v <= end {
                values.insert(v);
                v += step;
            }
        } else {
            let mut val: u32 = item.parse().map_err(|_| {
                anyhow!("Invalid number '{}' in field '{}'", item, field_str)
            })?;
            if allow_seven_as_zero && val == 7 {
                val = 0;
            }
            if val < min || val > max {
                return Err(anyhow!(
                    "Value '{}' out of allowed bounds [{}..{}]",
                    val,
                    min,
                    max
                ));
            }
            values.insert(val);
        }
    }

    if values.is_empty() {
        return Err(anyhow!("No valid values for field '{}'", field_str));
    }

    Ok(values)
}

// ============================================================================
// 3. TaskScheduler Engine (OS Background Thread)
// ============================================================================

pub type ScheduledRunner = Arc<dyn Fn() -> Result<String> + Send + Sync + 'static>;

#[derive(Clone)]
struct ScheduledTaskEntry {
    id: String,
    name: String,
    description: String,
    schedule: CronSchedule,
    max_iterations: Option<usize>,
    current_iterations: usize,
    active: bool,
    last_run: Option<SystemTime>,
    runner: ScheduledRunner,
}

/// Serializable DTO representing scheduled task status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledTaskStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub schedule: String,
    pub active: bool,
    pub current_iterations: usize,
    pub max_iterations: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<String>,
}

pub struct TaskScheduler {
    tasks: RwLock<HashMap<String, ScheduledTaskEntry>>,
    running: AtomicBool,
    tick_interval: Duration,
    condvar: Condvar,
    lock: Mutex<()>,
    worker_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl TaskScheduler {
    /// Creates a scheduler with the standard 1-second tick interval.
    pub fn new() -> Arc<Self> {
        Self::with_tick_interval(Duration::from_secs(1))
    }

    /// Creates a scheduler with a custom tick interval (useful for fast automated tests).
    pub fn with_tick_interval(tick_interval: Duration) -> Arc<Self> {
        Arc::new(Self {
            tasks: RwLock::new(HashMap::new()),
            running: AtomicBool::new(false),
            tick_interval,
            condvar: Condvar::new(),
            lock: Mutex::new(()),
            worker_handle: Mutex::new(None),
        })
    }

    /// Registers a scheduled task with a cron expression and optional iteration cap.
    pub fn register_task<F>(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        cron_expr: &str,
        max_iterations: Option<usize>,
        runner: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<String> + Send + Sync + 'static,
    {
        let id_str = id.into();
        let schedule = CronSchedule::parse(cron_expr)?;

        let entry = ScheduledTaskEntry {
            id: id_str.clone(),
            name: name.into(),
            description: description.into(),
            schedule,
            max_iterations,
            current_iterations: 0,
            active: true,
            last_run: None,
            runner: Arc::new(runner),
        };

        let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
        tasks.insert(id_str, entry);
        Ok(())
    }

    /// Evaluates all active tasks against current time and triggers due tasks.
    pub fn tick(&self) -> usize {
        let now = SystemTime::now();
        let mut to_run = Vec::new();

        {
            let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
            for entry in tasks.values_mut() {
                if !entry.active {
                    continue;
                }

                if entry.schedule.matches(now, entry.last_run) {
                    entry.current_iterations += 1;
                    entry.last_run = Some(now);

                    if let Some(max) = entry.max_iterations {
                        if entry.current_iterations >= max {
                            entry.active = false;
                        }
                    }

                    to_run.push(entry.runner.clone());
                }
            }
        }

        let triggered = to_run.len();
        for runner in to_run {
            thread::spawn(move || {
                let _ = runner();
            });
        }

        triggered
    }

    /// Starts the scheduler background OS thread.
    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            // Already running
            return;
        }

        let scheduler = self.clone();
        let handle = thread::Builder::new()
            .name("task-scheduler-loop".into())
            .spawn(move || {
                while scheduler.running.load(Ordering::Relaxed) {
                    scheduler.tick();

                    let guard = scheduler.lock.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = scheduler
                        .condvar
                        .wait_timeout(guard, scheduler.tick_interval);
                }
            })
            .expect("Failed to spawn scheduler worker thread");

        let mut worker = self.worker_handle.lock().unwrap();
        *worker = Some(handle);
    }

    /// Stops the scheduler and waits cleanly for the background thread to exit.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.condvar.notify_all();

        let mut handle_guard = self.worker_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            let _ = handle.join();
        }
    }

    /// Cancels a scheduled task by ID.
    pub fn cancel_schedule(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = tasks.get_mut(id) {
            entry.active = false;
            true
        } else {
            false
        }
    }

    /// Lists status of all registered schedules.
    pub fn list_schedules(&self) -> Vec<ScheduledTaskStatus> {
        let tasks = self.tasks.read().unwrap_or_else(|e| e.into_inner());
        let mut list: Vec<ScheduledTaskStatus> = tasks
            .values()
            .map(|e| ScheduledTaskStatus {
                id: e.id.clone(),
                name: e.name.clone(),
                description: e.description.clone(),
                schedule: e.schedule.expr().to_string(),
                active: e.active,
                current_iterations: e.current_iterations,
                max_iterations: e.max_iterations,
                last_run: e.last_run.map(crate::agent::tasks::format_utc_timestamp),
            })
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}

impl Drop for TaskScheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_cron_parsing_5_fields_and_wildcards() {
        let s = CronSchedule::parse("* * * * *").unwrap();
        if let CronSchedule::Pattern(p) = s {
            assert_eq!(p.minutes.len(), 60);
            assert_eq!(p.hours.len(), 24);
            assert_eq!(p.days_of_month.len(), 31);
            assert_eq!(p.months.len(), 12);
            assert_eq!(p.days_of_week.len(), 7);
        } else {
            panic!("Expected pattern");
        }
    }

    #[test]
    fn test_cron_step_values_and_ranges() {
        let s = CronSchedule::parse("*/15 1-5 * * 1-5").unwrap();
        if let CronSchedule::Pattern(p) = s {
            assert_eq!(p.minutes, HashSet::from([0, 15, 30, 45]));
            assert_eq!(p.hours, HashSet::from([1, 2, 3, 4, 5]));
            assert_eq!(p.days_of_week, HashSet::from([1, 2, 3, 4, 5]));
        } else {
            panic!("Expected pattern");
        }
    }

    #[test]
    fn test_cron_macros_and_intervals() {
        let hourly = CronSchedule::parse("@hourly").unwrap();
        assert_eq!(hourly.expr(), "@hourly");

        let daily = CronSchedule::parse("@daily").unwrap();
        assert_eq!(daily.expr(), "@daily");

        let interval = CronSchedule::parse("@every 100ms").unwrap();
        if let CronSchedule::Interval { duration, .. } = interval {
            assert_eq!(duration, Duration::from_millis(100));
        } else {
            panic!("Expected interval");
        }
    }

    #[test]
    fn test_cron_invalid_expression() {
        assert!(CronSchedule::parse("invalid").is_err());
        assert!(CronSchedule::parse("* * * *").is_err()); // only 4 fields
        assert!(CronSchedule::parse("65 * * * *").is_err()); // minute 65
    }

    #[test]
    fn test_scheduler_interval_execution_and_max_iterations() {
        let scheduler = TaskScheduler::with_tick_interval(Duration::from_millis(10));
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        scheduler
            .register_task(
                "test-sched-1",
                "counter",
                "increment counter",
                "@every 20ms",
                Some(3),
                move || {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok("ok".into())
                },
            )
            .unwrap();

        scheduler.start();

        // Wait until it reaches 3 iterations
        let start = std::time::Instant::now();
        while count.load(Ordering::SeqCst) < 3 && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(20));
        }

        scheduler.stop();

        assert_eq!(count.load(Ordering::SeqCst), 3);
        let list = scheduler.list_schedules();
        assert!(!list[0].active);
        assert_eq!(list[0].current_iterations, 3);
    }
}
