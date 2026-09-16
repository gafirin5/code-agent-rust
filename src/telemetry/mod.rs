//! Pure-Rust Core Resource Telemetry Engine.
//!
//! Provides zero-external-dependency process metrics collection (RAM RSS and peak,
//! CPU utilization percentage, active thread count, handle/fd count, and storage footprints)
//! with cross-platform support for Windows (Win32 FFI) and Unix (/proc & getrusage).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
use unix as platform;

/// Process physical and virtual memory metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryMetrics {
    /// Resident Set Size / Working Set physically resident in RAM (bytes).
    pub rss_bytes: u64,
    /// Peak Resident Set Size observed since process startup (bytes).
    pub peak_rss_bytes: u64,
    /// Virtual memory / committed address space size (bytes).
    pub virtual_bytes: u64,
    /// Human-readable formatted RSS (e.g. "14.25 MB").
    pub formatted_rss: String,
    /// Human-readable formatted Peak RSS (e.g. "18.50 MB").
    pub formatted_peak: String,
}

/// Process CPU utilization and execution time metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CpuMetrics {
    /// Normalized process CPU utilization percentage (0.0 - 100.0%).
    pub process_pct: f64,
    /// Cumulative user-mode CPU execution time in milliseconds.
    pub user_ms: u64,
    /// Cumulative kernel/system-mode CPU execution time in milliseconds.
    pub kernel_ms: u64,
    /// Total cumulative CPU execution time (user + kernel) in milliseconds.
    pub total_ms: u64,
}

/// Active thread and process handle count metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ThreadMetrics {
    /// Count of active OS threads belonging to this process.
    pub active_threads: usize,
    /// Count of open OS process handles or file descriptors (if supported).
    pub process_handles: Option<usize>,
}

/// Disk and storage footprints for workspace metadata and logs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StorageMetrics {
    /// Total storage footprint of the `.ctrl/` directory in bytes.
    pub ctrl_dir_bytes: u64,
    /// Cumulative storage footprint of task log files in bytes.
    pub task_logs_bytes: u64,
    /// Human-readable total `.ctrl/` directory footprint (e.g. "142.60 KB").
    pub formatted_ctrl: String,
    /// Total number of files inside the `.ctrl/` directory.
    pub file_count: usize,
}

/// Unified process resource metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProcessMetrics {
    /// Memory counters (RSS, peak, virtual).
    pub memory: MemoryMetrics,
    /// CPU utilization and cumulative times.
    pub cpu: CpuMetrics,
    /// Active OS threads and handle counts.
    pub threads: ThreadMetrics,
    /// Persistent storage and log footprints.
    pub storage: StorageMetrics,
    /// Epoch timestamp in seconds when the snapshot was captured.
    pub timestamp: u64,
}

/// Formats a raw byte quantity into a concise human-readable string (B, KB, MB, GB, TB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Recursively inspects the given `.ctrl` directory (or default `.ctrl`)
/// and computes total bytes, task log file bytes, and file count.
/// Gracefully handles missing, empty, or unreadable directories.
pub fn calculate_storage_metrics(ctrl_dir: Option<&Path>) -> StorageMetrics {
    let default_path;
    let path = match ctrl_dir {
        Some(p) => p,
        None => {
            default_path = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".ctrl");
            &default_path
        }
    };

    if !path.exists() {
        return StorageMetrics {
            ctrl_dir_bytes: 0,
            task_logs_bytes: 0,
            formatted_ctrl: format_bytes(0),
            file_count: 0,
        };
    }

    let mut total_bytes = 0u64;
    let mut task_logs_bytes = 0u64;
    let mut file_count = 0usize;

    walk_storage_dir(path, &mut total_bytes, &mut task_logs_bytes, &mut file_count);

    StorageMetrics {
        ctrl_dir_bytes: total_bytes,
        task_logs_bytes,
        formatted_ctrl: format_bytes(total_bytes),
        file_count,
    }
}

