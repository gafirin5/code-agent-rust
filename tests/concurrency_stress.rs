#[path = "../src/agent/tasks.rs"]
mod tasks;

use anyhow::anyhow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tasks::{OutputSink, TaskManager, TaskStatus};

// ============================================================================
// STRESS TEST 1: 100 Concurrent Tasks Spawning & Throughput
// ============================================================================

#[test]
fn test_stress_100_concurrent_tasks_throughput() {
    let manager = Arc::new(TaskManager::new());
    const THREAD_COUNT: usize = 10;
    const TASKS_PER_THREAD: usize = 10;
    const TOTAL_TASKS: usize = THREAD_COUNT * TASKS_PER_THREAD;

    let start = Instant::now();
    let all_task_ids = Arc::new(std::sync::Mutex::new(Vec::with_capacity(TOTAL_TASKS)));

    let mut spawn_handles = Vec::new();
    for thread_idx in 0..THREAD_COUNT {
        let mgr = manager.clone();
        let ids_acc = all_task_ids.clone();
        spawn_handles.push(thread::spawn(move || {
            let mut local_ids = Vec::with_capacity(TASKS_PER_THREAD);
            for task_idx in 0..TASKS_PER_THREAD {
                let task_num = thread_idx * TASKS_PER_THREAD + task_idx;
                let (id, _) = mgr
                    .spawn_task(
                        format!("load-{}", task_num),
                        format!("Task payload {}", task_num),
                        move |_| {
                            // Simulate small compute and sleep
                            let mut acc: u64 = 0;
                            for k in 0..10_000 {
                                acc = acc.wrapping_add(k);
                            }
                            thread::sleep(Duration::from_millis(10));
                            Ok(format!("done-{}", task_num))
                        },
                    )
                    .expect("Spawn must succeed");
                local_ids.push(id);
            }
            let mut guard = ids_acc.lock().unwrap();
            guard.extend(local_ids);
        }));
    }

    for h in spawn_handles {
        h.join().unwrap();
    }

    let ids = all_task_ids.lock().unwrap().clone();
    assert_eq!(ids.len(), TOTAL_TASKS);

    // Verify task IDs are unique
    let unique_ids: HashSet<_> = ids.iter().cloned().collect();
    assert_eq!(
        unique_ids.len(),
        TOTAL_TASKS,
        "All task IDs must be globally unique"
    );

    // Await all tasks concurrently with a pool of awaiters
    let mut await_handles = Vec::new();
    for id_chunk in ids.chunks(10) {
        let chunk = id_chunk.to_vec();
        let mgr = manager.clone();
        await_handles.push(thread::spawn(move || {
            for tid in chunk {
                let snap = mgr
                    .await_task(&tid, Some(Duration::from_secs(5)))
                    .expect("Task await must succeed within timeout");
                assert_eq!(snap.status, TaskStatus::Completed);
                assert!(snap.result.is_some());
            }
        }));
    }

    for h in await_handles {
        h.join().unwrap();
    }

    let total_elapsed = start.elapsed();
    println!(
        "[STRESS 1] 100 tasks completed in {:?} (avg {:?} per task)",
        total_elapsed,
        total_elapsed / (TOTAL_TASKS as u32)
    );

    let all_snapshots = manager.list_tasks();
    assert_eq!(all_snapshots.len(), TOTAL_TASKS);
}

// ============================================================================
// STRESS TEST 2: Rapid Cancellation Under Load (Race Window Stress)
// ============================================================================

