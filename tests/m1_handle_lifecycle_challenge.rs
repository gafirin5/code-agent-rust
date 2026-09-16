//! Adversarial Handle & Lifecycle Empirical Challenge Test Harness for Milestone 1.
//!
//! Verifies:
//! 1. 1,000-iteration tight loop `capture_metrics` does not leak OS handles (Win32 `GetProcessHandleCount`).
//! 2. OS thread lifecycle tracking (`active_threads` increases by 10 when spawned and returns to baseline when joined).
//! 3. JSON round-trip serialization and deserialization integrity across extreme values (0, u64::MAX, usize::MAX, subnormals, unicode, adversarial payloads).
//! 4. Toolhelp snapshot handle safety and lifecycle stability.
//! 5. `format_bytes` and `format_metrics_table` boundary stress.

#[path = "../src/telemetry/mod.rs"]
mod telemetry;

use std::fs;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use telemetry::{
    format_bytes, format_metrics_table, CpuMetrics, MemoryMetrics, ProcessMetrics, StorageMetrics,
    ThreadMetrics,
};

/// Global lock ensuring process-wide thread counts and handle counters are not
/// disturbed by concurrent test execution in the same process.
static PROCESS_WIDE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
fn get_current_process_handle_count() -> u32 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessHandleCount(hProcess: *mut std::ffi::c_void, pdwHandleCount: *mut u32) -> i32;
    }
    unsafe {
        let mut count = 0u32;
        let proc = GetCurrentProcess();
        let ret = GetProcessHandleCount(proc, &mut count);
        assert_ne!(ret, 0, "GetProcessHandleCount failed");
        count
    }
}

#[cfg(not(windows))]
fn get_current_process_handle_count() -> u32 {
    0
}

// ============================================================================
// SUITE 1: 1,000-ITERATION HANDLE LEAK RESISTANCE HARNESS
// ============================================================================

#[test]
fn challenge_capture_metrics_1000_loop_no_handle_leak() {
    #[cfg(windows)]
    {
        let _guard = PROCESS_WIDE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // 1. Warm-up iterations to settle thread pools, runtime init, and lazy statics
        for _ in 0..20 {
            let _ = telemetry::capture_metrics(None);
        }

        // 2. Measure starting handle count
        let initial_handles = get_current_process_handle_count();

        // 3. Execute 1,000 tight-loop calls
        for i in 0..1000 {
            let metrics = telemetry::capture_metrics(None);
            assert!(
                metrics.threads.active_threads >= 1,
                "Iteration {}: active_threads must be >= 1",
                i
            );
            assert!(
                metrics.threads.process_handles.is_some(),
                "Iteration {}: process_handles must be populated on Windows",
                i
            );
        }

        // 4. Measure ending handle count
        let final_handles = get_current_process_handle_count();
        let delta = (final_handles as i64) - (initial_handles as i64);

        println!(
            "1000-Loop Handle Test: Initial={}, Final={}, Delta={}",
            initial_handles, final_handles, delta
        );

        // If CreateToolhelp32Snapshot or internal operations leaked handles,
        // delta would be at least +1000. We assert delta <= 2 to account for
        // negligible OS runtime background handle fluctuation.
        assert!(
            delta <= 2,
            "HANDLE LEAK DETECTED: 1,000 tight loop capture_metrics leaked {} handles (initial={}, final={})",
            delta,
            initial_handles,
            final_handles
        );
    }
}

