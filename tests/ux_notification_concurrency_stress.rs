#[path = "../src/agent/tasks.rs"]
mod tasks;

use std::collections::HashSet;
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use tasks::{TaskManager, TaskStatus};

// ============================================================================
// STRESS TEST 1: Stampede Drain Under High Concurrency (Zero Duplicates / Zero Drops)
// ============================================================================

#[test]
fn test_ux_stampede_drain_zero_duplicates_high_contention() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 200;
    const SPAWNER_THREADS: usize = 20;
    const TASKS_PER_SPAWNER: usize = TOTAL_TASKS / SPAWNER_THREADS;
    const DRAIN_THREADS: usize = 16;

    let active_spawners = Arc::new(AtomicUsize::new(SPAWNER_THREADS));
    let running = Arc::new(AtomicBool::new(true));
    let drained_snapshots = Arc::new(std::sync::Mutex::new(Vec::with_capacity(TOTAL_TASKS)));
    let all_spawned_ids = Arc::new(std::sync::Mutex::new(Vec::with_capacity(TOTAL_TASKS)));

    // 1. Launch 16 concurrent drain threads pounding drain_unnotified_terminal_tasks()
    let mut drain_handles = Vec::with_capacity(DRAIN_THREADS);
    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let drained_acc = drained_snapshots.clone();
        drain_handles.push(thread::spawn(move || {
            let mut local_drained = Vec::new();
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    local_drained.extend(drained);
                }
                // Yield or minimal sleep to maximize interleaving
                thread::yield_now();
            }
            // Final drain sweep
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                local_drained.extend(drained);
            }
            let mut lock = drained_acc.lock().unwrap();
            lock.extend(local_drained);
        }));
    }

    // 2. Launch 20 concurrent spawner threads producing 200 tasks total
    let mut spawner_handles = Vec::with_capacity(SPAWNER_THREADS);
    for spawner_idx in 0..SPAWNER_THREADS {
        let mgr = manager.clone();
        let spawners_left = active_spawners.clone();
        let id_sink = all_spawned_ids.clone();
        spawner_handles.push(thread::spawn(move || {
            let mut local_ids = Vec::with_capacity(TASKS_PER_SPAWNER);
            for i in 0..TASKS_PER_SPAWNER {
                let task_num = spawner_idx * TASKS_PER_SPAWNER + i;
                let sleep_ms = (task_num % 15) as u64;
                let (id, _) = mgr
                    .spawn_task(
                        format!("stampede-worker-{}", task_num),
                        format!("Payload calculation {}", task_num),
                        move |_cancel_tok| {
                            if sleep_ms > 0 {
                                thread::sleep(Duration::from_millis(sleep_ms));
                            }
                            Ok(format!("output-{}", task_num))
                        },
                    )
                    .expect("Task spawn must succeed");
                local_ids.push(id);
            }
            id_sink.lock().unwrap().extend(local_ids);
            spawners_left.fetch_sub(1, Ordering::SeqCst);
        }));
    }

    // Wait for all producers to finish spawning
    for h in spawner_handles {
        h.join().unwrap();
    }

    let spawned_ids = all_spawned_ids.lock().unwrap().clone();
    assert_eq!(spawned_ids.len(), TOTAL_TASKS);

    // Wait for all 200 tasks to complete in background
    for id in &spawned_ids {
        let snap = manager
            .await_task(id, Some(Duration::from_secs(10)))
            .expect("Task await must succeed within timeout");
        assert!(snap.status.is_terminal());
    }

    // Allow drain threads to process any remaining tasks
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let count = drained_snapshots.lock().unwrap().len();
        if count >= TOTAL_TASKS {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    // Stop drain threads
    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }

    let all_drained = drained_snapshots.lock().unwrap().clone();

    // Verification 1: Exactly TOTAL_TASKS drained
    assert_eq!(
        all_drained.len(),
        TOTAL_TASKS,
        "Total drained tasks must exactly equal total spawned tasks"
    );

    // Verification 2: Zero duplicates (every task drained once and only once)
    let mut seen_ids = HashSet::new();
    for snap in &all_drained {
        assert!(
            seen_ids.insert(snap.id.clone()),
            "Duplicate notification drained for task ID '{}'",
            snap.id
        );
        assert!(snap.notified, "Drained snapshot must be marked notified");
        assert!(snap.status.is_terminal(), "Drained snapshot must be in a terminal state");
        assert_eq!(snap.status, TaskStatus::Completed);
    }

    // Verification 3: Zero drops (set of drained IDs equals set of spawned IDs)
    let spawned_set: HashSet<String> = spawned_ids.into_iter().collect();
    assert_eq!(seen_ids, spawned_set, "Drained task set must match spawned task set");

    // Verification 4: Post-drain idempotency
    let post_drain = manager.drain_unnotified_terminal_tasks();
    assert!(
        post_drain.is_empty(),
        "Subsequent drain must return empty vector once all tasks are drained"
    );
}

