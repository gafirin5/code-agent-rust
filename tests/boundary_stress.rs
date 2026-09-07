#[path = "../src/agent/tasks.rs"]
mod tasks;

use std::panic;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tasks::{TaskError, TaskManager, TaskStatus};

// ============================================================================
// SUITE 1: ZERO-DURATION & ULTRA-SHORT TIMEOUT BEHAVIORS
// ============================================================================

#[test]
fn test_zero_duration_timeout_on_running_task() {
    let manager = TaskManager::new();
    let (id, _token) = manager
        .spawn_task("slow".into(), "runs for 500ms".into(), |_| {
            thread::sleep(Duration::from_millis(500));
            Ok("done".into())
        })
        .unwrap();

    let start = Instant::now();
    let res = manager.await_task(&id, Some(Duration::ZERO));
    let elapsed = start.elapsed();

    // Must return immediately without hanging
    assert!(res.is_err(), "Expected timeout error on running task");
    match res.unwrap_err() {
        TaskError::Timeout(dur) => assert_eq!(dur, Duration::ZERO),
        other => panic!("Expected Timeout, got {:?}", other),
    }
    assert!(
        elapsed < Duration::from_millis(50),
        "Zero-duration timeout took {:?}, should be nearly instantaneous",
        elapsed
    );
}

#[test]
fn test_zero_duration_timeout_on_completed_task() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("instant".into(), "finishes immediately".into(), |_| {
            Ok("finished payload".into())
        })
        .unwrap();

    // Wait for completion first
    let snap1 = manager.await_task(&id, None).unwrap();
    assert_eq!(snap1.status, TaskStatus::Completed);

    // Now await with Duration::ZERO
    let start = Instant::now();
    let snap2 = manager
        .await_task(&id, Some(Duration::ZERO))
        .expect("Zero-timeout on completed task must return Ok");
    let elapsed = start.elapsed();

    assert_eq!(snap2.status, TaskStatus::Completed);
    assert_eq!(snap2.result.as_deref(), Some("finished payload"));
    assert!(
        elapsed < Duration::from_millis(20),
        "Await completed with zero duration took {:?}",
        elapsed
    );
}

#[test]
fn test_zero_duration_timeout_on_failed_and_cancelled_tasks() {
    let manager = TaskManager::new();

    // Failed task
    let (fail_id, _) = manager
        .spawn_task("f".into(), "f".into(), |_| Err(anyhow::anyhow!("err")))
        .unwrap();
    let _ = manager.await_task(&fail_id, None);
    let fail_snap = manager
        .await_task(&fail_id, Some(Duration::ZERO))
        .expect("Completed/failed tasks must return snapshot with zero timeout");
    assert_eq!(fail_snap.status, TaskStatus::Failed);

    // Cancelled task
    let (canc_id, _) = manager
        .spawn_task("c".into(), "c".into(), |token| {
            while !token.is_cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            Err(anyhow::anyhow!("cancelled"))
        })
        .unwrap();
    manager.cancel_task(&canc_id).unwrap();
    let _ = manager.await_task(&canc_id, None);
    let canc_snap = manager
        .await_task(&canc_id, Some(Duration::ZERO))
        .expect("Cancelled task must return snapshot with zero timeout");
    assert_eq!(canc_snap.status, TaskStatus::Cancelled);
}

#[test]
fn test_sub_millisecond_timeouts() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("sub-ms".into(), "sleep 200ms".into(), |_| {
            thread::sleep(Duration::from_millis(200));
            Ok("done".into())
        })
        .unwrap();

    let nano_dur = Duration::from_nanos(1);
    let start = Instant::now();
    let res = manager.await_task(&id, Some(nano_dur));
    let elapsed = start.elapsed();

    assert!(res.is_err());
    match res.unwrap_err() {
        TaskError::Timeout(d) => assert_eq!(d, nano_dur),
        other => panic!("Expected Timeout, got {:?}", other),
    }
    assert!(
        elapsed < Duration::from_millis(250),
        "1ns timeout took {:?}",
        elapsed
    );

    let micro_dur = Duration::from_micros(500);
    let start = Instant::now();
    let res2 = manager.await_task(&id, Some(micro_dur));
    let elapsed2 = start.elapsed();

    assert!(res2.is_err());
    assert!(
        elapsed2 < Duration::from_millis(250),
        "500us timeout took {:?}",
        elapsed2
    );
}

