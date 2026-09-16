//! Comprehensive Automated Resource Telemetry & Performance Benchmark Suite (Milestone 3)
//!
//! Suites:
//! 1. RAM & Memory Benchmarks:
//!    - benchmark_idle_baseline_ram_strictly_under_20mb
//!    - benchmark_repetitive_1000_mock_tasks_zero_memory_leak
//!    - benchmark_concurrent_tasks_peak_memory_bounded_and_reclaimed
//! 2. CPU Utilization & Non-Busy-Wait Benchmarks:
//!    - benchmark_idle_baseline_cpu
//!    - benchmark_idle_scheduler_tick_cpu
//!    - benchmark_idle_http_server_accept_cpu
//!    - benchmark_idle_condvar_await_worker_cpu
//!    - benchmark_combined_idle_subsystems_cpu
//!    - benchmark_cpu_measurement_detects_intentional_busy_loop
//! 3. Storage Isolation & Boundedness Benchmarks:
//!    - benchmark_storage_tasks_jsonl_clean_and_bounded
//!    - benchmark_storage_task_logs_concurrency_isolation
//!    - benchmark_storage_raii_tempdir_zero_orphan_files
//! 4. Binary Size Boundary Enforcement:
//!    - benchmark_release_binary_size_boundary
//! 5. Thread & Socket/Handle Lifecycle Benchmarks:
//!    - benchmark_thread_lifecycle_100_spawn_cancel_cycles
//!    - benchmark_handle_lifecycle_100_spawn_cancel_cycles
//!    - benchmark_socket_lifecycle_100_http_requests

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
    #[path = "../../src/agent/scheduler.rs"]
    pub mod scheduler;
}

#[path = "../src/server.rs"]
pub mod server;

pub use server::telemetry;

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent::scheduler::TaskScheduler;
use agent::tasks::{
    CancellationToken, OutputSink, TaskLogBuffer, TaskManager, TaskSnapshot, TaskStatus,
};
use telemetry::capture_metrics;

/// Global test lock serializing process-wide resource benchmarks to prevent
/// concurrent test distortion of memory, CPU counters, and OS handles.
static BENCHMARK_MUTEX: Mutex<()> = Mutex::new(());

/// Self-cleaning isolated temporary directory for test filesystem isolation.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "bench_temp_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Failed to create temporary directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(windows)]
#[allow(non_snake_case)]
fn get_process_cpu_times() -> (u64, u64) {
    #[repr(C)]
    #[derive(Default)]
    #[allow(clippy::upper_case_acronyms)]
    struct FILETIME {
        dwLowDateTime: u32,
        dwHighDateTime: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            hProcess: *mut std::ffi::c_void,
            lpCreationTime: *mut FILETIME,
            lpExitTime: *mut FILETIME,
            lpKernelTime: *mut FILETIME,
            lpUserTime: *mut FILETIME,
        ) -> i32;
    }
    unsafe {
        let mut create = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let proc = GetCurrentProcess();
        if GetProcessTimes(proc, &mut create, &mut exit, &mut kernel, &mut user) != 0 {
            let kernel_100ns =
                ((kernel.dwHighDateTime as u64) << 32) | (kernel.dwLowDateTime as u64);
            let user_100ns =
                ((user.dwHighDateTime as u64) << 32) | (user.dwLowDateTime as u64);
            (user_100ns / 10_000, kernel_100ns / 10_000)
        } else {
            (0, 0)
        }
    }
}

#[cfg(windows)]
fn trim_working_set() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32EmptyWorkingSet(hProcess: *mut std::ffi::c_void) -> i32;
    }
    unsafe {
        let proc = GetCurrentProcess();
        let _ = K32EmptyWorkingSet(proc);
    }
    thread::sleep(Duration::from_millis(50));
}

#[cfg(not(windows))]
fn trim_working_set() {}

#[cfg(not(windows))]
fn get_process_cpu_times() -> (u64, u64) {
    let metrics = capture_metrics(None);
    (metrics.cpu.user_ms, metrics.cpu.kernel_ms)
}

/// Helper measuring genuine process-wide CPU utilization over a designated observation window.
/// Accurately captures cumulative CPU execution time across all subsystem threads in the process
/// (scheduler, HTTP server, task workers, awaiters), while stabilizing transient thread startup
/// noise before establishing the baseline.
fn measure_idle_cpu_window(idle_duration: Duration) -> (f64, u64, Duration) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1) as f64;

    // 1. Stabilize transient warmup and background thread init:
    // Allow thread creation/spawning to settle and verify quiescence before capturing baseline
    thread::sleep(Duration::from_millis(150));
    let stabilize_deadline = Instant::now() + Duration::from_millis(2000);
    let mut prev_times = get_process_cpu_times();
    while Instant::now() < stabilize_deadline {
        thread::sleep(Duration::from_millis(100));
        let curr_times = get_process_cpu_times();
        let delta = (curr_times.0.saturating_sub(prev_times.0))
            + (curr_times.1.saturating_sub(prev_times.1));
        if delta <= 2 {
            break;
        }
        prev_times = curr_times;
    }

    // 2. Establish starting baseline across the entire process
    let (start_user, start_kernel) = get_process_cpu_times();
    let start_wall = Instant::now();

    // 3. Idle observation window
    thread::sleep(idle_duration);

    // 4. Capture ending metrics across the entire process
    let (end_user, end_kernel) = get_process_cpu_times();
    let wall_elapsed = start_wall.elapsed();
    let delta_wall_ms = wall_elapsed.as_secs_f64() * 1000.0;

    let delta_user = end_user.saturating_sub(start_user);
    let delta_kernel = end_kernel.saturating_sub(start_kernel);
    let delta_cpu_ms = delta_user.saturating_add(delta_kernel);

    let cpu_pct = if delta_wall_ms < 1.0 {
        0.0
    } else {
        ((delta_cpu_ms as f64 / delta_wall_ms) / cores * 100.0).clamp(0.0, 100.0)
    };

    println!(
        "DEBUG CPU: delta_user={}, delta_kernel={}, delta_cpu_ms={}, cpu_pct={:.2}% over {:.2} ms wall",
        delta_user, delta_kernel, delta_cpu_ms, cpu_pct, delta_wall_ms
    );

    (cpu_pct, delta_cpu_ms, wall_elapsed)
}

