#[path = "../src/agent/tasks.rs"]
mod tasks;

use anyhow::anyhow;
use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tasks::{OutputSink, TaskLogBuffer, TaskManager, TaskStatus};

// ============================================================================
// SUITE 1: EMPIRICAL OS-LEVEL STDOUT / STDERR SILENCE HARNESS
// ============================================================================

/// Worker routine executed when invoked as a child process.
/// Runs intensive logging, spinners, clears, tools, errors, and cancellations
/// through `OutputSink::Buffered(logs)` and `TaskManager::spawn_task_with_sink`.
/// Must produce ZERO bytes of stdout and ZERO escape codes.
fn run_subprocess_silence_worker() {
    let manager = Arc::new(TaskManager::new());
    let (task_id, _cancel_token, logs) = manager
        .spawn_task_with_sink(
            "silent-worker".into(),
            "Empirical silence test worker".into(),
            |token, task_logs| {
                let sink = OutputSink::Buffered(task_logs.clone());
                assert!(sink.is_silent());

                // 1. Emit spinners and clears (must be 100% silent)
                for _ in 0..100 {
                    sink.emit_spinner("✦ Thinking...");
                    sink.clear_spinner();
                }

                // 2. Emit logs
                for i in 0..200 {
                    sink.emit(&format!("Task log line {}", i));
                    sink.log(format!("Convenience log line {}", i));
                }

                // 3. Check cancellation
                if token.is_cancelled() {
                    return Err(anyhow!("Cancelled"));
                }

                Ok("worker-completed".into())
            },
        )
        .expect("Spawn must succeed");

    // Also directly invoke OutputSink::Buffered in this thread
    let direct_buf = Arc::new(TaskLogBuffer::new());
    let direct_sink = OutputSink::Buffered(direct_buf.clone());

    for _ in 0..100 {
        direct_sink.emit_spinner("\r\x1B[36m✦\x1B[0m \x1B[90mThinking...\x1B[0m");
        direct_sink.clear_spinner();
        direct_sink.emit("Direct buffered emission line");
    }

    // Await the task to completion
    let snap = manager
        .await_task(&task_id, Some(Duration::from_secs(5)))
        .expect("Await must succeed");
    assert_eq!(snap.status, TaskStatus::Completed);

    // Verify logs were properly retained in the buffer
    assert_eq!(logs.len(), 400); // 200 emit + 200 log
    assert_eq!(direct_buf.len(), 100);

    // Exit cleanly without printing anything
}