#[test]
fn test_stress_rapid_cancellation_under_load() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 60;
    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);

    for i in 0..TOTAL_TASKS {
        let (id, _) = manager
            .spawn_task(
                format!("canc-{}", i),
                "rapid cancel target".into(),
                move |token| {
                    for step in 0..50 {
                        if token.is_cancelled() {
                            return Err(anyhow::anyhow!("cancelled at step {}", step));
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Ok("completed normally".into())
                },
            )
            .unwrap();
        task_ids.push(id);
    }

    // Launch cancellation threads with varied delays
    let mut cancel_handles = Vec::new();
    for (i, tid) in task_ids.iter().enumerate() {
        let mgr = manager.clone();
        let id_clone = tid.clone();
        cancel_handles.push(thread::spawn(move || {
            let delay_micros = (i * 200) % 5000;
            if delay_micros > 0 {
                thread::sleep(Duration::from_micros(delay_micros as u64));
            }
            let _ = mgr.cancel_task(&id_clone);
        }));
    }

    for h in cancel_handles {
        h.join().unwrap();
    }

    // Await all tasks and examine outcomes
    let mut cancelled_count = 0;
    let mut completed_count = 0;
    let mut duration_none_count = 0;

    for tid in &task_ids {
        let snap = manager
            .await_task(tid, Some(Duration::from_secs(3)))
            .expect("Task must reach terminal state");
        assert!(
            snap.status.is_terminal(),
            "Task {} must be in terminal state, was {:?}",
            tid,
            snap.status
        );
        match snap.status {
            TaskStatus::Cancelled => {
                cancelled_count += 1;
                if snap.duration_ms.is_none() {
                    duration_none_count += 1;
                }
            }
            TaskStatus::Completed => completed_count += 1,
            other => panic!("Unexpected terminal status: {:?}", other),
        }
    }

    println!(
        "[STRESS 2] Out of {} tasks: {} Cancelled, {} Completed. Cancelled with duration_ms == None: {}",
        TOTAL_TASKS, cancelled_count, completed_count, duration_none_count
    );

    assert!(
        cancelled_count > 0,
        "At least some tasks should have been cancelled"
    );
}

// ============================================================================
// STRESS TEST 3: Simultaneous Multi-Awaiter Stampede (50 Threads on 1 Task)
// ============================================================================

#[test]
fn test_stress_simultaneous_multi_awaiter_stampede() {
    let manager = Arc::new(TaskManager::new());
    const AWAITERS_COUNT: usize = 50;

    let (id, _) = manager
        .spawn_task(
            "single-target".into(),
            "target for 50 awaiters".into(),
            |_| {
                thread::sleep(Duration::from_millis(40));
                Ok("broadcast payload 42".to_string())
            },
        )
        .unwrap();

    let barrier = Arc::new(std::sync::Barrier::new(AWAITERS_COUNT + 1));
    let mut await_handles = Vec::with_capacity(AWAITERS_COUNT);

    for idx in 0..AWAITERS_COUNT {
        let mgr = manager.clone();
        let tid = id.clone();
        let b = barrier.clone();
        await_handles.push(thread::spawn(move || {
            b.wait(); // Synchronize all awaiters to start at exact same instant
            let start = Instant::now();
            let snap = mgr
                .await_task(&tid, Some(Duration::from_secs(10)))
                .expect("Awaiter must successfully receive task completion");
            let elapsed = start.elapsed();
            (idx, snap, elapsed)
        }));
    }

    // Release all 50 awaiters simultaneously
    barrier.wait();

    for h in await_handles {
        let (idx, snap, elapsed) = h.join().unwrap();
        assert_eq!(snap.status, TaskStatus::Completed, "Awaiter {} status", idx);
        assert_eq!(
            snap.result.as_deref(),
            Some("broadcast payload 42"),
            "Awaiter {} result payload",
            idx
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "Awaiter {} took too long: {:?}",
            idx,
            elapsed
        );
    }
}

// ============================================================================
// STRESS TEST 4: High-Contention Chaos Hammer (Zero Deadlocks, Zero Corruption)
// ============================================================================

#[test]
fn test_stress_high_contention_chaos_hammer() {
    let manager = Arc::new(TaskManager::new());
    let stop_signal = Arc::new(AtomicBool::new(false));
    let spawn_counter = Arc::new(AtomicUsize::new(0));

    // Thread 1: Rapid Spawner
    let t1_mgr = manager.clone();
    let t1_stop = stop_signal.clone();
    let t1_counter = spawn_counter.clone();
    let h_spawner = thread::spawn(move || {
        while !t1_stop.load(Ordering::Relaxed) {
            let cnt = t1_counter.fetch_add(1, Ordering::SeqCst);
            let _ = t1_mgr.spawn_task(
                format!("chaos-{}", cnt),
                "chaos task".into(),
                move |token| {
                    if token.is_cancelled() {
                        return Err(anyhow::anyhow!("cancelled early"));
                    }
                    thread::sleep(Duration::from_millis(5));
                    Ok(format!("result-{}", cnt))
                },
            );
            thread::sleep(Duration::from_micros(200));
        }
    });

    // Thread 2: Rapid Canceller
    let t2_mgr = manager.clone();
    let t2_stop = stop_signal.clone();
    let h_canceller = thread::spawn(move || {
        while !t2_stop.load(Ordering::Relaxed) {
            let list = t2_mgr.list_tasks();
            for task in list.into_iter().take(5) {
                if !task.status.is_terminal() {
                    let _ = t2_mgr.cancel_task(&task.id);
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
    });

    // Thread 3: List & Invariant Inspector
    let t3_mgr = manager.clone();
    let t3_stop = stop_signal.clone();
    let h_inspector = thread::spawn(move || {
        while !t3_stop.load(Ordering::Relaxed) {
            let tasks = t3_mgr.list_tasks();
            for t in tasks {
                // Verify invariant: if terminal, duration or finished_at is consistent
                if t.status == TaskStatus::Completed {
                    assert!(t.result.is_some(), "Completed task must have result");
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
    });

    // Thread 4: Pruner & Clear Completed
    let t4_mgr = manager.clone();
    let t4_stop = stop_signal.clone();
    let h_pruner = thread::spawn(move || {
        while !t4_stop.load(Ordering::Relaxed) {
            let _ = t4_mgr.prune_tasks(50);
            let _ = t4_mgr.clear_completed();
            thread::sleep(Duration::from_millis(5));
        }
    });

    // Thread 5: Random Awaiter
    let t5_mgr = manager.clone();
    let t5_stop = stop_signal.clone();
    let h_awaiter = thread::spawn(move || {
        while !t5_stop.load(Ordering::Relaxed) {
            let list = t5_mgr.list_tasks();
            if let Some(target) = list.first() {
                let _ = t5_mgr.await_task(&target.id, Some(Duration::from_millis(10)));
            }
            thread::sleep(Duration::from_millis(2));
        }
    });

    // Run chaos hammer for 2.5 seconds
    thread::sleep(Duration::from_millis(2500));
    stop_signal.store(true, Ordering::SeqCst);

    // Join all threads within reasonable deadline (detect deadlocks)
    let join_start = Instant::now();
    h_spawner.join().expect("Spawner must terminate");
    h_canceller.join().expect("Canceller must terminate");
    h_inspector.join().expect("Inspector must terminate");
    h_pruner.join().expect("Pruner must terminate");
    h_awaiter.join().expect("Awaiter must terminate");

    let join_time = join_start.elapsed();
    println!(
        "[STRESS 4] Chaos hammer finished gracefully in {:?}. Total tasks spawned: {}",
        join_time,
        spawn_counter.load(Ordering::SeqCst)
    );
    assert!(
        join_time < Duration::from_secs(5),
        "All threads must join cleanly without deadlocks"
    );
}

// ============================================================================
// STRESS TEST 5: Panic Catch Unwind & Lock Recovery Under High Load
// ============================================================================

#[test]
fn test_stress_panic_catch_unwind_resilience() {
    let manager = Arc::new(TaskManager::new());
    const PANIC_TASKS: usize = 20;

    let mut ids = Vec::new();
    for i in 0..PANIC_TASKS {
        let (id, _) = manager
            .spawn_task(
                format!("panic-{}", i),
                "deliberate panic".into(),
                move |_| {
                    if i % 2 == 0 {
                        panic!("static str panic from task {}", i);
                    } else {
                        panic!("{}", format!("heap string panic from task {}", i));
                    }
                },
            )
            .unwrap();
        ids.push(id);
    }

    for id in &ids {
        let snap = manager
            .await_task(id, Some(Duration::from_secs(2)))
            .expect("Await must unblock on panicked worker thread");
        assert_eq!(snap.status, TaskStatus::Failed);
        assert!(snap.result.is_none());
        assert!(
            snap.error.as_ref().unwrap().contains("Panicked:"),
            "Error should contain 'Panicked:', got {:?}",
            snap.error
        );
    }

    // Registry must remain operational after 20 panics
    let (normal_id, _) = manager
        .spawn_task("normal".into(), "normal task".into(), |_| {
            Ok("all systems operational".into())
        })
        .unwrap();

    let normal_snap = manager.await_task(&normal_id, None).unwrap();
    assert_eq!(normal_snap.status, TaskStatus::Completed);
    assert_eq!(
        normal_snap.result.as_deref(),
        Some("all systems operational")
    );
}

// ============================================================================
// STRESS TEST 6: Concurrent Notification Drain Zero Duplicates
// ============================================================================

#[test]
fn test_stress_concurrent_notification_drain_zero_duplicates() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 60;
    const DRAIN_THREADS: usize = 8;

    // Spawn 60 tasks with varied durations and some intentional errors
    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);
    for i in 0..TOTAL_TASKS {
        let (id, _) = manager
            .spawn_task(
                format!("notif-task-{}", i),
                format!("Payload {}", i),
                move |_| {
                    if i % 7 == 0 {
                        thread::sleep(Duration::from_millis(5));
                        Err(anyhow!("intentional error"))
                    } else {
                        thread::sleep(Duration::from_millis(10));
                        Ok(format!("done {}", i))
                    }
                },
            )
            .unwrap();
        task_ids.push(id);
    }

    let running = Arc::new(AtomicBool::new(true));
    let collected_snapshots = Arc::new(std::sync::Mutex::new(Vec::new()));

    let mut drain_handles = Vec::new();
    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let collected = collected_snapshots.clone();
        drain_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    let mut lock = collected.lock().unwrap();
                    lock.extend(drained);
                }
                thread::sleep(Duration::from_millis(2));
            }
            // Final drain pass
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                let mut lock = collected.lock().unwrap();
                lock.extend(drained);
            }
        }));
    }

    // Wait for all tasks to reach terminal state
    for id in &task_ids {
        let snap = manager
            .await_task(id, Some(Duration::from_secs(5)))
            .expect("Task should complete");
        assert!(snap.status.is_terminal());
    }

    // Allow drain threads to process remaining unnotified tasks
    let start_wait = Instant::now();
    loop {
        let count = collected_snapshots.lock().unwrap().len();
        if count == TOTAL_TASKS || start_wait.elapsed() > Duration::from_secs(15) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Stop drain threads
    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }

    let final_snaps = collected_snapshots.lock().unwrap().clone();
    assert_eq!(
        final_snaps.len(),
        TOTAL_TASKS,
        "Every terminal task must be drained exactly once"
    );

    // Verify zero duplicates
    let mut seen_ids = HashSet::new();
    for snap in &final_snaps {
        assert!(
            seen_ids.insert(snap.id.clone()),
            "Duplicate notification drained for task {}",
            snap.id
        );
        assert!(snap.notified, "Drained snapshot must be marked notified");
        assert!(
            snap.status.is_terminal(),
            "Drained snapshot must be terminal"
        );
    }

    // Verify subsequent drain is empty
    let extra_drained = manager.drain_unnotified_terminal_tasks();
    assert!(
        extra_drained.is_empty(),
        "No further tasks should be unnotified after all drained"
    );
}