// ============================================================================
// STRESS TEST 2: Stampede Drain With Mixed Terminal States and Worker Panics
// ============================================================================

#[test]
fn test_ux_stampede_drain_mixed_terminal_states_and_panics() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 160;
    const DRAIN_THREADS: usize = 8;

    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);

    // Cohort 1: 40 tasks that complete successfully
    for i in 0..40 {
        let (id, _) = manager
            .spawn_task(
                format!("success-{}", i),
                format!("Success payload {}", i),
                move |_| {
                    thread::sleep(Duration::from_millis(2));
                    Ok(format!("done-{}", i))
                },
            )
            .unwrap();
        task_ids.push((id, TaskStatus::Completed));
    }

    // Cohort 2: 40 tasks that return Err (Failed)
    for i in 0..40 {
        let (id, _) = manager
            .spawn_task(
                format!("fail-{}", i),
                format!("Failing payload {}", i),
                move |_| {
                    thread::sleep(Duration::from_millis(2));
                    Err(anyhow!("Explicit failure {}", i))
                },
            )
            .unwrap();
        task_ids.push((id, TaskStatus::Failed));
    }

    // Cohort 3: 40 tasks that get cancelled mid-execution (Cancelled)
    for i in 0..40 {
        let (id, token) = manager
            .spawn_task(
                format!("cancel-{}", i),
                format!("Cancellable payload {}", i),
                move |tok| {
                    while !tok.is_cancelled() {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(anyhow!("Cancelled cooperatively"))
                },
            )
            .unwrap();
        // Cancel shortly after spawn
        let tok_clone = token.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            tok_clone.cancel();
        });
        task_ids.push((id, TaskStatus::Cancelled));
    }

    // Cohort 4: 40 tasks where workers panic (Failed via catch_unwind)
    for i in 0..40 {
        let (id, _) = manager
            .spawn_task(
                format!("panic-{}", i),
                format!("Panicking payload {}", i),
                move |_| {
                    thread::sleep(Duration::from_millis(2));
                    if i % 3 == 0 {
                        panic!("Heap string panic payload from worker {}", i);
                    } else if i % 3 == 1 {
                        panic!("static str panic from worker {}", i);
                    } else {
                        // Non-string panic
                        panic::panic_any(42usize);
                    }
                },
            )
            .unwrap();
        task_ids.push((id, TaskStatus::Failed));
    }

    // Launch drain threads
    let running = Arc::new(AtomicBool::new(true));
    let drained_acc = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut drain_handles = Vec::new();

    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let acc = drained_acc.clone();
        drain_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    acc.lock().unwrap().extend(drained);
                }
                thread::sleep(Duration::from_millis(1));
            }
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                acc.lock().unwrap().extend(drained);
            }
        }));
    }

    // Await all tasks to finish
    for (id, _) in &task_ids {
        let _ = manager.await_task(id, Some(Duration::from_secs(5)));
    }

    // Wait until all 160 are collected
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if drained_acc.lock().unwrap().len() >= TOTAL_TASKS {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }

    let collected = drained_acc.lock().unwrap().clone();
    assert_eq!(collected.len(), TOTAL_TASKS, "All 160 mixed tasks must be drained");

    let mut seen = HashSet::new();
    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut cancelled_count = 0;

    for snap in &collected {
        assert!(seen.insert(snap.id.clone()), "Duplicate drained: {}", snap.id);
        assert!(snap.notified);
        assert!(snap.status.is_terminal());

        // Verify notification formatting succeeds without panics
        let tty_formatted = snap.format_notification(true);
        let plain_formatted = snap.format_notification(false);
        assert!(!tty_formatted.is_empty());
        assert!(!plain_formatted.is_empty());
        assert!(tty_formatted.contains(&snap.id));
        assert!(plain_formatted.contains(&snap.id));

        match snap.status {
            TaskStatus::Completed => completed_count += 1,
            TaskStatus::Failed => {
                failed_count += 1;
                assert!(snap.error.is_some(), "Failed task must have error field");
            }
            TaskStatus::Cancelled => cancelled_count += 1,
            _ => panic!("Unexpected non-terminal status in drained: {:?}", snap.status),
        }
    }

    assert_eq!(completed_count, 40, "Expected 40 completed tasks");
    assert_eq!(failed_count, 80, "Expected 80 failed tasks (40 regular + 40 panics)");
    assert_eq!(cancelled_count, 40, "Expected 40 cancelled tasks");
}