#[test]
fn challenge_storage_traversal_no_handle_leak() {
    #[cfg(windows)]
    {
        let _guard = PROCESS_WIDE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Setup temporary directory structure with multiple nested files and logs
        let temp_dir = std::env::temp_dir().join(format!(
            "ctrl_challenge_storage_leak_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let tasks_dir = temp_dir.join("tasks").join("subtasks");
        fs::create_dir_all(&tasks_dir).expect("create test tasks dir");

        for i in 0..15 {
            let log_file = tasks_dir.join(format!("task_{}.log", i));
            fs::write(&log_file, format!("Simulated log payload for task {}", i)).expect("write log");
            let meta_file = temp_dir.join(format!("meta_{}.json", i));
            fs::write(&meta_file, b"{\"status\":\"completed\"}").expect("write meta");
        }

        // Warm up
        for _ in 0..10 {
            let _ = telemetry::calculate_storage_metrics(Some(&temp_dir));
        }

        let initial_handles = get_current_process_handle_count();

        // 1,000 tight-loop iterations traversing directory entries and opening metadata
        for i in 0..1000 {
            let storage = telemetry::calculate_storage_metrics(Some(&temp_dir));
            assert_eq!(
                storage.file_count, 30,
                "Iteration {}: Expected 30 files in storage scanner",
                i
            );
        }

        let final_handles = get_current_process_handle_count();
        let delta = (final_handles as i64) - (initial_handles as i64);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);

        println!(
            "Storage Handle Test: Initial={}, Final={}, Delta={}",
            initial_handles, final_handles, delta
        );

        assert!(
            delta <= 2,
            "STORAGE DIRECTORY HANDLE LEAK DETECTED: 1,000 storage traversals leaked {} handles (initial={}, final={})",
            delta,
            initial_handles,
            final_handles
        );
    }
}

// ============================================================================
// SUITE 2: OS THREAD LIFECYCLE TRACKING HARNESS
// ============================================================================

#[test]
fn challenge_os_thread_lifecycle_spawning_10_threads() {
    #[cfg(windows)]
    {
        let _guard = PROCESS_WIDE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Settle active threads to allow ephemeral background test runner threads to stabilize
        let mut baseline = telemetry::capture_metrics(None).threads.active_threads;
        let settle_deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < settle_deadline {
            thread::sleep(Duration::from_millis(15));
            let current = telemetry::capture_metrics(None).threads.active_threads;
            if current == baseline {
                break;
            }
            baseline = current;
        }
        assert!(baseline >= 1, "Baseline active_threads must be >= 1");

        // 2. Barrier for 11 participants (1 main + 10 workers)
        let startup_barrier = Arc::new(Barrier::new(11));
        let shutdown_barrier = Arc::new(Barrier::new(11));

        // 3. Spawn exactly 10 OS threads
        let mut worker_handles = Vec::new();
        for _ in 0..10 {
            let start_b = Arc::clone(&startup_barrier);
            let stop_b = Arc::clone(&shutdown_barrier);
            worker_handles.push(thread::spawn(move || {
                start_b.wait(); // Wait until all 10 are alive
                stop_b.wait();  // Wait until main thread verifies count
            }));
        }

        // Wait until all 10 threads are running
        startup_barrier.wait();

        // 4. Verify thread count increased by approximately 10 (bounded tolerance for OS runtime thread fluctuation)
        let active_during = telemetry::capture_metrics(None).threads.active_threads;
        println!(
            "Thread Lifecycle: Baseline={}, ActiveDuring={}",
            baseline, active_during
        );
        let delta_during = (active_during as i64) - (baseline as i64);
        assert!(
            (delta_during - 10).abs() <= 2 && active_during >= baseline + 8,
            "Spawning 10 OS threads must increase active_threads by ~10 (baseline={}, active={}, delta={})",
            baseline,
            active_during,
            delta_during
        );

        // 5. Release worker threads
        shutdown_barrier.wait();

        // 6. Join all 10 worker threads
        for h in worker_handles {
            h.join().expect("Worker thread join failed");
        }

        // 7. Verify active_threads returns to baseline or lower (with convergence timeout for OS kernel reaping)
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut active_after = telemetry::capture_metrics(None).threads.active_threads;
        while active_after > baseline && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(15));
            active_after = telemetry::capture_metrics(None).threads.active_threads;
        }

        println!(
            "Thread Lifecycle: Baseline={}, ActiveAfterJoin={}",
            baseline, active_after
        );
        assert!(
            active_after <= baseline,
            "Joining 10 OS threads must return active_threads to baseline {} or lower (got {})",
            baseline, active_after
        );
    }
}

#[test]
fn challenge_os_thread_lifecycle_repeated_churn() {
    #[cfg(windows)]
    {
        let _guard = PROCESS_WIDE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Perform 3 repeated cycles of spawning 5 threads and joining them
        for cycle in 0..3 {
            // Settle threads before baseline sample to avoid counting retiring OS threads
            let mut baseline = telemetry::capture_metrics(None).threads.active_threads;
            let settle_deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < settle_deadline {
                thread::sleep(Duration::from_millis(15));
                let current = telemetry::capture_metrics(None).threads.active_threads;
                if current == baseline {
                    break;
                }
                baseline = current;
            }

            let startup = Arc::new(Barrier::new(6));
            let shutdown = Arc::new(Barrier::new(6));

            let mut threads = Vec::new();
            for _ in 0..5 {
                let su = Arc::clone(&startup);
                let sd = Arc::clone(&shutdown);
                threads.push(thread::spawn(move || {
                    su.wait();
                    sd.wait();
                }));
            }

            startup.wait();
            let mid = telemetry::capture_metrics(None).threads.active_threads;
            let delta_mid = (mid as i64) - (baseline as i64);
            assert!(
                (delta_mid - 5).abs() <= 1 && mid >= baseline + 4,
                "Cycle {}: Expected active_threads delta to be ~5 (baseline={}, mid={}, delta={})",
                cycle,
                baseline,
                mid,
                delta_mid
            );

            shutdown.wait();
            for t in threads {
                t.join().expect("join thread");
            }

            let deadline = Instant::now() + Duration::from_millis(1500);
            let mut post = telemetry::capture_metrics(None).threads.active_threads;
            while post > baseline && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(15));
                post = telemetry::capture_metrics(None).threads.active_threads;
            }
            assert!(
                post <= baseline && mid >= post + 4,
                "Cycle {}: Thread lifecycle churn anomaly (thread leaked or failed to terminate): baseline={}, mid={}, post={}",
                cycle, baseline, mid, post
            );
        }
    }
}