// ============================================================================
// STRESS TEST 7: Interleaved Wait/Cancel Notification Suppression vs Concurrent Drain
// ============================================================================

#[test]
fn test_stress_tasks_wait_and_cancel_marking_concurrency() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 40;
    const DRAIN_THREADS: usize = 4;

    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);
    for i in 0..TOTAL_TASKS {
        let (id, _) = manager
            .spawn_task(
                format!("interleave-{}", i),
                format!("Payload {}", i),
                move |cancel_tok| {
                    for _ in 0..50 {
                        if cancel_tok.is_cancelled() {
                            return Err(anyhow!("Cancelled by user"));
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Ok(format!("completed-{}", i))
                },
            )
            .unwrap();
        task_ids.push(id);
    }

    let manually_notified = Arc::new(std::sync::Mutex::new(HashSet::new()));
    let drained_snapshots = Arc::new(std::sync::Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));

    // Spawn background drainers
    let mut drain_handles = Vec::new();
    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let drained_acc = drained_snapshots.clone();
        drain_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    let mut lock = drained_acc.lock().unwrap();
                    lock.extend(drained);
                }
                thread::sleep(Duration::from_millis(3));
            }
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                let mut lock = drained_acc.lock().unwrap();
                lock.extend(drained);
            }
        }));
    }

    // Concurrently cancel the first 10 tasks and mark them notified
    for id in &task_ids[0..10] {
        let _ = manager.cancel_task(id);
        if manager.mark_task_notified(id) {
            manually_notified.lock().unwrap().insert(id.clone());
        }
    }

    // Await the next 10 tasks and mark them notified
    for id in &task_ids[10..20] {
        let _ = manager.await_task(id, Some(Duration::from_secs(3)));
        if manager.mark_task_notified(id) {
            manually_notified.lock().unwrap().insert(id.clone());
        }
    }

    // Remaining 20 tasks (indices 20..40) finish naturally and must be drained by drain threads
    for id in &task_ids[20..40] {
        let _ = manager.await_task(id, Some(Duration::from_secs(3)));
    }

    // Wait until drainers capture remaining unnotified tasks
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let drained_count = drained_snapshots.lock().unwrap().len();
        let manual_count = manually_notified.lock().unwrap().len();
        if drained_count + manual_count >= TOTAL_TASKS {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }

    let manual = manually_notified.lock().unwrap().clone();
    let drained = drained_snapshots.lock().unwrap().clone();

    // Verify zero intersection between manually notified and drained tasks
    let mut drained_set = HashSet::new();
    for snap in &drained {
        assert!(
            drained_set.insert(snap.id.clone()),
            "Duplicate notification drained for task {}",
            snap.id
        );
        assert!(
            !manual.contains(&snap.id),
            "Task {} was manually marked notified but was also drained!",
            snap.id
        );
    }

    // Union of manual + drained must cover all tasks
    let total_accounted = manual.len() + drained_set.len();
    assert_eq!(
        total_accounted, TOTAL_TASKS,
        "All {} tasks must be accounted for by either manual notification or drain (manual={}, drained={})",
        TOTAL_TASKS, manual.len(), drained_set.len()
    );
}

