//! Empirical Challenge Test Suite for Milestone 3: DAG Dependencies & Cron Scheduler
//!
//! Authored by: challenger_m3_2
//! Objectives:
//! 1. Empirical verification of DAG linear chains and diamond execution graphs.
//! 2. Empirical verification of cycle detection (direct A->B->A, self A->A, transitive A->B->C->A, 5-node) and missing prerequisite rejection.
//! 3. Empirical verification of failure and cancellation cascades across dependent tasks.
//! 4. Empirical verification of 5-field cron parsing (wildcards, steps, ranges, lists, macros, @every).
//! 5. Empirical verification of background OS TaskScheduler thread execution, iteration caps, cancellation, and runner panic tolerance.

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
    #[path = "../../src/agent/scheduler.rs"]
    pub mod scheduler;
    #[path = "../../src/agent/dag.rs"]
    pub mod dag;
}

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent::dag::{DagTask, DagValidationError, DagValidator};
use agent::scheduler::{CronSchedule, TaskScheduler, UtcDateTime};
use agent::tasks::{TaskError, TaskManager, TaskStatus};

/// Self-cleaning isolated temporary directory for isolated TaskManager storage.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "m3_challenge_dag_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
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

// ============================================================================
// SUITE 1: DAG Dependency Execution & Ordering Verification
// ============================================================================

#[test]
fn challenge_dag_linear_chain_execution_order() {
    let temp = TestTempDir::new("linear_chain");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    let execution_order = Arc::new(Mutex::new(Vec::<String>::new()));

    // Task 1: No dependencies
    let order_clone = execution_order.clone();
    let (t1_id, _) = manager
        .spawn_task_with_dependencies("task_1".into(), "first task".into(), vec![], move |_| {
            thread::sleep(Duration::from_millis(40));
            order_clone.lock().unwrap().push("T1".into());
            Ok("T1 result".into())
        })
        .expect("T1 must spawn");

    // Task 2: Depends on T1
    let order_clone = execution_order.clone();
    let (t2_id, _) = manager
        .spawn_task_with_dependencies(
            "task_2".into(),
            "second task".into(),
            vec![t1_id.clone()],
            move |_| {
                thread::sleep(Duration::from_millis(30));
                order_clone.lock().unwrap().push("T2".into());
                Ok("T2 result".into())
            },
        )
        .expect("T2 must spawn");

    // Task 3: Depends on T2
    let order_clone = execution_order.clone();
    let (t3_id, _) = manager
        .spawn_task_with_dependencies(
            "task_3".into(),
            "third task".into(),
            vec![t2_id.clone()],
            move |_| {
                thread::sleep(Duration::from_millis(20));
                order_clone.lock().unwrap().push("T3".into());
                Ok("T3 result".into())
            },
        )
        .expect("T3 must spawn");

    // Await third task with timeout
    let t3_snap = manager
        .await_task(&t3_id, Some(Duration::from_secs(5)))
        .expect("T3 must complete successfully");
    assert_eq!(t3_snap.status, TaskStatus::Completed);

    let t1_snap = manager.get_task(&t1_id).unwrap();
    let t2_snap = manager.get_task(&t2_id).unwrap();
    assert_eq!(t1_snap.status, TaskStatus::Completed);
    assert_eq!(t2_snap.status, TaskStatus::Completed);

    // Verify strict serial execution order: T1 -> T2 -> T3
    let order = execution_order.lock().unwrap().clone();
    assert_eq!(
        order,
        vec!["T1", "T2", "T3"],
        "Tasks must execute in strict topological dependency order"
    );
}