#[test]
fn test_empirical_subprocess_zero_stdout_and_zero_escape_codes() {
    // If we are the child worker, execute the worker routine and exit
    if std::env::var("CTRL_CLI_SILENCE_WORKER").is_ok() {
        run_subprocess_silence_worker();
        return;
    }

    // Parent process: spawn child process running this exact test with the environment variable set
    let current_exe = std::env::current_exe().expect("Must locate current test binary");
    let output = Command::new(current_exe)
        .arg("--nocapture")
        .arg("test_empirical_subprocess_zero_stdout_and_zero_escape_codes")
        .env("CTRL_CLI_SILENCE_WORKER", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute child test process");

    assert!(
        output.status.success(),
        "Child process failed with status {:?}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);

    // Filter out standard test harness banners like "running 1 test\ntest ... ok"
    // to strictly check if any worker log lines, spinners, or escape codes leaked.
    let forbidden_patterns = [
        "\r\x1B[K",
        "\x1B[K",
        "Thinking...",
        "Task log line",
        "Convenience log line",
        "Direct buffered emission line",
        "worker-completed",
        "Empirical silence test worker",
    ];

    for pattern in &forbidden_patterns {
        assert!(
            !stdout_str.contains(pattern),
            "LEAK DETECTED in stdout! Found pattern {:?} in child process stdout:\n{}",
            pattern,
            stdout_str
        );
        assert!(
            !stderr_str.contains(pattern),
            "LEAK DETECTED in stderr! Found pattern {:?} in child process stderr:\n{}",
            pattern,
            stderr_str
        );
    }
}

// ============================================================================
// SUITE 2: OUTPUTSINK ISOLATION CONTRACT & SPINNER SUPPRESSION
// ============================================================================

#[test]
fn test_output_sink_buffered_suppresses_spinners_completely() {
    let buffer = Arc::new(TaskLogBuffer::new());
    let sink = OutputSink::buffered(buffer.clone());

    assert!(sink.is_silent());
    assert!(sink.buffer().is_some());

    // Calling emit_spinner and clear_spinner must not affect buffer
    sink.emit_spinner("Test spinner text");
    sink.clear_spinner();
    assert_eq!(buffer.len(), 0, "Spinners must not write to log buffer");

    // Regular emit must write to buffer
    sink.emit("First real log line");
    sink.log("Second real log line");
    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.lines()[0], "First real log line");
    assert_eq!(buffer.lines()[1], "Second real log line");
}

#[test]
fn test_output_sink_terminal_contracts() {
    let sink = OutputSink::Terminal;
    assert!(!sink.is_silent());
    assert!(sink.buffer().is_none());
}

// ============================================================================
// SUITE 3: LOG RETENTION UNDER DIVERSE FAILURE MODES & EDGE CASES
// ============================================================================

#[test]
fn test_log_retention_empty_task() {
    let manager = TaskManager::new();
    let (id, _token, logs) = manager
        .spawn_task_with_sink("empty".into(), "no logs emitted".into(), |_token, _logs| {
            Ok("done".into())
        })
        .unwrap();

    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Completed);

    assert_eq!(logs.len(), 0);
    assert!(logs.is_empty());
    assert_eq!(logs.lines(), Vec::<String>::new());
    assert_eq!(logs.text(), "");
    assert_eq!(logs.formatted(), "");
    assert_eq!(logs.tail(10), Vec::<String>::new());
    assert_eq!(logs.get_lines(0, 10), Vec::<String>::new());

    let queried_logs = manager.get_task_logs(&id).expect("Logs must exist");
    assert!(queried_logs.is_empty());
}

#[test]
fn test_log_retention_failing_task_preserves_all_diagnostics() {
    let manager = TaskManager::new();
    const LOG_COUNT: usize = 75;

    let (id, _token, logs) = manager
        .spawn_task_with_sink(
            "failing-task".into(),
            "Task that fails after logging".into(),
            |_token, task_logs| {
                for i in 0..LOG_COUNT {
                    task_logs.push(format!("Step {}: diagnostic log message", i));
                }
                Err(anyhow!("Network timeout connecting to upstream provider"))
            },
        )
        .unwrap();

    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);
    assert!(snap.result.is_none());
    assert!(snap.error.is_some());
    assert!(snap.error.unwrap().contains("Network timeout"));

    // Verify 100% of logs are preserved
    assert_eq!(logs.len(), LOG_COUNT);
    let queried = manager.get_task_logs(&id).expect("Logs must be retrieved");
    assert_eq!(queried.len(), LOG_COUNT);
    for (i, line) in queried.iter().enumerate() {
        assert_eq!(line, &format!("Step {}: diagnostic log message", i));
    }
}