// ============================================================================
// SUITE 1: RAM & MEMORY BENCHMARKS
// ============================================================================

#[test]
fn benchmark_idle_baseline_ram_strictly_under_20mb() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Warm-up telemetry caches and trim allocator pages
    trim_working_set();
    for _ in 0..5 {
        let _ = capture_metrics(None);
    }

    // 2. Measure in-process idle metrics
    let idle_metrics = capture_metrics(None);
    let rss = idle_metrics.memory.rss_bytes;
    let rss_mb = rss as f64 / (1024.0 * 1024.0);

    println!(
        "📊 [BENCHMARK] In-Process Idle RAM: {} bytes ({:.2} MB)",
        rss, rss_mb
    );

    assert!(rss > 0, "Idle RSS must be greater than zero");
    const MAX_IDLE_RAM_BYTES: u64 = 20 * 1024 * 1024; // 20 MB threshold
    assert!(
        rss < MAX_IDLE_RAM_BYTES,
        "VIOLATION: In-process idle RAM ({:.2} MB) exceeded 20 MB threshold!",
        rss_mb
    );

    // 3. Standalone child CLI process assertion
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = std::process::Command::new(bin)
        .arg("--cli")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli --cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open child stdin");
        let _ = writeln!(stdin, "/stats --json");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Child process failed to exit");
    assert!(out.status.success(), "Child process exited with failure");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json_start = stdout.find('{').expect("Must find JSON output in stdout");
    let json_end = stdout.rfind('}').expect("Must find JSON closing brace");
    let json_str = &stdout[json_start..=json_end];

    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Must parse JSON from child /stats --json");
    let child_rss = parsed["memory"]["rss_bytes"]
        .as_u64()
        .expect("Must extract memory.rss_bytes");
    let child_rss_mb = child_rss as f64 / (1024.0 * 1024.0);

    println!(
        "📊 [BENCHMARK] Standalone CLI Idle RAM: {} bytes ({:.2} MB)",
        child_rss, child_rss_mb
    );

    assert!(child_rss > 0, "Child RSS must be greater than zero");
    assert!(
        child_rss < MAX_IDLE_RAM_BYTES,
        "VIOLATION: Standalone CLI process idle RAM ({:.2} MB) exceeded 20 MB threshold!",
        child_rss_mb
    );
}

#[test]
fn benchmark_repetitive_1000_mock_tasks_zero_memory_leak() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let manager = TaskManager::new();

    // Deterministic mock agent runner simulating multi-step agent turn
    fn mock_agent_turn(
        token: CancellationToken,
        logs: Arc<TaskLogBuffer>,
    ) -> anyhow::Result<String> {
        let sink = OutputSink::Buffered(logs);
        sink.emit("Step 1: Parsing user request and analyzing intent...");

        // Simulate turn memory allocations (JSON parsing, message creation)
        let simulated_payload = serde_json::json!({
            "model": "mock-llm",
            "messages": [
                {"role": "system", "content": "You are a code agent."},
                {"role": "user", "content": "Execute benchmark checks."}
            ],
            "tools": [
                {"name": "read_file", "arguments": "{\"path\": \"src/main.rs\"}"}
            ]
        });

        let json_str = serde_json::to_string(&simulated_payload)?;
        let parsed: serde_json::Value = serde_json::from_str(&json_str)?;
        assert!(parsed.is_object());

        sink.emit("Step 2: Executed mock tool read_file (1,240 bytes read)");
        sink.log("Debug log line: tool execution succeeded".to_string());

        token.check().map_err(|e| anyhow::anyhow!("{}", e))?;

        sink.emit("Step 3: Generating final turn summary");
        Ok("Turn completed successfully with 1 tool call".to_string())
    }

    // 1. Warm-up phase: 30 iterations to prime heap allocator pools and thread caches
    for i in 0..30 {
        let (id, _, _) = manager
            .spawn_task_with_sink(
                format!("warmup-{}", i),
                "warmup".into(),
                mock_agent_turn,
            )
            .expect("Warmup task spawn must succeed");
        let _ = manager.await_task(&id, None).expect("Warmup await failed");
        manager.clear_completed();
    }

    thread::sleep(Duration::from_millis(50));

    // 2. Measure starting reference metrics
    let initial_metrics = capture_metrics(None);
    let rss_before = initial_metrics.memory.rss_bytes;
    let initial_threads = initial_metrics.threads.active_threads;

    println!(
        "📊 [BENCHMARK] 1,000 Tasks Start: RSS={} bytes ({:.2} MB), Threads={}",
        rss_before,
        rss_before as f64 / (1024.0 * 1024.0),
        initial_threads
    );

    let start_time = Instant::now();
    const ITERATIONS: usize = 1000;

    // 3. Execute 1,000 repetitive mock task turns
    for i in 0..ITERATIONS {
        let (id, _, _) = manager
            .spawn_task_with_sink(
                format!("rep-task-{}", i),
                "repetitive mock turn".into(),
                mock_agent_turn,
            )
            .expect("Spawn must succeed");

        let snap = manager.await_task(&id, None).expect("Await must succeed");
        assert_eq!(snap.status, TaskStatus::Completed);

        let cleared = manager.clear_completed();
        assert_eq!(cleared, 1, "Must clear exactly 1 completed task per iteration");
    }

    let elapsed = start_time.elapsed();
    println!(
        "📊 [BENCHMARK] Executed {} mock tasks in {:?} ({:.2} ms/task)",
        ITERATIONS,
        elapsed,
        elapsed.as_secs_f64() * 1000.0 / ITERATIONS as f64
    );

    // 4. Convergence loop to allow OS worker threads to terminate
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut current_threads = capture_metrics(None).threads.active_threads;
    while current_threads > initial_threads && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
        current_threads = capture_metrics(None).threads.active_threads;
    }

    // 5. Measure ending reference metrics
    let final_metrics = capture_metrics(None);
    let rss_after = final_metrics.memory.rss_bytes;
    let delta_rss = (rss_after as i64) - (rss_before as i64);

    println!(
        "📊 [BENCHMARK] 1,000 Tasks End: RSS Before={} B, After={} B, Delta={} B ({:.2} KB)",
        rss_before,
        rss_after,
        delta_rss,
        delta_rss as f64 / 1024.0
    );

    // Allocator noise threshold: strictly < 1 MB (1,048,576 bytes)
    const MAX_ALLOCATOR_NOISE_BYTES: i64 = 1_048_576;
    assert!(
        delta_rss < MAX_ALLOCATOR_NOISE_BYTES,
        "MEMORY LEAK DETECTED: RSS grew by {} bytes ({:.2} MB) over 1,000 iterations (threshold: 1.0 MB)",
        delta_rss,
        delta_rss as f64 / (1024.0 * 1024.0)
    );

    // Thread leak assertion: active threads must not exceed initial baseline
    assert!(
        current_threads <= initial_threads,
        "THREAD LEAK DETECTED: Active threads ({}) exceeded baseline ({})",
        current_threads, initial_threads
    );
}