// ============================================================================
// SUITE 3: JSON ROUND-TRIP INTEGRITY ACROSS EXTREME VALUES
// ============================================================================

#[test]
fn challenge_json_roundtrip_all_zero_and_defaults() {
    let original = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 0,
            peak_rss_bytes: 0,
            virtual_bytes: 0,
            formatted_rss: "0 B".to_string(),
            formatted_peak: "0 B".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 0.0,
            user_ms: 0,
            kernel_ms: 0,
            total_ms: 0,
        },
        threads: ThreadMetrics {
            active_threads: 0,
            process_handles: None,
        },
        storage: StorageMetrics {
            ctrl_dir_bytes: 0,
            task_logs_bytes: 0,
            formatted_ctrl: "0 B".to_string(),
            file_count: 0,
        },
        timestamp: 0,
    };

    let json = serde_json::to_string(&original).expect("Serialize all-zero defaults");
    let deserialized: ProcessMetrics =
        serde_json::from_str(&json).expect("Deserialize all-zero defaults");

    assert_eq!(deserialized, original, "All-zero default struct must round-trip bitwise");
}

#[test]
fn challenge_json_roundtrip_max_integers() {
    let original = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: u64::MAX,
            peak_rss_bytes: u64::MAX,
            virtual_bytes: u64::MAX,
            formatted_rss: "16777216.00 TB".to_string(),
            formatted_peak: "16777216.00 TB".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 100.0,
            user_ms: u64::MAX,
            kernel_ms: u64::MAX,
            total_ms: u64::MAX,
        },
        threads: ThreadMetrics {
            active_threads: usize::MAX,
            process_handles: Some(usize::MAX),
        },
        storage: StorageMetrics {
            ctrl_dir_bytes: u64::MAX,
            task_logs_bytes: u64::MAX,
            formatted_ctrl: "16777216.00 TB".to_string(),
            file_count: usize::MAX,
        },
        timestamp: u64::MAX,
    };

    let json = serde_json::to_string(&original).expect("Serialize u64::MAX integers");
    let deserialized: ProcessMetrics =
        serde_json::from_str(&json).expect("Deserialize u64::MAX integers");

    assert_eq!(deserialized, original, "Max integer values must round-trip exactly");
    assert_eq!(deserialized.memory.rss_bytes, u64::MAX);
    assert_eq!(deserialized.threads.active_threads, usize::MAX);
    assert_eq!(deserialized.threads.process_handles, Some(usize::MAX));
}

#[test]
fn challenge_json_roundtrip_floating_point_precision_and_extremes() {
    let test_percentages = [
        0.0,
        100.0,
        0.00000001,
        99.99999999,
        50.123456789,
        0.001,
        1e-12,
        99.999,
    ];

    for &pct in &test_percentages {
        let original = CpuMetrics {
            process_pct: pct,
            user_ms: 12345,
            kernel_ms: 6789,
            total_ms: 19134,
        };

        let json = serde_json::to_string(&original).expect("Serialize float pct");
        let deserialized: CpuMetrics = serde_json::from_str(&json).expect("Deserialize float pct");

        let diff = (deserialized.process_pct - original.process_pct).abs();
        assert!(
            diff < 1e-9,
            "Floating point round-trip error for {}: diff={}",
            pct,
            diff
        );
        assert_eq!(deserialized.total_ms, original.total_ms);
    }
}

#[test]
fn challenge_json_roundtrip_unicode_escapes_and_massive_strings() {
    let huge_str = "Z".repeat(100_000);
    let adversarial_str =
        "Line1\nLine2\r\t\"quotes\"\\backslash/slash\u{001b}[32mANSI\u{001b}[0m 🦀 🚀 💾 漢字 العربية \0null_attempt";

    let original = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: 1024,
            peak_rss_bytes: 2048,
            virtual_bytes: 4096,
            formatted_rss: adversarial_str.to_string(),
            formatted_peak: huge_str.clone(),
        },
        cpu: CpuMetrics {
            process_pct: 12.34,
            user_ms: 100,
            kernel_ms: 200,
            total_ms: 300,
        },
        threads: ThreadMetrics {
            active_threads: 4,
            process_handles: Some(42),
        },
        storage: StorageMetrics {
            ctrl_dir_bytes: 9999,
            task_logs_bytes: 1111,
            formatted_ctrl: adversarial_str.to_string(),
            file_count: 7,
        },
        timestamp: 1726320000,
    };

    let json = serde_json::to_string(&original).expect("Serialize massive strings");
    let deserialized: ProcessMetrics =
        serde_json::from_str(&json).expect("Deserialize massive strings");

    assert_eq!(deserialized.memory.formatted_rss, adversarial_str);
    assert_eq!(deserialized.memory.formatted_peak.len(), 100_000);
    assert_eq!(deserialized.storage.formatted_ctrl, adversarial_str);
    assert_eq!(deserialized, original);
}