#[test]
fn test_race_between_completion_and_short_timeout() {
    // Stress test: 30 tasks completing in ~4ms awaited with 25ms timeout
    let manager = Arc::new(TaskManager::new());
    for i in 0..30 {
        let (id, _) = manager
            .spawn_task(format!("race-{}", i), "racing task".into(), |_| {
                thread::sleep(Duration::from_millis(4));
                Ok("race winner".into())
            })
            .unwrap();

        let res = manager.await_task(&id, Some(Duration::from_millis(25)));
        match res {
            Ok(snap) => {
                assert_eq!(snap.status, TaskStatus::Completed);
                assert_eq!(snap.result.as_deref(), Some("race winner"));
            }
            Err(TaskError::Timeout(_)) => {
                let snap = manager.await_task(&id, Some(Duration::from_millis(500))).unwrap();
                assert_eq!(snap.status, TaskStatus::Completed);
            }
            Err(other) => panic!("Unexpected error: {:?}", other),
        }
    }
}

// ============================================================================
// SUITE 2: PANIC RECOVERY UNDER CATCH_UNWIND (HOSTILE PAYLOAD VARIETIES)
// ============================================================================

#[test]
fn test_panic_with_static_str() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("panic-str".into(), "panics with &str".into(), |_| {
            panic!("static string error message");
        })
        .unwrap();

    let snap = manager.await_task(&id, Some(Duration::from_millis(1000))).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);
    assert!(snap.result.is_none());
    let err = snap.error.expect("Must have error message");
    assert_eq!(err, "Panicked: static string error message");
}

#[test]
fn test_panic_with_heap_string() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("panic-string".into(), "panics with String".into(), |_| {
            panic!("{}", format!("dynamically formatted panic id={}", 4242));
        })
        .unwrap();

    let snap = manager.await_task(&id, Some(Duration::from_millis(1000))).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);
    assert!(snap.result.is_none());
    let err = snap.error.expect("Must have error message");
    assert_eq!(err, "Panicked: dynamically formatted panic id=4242");
}

#[derive(Debug)]
struct CustomPanicPayload {
    #[allow(dead_code)]
    code: u32,
    #[allow(dead_code)]
    msg: &'static str,
}

#[test]
fn test_panic_with_custom_non_string_type() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("panic-custom".into(), "panics with arbitrary type".into(), |_| {
            panic::panic_any(CustomPanicPayload {
                code: 500,
                msg: "custom type crash",
            });
        })
        .unwrap();

    let snap = manager.await_task(&id, Some(Duration::from_millis(1000))).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);
    let err = snap.error.expect("Must have error message");
    assert!(
        err.contains("Panicked: Worker thread panicked with unknown payload"),
        "Error message was: {}",
        err
    );
}

#[test]
fn test_multiple_concurrent_panicking_workers() {
    let manager = Arc::new(TaskManager::new());
    const COUNT: usize = 15;
    let mut ids = Vec::new();

    for i in 0..COUNT {
        let (id, _) = manager
            .spawn_task(format!("multi-panic-{}", i), "panic burst".into(), move |_| {
                if i % 2 == 0 {
                    panic!("even panic {}", i);
                } else {
                    panic::panic_any(i);
                }
            })
            .unwrap();
        ids.push((id, i));
    }

    for (id, i) in ids {
        let snap = manager.await_task(&id, Some(Duration::from_millis(1000))).unwrap();
        assert_eq!(snap.status, TaskStatus::Failed);
        let err = snap.error.unwrap();
        if i % 2 == 0 {
            assert!(err.contains(&format!("even panic {}", i)));
        } else {
            assert!(err.contains("unknown payload"));
        }
    }

    // Registry must remain intact and functional
    let tasks = manager.list_tasks();
    assert_eq!(tasks.len(), COUNT);
}