#[test]
fn challenge_dag_diamond_concurrency_and_final_join() {
    let temp = TestTempDir::new("diamond_graph");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    let execution_log = Arc::new(Mutex::new(Vec::<String>::new()));

    // Root Task A
    let log_clone = execution_log.clone();
    let (root_id, _) = manager
        .spawn_task_with_dependencies("root_A".into(), "Root task".into(), vec![], move |_| {
            thread::sleep(Duration::from_millis(30));
            log_clone.lock().unwrap().push("Root".into());
            Ok("Root done".into())
        })
        .expect("Root must spawn");

    // Branch B depends on Root
    let log_clone = execution_log.clone();
    let (b_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_B".into(),
            "Branch B".into(),
            vec![root_id.clone()],
            move |_| {
                thread::sleep(Duration::from_millis(60));
                log_clone.lock().unwrap().push("BranchB".into());
                Ok("BranchB done".into())
            },
        )
        .expect("Branch B must spawn");

    // Branch C depends on Root
    let log_clone = execution_log.clone();
    let (c_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_C".into(),
            "Branch C".into(),
            vec![root_id.clone()],
            move |_| {
                thread::sleep(Duration::from_millis(60));
                log_clone.lock().unwrap().push("BranchC".into());
                Ok("BranchC done".into())
            },
        )
        .expect("Branch C must spawn");

    // Join D depends on both B and C
    let log_clone = execution_log.clone();
    let (d_id, _) = manager
        .spawn_task_with_dependencies(
            "join_D".into(),
            "Join D".into(),
            vec![b_id.clone(), c_id.clone()],
            move |_| {
                log_clone.lock().unwrap().push("JoinD".into());
                Ok("JoinD done".into())
            },
        )
        .expect("Join D must spawn");

    let d_snap = manager
        .await_task(&d_id, Some(Duration::from_secs(5)))
        .expect("Join D must finish");
    assert_eq!(d_snap.status, TaskStatus::Completed);

    let log = execution_log.lock().unwrap().clone();
    assert_eq!(log[0], "Root", "Root must execute first");
    assert_eq!(log[3], "JoinD", "Join D must execute last after both branches");
    assert!(
        (log[1] == "BranchB" && log[2] == "BranchC") || (log[1] == "BranchC" && log[2] == "BranchB"),
        "Both branches must execute before JoinD"
    );
}

#[test]
fn challenge_dag_topological_sort_kahn_validation() {
    // Construct multi-node DAG:
    // A -> B, A -> C
    // B -> D
    // C -> D, C -> E
    // D -> F, E -> F
    let tasks = vec![
        DagTask::new("A", vec![]),
        DagTask::new("B", vec!["A".into()]),
        DagTask::new("C", vec!["A".into()]),
        DagTask::new("D", vec!["B".into(), "C".into()]),
        DagTask::new("E", vec!["C".into()]),
        DagTask::new("F", vec!["D".into(), "E".into()]),
    ];

    assert!(DagValidator::validate(&tasks).is_ok());
    let sorted = DagValidator::topological_sort(&tasks).expect("Sort must succeed");

    // Verify properties of topological sort
    let pos: std::collections::HashMap<&str, usize> =
        sorted.iter().enumerate().map(|(idx, s)| (s.as_str(), idx)).collect();

    assert!(pos["A"] < pos["B"]);
    assert!(pos["A"] < pos["C"]);
    assert!(pos["B"] < pos["D"]);
    assert!(pos["C"] < pos["D"]);
    assert!(pos["C"] < pos["E"]);
    assert!(pos["D"] < pos["F"]);
    assert!(pos["E"] < pos["F"]);
}

// ============================================================================
// SUITE 2: Cycle Detection & Missing Dependency Verification
// ============================================================================

#[test]
fn challenge_dag_reject_self_cycle() {
    let temp = TestTempDir::new("self_cycle");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    // 1. Direct validation check
    let self_task = vec![DagTask::new("T1", vec!["T1".into()])];
    let err = DagValidator::validate(&self_task).unwrap_err();
    assert!(
        matches!(err, DagValidationError::CycleDetected(_)),
        "DagValidator must detect self cycle"
    );

    // 2. TaskManager check when trying to spawn self-dependency
    let res = manager.spawn_task_with_dependencies(
        "self_cycle_task".into(),
        "depends on next id".into(),
        vec!["task-1".into()], // Since counter starts at 1, "task-1" would be self
        |_| Ok("never".into()),
    );
    assert!(
        res.is_err(),
        "TaskManager must reject task referencing its own ID or missing ID"
    );
}