#[test]
fn benchmark_concurrent_tasks_peak_memory_bounded_and_reclaimed() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let manager = TaskManager::new();
    const NUM_PARALLEL: usize = 25;
    const BUFFER_SIZE: usize = 256 * 1024; // 256 KB per task

    // 1. Establish baseline
    trim_working_set();
    let baseline_metrics = capture_metrics(None);
    let baseline_rss = baseline_metrics.memory.rss_bytes;
    let baseline_threads = baseline_metrics.threads.active_threads;

    println!(
        "📊 [BENCHMARK] Concurrent Burst Baseline: RSS={} bytes ({:.2} MB), Threads={}",
        baseline_rss,
        baseline_rss as f64 / (1024.0 * 1024.0),
        baseline_threads
    );

    // Barriers: 25 workers + 1 coordinator = 26
    let start_barrier = Arc::new(Barrier::new(NUM_PARALLEL + 1));
    let release_barrier = Arc::new(Barrier::new(NUM_PARALLEL + 1));

    let mut task_ids = Vec::with_capacity(NUM_PARALLEL);

    // 2. Spawn 25 parallel tasks holding active buffers
    for i in 0..NUM_PARALLEL {
        let sb = start_barrier.clone();
        let rb = release_barrier.clone();

        let (id, _, _) = manager
            .spawn_task_with_sink(
                format!("burst-task-{}", i),
                "Concurrent peak memory workload".into(),
                move |_token, logs| {
                    let sink = OutputSink::Buffered(logs);
                    sink.emit("Worker started, allocating buffer...");

                    // Allocate 256 KB buffer and touch every page
                    let mut payload = vec![0xA5u8; BUFFER_SIZE];
                    payload[0] = (i % 255) as u8;
                    payload[BUFFER_SIZE - 1] = 0x5A;

                    // Signal coordinator that buffer is allocated and held
                    sb.wait();

                    // Wait for coordinator to sample peak memory
                    rb.wait();

                    assert_eq!(payload[BUFFER_SIZE - 1], 0x5A);
                    Ok(format!("task-{} done", i))
                },
            )
            .expect("Concurrent task spawn failed");

        task_ids.push(id);
    }

    // 3. Wait until all 25 workers are simultaneously active holding memory
    start_barrier.wait();

    // 4. Sample peak metrics during synchronized concurrency peak
    let peak_metrics = capture_metrics(None);
    let peak_rss = peak_metrics.memory.rss_bytes;
    let peak_threads = peak_metrics.threads.active_threads;

    println!(
        "📊 [BENCHMARK] Concurrent Peak: RSS={} bytes ({:.2} MB), Threads={} (Expected >= {})",
        peak_rss,
        peak_rss as f64 / (1024.0 * 1024.0),
        peak_threads,
        baseline_threads + NUM_PARALLEL
    );

    assert!(
        peak_threads >= baseline_threads + NUM_PARALLEL,
        "Expected at least {} active threads during burst, observed {}",
        baseline_threads + NUM_PARALLEL,
        peak_threads
    );

    const MAX_PEAK_RAM_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
    assert!(
        peak_rss < MAX_PEAK_RAM_BYTES,
        "VIOLATION: Concurrent peak RAM ({:.2} MB) exceeded 50 MB ceiling!",
        peak_rss as f64 / (1024.0 * 1024.0)
    );

    // 5. Release workers to finish and deallocate
    release_barrier.wait();

    // 6. Await completion of all tasks
    for id in &task_ids {
        let snap = manager
            .await_task(id, Some(Duration::from_secs(5)))
            .expect("Task must finish within timeout");
        assert_eq!(snap.status, TaskStatus::Completed);
    }

    let cleared = manager.clear_completed();
    assert_eq!(cleared, NUM_PARALLEL);

    // 7. Convergence loop to allow thread exit
    let deadline = Instant::now() + Duration::from_millis(2000);
    let mut after_threads = capture_metrics(None).threads.active_threads;
    while after_threads > baseline_threads && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
        after_threads = capture_metrics(None).threads.active_threads;
    }

    assert!(
        after_threads <= baseline_threads,
        "Thread leak: active threads ({}) exceeded baseline ({})",
        after_threads, baseline_threads
    );

    // 8. Verify post-burst memory reclamation
    let after_metrics = capture_metrics(None);
    let after_rss = after_metrics.memory.rss_bytes;
    let retained_delta = (after_rss as i64) - (baseline_rss as i64);

    println!(
        "📊 [BENCHMARK] Burst Reclaimed: Baseline={} B, Post-Burst={} B, Retained Delta={} B ({:.2} KB)",
        baseline_rss,
        after_rss,
        retained_delta,
        retained_delta as f64 / 1024.0
    );

    const MAX_RETAINED_BYTES: i64 = 5 * 1024 * 1024; // 5 MB threshold
    assert!(
        retained_delta < MAX_RETAINED_BYTES,
        "Memory reclamation failed: post-burst RSS retained {} bytes ({:.2} MB) over baseline (threshold: 5.0 MB)",
        retained_delta,
        retained_delta as f64 / (1024.0 * 1024.0)
    );
}