// ============================================================================
// SUITE 3: LOCK POISONING RECOVERY ACROSS ALL LOCKS & CONDVARS
// ============================================================================

#[test]
fn test_hostile_task_inner_mutex_poisoning() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("poison-victim".into(), "desc".into(), |_| {
            thread::sleep(Duration::from_millis(20));
            Ok("recovered".into())
        })
        .unwrap();

    let inner = manager.get_inner(&id).unwrap();

    // Deliberately poison the per-task mutex from another thread
    let inner_clone = inner.clone();
    let poison_handle = thread::spawn(move || {
        let _guard = inner_clone.record.lock().unwrap();
        panic!("hostile abort while holding TaskInner record mutex");
    });
    let _ = poison_handle.join(); // Deliberately expect Err(panic)

    // Verify record mutex is indeed poisoned
    assert!(inner.record.is_poisoned(), "Mutex should be poisoned");

    // Operations on manager must continue working without panic
    let snap = manager.get_task(&id).expect("get_task should recover from poisoned mutex");
    assert_eq!(snap.id, id);

    let list = manager.list_tasks();
    assert_eq!(list.len(), 1);

    let final_snap = manager.await_task(&id, Some(Duration::from_millis(1000))).unwrap();
    assert_eq!(final_snap.status, TaskStatus::Completed);
    assert_eq!(final_snap.result.as_deref(), Some("recovered"));
}

#[test]
fn test_poison_recovery_on_multiple_operations() {
    let manager = TaskManager::new();
    let (id1, _) = manager
        .spawn_task("t1".into(), "desc".into(), |_| {
            thread::sleep(Duration::from_millis(10));
            Ok("ok".into())
        })
        .unwrap();

    let inner = manager.get_inner(&id1).unwrap();

    // Poison the lock
    let inner_clone = inner.clone();
    let h = thread::spawn(move || {
        let _guard = inner_clone.record.lock().unwrap();
        panic!("deliberate abort");
    });
    let _ = h.join();
    assert!(inner.record.is_poisoned());

    // 1. cancel_task must recover
    let _ = manager.cancel_task(&id1);

    // 2. await_task must recover
    let snap = manager.await_task(&id1, Some(Duration::from_millis(500))).unwrap();
    assert!(snap.is_terminal());

    // 3. remove_task must recover
    let removed = manager.remove_task(&id1).unwrap();
    assert!(removed);

    // 4. subsequent tasks can be spawned and completed
    let (id2, _) = manager
        .spawn_task("t2".into(), "desc".into(), |_| Ok("clean".into()))
        .unwrap();
    let snap2 = manager.await_task(&id2, None).unwrap();
    assert_eq!(snap2.status, TaskStatus::Completed);
    assert_eq!(snap2.result.as_deref(), Some("clean"));
}

// ============================================================================
// SUITE 4: BOUNDARY CONDITIONS & EDGE CASES
// ============================================================================

#[test]
fn test_empty_and_massive_task_metadata() {
    let manager = TaskManager::new();

    // 1. Empty strings for name and description
    let (id_empty, _) = manager
        .spawn_task("".into(), "".into(), |_| Ok("empty metadata ok".into()))
        .unwrap();

    let snap = manager.await_task(&id_empty, None).unwrap();
    assert_eq!(snap.name, "");
    assert_eq!(snap.description, "");
    assert_eq!(snap.status, TaskStatus::Completed);
    assert_eq!(snap.result.as_deref(), Some("empty metadata ok"));

    // 2. Huge strings (100 KB description)
    let huge_desc = "x".repeat(100_000);
    let (id_huge, _) = manager
        .spawn_task("huge-task".into(), huge_desc.clone(), |_| Ok("huge done".into()))
        .unwrap();

    let snap_huge = manager.await_task(&id_huge, None).unwrap();
    assert_eq!(snap_huge.description.len(), 100_000);
    assert_eq!(snap_huge.status, TaskStatus::Completed);
}