// ============================================================================
// STRESS TEST 3: Interleaved Interactive Commands vs Concurrent Draining (Strict Disjointness)
// ============================================================================

#[test]
fn test_ux_interleaved_wait_cancel_vs_drain_strict_disjointness() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 120;
    const DRAIN_THREADS: usize = 8;
    const INTERACTIVE_THREADS: usize = 8;

    let mut task_ids = Vec::with_capacity(TOTAL_TASKS);
    for i in 0..TOTAL_TASKS {
        let (id, _) = manager
            .spawn_task(
                format!("task-interleave-{}", i),
                format!("Concurrent interaction test {}", i),
                move |tok| {
                    for _ in 0..30 {
                        if tok.is_cancelled() {
                            return Err(anyhow!("Cancelled by user command"));
                        }
                        thread::sleep(Duration::from_millis(3));
                    }
                    Ok(format!("finished-{}", i))
                },
            )
            .unwrap();
        task_ids.push(id);
    }

    let manually_marked = Arc::new(std::sync::Mutex::new(HashSet::new()));
    let drained_snaps = Arc::new(std::sync::Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));

    // 1. Launch 8 REPL drainer threads
    let mut drain_handles = Vec::new();
    for _ in 0..DRAIN_THREADS {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let drained_acc = drained_snaps.clone();
        drain_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    drained_acc.lock().unwrap().extend(drained);
                }
                thread::sleep(Duration::from_millis(2));
            }
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                drained_acc.lock().unwrap().extend(drained);
            }
        }));
    }

    // 2. Launch 8 interactive command simulation threads (waiters and cancellers)
    let chunks: Vec<Vec<String>> = task_ids
        .chunks(TOTAL_TASKS / INTERACTIVE_THREADS)
        .map(|c| c.to_vec())
        .collect();

    let mut interactive_handles = Vec::new();
    for (thread_idx, chunk) in chunks.into_iter().enumerate() {
        let mgr = manager.clone();
        let manual_acc = manually_marked.clone();
        interactive_handles.push(thread::spawn(move || {
            for (idx, id) in chunk.iter().enumerate() {
                if (thread_idx + idx) % 2 == 0 {
                    // Simulate /tasks cancel <id> + mark_task_notified
                    let _ = mgr.cancel_task(id);
                    if mgr.mark_task_notified(id) {
                        manual_acc.lock().unwrap().insert(id.clone());
                    }
                } else {
                    // Simulate /tasks wait <id> + mark_task_notified
                    let _ = mgr.await_task(id, Some(Duration::from_secs(3)));
                    if mgr.mark_task_notified(id) {
                        manual_acc.lock().unwrap().insert(id.clone());
                    }
                }
                thread::sleep(Duration::from_millis(1));
            }
        }));
    }

    // Wait for all interactive threads to complete
    for h in interactive_handles {
        h.join().unwrap();
    }

    // Ensure all tasks reached terminal state
    for id in &task_ids {
        let _ = manager.await_task(id, Some(Duration::from_secs(5)));
    }

    // Allow drain threads to process any leftover unnotified tasks
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let manual_count = manually_marked.lock().unwrap().len();
        let drain_count = drained_snaps.lock().unwrap().len();
        if manual_count + drain_count >= TOTAL_TASKS {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }

    let manual_set = manually_marked.lock().unwrap().clone();
    let drained_vec = drained_snaps.lock().unwrap().clone();

    // Verification 1: Drained snapshots have 0 internal duplicates
    let mut drained_set = HashSet::new();
    for snap in &drained_vec {
        assert!(
            drained_set.insert(snap.id.clone()),
            "Duplicate task drained: {}",
            snap.id
        );
        assert!(snap.notified);
        assert!(snap.status.is_terminal());
    }

    // Verification 2: STRICT DISJOINTNESS (M ∩ D == ∅)
    let intersection: Vec<_> = manual_set.intersection(&drained_set).collect();
    assert!(
        intersection.is_empty(),
        "Violation of strict disjointness: tasks present in BOTH manual notification and drain: {:?}",
        intersection
    );

    // Verification 3: COMPLETENESS (M ∪ D == TOTAL_TASKS)
    let total_accounted = manual_set.len() + drained_set.len();
    assert_eq!(
        total_accounted,
        TOTAL_TASKS,
        "All {} tasks must be accounted for by either manual marking or drain (manual={}, drained={})",
        TOTAL_TASKS,
        manual_set.len(),
        drained_set.len()
    );

    // Verification 4: Post-condition: subsequent drain is completely empty
    assert!(manager.drain_unnotified_terminal_tasks().is_empty());
}