// ============================================================================
// SUITE 2: CPU UTILIZATION & NON-BUSY-WAIT BENCHMARKS
// ============================================================================

#[test]
fn benchmark_idle_baseline_cpu() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let _tm = TaskManager::new();

    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(1000));

    println!(
        "📊 [BENCHMARK] Baseline Idle CPU: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 15,
        "Baseline idle CPU utilization ({:.2}%, {} ms delta) must be strictly < 1.0% or <= 15 ms",
        cpu_pct,
        delta_cpu_ms
    );
    assert!(
        delta_cpu_ms <= 30,
        "Baseline idle CPU time delta ({} ms) exceeded maximum noise allowance",
        delta_cpu_ms
    );
}

#[test]
fn benchmark_idle_scheduler_tick_cpu() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let scheduler = TaskScheduler::new();
    scheduler
        .register_task(
            "future-bench-job",
            "Future Cron",
            "Does not trigger during test",
            "0 0 1 1 *", // Jan 1st future
            None,
            || Ok("executed".into()),
        )
        .expect("register task");

    scheduler.start();

    // 1200ms ensures at least one 1s tick and condvar wait_timeout
    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(1200));

    scheduler.stop();

    println!(
        "📊 [BENCHMARK] Scheduler Idle CPU: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 15,
        "Scheduler idle CPU utilization ({:.2}%, {} ms delta) must be strictly < 1.0% or <= 15 ms",
        cpu_pct,
        delta_cpu_ms
    );
    assert!(
        delta_cpu_ms <= 30,
        "Scheduler idle CPU delta ({} ms) exceeded allowable threshold",
        delta_cpu_ms
    );
}

#[test]
fn benchmark_idle_http_server_accept_cpu() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let shutdown_token = CancellationToken::new();
    let (bound_port, handle) = server::spawn_server("127.0.0.1", 0, shutdown_token.clone())
        .expect("spawn HTTP server");

    assert!(bound_port > 0);

    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(1000));

    shutdown_token.cancel();
    let _ = handle.join();

    println!(
        "📊 [BENCHMARK] HTTP Server Idle CPU: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 15,
        "HTTP Server idle CPU utilization ({:.2}%, {} ms delta) must be strictly < 1.0% or <= 15 ms",
        cpu_pct,
        delta_cpu_ms
    );
    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 35,
        "HTTP Server idle CPU delta ({} ms) exceeded allowable threshold",
        delta_cpu_ms
    );
}

#[test]
fn benchmark_idle_condvar_await_worker_cpu() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let tm = TaskManager::new();
    let (task_id, cancel_token) = tm
        .spawn_task(
            "bench-condvar-worker".into(),
            "Simulating long task".into(),
            |token| {
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(50));
                }
                Ok("done".into())
            },
        )
        .expect("spawn task");

    let tm_clone = tm.clone();
    let task_id_clone = task_id.clone();
    let awaiter_handle = thread::spawn(move || tm_clone.await_task(&task_id_clone, None));

    let _ = tm.await_running(&task_id, Some(Duration::from_secs(2)));

    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(1000));

    cancel_token.cancel();
    let _ = awaiter_handle.join();

    println!(
        "📊 [BENCHMARK] Condvar Awaiter Idle CPU: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 15,
        "Condvar awaiter idle CPU utilization ({:.2}%, {} ms delta) must be strictly < 1.0% or <= 15 ms",
        cpu_pct,
        delta_cpu_ms
    );
    assert!(
        delta_cpu_ms <= 30,
        "Condvar awaiter idle CPU delta ({} ms) exceeded allowable threshold",
        delta_cpu_ms
    );
}