#[test]
fn test_log_retention_cooperative_cancellation_mid_turn() {
    let manager = Arc::new(TaskManager::new());
    const LOGS_BEFORE_CANCEL: usize = 42;

    let started_logging = Arc::new(AtomicBool::new(false));
    let started_logging_clone = started_logging.clone();

    let (id, _token, logs) = manager
        .spawn_task_with_sink(
            "cancelled-task".into(),
            "Task cancelled mid-execution".into(),
            move |cancel_tok, task_logs| {
                for i in 0..LOGS_BEFORE_CANCEL {
                    task_logs.push(format!("Pre-cancel progress {}", i));
                }
                started_logging_clone.store(true, Ordering::SeqCst);

                // Wait until cancellation signal is delivered
                while !cancel_tok.is_cancelled() {
                    thread::sleep(Duration::from_millis(2));
                }
                task_logs.push("Received cancellation signal, shutting down cleanly.");
                Err(anyhow!("Operation cancelled by user"))
            },
        )
        .unwrap();

    // Ensure task has started running and logged its pre-cancel progress
    let _ = manager.await_running(&id, Some(Duration::from_secs(5))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started_logging.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }

    assert!(manager.cancel_task(&id).is_ok());

    let snap = manager.await_task(&id, Some(Duration::from_secs(5))).unwrap();
    assert_eq!(snap.status, TaskStatus::Cancelled);

    // Verify all logs including the shutdown log are preserved in log buffer
    assert_eq!(logs.len(), LOGS_BEFORE_CANCEL + 1);
    let queried = manager.get_task_logs(&id).expect("Logs must be retrieved");
    assert_eq!(queried.len(), LOGS_BEFORE_CANCEL + 1);
    assert_eq!(
        queried.last().unwrap(),
        "Received cancellation signal, shutting down cleanly."
    );
}

#[test]
fn test_log_retention_panicking_worker_resilience() {
    let manager = TaskManager::new();
    const LOGS_BEFORE_PANIC: usize = 25;

    let (id, _token, logs) = manager
        .spawn_task_with_sink(
            "panicking-task".into(),
            "Task that panics after logging".into(),
            |_token, task_logs| {
                for i in 0..LOGS_BEFORE_PANIC {
                    task_logs.push(format!("Pre-panic step {}", i));
                }
                panic!("Fatal unexpected assertion failure in worker!");
            },
        )
        .unwrap();

    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);
    assert!(snap.error.as_ref().unwrap().contains("Panicked"));

    // Verify logs written before panic are preserved
    assert_eq!(logs.len(), LOGS_BEFORE_PANIC);
    let queried = manager.get_task_logs(&id).expect("Logs must be retrieved");
    assert_eq!(queried.len(), LOGS_BEFORE_PANIC);
    assert_eq!(queried[0], "Pre-panic step 0");
    assert_eq!(queried[LOGS_BEFORE_PANIC - 1], format!("Pre-panic step {}", LOGS_BEFORE_PANIC - 1));
}

// ============================================================================
// SUITE 4: EXTREME PAYLOAD, PAGINATION, AND MASSIVE LOG STRESS
// ============================================================================

#[test]
fn test_massive_log_lines_stress_100k() {
    let buffer = TaskLogBuffer::new();
    const TOTAL_LINES: usize = 100_000;

    let start = Instant::now();
    for i in 0..TOTAL_LINES {
        buffer.push(format!("Log line sequence index {:06} [payload data]", i));
    }
    let elapsed = start.elapsed();
    println!("Appended {} log lines in {:?}", TOTAL_LINES, elapsed);

    assert_eq!(buffer.len(), TOTAL_LINES);
    assert!(!buffer.is_empty());

    // Tail inspection
    let tail_50 = buffer.tail(50);
    assert_eq!(tail_50.len(), 50);
    assert_eq!(
        tail_50.last().unwrap(),
        &format!("Log line sequence index {:06} [payload data]", TOTAL_LINES - 1)
    );

    // Paginated slice inspection
    let slice = buffer.get_lines(99_950, 50);
    assert_eq!(slice, tail_50);

    // Boundary conditions
    assert_eq!(buffer.get_lines(TOTAL_LINES, 10), Vec::<String>::new());
    assert_eq!(buffer.get_lines(TOTAL_LINES + 500, 10), Vec::<String>::new());
    assert_eq!(buffer.get_lines(0, 0), Vec::<String>::new());

    // Overflow protection: offset + limit exceeding total
    let overflow_slice = buffer.get_lines(TOTAL_LINES - 10, 100);
    assert_eq!(overflow_slice.len(), 10);

    // Clear buffer
    buffer.clear();
    assert_eq!(buffer.len(), 0);
    assert!(buffer.is_empty());
}