fn walk_storage_dir(
    dir: &Path,
    total_bytes: &mut u64,
    task_logs_bytes: &mut u64,
    file_count: &mut usize,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    // Avoid following directory symlinks to eliminate infinite recursion loops
                    if !meta.file_type().is_symlink() {
                        walk_storage_dir(&entry_path, total_bytes, task_logs_bytes, file_count);
                    }
                } else if meta.is_file() {
                    let len = meta.len();
                    *total_bytes = total_bytes.saturating_add(len);
                    *file_count += 1;

                    let is_log = entry_path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("log"))
                        .unwrap_or(false);

                    if is_log {
                        *task_logs_bytes = task_logs_bytes.saturating_add(len);
                    }
                }
            }
        }
    }
}

/// Formats a `ProcessMetrics` snapshot into an ANSI styled visual summary box.
pub fn format_metrics_table(metrics: &ProcessMetrics) -> String {
    let handle_str = match metrics.threads.process_handles {
        Some(h) => format!("{} (Handles: {})", metrics.threads.active_threads, h),
        None => format!("{} threads", metrics.threads.active_threads),
    };

    let lines = [
        format!(
            "RAM (RSS / Working Set) : {} (Peak: {})",
            metrics.memory.formatted_rss, metrics.memory.formatted_peak
        ),
        format!(
            "Virtual Memory          : {}",
            format_bytes(metrics.memory.virtual_bytes)
        ),
        format!(
            "CPU Utilization         : {:.2}% (Total: {} ms)",
            metrics.cpu.process_pct, metrics.cpu.total_ms
        ),
        format!(
            "CPU Times (User/Kernel) : {} ms / {} ms",
            metrics.cpu.user_ms, metrics.cpu.kernel_ms
        ),
        format!("Active OS Threads       : {}", handle_str),
        format!(
            "Disk (.ctrl/ footprint) : {} ({} files)",
            metrics.storage.formatted_ctrl, metrics.storage.file_count
        ),
        format!(
            "Task Logs Footprint     : {}",
            format_bytes(metrics.storage.task_logs_bytes)
        ),
        format!("Timestamp               : {}", metrics.timestamp),
    ];

    let header = "📊 Process Resource Telemetry & Profiling Metrics";
    let max_len = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(50)
        .max(header.chars().count() + 2);
    let border = "─".repeat(max_len + 4);

    let mut out = String::new();
    out.push('╭');
    out.push_str(&border);
    out.push_str("╮\n");

    out.push_str(&format!("│  {:<width$}  │\n", header, width = max_len));
    out.push('├');
    out.push_str(&border);
    out.push_str("┤\n");

    for line in &lines {
        out.push_str(&format!("│  {:<width$}  │\n", line, width = max_len));
    }

    out.push('╰');
    out.push_str(&border);
    out.push_str("╯\n");

    out
}

/// Stateful CPU sampler tracking previous CPU execution times and wall-clock Instant
/// to accurately compute delta CPU utilization percentage normalized to 0.0 - 100.0%.
#[derive(Debug, Clone)]
pub struct CpuSampler {
    last_sample_instant: std::time::Instant,
    last_user_ms: u64,
    last_kernel_ms: u64,
    logical_cores: usize,
    last_pct: f64,
}

impl CpuSampler {
    /// Creates a new CPU sampler initialized with the current process CPU times
    /// and detected logical processor core count.
    pub fn new() -> Self {
        let (user_ms, kernel_ms) = platform::get_cpu_times();
        let logical_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        Self {
            last_sample_instant: std::time::Instant::now(),
            last_user_ms: user_ms,
            last_kernel_ms: kernel_ms,
            logical_cores,
            last_pct: 0.0,
        }
    }

