//! Empirical Adversarial Challenge Test Harness for Milestone 1 (Resource Telemetry Engine)
//!
//! Stress-tests:
//! 1. Concurrent telemetry capture across 50 threads with barrier synchronization.
//! 2. Invariants: `peak_rss >= rss > 0`, `active_threads >= 1`, `cpu_pct in [0.0, 100.0]`.
//! 3. `CpuSampler` with rapid sub-millisecond calls (<1ms debounce), zero CPU deltas, backward/forward time jumps, and core bounds.
//! 4. Storage calculation under deeply nested hierarchies (40+ levels), non-directory files, nonexistent paths, and permission boundaries.
//! 5. Handle leak detection over hundreds of successive captures.

#[path = "../src/telemetry/mod.rs"]
mod telemetry;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, SystemTime};
use telemetry::{
    calculate_storage_metrics, capture_metrics, format_bytes, format_metrics_table, CpuSampler,
};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos));
    fs::create_dir_all(&dir).expect("Failed to create temp directory");
    dir
}

// ============================================================================
// SUITE 1: CONCURRENT TELEMETRY CAPTURE (50 THREADS) & INVARIANT ENFORCEMENT
// ============================================================================

#[test]
fn challenge_concurrent_telemetry_capture_50_threads() {
    const NUM_THREADS: usize = 50;
    const ITERATIONS_PER_THREAD: usize = 5;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let collected_metrics = Arc::new(std::sync::Mutex::new(Vec::with_capacity(
        NUM_THREADS * ITERATIONS_PER_THREAD,
    )));
    let mut handles = Vec::with_capacity(NUM_THREADS);

    for _ in 0..NUM_THREADS {
        let b = Arc::clone(&barrier);
        let sink = Arc::clone(&collected_metrics);

        let handle = thread::spawn(move || {
            // Synchronize all 50 threads so they burst simultaneously
            b.wait();

            let mut local_snapshots = Vec::with_capacity(ITERATIONS_PER_THREAD);
            for _ in 0..ITERATIONS_PER_THREAD {
                let m = capture_metrics(None);
                local_snapshots.push(m);
            }

            let mut guard = sink.lock().expect("lock collected metrics");
            guard.extend(local_snapshots);
        });

        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread panicked during concurrent capture");
    }

    let guard = collected_metrics.lock().expect("lock collected metrics");
    assert_eq!(
        guard.len(),
        NUM_THREADS * ITERATIONS_PER_THREAD,
        "All 250 capture operations must complete successfully"
    );

    let mut max_observed_threads = 0;

    for (idx, m) in guard.iter().enumerate() {
        // Invariant 1: Physical RSS must be non-zero in a running process
        assert!(
            m.memory.rss_bytes > 0,
            "Invariant violation at sample #{}: RSS bytes must be > 0 (observed: {})",
            idx,
            m.memory.rss_bytes
        );

        // Invariant 2: Peak RSS must always be greater than or equal to current RSS
        assert!(
            m.memory.peak_rss_bytes >= m.memory.rss_bytes,
            "Invariant violation at sample #{}: Peak RSS ({} bytes) must be >= RSS ({} bytes)",
            idx,
            m.memory.peak_rss_bytes,
            m.memory.rss_bytes
        );

        // Invariant 3: Active thread count must be at least 1
        assert!(
            m.threads.active_threads >= 1,
            "Invariant violation at sample #{}: active_threads must be >= 1 (observed: {})",
            idx,
            m.threads.active_threads
        );

        if m.threads.active_threads > max_observed_threads {
            max_observed_threads = m.threads.active_threads;
        }

        // Invariant 4: CPU utilization must be bounded in [0.0%, 100.0%] and non-NaN
        assert!(
            !m.cpu.process_pct.is_nan(),
            "Invariant violation at sample #{}: process_pct is NaN",
            idx
        );
        assert!(
            m.cpu.process_pct >= 0.0 && m.cpu.process_pct <= 100.0,
            "Invariant violation at sample #{}: process_pct ({}) must be in [0.0, 100.0]",
            idx,
            m.cpu.process_pct
        );

        // Invariant 5: CPU total_ms must equal user_ms + kernel_ms
        assert_eq!(
            m.cpu.total_ms,
            m.cpu.user_ms + m.cpu.kernel_ms,
            "Invariant violation at sample #{}: total_ms must equal user_ms + kernel_ms",
            idx
        );

        // Invariant 6: Timestamp must be reasonable
        assert!(m.timestamp > 1700000000, "Valid epoch timestamp required");

        // Invariant 7: Formatted strings must not be blank
        assert!(!m.memory.formatted_rss.is_empty());
        assert!(!m.memory.formatted_peak.is_empty());
    }

    // Verify that multi-threading was observed by the OS thread enumerator
    assert!(
        max_observed_threads >= 2,
        "Expected thread enumerator to observe multi-threaded state (max observed: {})",
        max_observed_threads
    );
}