#[test]
fn benchmark_combined_idle_subsystems_cpu() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Start TaskScheduler
    let scheduler = TaskScheduler::new();
    scheduler.start();

    // 2. Start HTTP Server
    let server_token = CancellationToken::new();
    let (_port, server_handle) = server::spawn_server("127.0.0.1", 0, server_token.clone())
        .expect("spawn server");

    // 3. Start TaskManager with active Condvar awaiter
    let tm = TaskManager::new();
    let (task_id, task_token) = tm
        .spawn_task(
            "combined-bench-worker".into(),
            "Parked task".into(),
            |token| {
                while !token.is_cancelled() {
                    thread::sleep(Duration::from_millis(50));
                }
                Ok("done".into())
            },
        )
        .expect("spawn task");

    let tm_clone = tm.clone();
    let task_id_clone = task_id.clone();
    let awaiter_handle = thread::spawn(move || tm_clone.await_task(&task_id_clone, None));

    let _ = tm.await_running(&task_id, Some(Duration::from_secs(2)));

    // 4. Measure concurrent idle CPU across all active subsystems over 1000ms
    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(1000));

    // 5. Clean teardown
    task_token.cancel();
    let _ = awaiter_handle.join();
    server_token.cancel();
    let _ = server_handle.join();
    scheduler.stop();

    println!(
        "📊 [BENCHMARK] Combined Idle Subsystems CPU: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    assert!(
        cpu_pct < 1.0 || delta_cpu_ms <= 15,
        "Combined idle subsystems CPU utilization ({:.2}%, {} ms delta) must remain strictly < 1.0% or <= 15 ms",
        cpu_pct,
        delta_cpu_ms
    );
    assert!(
        delta_cpu_ms <= 35,
        "Combined idle subsystems CPU delta ({} ms) exceeded allowable threshold",
        delta_cpu_ms
    );
}

#[test]
fn benchmark_cpu_measurement_detects_intentional_busy_loop() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    use std::sync::atomic::{AtomicBool, Ordering};

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    // Spawn an intentional 100% CPU busy-spin background thread
    let spinner_handle = thread::spawn(move || {
        let mut sink: u64 = 0;
        while !stop_clone.load(Ordering::Relaxed) {
            sink = sink.wrapping_add(1);
            std::hint::spin_loop();
        }
        sink
    });

    // Measure idle CPU window over 500ms
    // If measure_idle_cpu_window were a facade querying only the sleeping test thread,
    // delta_cpu_ms would be 0 ms and cpu_pct would be 0.00%.
    // With genuine process CPU measurement, the spinning background thread burns CPU.
    let (cpu_pct, delta_cpu_ms, wall_elapsed) =
        measure_idle_cpu_window(Duration::from_millis(500));

    stop.store(true, Ordering::Relaxed);
    let _ = spinner_handle.join();

    println!(
        "📊 [BENCHMARK] Intentional Busy-Spin Detection: {:.2}% (delta CPU: {} ms over {} ms wall)",
        cpu_pct,
        delta_cpu_ms,
        wall_elapsed.as_millis()
    );

    // Over 500ms of spinning, a background busy thread consumes substantial CPU time.
    // Baseline idle CPU noise is bounded at <= 30-35 ms across all subsystems,
    // so delta_cpu_ms >= 80 ms provides robust margin (> 2x idle noise) while remaining
    // immune to parallel test scheduling starvation.
    assert!(
        delta_cpu_ms >= 80,
        "VIOLATION: Benchmark failed to detect background busy-spin! delta_cpu_ms={} ms (expected >= 80 ms)",
        delta_cpu_ms
    );
    assert!(
        cpu_pct > 0.0,
        "VIOLATION: Reported CPU percentage was 0.00% despite intentional busy-spin!"
    );
}

// ============================================================================
// SUITE 3: STORAGE ISOLATION & BOUNDEDNESS BENCHMARKS
// ============================================================================