#[test]
fn test_nonexistent_task_id_boundaries() {
    let manager = TaskManager::new();

    // Empty ID
    assert!(manager.get_task("").is_none());
    assert_eq!(manager.cancel_task(""), Err(TaskError::NotFound("".into())));
    assert_eq!(manager.await_task("", None), Err(TaskError::NotFound("".into())));
    assert_eq!(manager.remove_task(""), Ok(false));
    assert!(manager.get_task_logs("").is_none());

    // Strange symbols
    let weird = "task-null\0-newline\n-emoji??";
    assert!(manager.get_task(weird).is_none());
    assert_eq!(manager.cancel_task(weird), Err(TaskError::NotFound(weird.into())));
    assert_eq!(manager.await_task(weird, None), Err(TaskError::NotFound(weird.into())));
    assert_eq!(manager.remove_task(weird), Ok(false));
}

#[test]
fn test_prune_and_clear_boundary_conditions() {
    let manager = TaskManager::new();

    // Boundary: prune empty registry
    assert_eq!(manager.prune_tasks(0), 0);
    assert_eq!(manager.prune_tasks(10), 0);
    assert_eq!(manager.clear_completed(), 0);

    // Spawn 1 running task
    let (id_run, token) = manager
        .spawn_task("running".into(), "running".into(), |_| {
            thread::sleep(Duration::from_millis(300));
            Ok("done".into())
        })
        .unwrap();

    // Ensure running task is NEVER pruned or cleared
    assert_eq!(manager.prune_tasks(0), 0);
    assert_eq!(manager.clear_completed(), 0);
    assert_eq!(manager.list_tasks().len(), 1);

    // Attempting to remove running task should error
    let rem_err = manager.remove_task(&id_run);
    assert_eq!(rem_err, Err(TaskError::CannotRemoveRunning(id_run.clone())));

    token.cancel();
    let _ = manager.await_task(&id_run, None);

    // Now it is terminal: prune with max_retained = 0 should remove it
    assert_eq!(manager.prune_tasks(0), 1);
    assert_eq!(manager.list_tasks().len(), 0);
}

#[test]
fn test_multiple_waiters_with_mixed_timeouts() {
    let manager = Arc::new(TaskManager::new());
    let (id, _) = manager
        .spawn_task("mixed-awaiters".into(), "sleeps 60ms".into(), |_| {
            thread::sleep(Duration::from_millis(60));
            Ok("broadcast data".into())
        })
        .unwrap();

    let m1 = manager.clone();
    let id1 = id.clone();
    // Waiter 1: 15ms timeout (should time out)
    let h1 = thread::spawn(move || m1.await_task(&id1, Some(Duration::from_millis(15))));

    let m2 = manager.clone();
    let id2 = id.clone();
    // Waiter 2: 1000ms timeout (should succeed)
    let h2 = thread::spawn(move || m2.await_task(&id2, Some(Duration::from_millis(1000))));

    let m3 = manager.clone();
    let id3 = id.clone();
    // Waiter 3: No timeout (should succeed)
    let h3 = thread::spawn(move || m3.await_task(&id3, None));

    let res1 = h1.join().unwrap();
    assert!(res1.is_err(), "Waiter 1 must time out");
    match res1.unwrap_err() {
        TaskError::Timeout(d) => assert_eq!(d, Duration::from_millis(15)),
        other => panic!("Expected timeout, got {:?}", other),
    }

    let res2 = h2.join().unwrap().unwrap();
    assert_eq!(res2.status, TaskStatus::Completed);
    assert_eq!(res2.result.as_deref(), Some("broadcast data"));

    let res3 = h3.join().unwrap().unwrap();
    assert_eq!(res3.status, TaskStatus::Completed);
    assert_eq!(res3.result.as_deref(), Some("broadcast data"));
}