    /// Creates a CPU sampler configured with an explicit number of logical cores.
    pub fn with_cores(logical_cores: usize) -> Self {
        let (user_ms, kernel_ms) = platform::get_cpu_times();
        Self {
            last_sample_instant: std::time::Instant::now(),
            last_user_ms: user_ms,
            last_kernel_ms: kernel_ms,
            logical_cores: logical_cores.max(1),
            last_pct: 0.0,
        }
    }

    /// Computes delta CPU utilization percentage between the previous sample and current sample,
    /// normalized across the available logical CPU cores to the range 0.0% - 100.0%.
    pub fn sample(&mut self, current_user_ms: u64, current_kernel_ms: u64) -> CpuMetrics {
        let now = std::time::Instant::now();
        let delta_wall = now.duration_since(self.last_sample_instant);
        let delta_wall_ms = delta_wall.as_secs_f64() * 1000.0;

        let current_total_ms = current_user_ms.saturating_add(current_kernel_ms);
        let prev_total_ms = self.last_user_ms.saturating_add(self.last_kernel_ms);
        let delta_cpu_ms = current_total_ms.saturating_sub(prev_total_ms);

        let cores = self.logical_cores.max(1) as f64;

        let process_pct = if delta_wall_ms < 1.0 {
            // Under 1 ms elapsed; return previous cached value to avoid division by near-zero (Edge Case E-11)
            self.last_pct
        } else {
            let raw_pct = (delta_cpu_ms as f64 / delta_wall_ms) / cores * 100.0;
            let clamped = if raw_pct.is_nan() || raw_pct < 0.0 {
                0.0
            } else if raw_pct > 100.0 {
                100.0
            } else {
                raw_pct
            };
            self.last_pct = clamped;
            self.last_sample_instant = now;
            self.last_user_ms = current_user_ms;
            self.last_kernel_ms = current_kernel_ms;
            clamped
        };

        CpuMetrics {
            process_pct,
            user_ms: current_user_ms,
            kernel_ms: current_kernel_ms,
            total_ms: current_total_ms,
        }
    }

    /// Resets the sampler window to the current moment.
    pub fn reset(&mut self) {
        let (user_ms, kernel_ms) = platform::get_cpu_times();
        self.last_sample_instant = std::time::Instant::now();
        self.last_user_ms = user_ms;
        self.last_kernel_ms = kernel_ms;
        self.last_pct = 0.0;
    }

    /// Returns the logical processor cores used for percentage normalization.
    pub fn logical_cores(&self) -> usize {
        self.logical_cores
    }
}

impl Default for CpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_CPU_SAMPLER: std::sync::Mutex<Option<CpuSampler>> = std::sync::Mutex::new(None);

/// Unified entrypoint to capture process metrics using the internal global CPU sampler.
pub fn capture_metrics(ctrl_dir: Option<&Path>) -> ProcessMetrics {
    let mut lock = GLOBAL_CPU_SAMPLER.lock().unwrap_or_else(|e| e.into_inner());
    let sampler = lock.get_or_insert_with(CpuSampler::new);
    capture_metrics_with_cpu(ctrl_dir, sampler)
}

/// Unified entrypoint to capture process metrics using a caller-provided stateful CPU sampler.
pub fn capture_metrics_with_cpu(
    ctrl_dir: Option<&Path>,
    sampler: &mut CpuSampler,
) -> ProcessMetrics {
    let (rss_bytes, peak_rss_bytes, virtual_bytes) = platform::get_memory();
    let (user_ms, kernel_ms) = platform::get_cpu_times();
    let cpu = sampler.sample(user_ms, kernel_ms);
    let active_threads = platform::get_active_threads();
    let process_handles = platform::get_process_handles();
    let storage = calculate_storage_metrics(ctrl_dir);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes,
            peak_rss_bytes,
            virtual_bytes,
            formatted_rss: format_bytes(rss_bytes),
            formatted_peak: format_bytes(peak_rss_bytes),
        },
        cpu,
        threads: ThreadMetrics {
            active_threads,
            process_handles,
        },
        storage,
        timestamp,
    }
}