// ============================================================================
// STRESS TEST 4: REPL Inter-Turn Empty-Line Enter Cycles & Idle Completions
// ============================================================================

#[test]
fn test_ux_repl_inter_turn_empty_line_enter_cycles() {
    let manager = TaskManager::new();

    // Turn 1: Fresh REPL startup, zero tasks
    let turn1_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(turn1_notifications.is_empty(), "Turn 1: Startup must produce 0 notifications");

    // Spawn a background task that takes 60ms
    let (task_a, _) = manager
        .spawn_task(
            "subagent-indexer".into(),
            "Indexing workspace".into(),
            |_| {
                thread::sleep(Duration::from_millis(60));
                Ok("indexed 500 files".into())
            },
        )
        .unwrap();

    // Turn 2: User immediately presses Enter on an empty line (task still running)
    let turn2_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(
        turn2_notifications.is_empty(),
        "Turn 2: Fast empty-line enter while task is running must produce 0 notifications"
    );

    // Turn 3: User presses Enter again after 15ms (task still running)
    thread::sleep(Duration::from_millis(15));
    let turn3_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(
        turn3_notifications.is_empty(),
        "Turn 3: Repeated empty-line enter while task is running must produce 0 notifications"
    );

    // Wait until Task A completes in background
    let snap_a = manager.await_task(&task_a, Some(Duration::from_secs(3))).unwrap();
    assert_eq!(snap_a.status, TaskStatus::Completed);

    // Turn 4: User presses Enter on empty line AFTER Task A completed
    let turn4_notifications = manager.drain_unnotified_terminal_tasks();
    assert_eq!(
        turn4_notifications.len(),
        1,
        "Turn 4: Task completion must be announced on the next prompt turn"
    );
    assert_eq!(turn4_notifications[0].id, task_a);
    assert!(turn4_notifications[0].notified);
    assert_eq!(turn4_notifications[0].status, TaskStatus::Completed);

    // Turn 5: User presses Enter AGAIN immediately (consecutive empty line enters)
    let turn5_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(
        turn5_notifications.is_empty(),
        "Turn 5: Consecutive empty line enter must NOT re-announce completed Task A"
    );

    // Turn 6: User presses Enter again
    let turn6_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(turn6_notifications.is_empty(), "Turn 6: Must remain quiet");

    // Spawn two tasks: Task B (fails immediately), Task C (completes immediately)
    let (task_b, _) = manager
        .spawn_task(
            "subagent-lint".into(),
            "Linter run".into(),
            |_| Err(anyhow!("Syntax error on line 42")),
        )
        .unwrap();

    let (task_c, _) = manager
        .spawn_task(
            "subagent-format".into(),
            "Formatter run".into(),
            |_| Ok("formatted 12 files".into()),
        )
        .unwrap();

    let _ = manager.await_task(&task_b, Some(Duration::from_secs(2))).unwrap();
    let _ = manager.await_task(&task_c, Some(Duration::from_secs(2))).unwrap();

    // Turn 7: Next user prompt turn announces both Task B and Task C in sorted order
    let turn7_notifications = manager.drain_unnotified_terminal_tasks();
    assert_eq!(turn7_notifications.len(), 2, "Turn 7: Must announce both finished tasks");
    assert_eq!(turn7_notifications[0].id, task_b);
    assert_eq!(turn7_notifications[1].id, task_c);
    assert_eq!(turn7_notifications[0].status, TaskStatus::Failed);
    assert_eq!(turn7_notifications[1].status, TaskStatus::Completed);

    // Turn 8: Subsequent prompt turn is quiet again
    let turn8_notifications = manager.drain_unnotified_terminal_tasks();
    assert!(turn8_notifications.is_empty(), "Turn 8: Must be empty");
}

// ============================================================================
// STRESS TEST 5: Rapid Creation, Clearing (clear_completed), and Drain Hammer
// ============================================================================