#[test]
fn challenge_concurrent_capture_with_simultaneous_storage_io() {
    let temp_dir = unique_temp_dir("ctrl_concurrent_io");
    let tasks_dir = temp_dir.join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let path = Arc::new(temp_dir);

    // Thread writing and removing files in bounded bursts
    let r1 = Arc::clone(&running);
    let p1 = Arc::clone(&path);
    let churn_handle = thread::spawn(move || {
        let mut i = 0usize;
        while r1.load(Ordering::Relaxed) {
            i += 1;
            let file_a = p1.join(format!("file_{}.tmp", i % 10));
            let file_b = p1.join("tasks").join(format!("task_{}.log", i % 10));
            let _ = fs::write(&file_a, b"test content");
            let _ = fs::write(&file_b, b"task log output stream");
            let _ = fs::remove_file(&file_a);
            thread::sleep(Duration::from_millis(5));
        }
    });

    // 10 threads reading metrics with storage path concurrently
    const THREADS: usize = 10;
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut reader_handles = Vec::with_capacity(THREADS);

    for _ in 0..THREADS {
        let b = Arc::clone(&barrier);
        let p = Arc::clone(&path);
        let handle = thread::spawn(move || {
            b.wait();
            for _ in 0..15 {
                let m = capture_metrics(Some(&p));
                assert!(m.memory.peak_rss_bytes >= m.memory.rss_bytes);
                assert!(m.threads.active_threads >= 1);
            }
        });
        reader_handles.push(handle);
    }

    for h in reader_handles {
        h.join().expect("Reader thread panicked during churn");
    }

    running.store(false, Ordering::Relaxed);
    churn_handle.join().expect("Churn thread panicked");

    let _ = fs::remove_dir_all(&*path);
}

// ============================================================================
// SUITE 2: CPU SAMPLER ADVERSARIAL STRESS & EDGE CASES
// ============================================================================

#[test]
fn challenge_cpu_sampler_rapid_sub_millisecond_debounce() {
    let mut sampler = CpuSampler::with_cores(4);

    // 1. Prime initial reference state (age past constructor instant)
    thread::sleep(Duration::from_millis(2));
    sampler.sample(1000, 500);

    // 2. Establish baseline sample beyond the 1ms debounce window
    thread::sleep(Duration::from_millis(15));
    let m1 = sampler.sample(1030, 520);
    assert!(!m1.process_pct.is_nan());
    assert!(m1.process_pct > 0.0 && m1.process_pct <= 100.0);

    // 3. Rapid invocations in tight loop within sub-millisecond window (< 1ms elapsed)
    // 25 iterations execute in ~20-30 microseconds, safely within the 1.0 ms debounce window.
    let start = std::time::Instant::now();
    for i in 1..=25 {
        if start.elapsed() >= Duration::from_micros(800) {
            break; // Stop cleanly if OS scheduler descheduled this thread
        }
        let m = sampler.sample(1030 + i, 520 + i);
        assert!(
            !m.process_pct.is_nan(),
            "Debounced sample #{} produced NaN",
            i
        );
        assert!(
            m.process_pct >= 0.0 && m.process_pct <= 100.0,
            "Debounced sample #{} out of bounds: {}",
            i,
            m.process_pct
        );
        // Under <1ms, previous percentage is safely preserved
        assert_eq!(
            m.process_pct, m1.process_pct,
            "Debounced sample #{} should preserve previous percentage ({})",
            i, m1.process_pct
        );
    }
}