#[test]
fn test_high_volume_spawn_stress_50_tasks() {
    let manager = Arc::new(TaskManager::new());
    const TOTAL: usize = 50;
    let mut ids = Vec::with_capacity(TOTAL);

    let start = Instant::now();
    for i in 0..TOTAL {
        let (id, _) = manager
            .spawn_task(
                format!("burst-{}", i),
                format!("desc {}", i),
                move |_| {
                    thread::sleep(Duration::from_millis(10));
                    Ok(format!("output-{}", i))
                },
            )
            .unwrap();
        ids.push((id, i));
    }

    assert_eq!(ids.len(), TOTAL);

    // Await all 50 tasks
    for (id, i) in ids {
        let snap = manager.await_task(&id, Some(Duration::from_millis(3000))).unwrap();
        assert_eq!(snap.status, TaskStatus::Completed);
        let expected = format!("output-{}", i);
        assert_eq!(snap.result.as_deref(), Some(expected.as_str()));
    }

    let elapsed = start.elapsed();
    // 50 tasks * 10ms = 500ms serial. In parallel, easily finishes within generous window under load.
    assert!(
        elapsed < Duration::from_millis(5000),
        "50 concurrent tasks took {:?}, expected < 5000ms",
        elapsed
    );
    assert_eq!(manager.list_tasks().len(), TOTAL);
}

// ============================================================================
// SUITE 5: QUEUED DURATION GUARANTEES & AWAIT_RUNNING BOUNDARY VERIFICATION
// ============================================================================

#[test]
fn test_failure_while_queued_duration_populated() {
    let token = tasks::CancellationToken::new();
    let logs = Arc::new(tasks::TaskLogBuffer::new());
    let mut rec = tasks::TaskRecord::new("queued-fail-explicit", "test", "desc", token, logs);

    assert_eq!(rec.status, TaskStatus::Queued);
    assert!(rec.duration.is_none());

    // Small delay to ensure measurable non-zero time since creation
    thread::sleep(Duration::from_millis(5));

    rec.mark_failed("Explicit spawn failure test".into())
        .unwrap();
    assert_eq!(rec.status, TaskStatus::Failed);
    assert!(
        rec.duration.is_some(),
        "record.duration must be populated when failed from Queued"
    );
    let dur = rec.duration.unwrap();
    assert!(
        dur >= Duration::from_millis(3),
        "duration {:?} should reflect elapsed time since created_at",
        dur
    );

    let snap = rec.snapshot();
    assert_eq!(snap.status, TaskStatus::Failed);
    assert!(
        snap.duration_ms.is_some(),
        "snapshot.duration_ms must be Some for terminal tasks"
    );
    assert!(
        snap.duration_ms.unwrap() >= 3,
        "snapshot.duration_ms {:?} should reflect elapsed ms",
        snap.duration_ms
    );
    assert!(snap.started_at.is_none(), "started_at must be None for queued failure");
    assert!(snap.finished_at.is_some(), "finished_at must be populated");
    assert_eq!(snap.error.as_deref(), Some("Explicit spawn failure test"));
}