// ============================================================================
// STRESS TEST 8: Silent Subagent Output Isolation & Concurrent Log Buffering
// ============================================================================

#[test]
fn test_stress_silent_subagent_log_buffering_and_retrieval() {
    let manager = Arc::new(TaskManager::new());
    const WORKER_COUNT: usize = 10;
    const LINES_PER_WORKER: usize = 100;
    const READER_COUNT: usize = 4;

    let (task_id, _cancel_token, log_buffer) = manager
        .spawn_task_with_sink(
            "subagent-stress".into(),
            "Heavy logging subagent".into(),
            move |_cancel_tok, _logs| {
                // Task placeholder
                Ok("completed".into())
            },
        )
        .unwrap();

    let sink = OutputSink::Buffered(log_buffer.clone());
    assert!(
        sink.is_silent(),
        "Buffered sink must report is_silent == true"
    );

    let done = Arc::new(AtomicBool::new(false));

    // Spawn 10 concurrent writer threads pushing logs to log_buffer
    let mut writer_handles = Vec::new();
    for w in 0..WORKER_COUNT {
        let buf = log_buffer.clone();
        writer_handles.push(thread::spawn(move || {
            for line_idx in 0..LINES_PER_WORKER {
                buf.push(format!("[worker-{}] step {} log output", w, line_idx));
                if line_idx % 20 == 0 {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }));
    }

    // Spawn 4 concurrent reader threads fetching tail and lines
    let mut reader_handles = Vec::new();
    for _ in 0..READER_COUNT {
        let buf = log_buffer.clone();
        let done_flag = done.clone();
        reader_handles.push(thread::spawn(move || {
            let mut read_ops = 0;
            while !done_flag.load(Ordering::Relaxed) {
                let _ = buf.lines();
                let _ = buf.tail(20);
                let _ = buf.formatted();
                let _ = buf.len();
                read_ops += 1;
                thread::sleep(Duration::from_millis(1));
            }
            read_ops
        }));
    }

    // Wait for all writers to complete
    for h in writer_handles {
        h.join().unwrap();
    }

    done.store(true, Ordering::Relaxed);

    for h in reader_handles {
        let ops = h.join().unwrap();
        assert!(
            ops > 0,
            "Reader thread should have performed read operations"
        );
    }

    assert_eq!(
        log_buffer.len(),
        WORKER_COUNT * LINES_PER_WORKER,
        "Log buffer must contain exactly all written lines"
    );

    let lines = log_buffer.lines();
    assert_eq!(lines.len(), WORKER_COUNT * LINES_PER_WORKER);

    let tail_lines = log_buffer.tail(15);
    assert_eq!(tail_lines.len(), 15);

    let formatted = log_buffer.formatted();
    assert!(!formatted.is_empty());

    let snap = manager
        .await_task(&task_id, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(snap.status, TaskStatus::Completed);
}