#[test]
fn test_special_characters_multiline_and_unicode_logs() {
    let buffer = TaskLogBuffer::new();

    let complex_entries = vec![
        "Line with emoji: 🚀 🤖 ⚡ 🧹 ✔ ✖ ❓",
        "Line with ANSI escapes: \x1B[1;36mBold Cyan\x1B[0m \x1B[32mGreen\x1B[0m",
        "Line with JSON payload: {\"status\": \"ok\", \"count\": 42, \"nested\": {\"key\": \"value\"}}",
        "Line with tabs and special symbols: \t\t--> <-- @ # $ % ^ & * ( ) _ +",
        "Indonesian text: Notifikasi inter-turn background subagent berhasil dialihkan secara silent.",
        "Japanese text: バックグラウンドサブエージェントのログバッファ分離テスト",
        "Empty line next:",
        "",
        "Line after empty line",
    ];

    for entry in &complex_entries {
        buffer.push(*entry);
    }

    assert_eq!(buffer.len(), complex_entries.len());
    let retrieved = buffer.lines();
    for (i, entry) in complex_entries.iter().enumerate() {
        assert_eq!(&retrieved[i], entry);
    }

    let joined = buffer.text();
    for entry in &complex_entries {
        assert!(joined.contains(entry));
    }
}

// ============================================================================
// SUITE 5: HIGH-CONCURRENCY CONTENTION STRESS (READ/WRITE RACE TESTING)
// ============================================================================

#[test]
fn test_high_contention_concurrent_log_buffer_stress() {
    let buffer = Arc::new(TaskLogBuffer::new());
    const WRITERS: usize = 20;
    const LINES_PER_WRITER: usize = 500;
    const READERS: usize = 10;
    const EXPECTED_TOTAL: usize = WRITERS * LINES_PER_WRITER;

    let running = Arc::new(AtomicBool::new(true));

    // Spawn 10 concurrent readers calling lines(), tail(), formatted(), len(), is_empty()
    let mut reader_handles = Vec::new();
    for _ in 0..READERS {
        let buf = buffer.clone();
        let run = running.clone();
        reader_handles.push(thread::spawn(move || {
            let mut read_count = 0;
            while run.load(Ordering::Relaxed) {
                let _ = buf.len();
                let _ = buf.is_empty();
                let _ = buf.tail(20);
                let _ = buf.get_lines(0, 10);
                let _ = buf.formatted();
                read_count += 1;
                thread::sleep(Duration::from_micros(100));
            }
            read_count
        }));
    }

    // Spawn 20 concurrent writers pushing logs
    let mut writer_handles = Vec::new();
    for w in 0..WRITERS {
        let buf = buffer.clone();
        writer_handles.push(thread::spawn(move || {
            for i in 0..LINES_PER_WRITER {
                buf.push(format!("writer-{:02} message index {:04}", w, i));
            }
        }));
    }

    // Await all writers
    for h in writer_handles {
        h.join().unwrap();
    }

    running.store(false, Ordering::Relaxed);

    // Await all readers
    for h in reader_handles {
        let count = h.join().unwrap();
        assert!(count > 0);
    }

    assert_eq!(buffer.len(), EXPECTED_TOTAL);
    let all = buffer.lines();
    assert_eq!(all.len(), EXPECTED_TOTAL);

    // Verify all writers' lines are present
    let mut counts_per_writer = vec![0; WRITERS];
    for line in all {
        if line.starts_with("writer-") {
            let writer_id: usize = line[7..9].parse().unwrap();
            counts_per_writer[writer_id] += 1;
        }
    }

    for (w, count) in counts_per_writer.iter().enumerate() {
        assert_eq!(*count, LINES_PER_WRITER, "Writer {} missed lines!", w);
    }
}

// ============================================================================
// SUITE 6: INTER-TURN NOTIFICATION FORMATTING & ZERO LEAK VALIDATION
// ============================================================================