#[test]
fn test_cancellation_while_queued_duration_populated_direct() {
    let token = tasks::CancellationToken::new();
    let logs = Arc::new(tasks::TaskLogBuffer::new());
    let mut rec = tasks::TaskRecord::new("queued-cancel-explicit", "test", "desc", token.clone(), logs);

    assert_eq!(rec.status, TaskStatus::Queued);
    assert!(rec.duration.is_none());

    // Small delay to ensure measurable non-zero time since creation
    thread::sleep(Duration::from_millis(5));

    rec.mark_cancelled(Some("Explicit pre-launch cancellation".into()))
        .unwrap();
    assert_eq!(rec.status, TaskStatus::Cancelled);
    assert!(
        rec.duration.is_some(),
        "record.duration must be populated when cancelled from Queued"
    );
    assert!(token.is_cancelled(), "cancellation_token must be flagged");

    let snap = rec.snapshot();
    assert_eq!(snap.status, TaskStatus::Cancelled);
    assert!(
        snap.duration_ms.is_some(),
        "snapshot.duration_ms must be Some for terminal tasks"
    );
    assert!(
        snap.duration_ms.unwrap() >= 3,
        "snapshot.duration_ms {:?} should reflect elapsed ms",
        snap.duration_ms
    );
    assert!(snap.started_at.is_none(), "started_at must be None for queued cancel");
    assert!(snap.finished_at.is_some(), "finished_at must be populated");
    assert_eq!(
        snap.error.as_deref(),
        Some("Cancelled: Explicit pre-launch cancellation")
    );
}

#[test]
fn test_queued_task_cancellation_via_manager_preserves_duration() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("cancel-mgr-direct".into(), "cancel immediately".into(), |_| {
            thread::sleep(Duration::from_millis(200));
            Ok("ok".into())
        })
        .unwrap();

    // Cancel immediately via convenience method
    assert!(manager.cancel(&id));

    let snap = manager
        .await_task(&id, Some(Duration::from_millis(500)))
        .expect("Await should succeed on cancelled task");
    assert_eq!(snap.status, TaskStatus::Cancelled);
    assert!(
        snap.duration_ms.is_some(),
        "duration_ms must be populated on cancelled task"
    );
    assert!(
        snap.duration_ms.unwrap() < 200,
        "duration should be short, was {:?}",
        snap.duration_ms
    );
}

#[test]
fn test_await_running_immediate_return_when_already_running() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("running-imm".into(), "sync".into(), |_| {
            thread::sleep(Duration::from_millis(300));
            Ok("done".into())
        })
        .unwrap();

    // Wait until task starts running
    let snap1 = manager
        .await_running(&id, Some(Duration::from_secs(2)))
        .expect("Task must start running");
    assert_eq!(snap1.status, TaskStatus::Running);

    // Immediate second call: must return Ok immediately without condvar wait
    let start = Instant::now();
    let snap2 = manager
        .await_running(&id, Some(Duration::from_millis(50)))
        .expect("Second await_running must return Ok immediately");
    let elapsed = start.elapsed();

    assert_eq!(snap2.status, TaskStatus::Running);
    assert!(
        elapsed < Duration::from_millis(20),
        "Immediate return took {:?}, expected < 20ms",
        elapsed
    );

    // Immediate third call with timeout=None: must also return immediately
    let start3 = Instant::now();
    let snap3 = manager
        .await_running(&id, None)
        .expect("Third await_running with None timeout must return Ok immediately");
    let elapsed3 = start3.elapsed();

    assert_eq!(snap3.status, TaskStatus::Running);
    assert!(
        elapsed3 < Duration::from_millis(20),
        "Immediate return took {:?}, expected < 20ms",
        elapsed3
    );

    let _ = manager.await_task(&id, Some(Duration::from_millis(600)));
}

#[test]
fn test_await_running_immediate_return_for_completed_task() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("fast-comp".into(), "done".into(), |_| Ok("immediate".into()))
        .unwrap();

    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Completed);

    // Calling await_running on completed task must return Ok(Completed) immediately
    let start = Instant::now();
    let snap_r = manager
        .await_running(&id, Some(Duration::from_millis(50)))
        .expect("await_running on completed task must succeed");
    let elapsed = start.elapsed();

    assert_eq!(snap_r.status, TaskStatus::Completed);
    assert!(
        elapsed < Duration::from_millis(20),
        "Took {:?}, expected < 20ms",
        elapsed
    );
}