#[test]
fn challenge_cpu_sampler_zero_cpu_deltas() {
    let mut sampler = CpuSampler::with_cores(2);

    // Age past initial constructor instant to register custom baseline
    thread::sleep(Duration::from_millis(2));
    sampler.sample(1000, 500);

    // Wait at least 15ms to exceed 1ms debounce window
    thread::sleep(Duration::from_millis(15));

    // Sample with EXACTLY zero CPU time advance
    let m = sampler.sample(1000, 500);
    assert_eq!(
        m.process_pct, 0.0,
        "Zero CPU time delta must result in 0.0% CPU utilization"
    );

    // Another interval with zero CPU delta
    thread::sleep(Duration::from_millis(15));
    let m2 = sampler.sample(1000, 500);
    assert_eq!(
        m2.process_pct, 0.0,
        "Repeated zero CPU delta must maintain 0.0%"
    );
}

#[test]
fn challenge_cpu_sampler_artificial_time_jumps_and_bounds() {
    let mut sampler = CpuSampler::with_cores(4);

    // Age past constructor instant to establish initial baseline
    thread::sleep(Duration::from_millis(2));
    sampler.sample(1000, 1000);

    // 1. Backward CPU time jump (clock anomaly / counter reset)
    thread::sleep(Duration::from_millis(10));
    let m_back = sampler.sample(500, 500);
    assert!(
        !m_back.process_pct.is_nan(),
        "Backward jump must not cause NaN"
    );
    assert_eq!(
        m_back.process_pct, 0.0,
        "Backward CPU jump must saturate to 0.0% without panicking"
    );

    // 2. Massive forward CPU time jump (e.g. 10,000,000 ms elapsed in 10ms wall time)
    thread::sleep(Duration::from_millis(10));
    let m_forward = sampler.sample(10_000_000, 10_000_000);
    assert!(
        !m_forward.process_pct.is_nan(),
        "Massive jump must not cause NaN"
    );
    assert_eq!(
        m_forward.process_pct, 100.0,
        "Extreme CPU jump must clamp to exactly 100.0%"
    );

    // 3. Extreme core counts
    let mut sampler_zero_cores = CpuSampler::with_cores(0);
    assert_eq!(
        sampler_zero_cores.logical_cores(),
        1,
        "0 cores must be normalized to at least 1 core"
    );
    thread::sleep(Duration::from_millis(5));
    let m_zero = sampler_zero_cores.sample(100, 50);
    assert!(!m_zero.process_pct.is_nan());

    let mut sampler_large_cores = CpuSampler::with_cores(4096);
    assert_eq!(sampler_large_cores.logical_cores(), 4096);
    thread::sleep(Duration::from_millis(5));
    let m_large = sampler_large_cores.sample(100, 50);
    assert!(!m_large.process_pct.is_nan());
    assert!(m_large.process_pct >= 0.0 && m_large.process_pct <= 100.0);

    // 4. Reset behavior
    sampler.reset();
    thread::sleep(Duration::from_millis(5));
    let m_reset = sampler.sample(10_000_000, 10_000_000);
    assert!(!m_reset.process_pct.is_nan());
}

// ============================================================================
// SUITE 3: STORAGE CALCULATION BOUNDARIES & DEEP HIERARCHIES
// ============================================================================

