//! Empirical Challenge Test Suite for Milestone 3: Task Persistence & Crash Recovery
//!
//! Authored by: challenger_m3_1
//! Objectives:
//! 1. Challenge 1: Corrupted & partially written JSONL lines resilience (no panics, skips garbage, reconciles valid in-flight).
//! 2. Challenge 2: Multi-cycle crash and re-crash monotonic task ID continuity (task-1..task-5 across 3 distinct lifecycles).
//! 3. Challenge 3: Cold-boot log buffer disk fallback (.ctrl/tasks/<id>.log) after process reboot.
//! 4. Challenge 4: Interleaved JSONL state history compaction (latest terminal state wins, not reverted).
//! 5. Challenge 5: High-concurrency simultaneous snapshot persistence and disk log mirroring stress.
//! 6. Challenge 6: Empty, whitespace-only, and non-existent storage path handling.

#[path = "../src/agent/tasks.rs"]
mod tasks;

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tasks::{TaskManager, TaskSnapshot, TaskStatus};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "m3_challenge_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Failed to create temporary directory for test");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ============================================================================
// CHALLENGE 1: Corrupted, Truncated & Malformed JSONL Lines Resilience
// ============================================================================

#[test]
fn challenge_corrupted_jsonl_graceful_recovery_and_monotonic_id() {
    let temp = TestDir::new("corrupt_jsonl");
    let ctrl_dir = temp.path().join(".ctrl");
    fs::create_dir_all(&ctrl_dir).unwrap();
    let tasks_file = ctrl_dir.join("tasks.jsonl");

    // Write a mixed file: valid completed task, garbage text, truncated JSON, valid running, valid queued
    let valid_t1 = TaskSnapshot {
        id: "task-1".into(),
        name: "clean-job".into(),
        description: "done job".into(),
        status: TaskStatus::Completed,
        created_at: "2026-09-14T01:00:00Z".into(),
        started_at: Some("2026-09-14T01:00:01Z".into()),
        finished_at: Some("2026-09-14T01:00:02Z".into()),
        elapsed_secs: 1.0,
        elapsed_human: "1.0s".into(),
        duration_ms: Some(1000),
        result: Some("clean success".into()),
        error: None,
        notified: true,
        dependencies: vec![],
    };

    let valid_t3_running = TaskSnapshot {
        id: "task-3".into(),
        name: "in-flight-worker".into(),
        description: "interrupted by power cut".into(),
        status: TaskStatus::Running,
        created_at: "2026-09-14T01:02:00Z".into(),
        started_at: Some("2026-09-14T01:02:01Z".into()),
        finished_at: None,
        elapsed_secs: 5.0,
        elapsed_human: "5.0s".into(),
        duration_ms: None,
        result: None,
        error: None,
        notified: false,
        dependencies: vec![],
    };

    let valid_t4_queued = TaskSnapshot {
        id: "task-4".into(),
        name: "queued-worker".into(),
        description: "stranded in queue".into(),
        status: TaskStatus::Queued,
        created_at: "2026-09-14T01:03:00Z".into(),
        started_at: None,
        finished_at: None,
        elapsed_secs: 0.0,
        elapsed_human: "0ms".into(),
        duration_ms: None,
        result: None,
        error: None,
        notified: false,
        dependencies: vec![],
    };

    let mut f = File::create(&tasks_file).unwrap();
    // Line 1: valid
    writeln!(f, "{}", serde_json::to_string(&valid_t1).unwrap()).unwrap();
    // Line 2: blank lines
    writeln!(f, "   ").unwrap();
    writeln!(f).unwrap();
    // Line 3: complete garbage / binary noise
    writeln!(f, ">>> CORRUPTED DATA BLOCK 0xDEADBEEF <<<").unwrap();
    // Line 4: truncated JSON line (simulating sudden power loss mid-write)
    writeln!(f, "{{\"id\": \"task-2\", \"name\": \"crashed-befo").unwrap();
    // Line 5: valid running task-3
    writeln!(f, "{}", serde_json::to_string(&valid_t3_running).unwrap()).unwrap();
    // Line 6: invalid JSON with mismatched braces
    writeln!(f, "{{\"id\": \"task-invalid\", \"name\": 12345 }}}}").unwrap();
    // Line 7: valid queued task-4
    writeln!(f, "{}", serde_json::to_string(&valid_t4_queued).unwrap()).unwrap();
    drop(f);

    // Boot manager on corrupted directory
    let manager = TaskManager::load_from_disk(temp.path()).expect("load_from_disk must succeed despite corrupted lines");

    // 1. Task-1 preserved as Completed
    let r1 = manager.get_task("task-1").expect("task-1 should be recovered");
    assert_eq!(r1.status, TaskStatus::Completed);
    assert_eq!(r1.result.as_deref(), Some("clean success"));

    // 2. Corrupted lines skipped without crash
    assert!(manager.get_task("task-2").is_none());

    // 3. Task-3 reconciled Running -> Failed
    let r3 = manager.get_task("task-3").expect("task-3 should be recovered");
    assert_eq!(r3.status, TaskStatus::Failed);
    assert!(r3.error.as_ref().unwrap().contains("interrupted"));

    // 4. Task-4 reconciled Queued -> Failed
    let r4 = manager.get_task("task-4").expect("task-4 should be recovered");
    assert_eq!(r4.status, TaskStatus::Failed);
    assert!(r4.error.as_ref().unwrap().contains("never started"));

    // 5. Monotonic counter synchronization: max valid parsed id was 4, so next task must be task-5
    let (id5, _) = manager
        .spawn_task("new-task-5".into(), "first task after recovery".into(), |_| {
            Ok("task 5 succeeded".into())
        })
        .expect("spawning task-5 should succeed");
    assert_eq!(id5, "task-5", "Task ID must be monotonically incremented to task-5");

    let snap5 = manager.await_task(&id5, None).unwrap();
    assert_eq!(snap5.status, TaskStatus::Completed);
}