#[test]
fn benchmark_storage_tasks_jsonl_clean_and_bounded() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let temp = TestTempDir::new("storage_jsonl_bench");
    let ctrl_dir = temp.path().join(".ctrl");
    fs::create_dir_all(&ctrl_dir).expect("create .ctrl dir");

    let manager = Arc::new(TaskManager::with_dir(ctrl_dir.clone()));
    const TOTAL_TASKS: usize = 50;

    // Spawn 50 tasks across 8 coordinator threads
    let (tx, rx) = std::sync::mpsc::channel();
    let mut coordinator_handles = Vec::new();

    for thread_idx in 0..8 {
        let mgr = manager.clone();
        let sender = tx.clone();
        let handle = thread::spawn(move || {
            let start = thread_idx * (TOTAL_TASKS / 8);
            let end = if thread_idx == 7 {
                TOTAL_TASKS
            } else {
                (thread_idx + 1) * (TOTAL_TASKS / 8)
            };

            for i in start..end {
                if i % 5 == 0 {
                    // Cancelled task
                    let (id, token) = mgr
                        .spawn_task(
                            format!("storage-cancel-{}", i),
                            "cancelled".into(),
                            move |tok| {
                                while !tok.is_cancelled() {
                                    thread::sleep(Duration::from_millis(5));
                                }
                                Ok("cancelled".into())
                            },
                        )
                        .expect("spawn");
                    token.cancel();
                    sender.send(id).expect("send id");
                } else if i % 5 == 1 {
                    // Failed task
                    let (id, _) = mgr
                        .spawn_task(
                            format!("storage-fail-{}", i),
                            "failed".into(),
                            move |_tok| Err(anyhow::anyhow!("simulated error")),
                        )
                        .expect("spawn");
                    sender.send(id).expect("send id");
                } else {
                    // Completed task
                    let (id, _) = mgr
                        .spawn_task(
                            format!("storage-ok-{}", i),
                            "ok".into(),
                            move |_tok| {
                                thread::sleep(Duration::from_millis(2));
                                Ok(format!("result-{}", i))
                            },
                        )
                        .expect("spawn");
                    sender.send(id).expect("send id");
                }
            }
        });
        coordinator_handles.push(handle);
    }
    drop(tx);

    for h in coordinator_handles {
        h.join().expect("join coordinator");
    }

    let mut task_ids = Vec::new();
    while let Ok(id) = rx.recv() {
        task_ids.push(id);
    }
    assert_eq!(task_ids.len(), TOTAL_TASKS);

    // Await all tasks to terminal state
    for id in &task_ids {
        let snap_res = manager.await_task(id, Some(Duration::from_secs(5)));
        assert!(snap_res.is_ok(), "Task {} failed to reach terminal state", id);
    }

    // Inspect .ctrl/tasks.jsonl
    let tasks_file = ctrl_dir.join("tasks.jsonl");
    assert!(tasks_file.exists(), "tasks.jsonl must exist on disk");

    let content = fs::read_to_string(&tasks_file).expect("read tasks.jsonl");
    let lines: Vec<&str> = content.lines().collect();

    println!(
        "📊 [BENCHMARK] tasks.jsonl: {} lines, {} bytes",
        lines.len(),
        content.len()
    );

    // Boundedness assertions: max 3 state transitions per task -> <= 150 lines
    assert!(
        lines.len() <= 150,
        "tasks.jsonl lines ({}) exceeded maximum 150 boundary",
        lines.len()
    );
    const MAX_JSONL_BYTES: usize = 50 * 1024; // 50 KB
    assert!(
        content.len() <= MAX_JSONL_BYTES,
        "tasks.jsonl size ({} bytes) exceeded 50 KB boundary",
        content.len()
    );

    // JSON cleanliness & monotonic transition validation
    let mut task_history: std::collections::HashMap<String, Vec<TaskStatus>> =
        std::collections::HashMap::new();

    for (line_idx, line) in lines.iter().enumerate() {
        assert!(
            !line.trim().is_empty(),
            "Line {} in tasks.jsonl was empty",
            line_idx
        );
        let snapshot: TaskSnapshot = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!(
                "Line {} in tasks.jsonl is invalid JSON: {}\nContent: {}",
                line_idx, e, line
            )
        });

        task_history
            .entry(snapshot.id)
            .or_default()
            .push(snapshot.status);
    }

    for (id, history) in task_history {
        assert!(
            !history.is_empty(),
            "Task {} must have at least one history record",
            id
        );
        let mut seen_terminal = false;
        for (i, status) in history.iter().enumerate() {
            if seen_terminal {
                panic!(
                    "Task {} emitted non-monotonic status after terminal: history={:?}",
                    id, history
                );
            }
            if status.is_terminal() {
                seen_terminal = true;
            }
            if i > 0 && *status == TaskStatus::Running {
                assert_eq!(
                    history[i - 1],
                    TaskStatus::Queued,
                    "Task {} transitioned to Running from non-Queued: {:?}",
                    id,
                    history
                );
            }
        }
    }
}

#[test]
fn benchmark_storage_task_logs_concurrency_isolation() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let temp = TestTempDir::new("storage_logs_isolation");
    let ctrl_dir = temp.path().join(".ctrl");
    fs::create_dir_all(&ctrl_dir).expect("create .ctrl dir");

    let manager = Arc::new(TaskManager::with_dir(ctrl_dir.clone()));
    const NUM_TASKS: usize = 20;
    const LINES_PER_TASK: usize = 50;

    let mut task_ids = Vec::with_capacity(NUM_TASKS);

    for i in 0..NUM_TASKS {
        let (id, _, _) = manager
            .spawn_task_with_sink(
                format!("iso-task-{}", i),
                "Concurrency isolation log workload".into(),
                move |_token, logs| {
                    let sink = OutputSink::Buffered(logs);
                    for step in 0..LINES_PER_TASK {
                        sink.emit(&format!("TASK_{:02}_STEP_{:02}_MARKER_{}", i, step, i * 1000 + step));
                    }
                    Ok(format!("task-{} done", i))
                },
            )
            .expect("spawn task");

        task_ids.push((i, id));
    }

    // Await all 20 tasks to finish
    for (_, id) in &task_ids {
        let snap = manager
            .await_task(id, Some(Duration::from_secs(5)))
            .expect("await task");
        assert_eq!(snap.status, TaskStatus::Completed);
    }

    // Inspect logs directory
    let logs_dir = ctrl_dir.join("tasks");
    assert!(logs_dir.exists(), "tasks log directory must exist");

    let entries = fs::read_dir(&logs_dir)
        .expect("read logs dir")
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "log")
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    println!(
        "📊 [BENCHMARK] Found {} isolated task log files in {:?}",
        entries.len(),
        logs_dir
    );

    assert_eq!(
        entries.len(),
        NUM_TASKS,
        "Expected exactly {} distinct .log files, found {}",
        NUM_TASKS,
        entries.len()
    );

    // Verify 0 cross-task line contamination
    for (task_idx, id) in &task_ids {
        let log_file = logs_dir.join(format!("{}.log", id));
        assert!(log_file.exists(), "Log file for task {} must exist", id);

        let content = fs::read_to_string(&log_file).expect("read task log file");
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(
            lines.len(),
            LINES_PER_TASK,
            "Task {} log file must contain exactly {} lines, found {}",
            id,
            LINES_PER_TASK,
            lines.len()
        );

        let expected_prefix = format!("TASK_{:02}_", task_idx);
        for line in lines {
            assert!(
                line.contains(&expected_prefix),
                "Task {} log line corrupted: '{}' did not contain expected prefix '{}'",
                id,
                line,
                expected_prefix
            );

            // Verify no line belongs to another task
            for other_idx in 0..NUM_TASKS {
                if other_idx != *task_idx {
                    let forbidden_marker = format!("TASK_{:02}_", other_idx);
                    assert!(
                        !line.contains(&forbidden_marker),
                        "CONTAMINATION DETECTED: Task {} log contains foreign marker '{}' on line '{}'",
                        id,
                        forbidden_marker,
                        line
                    );
                }
            }
        }
    }
}