/// Returns the current process active OS thread count.
#[allow(dead_code)]
pub fn get_active_threads() -> usize {
    platform::get_active_threads()
}

/// Returns the count of open process handles (Windows) or open file descriptors (Unix).
#[allow(dead_code)]
pub fn get_process_handles() -> Option<usize> {
    platform::get_process_handles()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(5242880), "5.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1099511627776), "1.00 TB");
    }

    #[test]
    fn test_storage_metrics_nonexistent_dir() {
        let bogus = Path::new("definitely_nonexistent_dir_random_983719");
        let storage = calculate_storage_metrics(Some(bogus));
        assert_eq!(storage.ctrl_dir_bytes, 0);
        assert_eq!(storage.task_logs_bytes, 0);
        assert_eq!(storage.file_count, 0);
        assert_eq!(storage.formatted_ctrl, "0 B");
    }

    #[test]
    fn test_storage_metrics_empty_dir() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ctrl_test_empty_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create empty temp dir");

        let storage = calculate_storage_metrics(Some(&temp_dir));
        assert_eq!(storage.ctrl_dir_bytes, 0);
        assert_eq!(storage.task_logs_bytes, 0);
        assert_eq!(storage.file_count, 0);
        assert_eq!(storage.formatted_ctrl, "0 B");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_storage_metrics_with_files_and_logs() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ctrl_test_storage_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let tasks_dir = temp_dir.join("tasks");
        fs::create_dir_all(&tasks_dir).expect("create tasks dir");

        // 1. tasks.jsonl (100 bytes)
        let jsonl_path = temp_dir.join("tasks.jsonl");
        fs::write(&jsonl_path, [b'a'; 100]).expect("write jsonl");

        // 2. task_1.log (150 bytes)
        let log_path = tasks_dir.join("task_1.log");
        fs::write(&log_path, [b'b'; 150]).expect("write log");

        // 3. session.json (50 bytes)
        let session_path = temp_dir.join("session.json");
        fs::write(&session_path, [b'c'; 50]).expect("write session");

        let storage = calculate_storage_metrics(Some(&temp_dir));
        assert_eq!(storage.file_count, 3);
        assert_eq!(storage.ctrl_dir_bytes, 300);
        assert_eq!(storage.task_logs_bytes, 150);
        assert_eq!(storage.formatted_ctrl, "300 B");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_capture_metrics_live() {
        let metrics = capture_metrics(None);
        // Valid non-zero RSS and peak RSS >= RSS
        assert!(
            metrics.memory.rss_bytes > 0,
            "RSS must be non-zero in a running process"
        );
        assert!(
            metrics.memory.peak_rss_bytes >= metrics.memory.rss_bytes,
            "Peak RSS ({}) must be >= RSS ({})",
            metrics.memory.peak_rss_bytes,
            metrics.memory.rss_bytes
        );
        // Positive thread count (>= 1)
        assert!(
            metrics.threads.active_threads >= 1,
            "Thread count must be at least 1"
        );
        // Valid timestamp
        assert!(metrics.timestamp > 0, "Timestamp must be positive");
        // Non-empty formatted strings
        assert!(!metrics.memory.formatted_rss.is_empty());
        assert!(!metrics.memory.formatted_peak.is_empty());
    }

    #[test]
    fn test_cpu_sampler_delta_and_reset() {
        let mut sampler = CpuSampler::with_cores(4);
        assert_eq!(sampler.logical_cores(), 4);

        // First sample
        let m1 = sampler.sample(100, 50);
        assert_eq!(m1.user_ms, 100);
        assert_eq!(m1.kernel_ms, 50);
        assert_eq!(m1.total_ms, 150);
        assert!(m1.process_pct >= 0.0 && m1.process_pct <= 100.0);

        // Advance simulated CPU times and wait slightly
        std::thread::sleep(std::time::Duration::from_millis(10));
        let m2 = sampler.sample(120, 60);
        assert_eq!(m2.user_ms, 120);
        assert_eq!(m2.kernel_ms, 60);
        assert_eq!(m2.total_ms, 180);
        assert!(m2.process_pct >= 0.0 && m2.process_pct <= 100.0);

        // Reset
        sampler.reset();
        assert_eq!(sampler.last_pct, 0.0);
    }

    #[test]
    fn test_cpu_sampler_sub_millisecond_debounce() {
        let mut sampler = CpuSampler::with_cores(2);
        let m1 = sampler.sample(10, 10);
        // Call immediately with near-zero elapsed time
        let m2 = sampler.sample(10, 10);
        assert!(!m2.process_pct.is_nan());
        assert_eq!(m2.process_pct, m1.process_pct);
    }

    #[test]
    fn test_serialization_deserialization_roundtrip() {
        let metrics = ProcessMetrics {
            memory: MemoryMetrics {
                rss_bytes: 14_250_000,
                peak_rss_bytes: 18_500_000,
                virtual_bytes: 42_000_000,
                formatted_rss: "13.59 MB".to_string(),
                formatted_peak: "17.64 MB".to_string(),
            },
            cpu: CpuMetrics {
                process_pct: 1.25,
                user_ms: 350,
                kernel_ms: 120,
                total_ms: 470,
            },
            threads: ThreadMetrics {
                active_threads: 4,
                process_handles: Some(42),
            },
            storage: StorageMetrics {
                ctrl_dir_bytes: 146_000,
                task_logs_bytes: 85_000,
                formatted_ctrl: "142.58 KB".to_string(),
                file_count: 5,
            },
            timestamp: 1726320000,
        };

        let json = serde_json::to_string(&metrics).expect("serialize ProcessMetrics");
        assert!(json.contains("\"rss_bytes\":14250000"));
        assert!(json.contains("\"process_pct\":1.25"));
        assert!(json.contains("\"active_threads\":4"));
        assert!(json.contains("\"process_handles\":42"));
        assert!(json.contains("\"ctrl_dir_bytes\":146000"));

        let deserialized: ProcessMetrics =
            serde_json::from_str(&json).expect("deserialize ProcessMetrics");
        assert_eq!(deserialized, metrics);
    }

    #[test]
    fn test_format_metrics_table() {
        let metrics = ProcessMetrics {
            memory: MemoryMetrics {
                rss_bytes: 10_485_760,
                peak_rss_bytes: 20_971_520,
                virtual_bytes: 52_428_800,
                formatted_rss: "10.00 MB".to_string(),
                formatted_peak: "20.00 MB".to_string(),
            },
            cpu: CpuMetrics {
                process_pct: 0.45,
                user_ms: 200,
                kernel_ms: 80,
                total_ms: 280,
            },
            threads: ThreadMetrics {
                active_threads: 3,
                process_handles: Some(25),
            },
            storage: StorageMetrics {
                ctrl_dir_bytes: 1024,
                task_logs_bytes: 512,
                formatted_ctrl: "1.00 KB".to_string(),
                file_count: 2,
            },
            timestamp: 1726320000,
        };

        let table = format_metrics_table(&metrics);
        assert!(table.contains("RAM (RSS / Working Set) : 10.00 MB (Peak: 20.00 MB)"));
        assert!(table.contains("CPU Utilization         : 0.45% (Total: 280 ms)"));
        assert!(table.contains("Active OS Threads       : 3 (Handles: 25)"));
        assert!(table.contains("Disk (.ctrl/ footprint) : 1.00 KB (2 files)"));
        assert!(table.contains("Task Logs Footprint     : 512 B"));
        assert!(table.contains("Timestamp               : 1726320000"));
        assert!(table.contains('╭'));
        assert!(table.contains('╰'));
    }
}