#[test]
fn test_notification_formatting_tty_and_plain_text() {
    let manager = TaskManager::new();

    // 1. Completed task
    let (c_id, _tok, _) = manager
        .spawn_task_with_sink("comp-task".into(), "completed description".into(), |_tok, _logs| {
            Ok("result".into())
        })
        .unwrap();
    let comp_snap = manager.await_task(&c_id, None).unwrap();

    let colored_comp = comp_snap.format_notification(true);
    assert!(colored_comp.contains("[COMPLETED]"));
    assert!(colored_comp.contains("\x1B[1;32m")); // Green badge
    assert!(colored_comp.contains("comp-task"));

    let plain_comp = comp_snap.format_notification(false);
    assert!(plain_comp.contains("[COMPLETED]"));
    assert!(!plain_comp.contains("\x1B"), "Plain notification must contain ZERO ANSI escape codes!");
    assert!(!plain_comp.contains("\r"), "Plain notification must contain ZERO carriage returns!");

    // 2. Failed task
    let (f_id, _tok, _) = manager
        .spawn_task_with_sink("fail-task".into(), "failed description".into(), |_tok, _logs| {
            Err(anyhow!("bad error"))
        })
        .unwrap();
    let fail_snap = manager.await_task(&f_id, None).unwrap();

    let colored_fail = fail_snap.format_notification(true);
    assert!(colored_fail.contains("[FAILED]"));
    assert!(colored_fail.contains("\x1B[1;31m")); // Red badge

    let plain_fail = fail_snap.format_notification(false);
    assert!(plain_fail.contains("[FAILED]"));
    assert!(!plain_fail.contains("\x1B"));
    assert!(!plain_fail.contains("\r"));

    // 3. Cancelled task
    let (cn_id, _tok, _) = manager
        .spawn_task_with_sink("canc-task".into(), "cancelled description".into(), |token, _logs| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            Err(anyhow!("cancelled"))
        })
        .unwrap();
    manager.cancel_task(&cn_id).unwrap();
    let canc_snap = manager.await_task(&cn_id, None).unwrap();

    let colored_canc = canc_snap.format_notification(true);
    assert!(colored_canc.contains("[CANCELLED]"));
    assert!(colored_canc.contains("\x1B[1;35m")); // Magenta badge for cancelled

    let plain_canc = canc_snap.format_notification(false);
    assert!(plain_canc.contains("[CANCELLED]"));
    assert!(!plain_canc.contains("\x1B"));
    assert!(!plain_canc.contains("\r"));
}

#[test]
fn test_atomic_drain_deduplication_under_high_concurrency() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 60;
    const DRAIN_THREADS: usize = 6;

    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);
    for i in 0..TOTAL_TASKS {
        let (id, _tok, _) = manager
            .spawn_task_with_sink(
                format!("task-{}", i),
                format!("Task payload {}", i),
                move |_tok, _logs| {
                    thread::sleep(Duration::from_millis(5));
                    Ok(format!("done-{}", i))
                },
            )
            .unwrap();
        task_ids.push(id);
    }

    // Await all tasks to terminal state
    for id in &task_ids {
        let snap = manager.await_task(id, Some(Duration::from_secs(5))).unwrap();
        assert!(snap.status.is_terminal());
    }

    let collected_drains = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let coll = collected_drains.clone();
        handles.push(thread::spawn(move || {
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                let mut lock = coll.lock().unwrap();
                lock.extend(drained);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let all_drained = collected_drains.lock().unwrap().clone();
    assert_eq!(
        all_drained.len(),
        TOTAL_TASKS,
        "Total drained tasks must exactly equal total spawned tasks"
    );

    // Verify zero duplicates
    let mut seen = HashSet::new();
    for snap in &all_drained {
        assert!(
            seen.insert(snap.id.clone()),
            "Duplicate task notification drained: {}",
            snap.id
        );
        assert!(snap.notified);
    }

    // Subsequent drain must be strictly empty
    let empty_drain = manager.drain_unnotified_terminal_tasks();
    assert!(empty_drain.is_empty());
}