#[test]
fn challenge_storage_deeply_nested_hierarchy() {
    let base = unique_temp_dir("ctrl_deep_nest");

    // Construct 40 levels of nested subdirectories
    let mut current = base.clone();
    let mut expected_total_bytes = 0u64;
    let mut expected_log_bytes = 0u64;
    let mut expected_files = 0usize;

    for depth in 1..=40 {
        current = current.join(format!("d_{}", depth));
        fs::create_dir_all(&current).expect("create nested directory level");

        if depth % 5 == 0 {
            // Add a task log file
            let log_file = current.join(format!("task_{}.LOG", depth));
            let content = format!("Log entry at depth {}\n", depth);
            fs::write(&log_file, &content).expect("write log file");
            expected_total_bytes += content.len() as u64;
            expected_log_bytes += content.len() as u64;
            expected_files += 1;
        }

        if depth % 3 == 0 {
            // Add a non-log file
            let meta_file = current.join("meta.json");
            let content = "{\"status\":\"ok\"}";
            fs::write(&meta_file, content).expect("write meta file");
            expected_total_bytes += content.len() as u64;
            expected_files += 1;
        }
    }

    let storage = calculate_storage_metrics(Some(&base));
    assert_eq!(
        storage.file_count, expected_files,
        "File count across deep hierarchy must match"
    );
    assert_eq!(
        storage.ctrl_dir_bytes, expected_total_bytes,
        "Total bytes across deep hierarchy must match"
    );
    assert_eq!(
        storage.task_logs_bytes, expected_log_bytes,
        "Log bytes across deep hierarchy must match"
    );

    let _ = fs::remove_dir_all(&base);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn challenge_storage_missing_and_non_directory_boundaries() {
    // 1. Non-existent path
    let bogus = Path::new("definitely_does_not_exist_77182910");
    let s_none = calculate_storage_metrics(Some(bogus));
    assert_eq!(s_none.ctrl_dir_bytes, 0);
    assert_eq!(s_none.task_logs_bytes, 0);
    assert_eq!(s_none.file_count, 0);
    assert_eq!(s_none.formatted_ctrl, "0 B");

    // 2. Passing a regular file instead of a directory
    let temp_dir = unique_temp_dir("ctrl_file_boundary");
    let file_path = temp_dir.join("not_a_dir.txt");
    fs::write(&file_path, b"hello world").expect("write file");

    let s_file = calculate_storage_metrics(Some(&file_path));
    assert_eq!(
        s_file.ctrl_dir_bytes, 0,
        "Targeting a regular file must yield 0 bytes safely without panicking"
    );
    assert_eq!(s_file.file_count, 0);

    // 3. Read-only / attribute boundaries
    let sub = temp_dir.join("readonly_sub");
    fs::create_dir_all(&sub).expect("create readonly sub");
    let log = sub.join("task.log");
    fs::write(&log, b"read-only content").expect("write log");

    // Mark file as read-only
    let mut perms = fs::metadata(&log).unwrap().permissions();
    perms.set_readonly(true);
    let _ = fs::set_permissions(&log, perms.clone());

    let s_perm = calculate_storage_metrics(Some(&temp_dir));
    assert!(
        s_perm.ctrl_dir_bytes >= 17,
        "Read-only file should still be readable for size metadata"
    );
    assert_eq!(s_perm.task_logs_bytes, 17);

    // Cleanup permissions before removing directory
    perms.set_readonly(false);
    let _ = fs::set_permissions(&log, perms);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn challenge_symlink_junction_traversal_behavior() {
    let temp_dir = unique_temp_dir("ctrl_symlink_test");
    let target_dir = temp_dir.join("real_folder");
    fs::create_dir_all(&target_dir).unwrap();
    let file1 = target_dir.join("data.txt");
    fs::write(&file1, b"hello world 11b").unwrap();

    let link_dir = temp_dir.join("link_folder");
    #[cfg(windows)]
    let link_created = std::os::windows::fs::symlink_dir(&target_dir, &link_dir).is_ok();
    #[cfg(not(windows))]
    let link_created = std::os::unix::fs::symlink(&target_dir, &link_dir).is_ok();

    if link_created {
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                if entry.path() == link_dir {
                    let entry_ft = entry.file_type().unwrap();
                    let meta = entry.metadata().unwrap();
                    let meta_ft = meta.file_type();

                    println!("ENTRY_FT_IS_SYMLINK: {}", entry_ft.is_symlink());
                    println!("META_FT_IS_SYMLINK: {}", meta_ft.is_symlink());
                    println!("META_IS_DIR: {}", meta.is_dir());
                    
                    assert!(entry_ft.is_symlink(), "Directory symlink must be recognized as symlink");
                    assert!(meta_ft.is_symlink(), "Metadata file_type must report symlink on Windows");
                    assert!(!meta.is_dir(), "Symlink directory metadata.is_dir() is false");
                }
            }
        }
        let metrics = calculate_storage_metrics(Some(&temp_dir));
        // Target folder contains data.txt (15 bytes). The symlink folder must NOT double-count files or bytes.
        assert_eq!(metrics.file_count, 1, "Symlink directory must not double count files");
        assert_eq!(metrics.ctrl_dir_bytes, 15, "Symlink directory must not double count bytes");
    }
    let _ = fs::remove_file(&link_dir).or_else(|_| fs::remove_dir(&link_dir));
    let _ = fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// SUITE 4: HANDLE LEAK DETECTION & RESOURCE STABILITY
// ============================================================================

#[test]
fn challenge_handle_stability_over_repetitive_captures() {
    // Warm up
    let initial = capture_metrics(None);
    let initial_handles = initial.threads.process_handles;

    if let Some(h_start) = initial_handles {
        // Execute 50 consecutive capture_metrics calls
        for _ in 0..50 {
            let m = capture_metrics(None);
            assert!(m.memory.peak_rss_bytes >= m.memory.rss_bytes);
            assert!(m.threads.active_threads >= 1);
        }

        let final_metrics = capture_metrics(None);
        let h_end = final_metrics.threads.process_handles.unwrap();

        // Handle count should not leak linearly with captures
        let handle_diff = (h_end as i64) - (h_start as i64);
        assert!(
            handle_diff < 50,
            "Detected potential OS handle leak: start={}, end={}, diff={}",
            h_start,
            h_end,
            handle_diff
        );
    }
}

// ============================================================================
// SUITE 5: TABLE FORMATTING AND BYTE FORMATTER BOUNDARIES
// ============================================================================

#[test]
fn challenge_format_bytes_boundaries() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1), "1 B");
    assert_eq!(format_bytes(1023), "1023 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1024 * 1024 - 1), "1024.00 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    // Extremely large byte quantity
    let max_b = format_bytes(u64::MAX);
    assert!(max_b.ends_with("TB"));
}

#[test]
fn challenge_format_metrics_table_boundary_alignment() {
    let mut m = capture_metrics(None);
    m.memory.rss_bytes = 100_000_000_000;
    m.memory.formatted_rss = format_bytes(m.memory.rss_bytes);
    m.cpu.process_pct = 99.99;
    m.threads.active_threads = 999;
    m.threads.process_handles = Some(99999);

    let table = format_metrics_table(&m);
    let lines: Vec<&str> = table.lines().collect();
    assert!(lines.len() >= 10, "Table must have header, borders, and rows");

    // Assert visual box integrity: all inner rows have matching character width
    let top_border_len = lines[0].chars().count();
    let bottom_border_len = lines.last().unwrap().chars().count();
    assert_eq!(top_border_len, bottom_border_len);

    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.chars().count(),
            top_border_len,
            "Row #{} width mismatch: '{}' (len {}) vs border (len {})",
            i,
            line,
            line.chars().count(),
            top_border_len
        );
    }
}