// ============================================================================
// CHALLENGE 2: Multi-Cycle Crash & Re-crash Monotonic Sequence
// ============================================================================

#[test]
fn challenge_multi_cycle_crash_reboot_monotonic_continuity() {
    let temp = TestDir::new("multi_cycle_reboot");

    // Lifecycle 1: Initial boot
    {
        let mgr1 = TaskManager::load_from_disk(temp.path()).unwrap();
        // Spawn task-1 (completes)
        let (id1, _) = mgr1.spawn_task("t1".into(), "done in cycle 1".into(), |_| Ok("c1_done".into())).unwrap();
        assert_eq!(id1, "task-1");
        mgr1.await_task(&id1, None).unwrap();

        // Spawn task-2 (stays running when process abruptly dies)
        let (id2, _) = mgr1.spawn_task("t2".into(), "crashes in cycle 1".into(), |_| {
            thread::sleep(Duration::from_secs(60));
            Ok("never reached".into())
        }).unwrap();
        assert_eq!(id2, "task-2");
        // Simulated process termination: mgr1 is dropped while task-2 is Running
    }

    // Lifecycle 2: First reboot & reconciliation
    {
        let mgr2 = TaskManager::load_from_disk(temp.path()).unwrap();
        let t1 = mgr2.get_task("task-1").unwrap();
        assert_eq!(t1.status, TaskStatus::Completed);

        let t2 = mgr2.get_task("task-2").unwrap();
        assert_eq!(t2.status, TaskStatus::Failed, "task-2 must reconcile to Failed");

        // Spawn task-3 (completes)
        let (id3, _) = mgr2.spawn_task("t3".into(), "done in cycle 2".into(), |_| Ok("c2_done".into())).unwrap();
        assert_eq!(id3, "task-3");
        mgr2.await_task(&id3, None).unwrap();

        // Spawn task-4 (stays running when process abruptly dies again)
        let (id4, _) = mgr2.spawn_task("t4".into(), "crashes in cycle 2".into(), |_| {
            thread::sleep(Duration::from_secs(60));
            Ok("never reached".into())
        }).unwrap();
        assert_eq!(id4, "task-4");
        // Simulated second process termination
    }

    // Lifecycle 3: Second reboot & reconciliation
    {
        let mgr3 = TaskManager::load_from_disk(temp.path()).unwrap();

        let t1 = mgr3.get_task("task-1").unwrap();
        assert_eq!(t1.status, TaskStatus::Completed);

        let t2 = mgr3.get_task("task-2").unwrap();
        assert_eq!(t2.status, TaskStatus::Failed);

        let t3 = mgr3.get_task("task-3").unwrap();
        assert_eq!(t3.status, TaskStatus::Completed);

        let t4 = mgr3.get_task("task-4").unwrap();
        assert_eq!(t4.status, TaskStatus::Failed, "task-4 must reconcile to Failed");

        // Spawn task-5
        let (id5, _) = mgr3.spawn_task("t5".into(), "cycle 3 task".into(), |_| Ok("c3_done".into())).unwrap();
        assert_eq!(id5, "task-5", "Next task ID across 3 crash/reboot cycles must be task-5");
        let snap5 = mgr3.await_task(&id5, None).unwrap();
        assert_eq!(snap5.status, TaskStatus::Completed);
    }
}