#[test]
fn benchmark_storage_raii_tempdir_zero_orphan_files() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Standard completion RAII cleanup verification
    let mut dropped_paths = Vec::new();
    for i in 0..50 {
        let temp = TestTempDir::new(&format!("raii_std_{}", i));
        let test_file = temp.path().join("marker.txt");
        fs::write(&test_file, b"test content").expect("write test file");
        assert!(test_file.exists());
        dropped_paths.push(temp.path().to_path_buf());
        // drop temp
    }

    for path in &dropped_paths {
        assert!(
            !path.exists(),
            "ORPHAN FILE DETECTED: Temporary directory {:?} was not deleted on normal drop",
            path
        );
    }

    // 2. Unwind panic RAII cleanup verification
    let mut panic_paths = Vec::new();
    for i in 0..20 {
        let mut created_path = PathBuf::new();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let temp = TestTempDir::new(&format!("raii_panic_{}", i));
            created_path = temp.path().to_path_buf();
            fs::write(temp.path().join("data.bin"), vec![0xEE; 1024]).expect("write data");
            assert!(created_path.exists());
            if i % 2 == 0 {
                panic!("Simulated worker panic inside RAII scope");
            }
        }));

        assert!(res.is_err() || res.is_ok());
        assert!(!created_path.as_os_str().is_empty());
        panic_paths.push(created_path);
    }

    for path in &panic_paths {
        assert!(
            !path.exists(),
            "ORPHAN FILE DETECTED: Temporary directory {:?} remained on disk after panic unwind",
            path
        );
    }

    println!(
        "📊 [BENCHMARK] RAII TempDir Zero Orphan Files: Verified {} paths completely removed",
        dropped_paths.len() + panic_paths.len()
    );
}

// ============================================================================
// SUITE 4: BINARY SIZE BOUNDARY ENFORCEMENT
// ============================================================================

#[test]
fn benchmark_release_binary_size_boundary() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Locate release binary
    let candidates = [
        PathBuf::from("target/release/ctrl-cli.exe"),
        PathBuf::from("target/release/ctrl-cli"),
        PathBuf::from("../target/release/ctrl-cli.exe"),
        PathBuf::from("../target/release/ctrl-cli"),
    ];

    let found_bin = candidates.iter().find(|p| p.exists()).cloned();
    let bin_path = match found_bin {
        Some(p) => p,
        None => {
            let exe = env!("CARGO_BIN_EXE_ctrl-cli");
            PathBuf::from(exe)
        }
    };

    let meta = fs::metadata(&bin_path)
        .unwrap_or_else(|e| panic!("Failed to query metadata for {:?}: {}", bin_path, e));
    let size_bytes = meta.len();
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
    let size_mib = size_bytes as f64 / 1_000_000.0;

    println!(
        "📊 [BENCHMARK] Release Binary Path: {:?}",
        bin_path
    );
    println!(
        "📊 [BENCHMARK] Release Binary Size: {} bytes ({:.2} MiB / {:.2} MB)",
        size_bytes, size_mb, size_mib
    );

    #[cfg(windows)]
    const MAX_RELEASE_BINARY_BYTES: u64 = 3_355_443; // 3.2 MB (accounting for Win32 MSVC CRT tables)
    #[cfg(not(windows))]
    const MAX_RELEASE_BINARY_BYTES: u64 = 2_621_440; // 2.5 MB

    assert!(
        size_bytes <= MAX_RELEASE_BINARY_BYTES,
        "VIOLATION: Binary size ({} bytes, {:.2} MB) exceeded boundary ({} bytes)",
        size_bytes,
        size_mb,
        MAX_RELEASE_BINARY_BYTES
    );
}

// ============================================================================
// SUITE 5: THREAD & SOCKET/HANDLE LIFECYCLE BENCHMARKS
// ============================================================================

#[test]
fn benchmark_thread_lifecycle_100_spawn_cancel_cycles() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Stabilize thread count before capturing baseline (allowing any prior test teardown to finish)
    let mut baseline_threads = telemetry::get_active_threads();
    loop {
        thread::sleep(Duration::from_millis(150));
        let next = telemetry::get_active_threads();
        if next == baseline_threads {
            break;
        }
        baseline_threads = next;
    }

    let tm = TaskManager::new();

    const CYCLES: usize = 100;
    println!(
        "📊 [BENCHMARK] Thread Lifecycle: Baseline Threads={}",
        baseline_threads
    );

    for i in 0..CYCLES {
        let (id, token) = tm
            .spawn_task(
                format!("th-cycle-{}", i),
                "churn".into(),
                |tok| {
                    while !tok.is_cancelled() {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Ok("cancelled".into())
                },
            )
            .expect("spawn");

        token.cancel();
        let snap = tm
            .await_task(&id, Some(Duration::from_secs(2)))
            .expect("await");
        assert_eq!(snap.status, TaskStatus::Cancelled);
        tm.clear_completed();
    }

    // Convergence polling up to 1500ms
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut current_threads = telemetry::get_active_threads();
    while current_threads > baseline_threads && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
        current_threads = telemetry::get_active_threads();
    }

    println!(
        "📊 [BENCHMARK] Thread Lifecycle: Baseline={}, Final={}",
        baseline_threads, current_threads
    );

    assert!(
        current_threads <= baseline_threads,
        "THREAD LEAK DETECTED: Active threads ({}) exceeded baseline ({})",
        current_threads, baseline_threads
    );
}