#[test]
fn test_await_running_immediate_return_for_failed_task() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("fail-fast".into(), "fail".into(), |_| {
            Err(anyhow::anyhow!("deliberate task failure"))
        })
        .unwrap();

    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Failed);

    // Calling await_running on failed task must return Ok(Failed) immediately
    let start = Instant::now();
    let snap_r = manager
        .await_running(&id, Some(Duration::from_millis(50)))
        .expect("await_running on failed task must succeed");
    let elapsed = start.elapsed();

    assert_eq!(snap_r.status, TaskStatus::Failed);
    assert!(
        elapsed < Duration::from_millis(20),
        "Took {:?}, expected < 20ms",
        elapsed
    );
}

#[test]
fn test_await_running_immediate_return_for_cancelled_task() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("cancel-fast".into(), "cancel".into(), |_| {
            thread::sleep(Duration::from_millis(200));
            Ok("ok".into())
        })
        .unwrap();

    manager.cancel(&id);
    let snap = manager.await_task(&id, None).unwrap();
    assert_eq!(snap.status, TaskStatus::Cancelled);

    // Calling await_running on cancelled task must return Ok(Cancelled) immediately
    let start = Instant::now();
    let snap_r = manager
        .await_running(&id, Some(Duration::from_millis(50)))
        .expect("await_running on cancelled task must succeed");
    let elapsed = start.elapsed();

    assert_eq!(snap_r.status, TaskStatus::Cancelled);
    assert!(
        elapsed < Duration::from_millis(20),
        "Took {:?}, expected < 20ms",
        elapsed
    );
}

#[test]
fn test_await_running_not_found_boundary() {
    let manager = TaskManager::new();

    let res1 = manager.await_running("nonexistent-id-123", Some(Duration::from_millis(50)));
    assert!(res1.is_err());
    match res1.unwrap_err() {
        TaskError::NotFound(id) => assert_eq!(id, "nonexistent-id-123"),
        other => panic!("Expected NotFound, got {:?}", other),
    }

    let res2 = manager.await_running("nonexistent-id-123", None);
    assert!(res2.is_err());
    match res2.unwrap_err() {
        TaskError::NotFound(id) => assert_eq!(id, "nonexistent-id-123"),
        other => panic!("Expected NotFound, got {:?}", other),
    }
}

#[test]
fn test_await_running_zero_duration_timeout_behavior() {
    let manager = TaskManager::new();
    let (id, _) = manager
        .spawn_task("zero-dur".into(), "desc".into(), |_| {
            thread::sleep(Duration::from_millis(100));
            Ok("ok".into())
        })
        .unwrap();

    // First ensure it enters Running
    manager
        .await_running(&id, Some(Duration::from_secs(2)))
        .expect("Task must start running");

    // On active running task, Duration::ZERO returns Ok immediately
    let res = manager.await_running(&id, Some(Duration::ZERO));
    assert!(res.is_ok(), "Duration::ZERO on running task must succeed");
    assert_eq!(res.unwrap().status, TaskStatus::Running);

    let _ = manager.await_task(&id, Some(Duration::from_millis(500)));
}

#[test]
fn test_await_running_multi_waiter_stampede() {
    let manager = Arc::new(TaskManager::new());
    let (id, _) = manager
        .spawn_task("stampede".into(), "desc".into(), |_| {
            thread::sleep(Duration::from_millis(50));
            Ok("done".into())
        })
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let m = manager.clone();
        let tid = id.clone();
        handles.push(thread::spawn(move || {
            m.await_running(&tid, Some(Duration::from_secs(2)))
        }));
    }

    for h in handles {
        let res = h.join().unwrap();
        assert!(res.is_ok(), "All await_running waiters must succeed");
        let snap = res.unwrap();
        assert!(
            snap.status == TaskStatus::Running || snap.status == TaskStatus::Completed,
            "Observed status {:?}",
            snap.status
        );
    }

    let final_snap = manager.await_task(&id, None).unwrap();
    assert_eq!(final_snap.status, TaskStatus::Completed);
}