#[test]
fn challenge_dag_reject_direct_and_indirect_cycles() {
    // 1. Direct 2-node cycle: A -> B, B -> A
    let direct = vec![
        DagTask::new("A", vec!["B".into()]),
        DagTask::new("B", vec!["A".into()]),
    ];
    let err = DagValidator::validate(&direct).unwrap_err();
    if let DagValidationError::CycleDetected(path) = err {
        assert!(path.contains(&"A".to_string()) && path.contains(&"B".to_string()));
    } else {
        panic!("Expected CycleDetected error");
    }

    // 2. Transitive 3-node cycle: A -> B -> C -> A
    let transitive_3 = vec![
        DagTask::new("A", vec!["B".into()]),
        DagTask::new("B", vec!["C".into()]),
        DagTask::new("C", vec!["A".into()]),
    ];
    assert!(matches!(
        DagValidator::validate(&transitive_3).unwrap_err(),
        DagValidationError::CycleDetected(_)
    ));

    // 3. Transitive 5-node cycle: A -> B -> C -> D -> E -> A
    let transitive_5 = vec![
        DagTask::new("A", vec!["B".into()]),
        DagTask::new("B", vec!["C".into()]),
        DagTask::new("C", vec!["D".into()]),
        DagTask::new("D", vec!["E".into()]),
        DagTask::new("E", vec!["A".into()]),
    ];
    assert!(matches!(
        DagValidator::validate(&transitive_5).unwrap_err(),
        DagValidationError::CycleDetected(_)
    ));

    // 4. Disconnected graph with cycle in secondary subgraph
    let disconnected = vec![
        DagTask::new("Valid1", vec![]),
        DagTask::new("Valid2", vec!["Valid1".into()]),
        DagTask::new("CycleX", vec!["CycleY".into()]),
        DagTask::new("CycleY", vec!["CycleX".into()]),
    ];
    assert!(matches!(
        DagValidator::validate(&disconnected).unwrap_err(),
        DagValidationError::CycleDetected(_)
    ));
}

#[test]
fn challenge_dag_reject_missing_dependency() {
    let temp = TestTempDir::new("missing_dep");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    // Spawn task with non-existent prerequisite
    let res = manager.spawn_task_with_dependencies(
        "orphan".into(),
        "depends on ghost".into(),
        vec!["ghost-task-999".into()],
        |_| Ok("should not run".into()),
    );

    match res {
        Err(TaskError::MissingDependency { task: _, missing }) => {
            assert_eq!(missing, "ghost-task-999");
        }
        other => panic!("Expected MissingDependency error, got: {:?}", other),
    }

    // Also verify DagValidator::validate direct error type
    let tasks = vec![DagTask::new("A", vec!["phantom".into()])];
    let err = DagValidator::validate(&tasks).unwrap_err();
    assert_eq!(
        err,
        DagValidationError::MissingDependency {
            task: "A".into(),
            missing: "phantom".into(),
        }
    );
}

// ============================================================================
// SUITE 3: Cascades (Failure, Cancellation & Branch Isolation)
// ============================================================================