// ============================================================================
// CHALLENGE 3: Cold-Boot Log Buffer Disk Fallback (.ctrl/tasks/<id>.log)
// ============================================================================

#[test]
fn challenge_cold_boot_task_log_disk_fallback() {
    let temp = TestDir::new("cold_boot_logs");
    let id_target = "task-1";

    // Boot 1: Task produces 50 distinct log lines and exits
    {
        let mgr1 = TaskManager::load_from_disk(temp.path()).unwrap();
        let (id, _, _) = mgr1.spawn_task_with_sink("logged-job".into(), "produces output".into(), |_token, logs| {
            for i in 0..50 {
                logs.push(format!("diagnostic event index: {}", i));
            }
            Ok("logging done".into())
        }).unwrap();
        assert_eq!(id, id_target);
        mgr1.await_task(&id, None).unwrap();

        // Verify disk log file was written
        let disk_log = temp.path().join(".ctrl").join("tasks").join(format!("{}.log", id));
        assert!(disk_log.exists(), "Disk log file must exist at {:?}", disk_log);
    }

    // Boot 2: Clean process start. In-memory buffer is unpopulated for restored task.
    {
        let mgr2 = TaskManager::load_from_disk(temp.path()).unwrap();
        let logs_opt = mgr2.get_task_logs(id_target);
        assert!(logs_opt.is_some(), "Logs must be available via disk fallback");
        let logs = logs_opt.unwrap();
        assert_eq!(logs.len(), 50, "All 50 log lines must be recovered from disk file");
        assert_eq!(logs[0], "diagnostic event index: 0");
        assert_eq!(logs[49], "diagnostic event index: 49");
    }
}

// ============================================================================
// CHALLENGE 4: Interleaved JSONL State History Compaction
// ============================================================================

#[test]
fn challenge_interleaved_jsonl_state_compaction_and_precedence() {
    let temp = TestDir::new("interleaved_compaction");
    let ctrl_dir = temp.path().join(".ctrl");
    fs::create_dir_all(&ctrl_dir).unwrap();
    let tasks_file = ctrl_dir.join("tasks.jsonl");

    // Simulate journal where task-1 goes through Queued -> Running -> Completed
    let q_snap = TaskSnapshot {
        id: "task-1".into(),
        name: "transition-test".into(),
        description: "state progression".into(),
        status: TaskStatus::Queued,
        created_at: "2026-09-14T02:00:00Z".into(),
        started_at: None,
        finished_at: None,
        elapsed_secs: 0.0,
        elapsed_human: "0ms".into(),
        duration_ms: None,
        result: None,
        error: None,
        notified: false,
        dependencies: vec![],
    };

    let r_snap = TaskSnapshot {
        id: "task-1".into(),
        name: "transition-test".into(),
        description: "state progression".into(),
        status: TaskStatus::Running,
        created_at: "2026-09-14T02:00:00Z".into(),
        started_at: Some("2026-09-14T02:00:01Z".into()),
        finished_at: None,
        elapsed_secs: 2.0,
        elapsed_human: "2.0s".into(),
        duration_ms: None,
        result: None,
        error: None,
        notified: false,
        dependencies: vec![],
    };

    let c_snap = TaskSnapshot {
        id: "task-1".into(),
        name: "transition-test".into(),
        description: "state progression".into(),
        status: TaskStatus::Completed,
        created_at: "2026-09-14T02:00:00Z".into(),
        started_at: Some("2026-09-14T02:00:01Z".into()),
        finished_at: Some("2026-09-14T02:00:03Z".into()),
        elapsed_secs: 2.0,
        elapsed_human: "2.0s".into(),
        duration_ms: Some(2000),
        result: Some("final-state-preserved".into()),
        error: None,
        notified: true,
        dependencies: vec![],
    };

    let mut f = File::create(&tasks_file).unwrap();
    writeln!(f, "{}", serde_json::to_string(&q_snap).unwrap()).unwrap();
    writeln!(f, "{}", serde_json::to_string(&r_snap).unwrap()).unwrap();
    writeln!(f, "{}", serde_json::to_string(&c_snap).unwrap()).unwrap();
    drop(f);

    let mgr = TaskManager::load_from_disk(temp.path()).unwrap();
    let recovered = mgr.get_task("task-1").expect("task-1 must exist");

    // Must be Completed, NOT Failed (earlier Running/Queued must not trigger crash reconciliation)
    assert_eq!(recovered.status, TaskStatus::Completed);
    assert_eq!(recovered.result.as_deref(), Some("final-state-preserved"));
    assert!(recovered.error.is_none());
}