#[test]
fn test_ux_rapid_creation_clearing_and_drain_race() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL_TASKS: usize = 150;

    let running = Arc::new(AtomicBool::new(true));
    let drained_all = Arc::new(std::sync::Mutex::new(Vec::new()));

    // 4 drain threads
    let mut drain_handles = Vec::new();
    for _ in 0..4 {
        let mgr = manager.clone();
        let running_flag = running.clone();
        let acc = drained_all.clone();
        drain_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let drained = mgr.drain_unnotified_terminal_tasks();
                if !drained.is_empty() {
                    acc.lock().unwrap().extend(drained);
                }
                thread::sleep(Duration::from_millis(1));
            }
            let drained = mgr.drain_unnotified_terminal_tasks();
            if !drained.is_empty() {
                acc.lock().unwrap().extend(drained);
            }
        }));
    }

    // 2 clear threads
    let mut clear_handles = Vec::new();
    for _ in 0..2 {
        let mgr = manager.clone();
        let running_flag = running.clone();
        clear_handles.push(thread::spawn(move || {
            while running_flag.load(Ordering::Relaxed) {
                let _ = mgr.clear_completed();
                let _ = mgr.prune_tasks(10);
                thread::sleep(Duration::from_millis(5));
            }
        }));
    }

    // 6 producer threads spawning tasks
    let mut producer_handles = Vec::new();
    for p in 0..6 {
        let mgr = manager.clone();
        producer_handles.push(thread::spawn(move || {
            for i in 0..25 {
                let _ = mgr.spawn_task(
                    format!("churn-{}-{}", p, i),
                    "churn payload".into(),
                    move |_| {
                        thread::sleep(Duration::from_millis(1));
                        Ok("ok".into())
                    },
                );
            }
        }));
    }

    for h in producer_handles {
        h.join().unwrap();
    }

    // Allow background tasks to run and drainers/clearers to contend
    thread::sleep(Duration::from_millis(150));

    running.store(false, Ordering::Relaxed);
    for h in drain_handles {
        h.join().unwrap();
    }
    for h in clear_handles {
        h.join().unwrap();
    }

    let collected = drained_all.lock().unwrap().clone();

    // Crucial invariant: Across all concurrent clear/prune/drain calls,
    // ZERO duplicate notifications were drained!
    let mut seen = HashSet::new();
    for snap in &collected {
        assert!(
            seen.insert(snap.id.clone()),
            "Duplicate drained under clear hammer: {}",
            snap.id
        );
        assert!(snap.notified);
        assert!(snap.status.is_terminal());
    }

    // Total drained cannot exceed TOTAL_TASKS
    assert!(collected.len() <= TOTAL_TASKS);

    // Final drain must be empty
    assert!(manager.drain_unnotified_terminal_tasks().is_empty());
}

// ============================================================================
// STRESS TEST 6: Drain Sorting Invariant (Deterministic Numerical & Fallback)
// ============================================================================

#[test]
fn test_ux_drain_sorting_deterministic_numerical_order() {
    let manager = TaskManager::new();

    // Spawn 25 tasks: their IDs will be task-1, task-2, ..., task-25
    let mut ids = Vec::new();
    for i in 1..=25 {
        let (id, _) = manager
            .spawn_task(
                format!("sort-test-{}", i),
                "payload".into(),
                |_| Ok("ok".into()),
            )
            .unwrap();
        ids.push(id);
    }

    // Await all tasks
    for id in &ids {
        let _ = manager.await_task(id, Some(Duration::from_secs(3)));
    }

    let drained = manager.drain_unnotified_terminal_tasks();
    assert_eq!(drained.len(), 25);

    // Verify numerical order: task-1, task-2, ..., task-9, task-10, task-11, ..., task-25
    // Lexicographical order would have put task-10 before task-2!
    for (idx, snap) in drained.iter().enumerate() {
        let expected_num = idx + 1;
        let expected_id = format!("task-{}", expected_num);
        assert_eq!(
            snap.id, expected_id,
            "Tasks must be sorted numerically by ID: expected {}, got {}",
            expected_id, snap.id
        );
    }
}

// ============================================================================
// STRESS TEST 7: Read-Only Inspection Purity (get_unnotified Does Not Mutate)
// ============================================================================