#[test]
fn challenge_json_option_process_handles_null_vs_value() {
    // 1. None serializes to null and deserializes to None
    let tm_none = ThreadMetrics {
        active_threads: 5,
        process_handles: None,
    };
    let json_none = serde_json::to_string(&tm_none).unwrap();
    assert!(json_none.contains("\"process_handles\":null"));
    let de_none: ThreadMetrics = serde_json::from_str(&json_none).unwrap();
    assert_eq!(de_none.process_handles, None);

    // 2. Some(0) serializes to 0 and deserializes to Some(0)
    let tm_zero = ThreadMetrics {
        active_threads: 5,
        process_handles: Some(0),
    };
    let json_zero = serde_json::to_string(&tm_zero).unwrap();
    assert!(json_zero.contains("\"process_handles\":0"));
    let de_zero: ThreadMetrics = serde_json::from_str(&json_zero).unwrap();
    assert_eq!(de_zero.process_handles, Some(0));

    // 3. Some(1_000_000)
    let tm_val = ThreadMetrics {
        active_threads: 8,
        process_handles: Some(1_000_000),
    };
    let json_val = serde_json::to_string(&tm_val).unwrap();
    let de_val: ThreadMetrics = serde_json::from_str(&json_val).unwrap();
    assert_eq!(de_val.process_handles, Some(1_000_000));
}

#[test]
fn challenge_json_adversarial_malformed_payloads_graceful_rejection() {
    // Missing required fields
    let bad_json_missing = r#"{"memory": {}}"#;
    assert!(serde_json::from_str::<ProcessMetrics>(bad_json_missing).is_err());

    // Negative integer where unsigned integer expected
    let bad_json_neg = r#"{"rss_bytes": -42, "peak_rss_bytes": 0, "virtual_bytes": 0, "formatted_rss": "", "formatted_peak": ""}"#;
    assert!(serde_json::from_str::<MemoryMetrics>(bad_json_neg).is_err());

    // String where integer expected
    let bad_json_str = r#"{"active_threads": "ten", "process_handles": null}"#;
    assert!(serde_json::from_str::<ThreadMetrics>(bad_json_str).is_err());

    // Corrupted syntax
    let corrupted = r#"{"memory": {"rss_bytes": 1024, "peak_rss_bytes"#;
    assert!(serde_json::from_str::<ProcessMetrics>(corrupted).is_err());
}

// ============================================================================
// SUITE 4: FORMATTING BOUNDARY STRESS
// ============================================================================

#[test]
fn challenge_format_bytes_extreme_boundaries() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1), "1 B");
    assert_eq!(format_bytes(1023), "1023 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024 - 1), "1024.00 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 - 1), "1024.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 - 1), "1024.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    // u64::MAX boundary
    let max_formatted = format_bytes(u64::MAX);
    assert!(
        max_formatted.ends_with(" TB"),
        "u64::MAX must format as TB without panic, got: {}",
        max_formatted
    );
}

#[test]
fn challenge_format_metrics_table_extreme_values() {
    let extreme = ProcessMetrics {
        memory: MemoryMetrics {
            rss_bytes: u64::MAX,
            peak_rss_bytes: u64::MAX,
            virtual_bytes: u64::MAX,
            formatted_rss: "16777216.00 TB (HUGE RSS)".to_string(),
            formatted_peak: "16777216.00 TB (HUGE PEAK)".to_string(),
        },
        cpu: CpuMetrics {
            process_pct: 100.0,
            user_ms: u64::MAX,
            kernel_ms: u64::MAX,
            total_ms: u64::MAX,
        },
        threads: ThreadMetrics {
            active_threads: 999_999,
            process_handles: Some(5_000_000),
        },
        storage: StorageMetrics {
            ctrl_dir_bytes: u64::MAX,
            task_logs_bytes: u64::MAX,
            formatted_ctrl: "99999.99 TB".to_string(),
            file_count: 10_000_000,
        },
        timestamp: u64::MAX,
    };

    let table = format_metrics_table(&extreme);
    assert!(table.starts_with('╭'));
    assert!(table.ends_with("╯\n"));
    assert!(table.contains("16777216.00 TB (HUGE RSS)"));
    assert!(table.contains("999999 (Handles: 5000000)"));
}