// ============================================================================
// CHALLENGE 5: High-Concurrency Simultaneous Persistence & Log Contention
// ============================================================================

#[test]
fn challenge_concurrent_persistence_and_logging_under_stress() {
    let temp = TestDir::new("concurrent_stress");
    let mgr = Arc::new(TaskManager::load_from_disk(temp.path()).unwrap());

    const THREADS: usize = 8;
    const TASKS_PER_THREAD: usize = 5;
    const LOGS_PER_TASK: usize = 20;

    let mut handles = Vec::new();
    let counter = Arc::new(AtomicUsize::new(0));

    for thread_idx in 0..THREADS {
        let m = mgr.clone();
        let c = counter.clone();
        handles.push(thread::spawn(move || {
            for task_idx in 0..TASKS_PER_THREAD {
                let task_num = thread_idx * TASKS_PER_THREAD + task_idx;
                let (id, _, _) = m.spawn_task_with_sink(
                    format!("concurrent-task-{}", task_num),
                    "high contention test".into(),
                    move |_token, logs| {
                        for log_idx in 0..LOGS_PER_TASK {
                            logs.push(format!("msg-t{}-k{}", task_num, log_idx));
                        }
                        Ok(format!("done-{}", task_num))
                    },
                ).unwrap();

                let snap = m.await_task(&id, None).unwrap();
                assert_eq!(snap.status, TaskStatus::Completed);
                c.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), THREADS * TASKS_PER_THREAD);

    // Verify tasks.jsonl is non-corrupt and can be reloaded
    let reloaded = TaskManager::load_from_disk(temp.path()).expect("Reload after high-concurrency writes must succeed");
    let all_tasks = reloaded.list_tasks();
    assert_eq!(all_tasks.len(), THREADS * TASKS_PER_THREAD);

    for task in all_tasks {
        assert_eq!(task.status, TaskStatus::Completed);
        let logs = reloaded.get_task_logs(&task.id).expect("Logs must exist");
        assert_eq!(logs.len(), LOGS_PER_TASK);
    }
}

// ============================================================================
// CHALLENGE 6: Empty, Whitespace, and Missing Path Robustness
// ============================================================================

#[test]
fn challenge_empty_and_missing_paths_resilience() {
    // 1. Non-existent path
    let temp = TestDir::new("missing_path");
    let non_existent = temp.path().join("does_not_exist_yet");
    let mgr = TaskManager::load_from_disk(&non_existent).expect("Must succeed on non-existent path");
    assert_eq!(mgr.list_tasks().len(), 0);

    // Spawning should lazily create directories
    let (id, _) = mgr.spawn_task("lazy".into(), "test".into(), |_| Ok("ok".into())).unwrap();
    assert_eq!(id, "task-1");
    mgr.await_task(&id, None).unwrap();
    assert!(non_existent.join(".ctrl").join("tasks.jsonl").exists());

    // 2. Empty tasks.jsonl (0 bytes)
    let temp2 = TestDir::new("empty_jsonl");
    let ctrl2 = temp2.path().join(".ctrl");
    fs::create_dir_all(&ctrl2).unwrap();
    File::create(ctrl2.join("tasks.jsonl")).unwrap(); // 0 bytes

    let mgr2 = TaskManager::load_from_disk(temp2.path()).expect("Must succeed on empty file");
    assert_eq!(mgr2.list_tasks().len(), 0);
    let (id2, _) = mgr2.spawn_task("from_empty".into(), "test".into(), |_| Ok("ok".into())).unwrap();
    assert_eq!(id2, "task-1");
}