#[test]
fn challenge_dag_failure_cascade_aborts_all_downstream_tasks() {
    let temp = TestTempDir::new("failure_cascade");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    let downstream_ran = Arc::new(AtomicBool::new(false));

    // 1. Root task fails intentionally
    let (root_id, _) = manager
        .spawn_task_with_dependencies(
            "failing_root".into(),
            "fails immediately".into(),
            vec![],
            |_| anyhow::bail!("Catastrophic hardware fault in root"),
        )
        .unwrap();

    // 2. Branch 1 depends on root
    let ran_clone = downstream_ran.clone();
    let (b1_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_1".into(),
            "child 1".into(),
            vec![root_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("ran".into())
            },
        )
        .unwrap();

    // 3. Branch 2 depends on root
    let ran_clone = downstream_ran.clone();
    let (b2_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_2".into(),
            "child 2".into(),
            vec![root_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("ran".into())
            },
        )
        .unwrap();

    // 4. Join depends on Branch 1 and 2
    let ran_clone = downstream_ran.clone();
    let (join_id, _) = manager
        .spawn_task_with_dependencies(
            "join_leaf".into(),
            "grandchild".into(),
            vec![b1_id.clone(), b2_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("ran".into())
            },
        )
        .unwrap();

    // Await Join task to terminal state
    let join_snap = manager
        .await_task(&join_id, Some(Duration::from_secs(5)))
        .expect("Join task must terminate");

    // Invariants verification
    assert_eq!(join_snap.status, TaskStatus::Failed);
    assert!(
        join_snap.error.as_ref().unwrap().contains("failed"),
        "Join must indicate prerequisite failure in error message"
    );

    let b1_snap = manager.get_task(&b1_id).unwrap();
    let b2_snap = manager
        .await_task(&b2_id, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(b1_snap.status, TaskStatus::Failed);
    assert_eq!(b2_snap.status, TaskStatus::Failed);

    let root_snap = manager.get_task(&root_id).unwrap();
    assert_eq!(root_snap.status, TaskStatus::Failed);
    assert!(root_snap.error.unwrap().contains("Catastrophic hardware fault"));

    // CRITICAL: Ensure no downstream closures were ever invoked
    assert!(
        !downstream_ran.load(Ordering::SeqCst),
        "Downstream tasks MUST NOT execute closures when prerequisite fails"
    );
}

#[test]
fn challenge_dag_cancellation_cascade_aborts_all_downstream_tasks() {
    let temp = TestTempDir::new("cancel_cascade");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    let downstream_ran = Arc::new(AtomicBool::new(false));

    // 1. Root task sleeping
    let (root_id, _) = manager
        .spawn_task_with_dependencies(
            "long_root".into(),
            "slow root".into(),
            vec![],
            |token| {
                for _ in 0..50 {
                    if token.is_cancelled() {
                        anyhow::bail!("Cancelled in flight");
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Ok("completed".into())
            },
        )
        .unwrap();

    // 2. Child task
    let ran_clone = downstream_ran.clone();
    let (child_id, _) = manager
        .spawn_task_with_dependencies(
            "child_task".into(),
            "child of root".into(),
            vec![root_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("child ran".into())
            },
        )
        .unwrap();

    // 3. Grandchild task
    let ran_clone = downstream_ran.clone();
    let (grandchild_id, _) = manager
        .spawn_task_with_dependencies(
            "grandchild_task".into(),
            "grandchild of root".into(),
            vec![child_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("grandchild ran".into())
            },
        )
        .unwrap();

    // Cancel root task while in-flight
    thread::sleep(Duration::from_millis(20));
    let cancel_res = manager.cancel_task(&root_id);
    assert!(cancel_res.is_ok(), "Root cancellation must succeed");

    // Await grandchild
    let gc_snap = manager
        .await_task(&grandchild_id, Some(Duration::from_secs(5)))
        .expect("Grandchild must reach terminal state");

    assert_eq!(gc_snap.status, TaskStatus::Cancelled);
    let child_snap = manager.get_task(&child_id).unwrap();
    assert_eq!(child_snap.status, TaskStatus::Cancelled);

    assert!(
        !downstream_ran.load(Ordering::SeqCst),
        "Downstream closures MUST NOT execute when prerequisite was cancelled"
    );
}

#[test]
fn challenge_dag_partial_branch_failure_isolation() {
    let temp = TestTempDir::new("partial_failure");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    let (root_id, _) = manager
        .spawn_task_with_dependencies("root".into(), "root".into(), vec![], |_| Ok("Root OK".into()))
        .unwrap();

    // Branch A succeeds
    let (a_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_good".into(),
            "successful branch".into(),
            vec![root_id.clone()],
            |_| Ok("Branch A OK".into()),
        )
        .unwrap();

    // Branch B fails
    let (b_id, _) = manager
        .spawn_task_with_dependencies(
            "branch_bad".into(),
            "failing branch".into(),
            vec![root_id.clone()],
            |_| anyhow::bail!("Branch B exploded"),
        )
        .unwrap();

    // Join depends on both
    let (join_id, _) = manager
        .spawn_task_with_dependencies(
            "join".into(),
            "join".into(),
            vec![a_id.clone(), b_id.clone()],
            |_| Ok("Join OK".into()),
        )
        .unwrap();

    let join_snap = manager.await_task(&join_id, Some(Duration::from_secs(5))).unwrap();
    let a_snap = manager.get_task(&a_id).unwrap();
    let b_snap = manager.get_task(&b_id).unwrap();

    // Branch A succeeded cleanly despite sibling Branch B failing
    assert_eq!(a_snap.status, TaskStatus::Completed);
    assert_eq!(b_snap.status, TaskStatus::Failed);
    assert_eq!(join_snap.status, TaskStatus::Failed);
}

#[test]
fn challenge_dag_cancellation_while_awaiting_prerequisite() {
    let temp = TestTempDir::new("cancel_while_awaiting");
    let manager = TaskManager::with_dir(temp.path().join(".ctrl"));

    // Slow root task
    let (root_id, _) = manager
        .spawn_task_with_dependencies(
            "slow_root".into(),
            "runs 200ms".into(),
            vec![],
            |_| {
                thread::sleep(Duration::from_millis(200));
                Ok("slow root success".into())
            },
        )
        .unwrap();

    // Downstream task waiting on slow root
    let downstream_ran = Arc::new(AtomicBool::new(false));
    let ran_clone = downstream_ran.clone();
    let (downstream_id, _) = manager
        .spawn_task_with_dependencies(
            "downstream".into(),
            "waits on root".into(),
            vec![root_id.clone()],
            move |_| {
                ran_clone.store(true, Ordering::SeqCst);
                Ok("downstream ran".into())
            },
        )
        .unwrap();

    // Cancel downstream task WHILE it is queued/waiting for slow root
    thread::sleep(Duration::from_millis(30));
    assert!(manager.cancel_task(&downstream_id).is_ok());

    let down_snap = manager
        .await_task(&downstream_id, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(down_snap.status, TaskStatus::Cancelled);

    // Verify root finishes normally without corruption
    let root_snap = manager.await_task(&root_id, Some(Duration::from_secs(5))).unwrap();
    assert_eq!(root_snap.status, TaskStatus::Completed);

    assert!(
        !downstream_ran.load(Ordering::SeqCst),
        "Downstream task cancelled while queued must not execute"
    );
}

// ============================================================================
// SUITE 4: Pure Rust 5-Field Cron Parsing & Evaluation
// ============================================================================

#[test]
fn challenge_cron_5_field_syntax_parsing() {
    // 1. Wildcard expression: all values in range
    let wild = CronSchedule::parse("* * * * *").unwrap();
    if let CronSchedule::Pattern(p) = wild {
        assert_eq!(p.minutes.len(), 60);
        assert_eq!(p.hours.len(), 24);
        assert_eq!(p.days_of_month.len(), 31);
        assert_eq!(p.months.len(), 12);
        assert_eq!(p.days_of_week.len(), 7);
    } else {
        panic!("Expected Pattern");
    }

    // 2. Complex stepped ranges and lists:
    // */15 (minutes: 0, 15, 30, 45)
    // 9-17 (hours: 9..=17)
    // 1,15 (dom: 1, 15)
    // 1-6/2 (months: 1, 3, 5)
    // 1-5 (dow: 1..=5)
    let complex = CronSchedule::parse("*/15 9-17 1,15 1-6/2 1-5").unwrap();
    if let CronSchedule::Pattern(p) = complex {
        assert_eq!(p.minutes, HashSet::from([0, 15, 30, 45]));
        assert_eq!(p.hours, (9..=17).collect::<HashSet<u32>>());
        assert_eq!(p.days_of_month, HashSet::from([1, 15]));
        assert_eq!(p.months, HashSet::from([1, 3, 5]));
        assert_eq!(p.days_of_week, HashSet::from([1, 2, 3, 4, 5]));
    } else {
        panic!("Expected Pattern");
    }

    // 3. Day of week alias 7 translates to 0 (Sunday)
    let sunday = CronSchedule::parse("0 0 * * 7").unwrap();
    if let CronSchedule::Pattern(p) = sunday {
        assert!(p.days_of_week.contains(&0));
        assert!(!p.days_of_week.contains(&7));
    } else {
        panic!("Expected Pattern");
    }
}

#[test]
fn challenge_cron_macros_and_duration_intervals() {
    // Standard Cron Macros
    let hourly = CronSchedule::parse("@hourly").unwrap();
    assert_eq!(hourly.expr(), "@hourly");

    let daily = CronSchedule::parse("@daily").unwrap();
    assert_eq!(daily.expr(), "@daily");

    let midnight = CronSchedule::parse("@midnight").unwrap();
    assert_eq!(midnight.expr(), "@midnight");

    let weekly = CronSchedule::parse("@weekly").unwrap();
    assert_eq!(weekly.expr(), "@weekly");

    let monthly = CronSchedule::parse("@monthly").unwrap();
    assert_eq!(monthly.expr(), "@monthly");

    let yearly = CronSchedule::parse("@yearly").unwrap();
    assert_eq!(yearly.expr(), "@yearly");

    let annually = CronSchedule::parse("@annually").unwrap();
    assert_eq!(annually.expr(), "@annually");

    // Intervals via @every
    let ms_int = CronSchedule::parse("@every 15ms").unwrap();
    if let CronSchedule::Interval { duration, .. } = ms_int {
        assert_eq!(duration, Duration::from_millis(15));
    } else {
        panic!("Expected Interval");
    }

    let sec_int = CronSchedule::parse("@every 10s").unwrap();
    if let CronSchedule::Interval { duration, .. } = sec_int {
        assert_eq!(duration, Duration::from_secs(10));
    } else {
        panic!("Expected Interval");
    }

    let min_int = CronSchedule::parse("@every 5m").unwrap();
    if let CronSchedule::Interval { duration, .. } = min_int {
        assert_eq!(duration, Duration::from_secs(300));
    } else {
        panic!("Expected Interval");
    }

    let hr_int = CronSchedule::parse("@every 2h").unwrap();
    if let CronSchedule::Interval { duration, .. } = hr_int {
        assert_eq!(duration, Duration::from_secs(7200));
    } else {
        panic!("Expected Interval");
    }

    let day_int = CronSchedule::parse("@every 1d").unwrap();
    if let CronSchedule::Interval { duration, .. } = day_int {
        assert_eq!(duration, Duration::from_secs(86400));
    } else {
        panic!("Expected Interval");
    }
}

#[test]
fn challenge_cron_malformed_syntax_strict_rejection() {
    // Empty & field counts
    assert!(CronSchedule::parse("").is_err());
    assert!(CronSchedule::parse("* * * *").is_err(), "4 fields must fail");
    assert!(CronSchedule::parse("* * * * * *").is_err(), "6 fields must fail");

    // Out of bounds values
    assert!(CronSchedule::parse("60 * * * *").is_err(), "Minute 60 must fail");
    assert!(CronSchedule::parse("* 24 * * *").is_err(), "Hour 24 must fail");
    assert!(CronSchedule::parse("* * 0 * *").is_err(), "DOM 0 must fail");
    assert!(CronSchedule::parse("* * 32 * *").is_err(), "DOM 32 must fail");
    assert!(CronSchedule::parse("* * * 0 *").is_err(), "Month 0 must fail");
    assert!(CronSchedule::parse("* * * 13 *").is_err(), "Month 13 must fail");
    assert!(CronSchedule::parse("* * * * 8").is_err(), "DOW 8 must fail");

    // Invalid step & range
    assert!(CronSchedule::parse("*/0 * * * *").is_err(), "Step 0 must fail");
    assert!(CronSchedule::parse("20-10 * * * *").is_err(), "Inverted range must fail");

    // Malformed strings & unknown macros
    assert!(CronSchedule::parse("abc * * * *").is_err());
    assert!(CronSchedule::parse("@biweekly").is_err());
    assert!(CronSchedule::parse("@every").is_err());
    assert!(CronSchedule::parse("@every 100xyz").is_err());
}

#[test]
fn challenge_utc_datetime_hinnant_civil_conversion() {
    // 1970-01-01 00:00:00 UTC
    let epoch = SystemTime::UNIX_EPOCH;
    let dt = UtcDateTime::from_system_time(epoch);
    assert_eq!(dt.year, 1970);
    assert_eq!(dt.month, 1);
    assert_eq!(dt.day, 1);
    assert_eq!(dt.hour, 0);
    assert_eq!(dt.minute, 0);
    assert_eq!(dt.second, 0);
    assert_eq!(dt.weekday, 4); // Thursday

    // 2026-09-14 15:25:22 UTC (1789399522 secs)
    let dt2 = UtcDateTime::from_system_time(UNIX_EPOCH + Duration::from_secs(1789399522));
    assert_eq!(dt2.year, 2026);
    assert_eq!(dt2.month, 9);
    assert_eq!(dt2.day, 14);
    assert_eq!(dt2.hour, 15);
    assert_eq!(dt2.minute, 25);
    assert_eq!(dt2.second, 22);
    assert_eq!(dt2.weekday, 1); // Monday
}

// ============================================================================
// SUITE 5: TaskScheduler Background Thread Execution & Resilience
// ============================================================================

#[test]
fn challenge_scheduler_interval_execution_and_capping() {
    let scheduler = TaskScheduler::with_tick_interval(Duration::from_millis(5));
    let run_count = Arc::new(AtomicUsize::new(0));
    let count_clone = run_count.clone();

    scheduler
        .register_task(
            "capped-task",
            "Capped",
            "Increment 4 times",
            "@every 15ms",
            Some(4),
            move || {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok("done".into())
            },
        )
        .expect("Registration must succeed");

    scheduler.start();

    // Await execution
    let start = Instant::now();
    while run_count.load(Ordering::SeqCst) < 4 && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }

    scheduler.stop();

    // Verify exactly 4 iterations executed and schedule deactivated
    assert_eq!(run_count.load(Ordering::SeqCst), 4);
    let schedules = scheduler.list_schedules();
    assert_eq!(schedules.len(), 1);
    assert!(!schedules[0].active, "Schedule must be inactive after reaching max_iterations");
    assert_eq!(schedules[0].current_iterations, 4);
    assert_eq!(schedules[0].max_iterations, Some(4));
    assert!(schedules[0].last_run.is_some(), "last_run must be recorded");
}

#[test]
fn challenge_scheduler_concurrent_multi_task_execution() {
    let scheduler = TaskScheduler::with_tick_interval(Duration::from_millis(5));
    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));

    let c_a = counter_a.clone();
    scheduler
        .register_task("task-a", "A", "Fast task", "@every 10ms", Some(3), move || {
            c_a.fetch_add(1, Ordering::SeqCst);
            Ok("A".into())
        })
        .unwrap();

    let c_b = counter_b.clone();
    scheduler
        .register_task("task-b", "B", "Slower task", "@every 20ms", Some(2), move || {
            c_b.fetch_add(1, Ordering::SeqCst);
            Ok("B".into())
        })
        .unwrap();

    scheduler.start();

    let start = Instant::now();
    while (counter_a.load(Ordering::SeqCst) < 3 || counter_b.load(Ordering::SeqCst) < 2)
        && start.elapsed() < Duration::from_secs(3)
    {
        thread::sleep(Duration::from_millis(10));
    }

    scheduler.stop();

    assert_eq!(counter_a.load(Ordering::SeqCst), 3);
    assert_eq!(counter_b.load(Ordering::SeqCst), 2);
}

#[test]
fn challenge_scheduler_dynamic_cancellation() {
    let scheduler = TaskScheduler::with_tick_interval(Duration::from_millis(5));
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    scheduler
        .register_task("cancel-test", "Cancel", "Infinite task", "@every 10ms", None, move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok("tick".into())
        })
        .unwrap();

    scheduler.start();

    // Wait until at least 2 ticks
    let start = Instant::now();
    while counter.load(Ordering::SeqCst) < 2 && start.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }

    // Cancel schedule
    let cancelled = scheduler.cancel_schedule("cancel-test");
    assert!(cancelled, "cancel_schedule must return true for active task");

    let count_at_cancel = counter.load(Ordering::SeqCst);
    // Sleep to ensure no further triggers occur
    thread::sleep(Duration::from_millis(60));
    let count_after = counter.load(Ordering::SeqCst);

    scheduler.stop();

    assert!(
        count_after <= count_at_cancel + 1,
        "Cancelled schedule must halt triggers immediately. At cancel: {}, After: {}",
        count_at_cancel,
        count_after
    );
}

#[test]
fn challenge_scheduler_runner_panic_safety() {
    let scheduler = TaskScheduler::with_tick_interval(Duration::from_millis(5));
    let healthy_count = Arc::new(AtomicUsize::new(0));

    // Panicking task
    scheduler
        .register_task("panicker", "Panics", "Panicking task", "@every 10ms", Some(3), || {
            panic!("Intentional runner panic for adversarial test");
        })
        .unwrap();

    // Healthy task
    let healthy_clone = healthy_count.clone();
    scheduler
        .register_task("healthy", "Healthy", "Normal task", "@every 15ms", Some(3), move || {
            healthy_clone.fetch_add(1, Ordering::SeqCst);
            Ok("ok".into())
        })
        .unwrap();

    scheduler.start();

    let start = Instant::now();
    while healthy_count.load(Ordering::SeqCst) < 3 && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }

    scheduler.stop();

    // Healthy task must have completed all 3 iterations uninhibited by the panicking task
    assert_eq!(
        healthy_count.load(Ordering::SeqCst),
        3,
        "Scheduler background loop must survive runner panics"
    );
}