#[test]
fn benchmark_handle_lifecycle_100_spawn_cancel_cycles() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    thread::sleep(Duration::from_millis(50));
    let base_handles_opt = telemetry::get_process_handles();

    if let Some(base_handles) = base_handles_opt {
        let tm = TaskManager::new();
        const CYCLES: usize = 100;

        println!(
            "📊 [BENCHMARK] Handle Lifecycle: Baseline Handles={}",
            base_handles
        );

        for i in 0..CYCLES {
            let (id, _, _) = tm
                .spawn_task_with_sink(
                    format!("h-cycle-{}", i),
                    "churn".into(),
                    |tok, logs| {
                        let sink = OutputSink::Buffered(logs);
                        sink.emit("log line");
                        while !tok.is_cancelled() {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Ok("cancelled".into())
                    },
                )
                .expect("spawn");

            let _ = tm.cancel_task(&id);
            let snap = tm
                .await_task(&id, Some(Duration::from_secs(2)))
                .expect("await");
            assert_eq!(snap.status, TaskStatus::Cancelled);
            tm.clear_completed();
        }

        // Convergence polling up to 1500ms
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut final_handles = telemetry::get_process_handles().unwrap();
        while final_handles > base_handles + 2
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
            final_handles = telemetry::get_process_handles().unwrap();
        }

        let delta = (final_handles as i64) - (base_handles as i64);
        println!(
            "📊 [BENCHMARK] Handle Lifecycle: Baseline={}, Final={}, Delta={}",
            base_handles, final_handles, delta
        );

        assert!(
            final_handles <= base_handles + 2,
            "HANDLE LEAK DETECTED: Base handles={}, final handles={}, delta={} (allowance <= +2)",
            base_handles, final_handles, delta
        );
    }
}

#[test]
fn benchmark_socket_lifecycle_100_http_requests() {
    let _guard = BENCHMARK_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Warm up Winsock driver / provider subsystem so static OS helper handles are initialized
    {
        let warmup_token = CancellationToken::new();
        let (warmup_port, warmup_handle) =
            server::spawn_server("127.0.0.1", 0, warmup_token.clone()).expect("warmup server");
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", warmup_port)) {
            let _ = stream.write_all(b"GET /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
        }
        warmup_token.cancel();
        let _ = warmup_handle.join();
        thread::sleep(Duration::from_millis(100));
    }

    // 2. Stabilize thread and handle baselines before starting socket benchmarks
    let mut base_threads = telemetry::get_active_threads();
    loop {
        thread::sleep(Duration::from_millis(150));
        let next = telemetry::get_active_threads();
        if next == base_threads {
            break;
        }
        base_threads = next;
    }

    let mut base_handles_opt = telemetry::get_process_handles();
    if let Some(mut base_h) = base_handles_opt {
        loop {
            thread::sleep(Duration::from_millis(150));
            let next_h = telemetry::get_process_handles().unwrap();
            if next_h == base_h {
                break;
            }
            base_h = next_h;
        }
        base_handles_opt = Some(base_h);
    }

    let shutdown_token = CancellationToken::new();
    let (bound_port, handle) =
        server::spawn_server("127.0.0.1", 0, shutdown_token.clone()).expect("spawn server");

    println!(
        "📊 [BENCHMARK] Socket Lifecycle: Bound ephemeral port {}",
        bound_port
    );

    const REQUESTS: usize = 100;
    for req_idx in 0..REQUESTS {
        let mut stream =
            TcpStream::connect(("127.0.0.1", bound_port)).expect("connect to http server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");

        let request = format!(
            "GET /api/metrics HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            bound_port
        );
        stream
            .write_all(request.as_bytes())
            .expect("write http request");

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read http response");

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "Request {} returned non-200 response: {}",
            req_idx,
            response
        );

        let json_start = response.find('{').expect("find json start");
        let json_end = response.rfind('}').expect("find json end");
        let body = &response[json_start..=json_end];

        let metrics: serde_json::Value =
            serde_json::from_str(body).expect("parse json from /api/metrics");
        assert!(
            metrics["memory"]["rss_bytes"].is_number(),
            "Request {} missing rss_bytes",
            req_idx
        );

        drop(stream);
    }

    // Teardown HTTP server
    shutdown_token.cancel();
    let _ = handle.join();

    // Convergence polling up to 1500ms
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut current_threads = telemetry::get_active_threads();
    while current_threads > base_threads && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
        current_threads = telemetry::get_active_threads();
    }

    assert!(
        current_threads <= base_threads,
        "SOCKET/THREAD LEAK: Active threads ({}) exceeded baseline ({})",
        current_threads, base_threads
    );

    if let Some(base_handles) = base_handles_opt {
        let mut current_handles = telemetry::get_process_handles().unwrap();
        while current_handles > base_handles + 2
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
            current_handles = telemetry::get_process_handles().unwrap();
        }

        let delta = (current_handles as i64) - (base_handles as i64);
        println!(
            "📊 [BENCHMARK] Socket Lifecycle: Baseline Handles={}, Final Handles={}, Delta={}",
            base_handles, current_handles, delta
        );

        assert!(
            current_handles <= base_handles + 2,
            "SOCKET/HANDLE LEAK: Base handles={}, final handles={}, delta={} (allowance <= +2)",
            base_handles, current_handles, delta
        );
    }
}