#[test]
fn test_ux_read_only_get_unnotified_preserves_notified_state() {
    let manager = Arc::new(TaskManager::new());
    const TASK_COUNT: usize = 30;

    let mut ids = Vec::new();
    for i in 0..TASK_COUNT {
        let (id, _) = manager
            .spawn_task(
                format!("ro-{}", i),
                "read only payload".into(),
                |_| Ok("done".into()),
            )
            .unwrap();
        ids.push(id);
    }

    for id in &ids {
        let _ = manager.await_task(id, Some(Duration::from_secs(3)));
    }

    // 10 concurrent reader threads calling get_unnotified_terminal_tasks()
    let mut reader_handles = Vec::new();
    for _ in 0..10 {
        let mgr = manager.clone();
        reader_handles.push(thread::spawn(move || {
            for _ in 0..10 {
                let unnotified = mgr.get_unnotified_terminal_tasks();
                assert_eq!(unnotified.len(), TASK_COUNT);
                for snap in &unnotified {
                    assert!(!snap.notified, "get_unnotified must NOT mark task as notified");
                }
            }
        }));
    }

    for h in reader_handles {
        h.join().unwrap();
    }

    // Has unnotified completions must report true
    assert!(manager.has_unnotified_completions());

    // Single drain call extracts them and marks notified
    let drained = manager.drain_unnotified_terminal_tasks();
    assert_eq!(drained.len(), TASK_COUNT);
    for snap in &drained {
        assert!(snap.notified, "drain must mark tasks as notified");
    }

    // Subsequent get_unnotified is now empty
    assert!(manager.get_unnotified_terminal_tasks().is_empty());
    assert!(!manager.has_unnotified_completions());
    assert!(manager.drain_unnotified_terminal_tasks().is_empty());
}

// ============================================================================
// STRESS TEST 8: Notification Formatting Parity (TTY vs Non-TTY & Edge Case Payloads)
// ============================================================================

#[test]
fn test_ux_notification_formatting_tty_and_nontty_parity() {
    let manager = TaskManager::new();

    // 1. Completed task with short output
    let (id_comp, _) = manager
        .spawn_task(
            "subagent-worker".into(),
            "Simple calculation".into(),
            |_| Ok("finished".into()),
        )
        .unwrap();

    // 2. Failed task with massive error message (>200 chars to test preview truncation)
    let massive_error = "A".repeat(300);
    let err_clone = massive_error.clone();
    let (id_fail, _) = manager
        .spawn_task(
            "failing-worker".into(),
            "Error prone task".into(),
            move |_| Err(anyhow!("{}", err_clone)),
        )
        .unwrap();

    // 3. Cancelled task
    let (id_canc, token) = manager
        .spawn_task(
            "cancelled-worker".into(),
            "Will be aborted".into(),
            move |tok| {
                while !tok.is_cancelled() {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(anyhow!("Cancelled"))
            },
        )
        .unwrap();
    token.cancel();

    let _ = manager.await_task(&id_comp, Some(Duration::from_secs(3)));
    let _ = manager.await_task(&id_fail, Some(Duration::from_secs(3)));
    let _ = manager.await_task(&id_canc, Some(Duration::from_secs(3)));

    let drained = manager.drain_unnotified_terminal_tasks();
    assert_eq!(drained.len(), 3);

    for snap in &drained {
        // Test interactive TTY rendering
        let tty_out = snap.format_notification(true);
        assert!(tty_out.contains("\x1B["), "TTY output must contain ANSI color codes");
        assert!(tty_out.contains(&snap.id));
        assert!(tty_out.contains(&snap.name));
        assert!(tty_out.contains(&snap.elapsed_human));

        // Test non-TTY clean plain text rendering
        let plain_out = snap.format_notification(false);
        assert!(
            !plain_out.contains("\x1B["),
            "Non-TTY output must NOT contain ANSI escape codes"
        );
        assert!(plain_out.contains(&snap.id));
        assert!(plain_out.contains(&snap.name));
        assert!(plain_out.contains(&snap.elapsed_human));

        // Status badge verification
        match snap.status {
            TaskStatus::Completed => {
                assert!(plain_out.contains("[COMPLETED]"));
                assert!(tty_out.contains("[COMPLETED]"));
            }
            TaskStatus::Failed => {
                assert!(plain_out.contains("[FAILED]"));
                assert!(tty_out.contains("[FAILED]"));
                // Assert massive error was truncated with ellipsis
                assert!(plain_out.contains("..."));
                assert!(tty_out.contains("..."));
            }
            TaskStatus::Cancelled => {
                assert!(plain_out.contains("[CANCELLED]"));
                assert!(tty_out.contains("[CANCELLED]"));
            }
            _ => unreachable!(),
        }
    }
}
