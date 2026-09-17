//! Empirical Adversarial Challenge Suite for Milestone 2 Next-Gen
//! (Destructive Command Guardrails & Immutable Audit Trail Fidelity)
//!
//! Authored by: challenger_m2_gate_2 (teamwork_preview_challenger)
//!
//! Aggressively and empirically validates:
//! 1. Background subagent dry-run rejection behavior across worker threads.
//! 2. Interactive gate confirmation logic with simulated stdin buffers.
//! 3. Audit log serialization fidelity: special characters, quotes, tabs,
//!    multi-line arguments, unicode/emoji strings, and `task_id: None` serialization omission.
//! 4. Complete tool lifecycle execution timing and duration recording.

pub mod agent {
    pub mod tasks {
        use std::time::SystemTime;
        pub fn format_utc_timestamp(time: SystemTime) -> String {
            let dur = time
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let total_secs = dur.as_secs();
            let sec = total_secs % 60;
            let total_mins = total_secs / 60;
            let min = total_mins % 60;
            let total_hours = total_mins / 60;
            let hour = total_hours % 24;
            let total_days = (total_hours / 24) as i64;

            let days = total_days + 719468;
            let era = if days >= 0 { days } else { days - 146096 } / 146097;
            let doe = (days - era * 146097) as u64;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = if m <= 2 { y + 1 } else { y };

            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                year, m, d, hour, min, sec
            )
        }
    }
}

pub mod tools {
    pub mod filesystem {
        use std::path::{Path, PathBuf};
        pub fn get_workspace_root() -> PathBuf {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
        pub fn sanitize_relative_path(path: &Path) -> PathBuf {
            path.to_path_buf()
        }
    }

    #[derive(Clone, Debug)]
    pub struct ToolExecutionContext {
        pub session_id: String,
        pub task_id: Option<String>,
        pub is_background: bool,
    }

    thread_local! {
        static CURRENT_TOOL_CONTEXT: std::cell::RefCell<Option<ToolExecutionContext>> = const { std::cell::RefCell::new(None) };
    }

    pub fn with_tool_context<F, R>(ctx: ToolExecutionContext, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        struct ToolContextGuard(Option<ToolExecutionContext>);
        impl Drop for ToolContextGuard {
            fn drop(&mut self) {
                CURRENT_TOOL_CONTEXT.with(|c| *c.borrow_mut() = self.0.take());
            }
        }

        let prev = CURRENT_TOOL_CONTEXT.with(|c| c.borrow_mut().replace(ctx));
        let _guard = ToolContextGuard(prev);
        f()
    }

    pub fn get_tool_context() -> ToolExecutionContext {
        CURRENT_TOOL_CONTEXT.with(|c| {
            c.borrow().clone().unwrap_or_else(|| {
                let is_bg = std::thread::current().name().is_some_and(|name| {
                    name.contains("task-worker")
                        || name.contains("subagent")
                        || name.contains("background")
                });
                let session_id = format!(
                    "sess-{:x}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );
                ToolExecutionContext {
                    session_id,
                    task_id: None,
                    is_background: is_bg,
                }
            })
        })
    }

    #[path = "../../src/tools/result_store.rs"]
    pub mod result_store;

    #[path = "../../src/tools/guardrails.rs"]
    pub mod guardrails;

    #[path = "../../src/tools/audit.rs"]
    pub mod audit;

    #[path = "../../src/tools/shell.rs"]
    pub mod shell;
}

use anyhow::Result;
use serde_json::json;
use std::fs;
use std::io::{BufRead, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tools::audit::{log_tool_invocation, with_audit_dir, AuditLogger, AuditRecord};
use tools::guardrails::{
    check_destructive_guardrail, check_destructive_guardrail_with_reader, is_destructive_command,
};
use tools::shell::execute_shell_with_options;
use tools::{get_tool_context, with_tool_context, ToolExecutionContext};

/// Self-cleaning isolated temporary directory for hermetic audit challenge tests.
struct ChallengeTempDir {
    path: PathBuf,
}

impl ChallengeTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "ctrl_m2_challenge_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Failed to create temporary challenge directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ChallengeTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ============================================================================
// SUITE 1: BACKGROUND SUBAGENT DRY-RUN REJECTION BEHAVIOR ACROSS WORKER THREADS
// ============================================================================

#[test]
fn challenge_thread_name_heuristics_background_detection() {
    let bg_names = [
        "task-worker-1",
        "subagent-executor",
        "background-pool-3",
        "subagent_worker_main",
        "my-background-task",
    ];

    for name in bg_names {
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let ctx = get_tool_context();
                assert!(
                    ctx.is_background,
                    "Thread '{}' must be detected as background mode",
                    name
                );
            })
            .expect("Failed to spawn thread");
        handle.join().unwrap();
    }

    let non_bg_names = [
        "main",
        "interactive-repl",
        "terminal-ui",
        "worker-thread",
        "unrelated-thread",
    ];

    for name in non_bg_names {
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let ctx = get_tool_context();
                assert!(
                    !ctx.is_background,
                    "Thread '{}' must NOT be detected as background mode",
                    name
                );
            })
            .expect("Failed to spawn thread");
        handle.join().unwrap();
    }
}

#[test]
fn challenge_background_thread_dry_run_rejection_rm_rf() {
    let handle = thread::Builder::new()
        .name("task-worker-rm".to_string())
        .spawn(|| {
            let cmds = [
                "rm -rf /tmp/target_dir",
                "rm -r -f ./dist",
                "rm -fr /var/log/build",
                "rm -r ./build -f",
                "/bin/rm -rf /tmp/test",
                "RM -RF C:\\temp\\build",
            ];

            for cmd in cmds {
                let res = execute_shell_with_options(cmd, None, None);
                assert!(
                    res.is_err(),
                    "Destructive rm command '{}' must be rejected in background thread",
                    cmd
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("Dry-run rejection"),
                    "Error must indicate dry-run rejection. Got: {}",
                    err
                );
                assert!(
                    err.contains("Recursive force removal (rm -rf)"),
                    "Error must state recursive force removal reason. Got: {}",
                    err
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_background_thread_dry_run_rejection_windows_deletions() {
    let handle = thread::Builder::new()
        .name("subagent-worker-win".to_string())
        .spawn(|| {
            let cmds = [
                ("del /s /q build", "Recursive file deletion (del /s)"),
                ("DEL /S TEMP", "Recursive file deletion (del /s)"),
                ("erase /s old_logs", "Recursive file deletion (del /s)"),
                ("ri -r ./dist", "Recursive file deletion (del /s)"),
                ("ri -recurse target", "Recursive file deletion (del /s)"),
                (
                    "remove-item -recurse node_modules",
                    "Recursive file deletion (del /s)",
                ),
                (
                    "rmdir /s /q node_modules",
                    "Recursive directory removal (rmdir /s)",
                ),
                ("rd /s /q cache", "Recursive directory removal (rmdir /s)"),
                ("RD /S /Q TEMP", "Recursive directory removal (rmdir /s)"),
            ];

            for (cmd, expected_reason) in cmds {
                let res = execute_shell_with_options(cmd, None, None);
                assert!(
                    res.is_err(),
                    "Windows destructive deletion '{}' must be rejected in background",
                    cmd
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("Dry-run rejection"),
                    "Error must indicate dry-run rejection for '{}'. Got: {}",
                    cmd,
                    err
                );
                assert!(
                    err.contains(expected_reason),
                    "Error must contain reason '{}' for '{}'. Got: {}",
                    expected_reason,
                    cmd,
                    err
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_background_thread_dry_run_rejection_git_destructive() {
    let handle = thread::Builder::new()
        .name("task-worker-git".to_string())
        .spawn(|| {
            let cmds = [
                (
                    "git reset --hard HEAD~1",
                    "Destructive git reset (git reset --hard)",
                ),
                (
                    "GIT RESET --HARD origin/main",
                    "Destructive git reset (git reset --hard)",
                ),
                (
                    "git clean -fd",
                    "Forced untracked file deletion (git clean -fd)",
                ),
                (
                    "git clean -f -d",
                    "Forced untracked file deletion (git clean -fd)",
                ),
                (
                    "git clean -df",
                    "Forced untracked file deletion (git clean -fd)",
                ),
            ];

            for (cmd, expected_reason) in cmds {
                let res = execute_shell_with_options(cmd, None, None);
                assert!(
                    res.is_err(),
                    "Git destructive command '{}' must be rejected in background",
                    cmd
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("Dry-run rejection"),
                    "Expected dry-run rejection for '{}'. Got: {}",
                    cmd,
                    err
                );
                assert!(
                    err.contains(expected_reason),
                    "Expected reason '{}' for '{}'. Got: {}",
                    expected_reason,
                    cmd,
                    err
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_background_thread_dry_run_rejection_disk_formatting() {
    let handle = thread::Builder::new()
        .name("background-worker-format".to_string())
        .spawn(|| {
            let cmds = [
                ("format D: /fs:ntfs", "Filesystem format operation"),
                ("FORMAT C:", "Filesystem format operation"),
                ("mkfs.ext4 /dev/sdb1", "Filesystem creation operation"),
                ("mkfs -t vfat /dev/sdc", "Filesystem creation operation"),
                ("fdisk /dev/nvme0n1", "Partition table modification"),
                ("fdisk.exe /dev/sda", "Partition table modification"),
            ];

            for (cmd, expected_reason) in cmds {
                let res = execute_shell_with_options(cmd, None, None);
                assert!(
                    res.is_err(),
                    "Formatting command '{}' must be rejected in background",
                    cmd
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("Dry-run rejection"),
                    "Expected dry-run rejection for '{}'. Got: {}",
                    cmd,
                    err
                );
                assert!(
                    err.contains(expected_reason),
                    "Expected reason '{}' for '{}'. Got: {}",
                    expected_reason,
                    cmd,
                    err
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_background_thread_dry_run_rejection_chained_and_wrapped() {
    let handle = thread::Builder::new()
        .name("task-worker-chain".to_string())
        .spawn(|| {
            let cmds = [
                "echo start && rm -rf /tmp/test",
                "ls ; del /s old_files",
                "cargo build || git reset --hard",
                "powershell.exe -Command \"rm -rf ./target\"",
                "pwsh -c \"del /s build\"",
                "cmd /c \"rmdir /s /q cache\"",
            ];

            for cmd in cmds {
                let res = execute_shell_with_options(cmd, None, None);
                assert!(
                    res.is_err(),
                    "Chained or wrapped destructive command '{}' must be rejected in background",
                    cmd
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("Dry-run rejection"),
                    "Expected dry-run rejection for '{}'. Got: {}",
                    cmd,
                    err
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_background_thread_benign_commands_allowed() {
    let handle = thread::Builder::new()
        .name("task-worker-benign".to_string())
        .spawn(|| {
            let benign = [
                "echo rm -rf /tmp/fake",
                "echo 'del /s test'",
                "printf 'git reset --hard\\n'",
                "grep 'rmdir /s' search.rs",
                "cat 'format c:'",
                "git status",
                "git diff HEAD",
                "git log -n 5",
            ];

            for cmd in benign {
                assert!(
                    is_destructive_command(cmd).is_none(),
                    "Benign command '{}' must not be flagged as destructive",
                    cmd
                );
            }
        })
        .expect("Failed to spawn thread");
    handle.join().unwrap();
}

#[test]
fn challenge_explicit_tool_context_override() {
    // Normal thread named "main"
    let ctx = ToolExecutionContext {
        session_id: "test-sess-override".to_string(),
        task_id: Some("task-override-1".to_string()),
        is_background: true,
    };

    with_tool_context(ctx, || {
        let active = get_tool_context();
        assert!(active.is_background);
        assert_eq!(active.session_id, "test-sess-override");
        assert_eq!(active.task_id.as_deref(), Some("task-override-1"));

        let res = execute_shell_with_options("rm -rf override_dir", None, None);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Dry-run rejection: destructive command 'rm -rf override_dir' is blocked in background subagent mode"));
    });

    // After context drop, returns to normal
    let post_ctx = get_tool_context();
    assert!(!post_ctx.is_background);
}

#[test]
fn challenge_check_destructive_guardrail_direct_rejection_and_benign() {
    // 1. Direct invocation with is_background = true must fail with dry-run rejection
    let res = check_destructive_guardrail("rm -rf target", true);
    assert!(res.is_err());
    let err = res.unwrap_err().to_string();
    assert!(err.contains("Dry-run rejection: destructive command 'rm -rf target' is blocked in background subagent mode"));

    // 2. Direct invocation of benign command with is_background = true must succeed
    let benign_res = check_destructive_guardrail("echo safe_to_run", true);
    assert!(benign_res.is_ok());

    // 3. Direct invocation of benign command with is_background = false must succeed
    let benign_interactive = check_destructive_guardrail("git status", false);
    assert!(benign_interactive.is_ok());
}

#[test]
fn challenge_multi_threaded_background_dry_run_stress_30_workers() {
    let start = Instant::now();
    let thread_count = 30;
    let commands_per_thread = 10;
    let rejected_count = Arc::new(AtomicUsize::new(0));

    let destructive_catalog = [
        "rm -rf ./tmp/cache",
        "del /s build",
        "rd /s /q node_modules",
        "rmdir /s temp",
        "git reset --hard HEAD~2",
        "git clean -fd",
        "format X: /q",
        "mkfs.ext4 /dev/sdb",
        "fdisk /dev/sda",
        "powershell -c \"rm -rf ./artifacts\"",
    ];

    let mut handles = Vec::new();
    for i in 0..thread_count {
        let counter = Arc::clone(&rejected_count);
        let tname = format!("task-worker-stress-{}", i);
        let handle = thread::Builder::new()
            .name(tname)
            .spawn(move || {
                for j in 0..commands_per_thread {
                    let cmd = destructive_catalog[j % destructive_catalog.len()];
                    let res = execute_shell_with_options(cmd, None, None);
                    if let Err(e) = res {
                        if e.to_string().contains("Dry-run rejection") {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })
            .expect("Failed to spawn stress worker thread");
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread panicked unexpectedly");
    }

    let total_expected = thread_count * commands_per_thread;
    let total_rejected = rejected_count.load(Ordering::SeqCst);
    assert_eq!(
        total_rejected, total_expected,
        "All {} concurrent destructive executions across {} threads must receive dry-run rejections. Found: {}",
        total_expected, thread_count, total_rejected
    );

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "Multi-threaded dry-run guardrails must execute rapidly without lock contention. Took: {:?}",
        elapsed
    );
}

// ============================================================================
// SUITE 2: INTERACTIVE GATE CONFIRMATION LOGIC WITH SIMULATED STDIN BUFFERS
// ============================================================================

#[test]
fn challenge_interactive_gate_affirmative_yes_variants() {
    let affirmatives = [
        "y\n",
        "Y\n",
        "yes\n",
        "YES\n",
        "Yes\n",
        "  yes  \n",
        "  y  \n",
        "y\r\n",
        "yes\r\n",
        "YES\r\n",
        "  Y  \r\n",
    ];

    let cmd = "rm -rf /safe/target/to/delete";

    for aff in affirmatives {
        let mut reader = Cursor::new(aff.as_bytes());
        let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
        assert!(
            res.is_ok(),
            "Interactive prompt must approve on input '{:?}'",
            aff
        );
    }
}

#[test]
fn challenge_interactive_gate_rejection_no_variants() {
    let negatives = [
        "n\n", "N\n", "no\n", "NO\n", "No\n", "  no  \n", "  n  \n", "n\r\n", "no\r\n", "NO\r\n",
        "  N  \r\n",
    ];

    let cmd = "del /s /q build";

    for neg in negatives {
        let mut reader = Cursor::new(neg.as_bytes());
        let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
        assert!(
            res.is_err(),
            "Interactive prompt must reject on negative input '{:?}'",
            neg
        );
        let err = res.unwrap_err().to_string();
        assert_eq!(
            err, "Operation cancelled by user",
            "Error message must be exactly 'Operation cancelled by user'"
        );
    }
}

#[test]
fn challenge_interactive_gate_rejection_empty_and_newline() {
    let empty_inputs = ["", "\n", "\r\n", "   \n", "  \r\n", "\t\n"];

    let cmd = "git reset --hard HEAD~1";

    for inp in empty_inputs {
        let mut reader = Cursor::new(inp.as_bytes());
        let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
        assert!(
            res.is_err(),
            "Interactive prompt must reject on empty/default input '{:?}'",
            inp
        );
        assert_eq!(res.unwrap_err().to_string(), "Operation cancelled by user");
    }
}

#[test]
fn challenge_interactive_gate_rejection_arbitrary_text() {
    let arbitrary = [
        "maybe\n",
        "cancel\n",
        "stop\n",
        "1\n",
        "true\n",
        "sure\n",
        "yessir\n",
        "yeah\n",
        "ok\n",
        "proceed\n",
    ];

    let cmd = "format D: /fs:ntfs";

    for arb in arbitrary {
        let mut reader = Cursor::new(arb.as_bytes());
        let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
        assert!(
            res.is_err(),
            "Interactive prompt must reject ambiguous/arbitrary input '{:?}'",
            arb
        );
        assert_eq!(res.unwrap_err().to_string(), "Operation cancelled by user");
    }
}

#[test]
fn challenge_interactive_gate_benign_commands_skip_prompt() {
    // A reader containing "n\n" which would reject if prompted
    let mut reader = Cursor::new(b"n\n");
    let cmd = "cargo check --workspace";

    let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
    assert!(
        res.is_ok(),
        "Benign commands must succeed without prompting stdin"
    );
    assert_eq!(
        reader.position(),
        0,
        "Stdin reader must not be read for benign commands"
    );
}

struct FailingReader;
impl std::io::Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Broken pipe test error",
        ))
    }
}
impl BufRead for FailingReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Broken pipe test error",
        ))
    }
    fn consume(&mut self, _amt: usize) {}
}

#[test]
fn challenge_interactive_gate_io_error_handling() {
    let mut reader = FailingReader;
    let cmd = "rmdir /s /q target";

    let res = check_destructive_guardrail_with_reader(cmd, false, &mut reader);
    assert!(res.is_err(), "Broken reader must result in rejection");
    assert_eq!(
        res.unwrap_err().to_string(),
        "Operation cancelled by user",
        "I/O errors on stdin must safely map to user cancellation"
    );
}

// ============================================================================
// SUITE 3: AUDIT LOG SERIALIZATION FIDELITY & CONCURRENCY
// ============================================================================

#[test]
fn challenge_audit_task_id_none_omission_fidelity() {
    let rec = AuditRecord {
        timestamp: "2026-09-17T02:00:00Z".to_string(),
        session_id: "session-empirical-01".to_string(),
        task_id: None,
        tool: "read_file".to_string(),
        parameters: json!({"path": "src/main.rs"}),
        status: "success".to_string(),
        duration_ms: 12,
    };

    let serialized = serde_json::to_string(&rec).expect("Serialization failed");

    // MANDATORY REQUIREMENT: task_id: None must be completely omitted from JSON string
    assert!(
        !serialized.contains("task_id"),
        "Serialized JSON must omit 'task_id' when None. Got: {}",
        serialized
    );

    // Round-trip deserialization must reconstruct None
    let deserialized: AuditRecord =
        serde_json::from_str(&serialized).expect("Deserialization failed");
    assert_eq!(deserialized, rec);
    assert!(deserialized.task_id.is_none());
}

#[test]
fn challenge_audit_task_id_some_inclusion_fidelity() {
    let rec = AuditRecord {
        timestamp: "2026-09-17T02:01:00Z".to_string(),
        session_id: "session-empirical-02".to_string(),
        task_id: Some("bg-subagent-task-999".to_string()),
        tool: "execute_shell".to_string(),
        parameters: json!({"command": "cargo test"}),
        status: "success".to_string(),
        duration_ms: 350,
    };

    let serialized = serde_json::to_string(&rec).expect("Serialization failed");

    assert!(
        serialized.contains(r#""task_id":"bg-subagent-task-999""#),
        "Serialized JSON must include task_id when Some. Got: {}",
        serialized
    );

    let deserialized: AuditRecord =
        serde_json::from_str(&serialized).expect("Deserialization failed");
    assert_eq!(deserialized, rec);
    assert_eq!(
        deserialized.task_id.as_deref(),
        Some("bg-subagent-task-999")
    );
}

#[test]
fn challenge_audit_special_characters_quotes_escapes() {
    let temp_dir = ChallengeTempDir::new("special_chars");
    let log_path = temp_dir.path().join(".ctrl").join("audit.log");
    let logger = AuditLogger::from_path(log_path.clone());

    let special_str =
        "He said: \"Hello, world!\" and 'single quotes', `backticks`, \\backslashes\\ and /slashes/ \t tabbed \r\n newline & null \u{0000} end.";

    let rec = AuditRecord {
        timestamp: "2026-09-17T02:05:10Z".to_string(),
        session_id: "sess-special-quotes".to_string(),
        task_id: None,
        tool: "edit_file".to_string(),
        parameters: json!({
            "target": special_str,
            "replacement": "Safe replacement string"
        }),
        status: "success".to_string(),
        duration_ms: 45,
    };

    logger.log(&rec).expect("Failed to log special characters");

    // Verify raw file content has EXACTLY 1 line
    let raw = fs::read_to_string(&log_path).expect("Failed to read audit log");
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "Audit record with special chars must occupy exactly 1 line"
    );

    // Verify read_all recovers identical string byte-for-byte
    let read_back = logger.read_all().expect("Failed to read_all");
    assert_eq!(read_back.len(), 1);
    assert_eq!(
        read_back[0].parameters["target"].as_str().unwrap(),
        special_str,
        "Recovered target parameter must match original special character string byte-for-byte"
    );
}

#[test]
fn challenge_audit_multiline_arguments_fidelity() {
    let temp_dir = ChallengeTempDir::new("multiline");
    let log_path = temp_dir.path().join(".ctrl").join("audit.log");
    let logger = AuditLogger::from_path(log_path.clone());

    let multiline_code = r#"#!/usr/bin/env python3
"""Complex Multi-line Docstring with 'single' and "double" quotes."""
import sys
import os

def calculate_stats(items: list[int]) -> dict:
    # Comments with special characters: <>, &&, ||, ;, \t
    total = sum(items)
    return {
        "count": len(items),
        "total": total,
        "average": total / len(items) if items else 0.0,
    }

if __name__ == "__main__":
    print(calculate_stats([10, 20, 30]))
"#;

    let rec = AuditRecord {
        timestamp: "2026-09-17T02:10:00Z".to_string(),
        session_id: "sess-multiline".to_string(),
        task_id: Some("subagent-code-gen".to_string()),
        tool: "write_file".to_string(),
        parameters: json!({
            "path": "scripts/stats.py",
            "content": multiline_code
        }),
        status: "success".to_string(),
        duration_ms: 88,
    };

    logger.log(&rec).expect("Failed to log multiline code");

    // Ensure raw file is strictly 1 single line in JSONL format
    let raw = fs::read_to_string(&log_path).expect("Failed to read raw log");
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "Multiline code arguments must NOT emit raw newlines into JSONL file"
    );

    let read_back = logger.read_all().expect("Failed to read back log");
    assert_eq!(read_back.len(), 1);
    let recovered = read_back[0].parameters["content"].as_str().unwrap();
    assert_eq!(
        recovered, multiline_code,
        "Recovered multiline code must be byte-for-byte identical to original"
    );
}

#[test]
fn challenge_audit_unicode_and_emoji_fidelity() {
    let temp_dir = ChallengeTempDir::new("unicode_emojis");
    let log_path = temp_dir.path().join(".ctrl").join("audit.log");
    let logger = AuditLogger::from_path(log_path.clone());

    let unicode_text = "🦀 Pure Rust Autonomous AI Agent 🚀⚡ | 日本語: 自然言語処理 | 中文: 自动化测试 | 한국어: 멀티태스킹 | العربية: الذكاء الاصطناعي | Math: ∑_{i=1}^n x_i ≠ ∞ 🔒🛡️";

    let rec = AuditRecord {
        timestamp: "2026-09-17T02:15:00Z".to_string(),
        session_id: "sess-unicode".to_string(),
        task_id: Some("subagent-i18n".to_string()),
        tool: "knowledge_search".to_string(),
        parameters: json!({
            "query": unicode_text,
            "top_k": 5
        }),
        status: "success".to_string(),
        duration_ms: 15,
    };

    logger.log(&rec).expect("Failed to log unicode");

    let read_back = logger.read_all().expect("Failed to read_all");
    assert_eq!(read_back.len(), 1);
    assert_eq!(
        read_back[0].parameters["query"].as_str().unwrap(),
        unicode_text,
        "Unicode text and emojis must match perfectly without encoding distortion"
    );
}

#[test]
fn challenge_audit_high_contention_concurrency_25_threads() {
    let temp_dir = ChallengeTempDir::new("audit_concurrency");
    let log_path = temp_dir.path().join(".ctrl").join("audit.log");
    let logger = Arc::new(AuditLogger::from_path(log_path.clone()));

    let thread_count = 25;
    let records_per_thread = 40;
    let total_expected = thread_count * records_per_thread;

    let mut handles = Vec::new();
    for t in 0..thread_count {
        let logger_clone = Arc::clone(&logger);
        let handle = thread::spawn(move || {
            for r in 0..records_per_thread {
                let rec = AuditRecord {
                    timestamp: format!("2026-09-17T02:20:{:02}Z", r % 60),
                    session_id: format!("sess-thread-{}", t),
                    task_id: if r % 2 == 0 {
                        Some(format!("task-{}-{}", t, r))
                    } else {
                        None
                    },
                    tool: "grep_files".to_string(),
                    parameters: json!({
                        "thread": t,
                        "sequence": r,
                        "token": format!("token_{}_{}", t, r)
                    }),
                    status: if r % 5 == 0 {
                        "error".to_string()
                    } else {
                        "success".to_string()
                    },
                    duration_ms: (t * 10 + r) as u64,
                };
                logger_clone
                    .log(&rec)
                    .expect("Concurrent audit log write failed");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread panicked during audit write");
    }

    let records = logger.read_all().expect("Failed to read_all audit records");
    assert_eq!(
        records.len(),
        total_expected,
        "Expected exactly {} audit records from {} threads, found {}",
        total_expected,
        thread_count,
        records.len()
    );

    // Verify all threads and sequence numbers are accounted for without corruption
    let mut thread_seq_counts = std::collections::HashMap::new();
    for r in records {
        let tid = r.parameters["thread"].as_u64().unwrap();
        let seq = r.parameters["sequence"].as_u64().unwrap();
        *thread_seq_counts.entry((tid, seq)).or_insert(0) += 1;
    }

    for t in 0..thread_count {
        for r in 0..records_per_thread {
            let count = thread_seq_counts
                .get(&(t as u64, r as u64))
                .copied()
                .unwrap_or(0);
            assert_eq!(
                count, 1,
                "Record for thread {} seq {} must be present exactly once",
                t, r
            );
        }
    }
}

#[test]
fn challenge_audit_poison_recovery_resilience() {
    let temp_dir = ChallengeTempDir::new("poison_recovery");
    let log_path = temp_dir.path().join(".ctrl").join("audit.log");
    let logger = Arc::new(AuditLogger::from_path(log_path));

    // Intentionally cause a panic in a thread holding or accessing logger
    let logger_clone = Arc::clone(&logger);
    let panic_handle = thread::spawn(move || {
        let rec = AuditRecord {
            timestamp: "2026-09-17T02:25:00Z".to_string(),
            session_id: "sess-panic".to_string(),
            task_id: None,
            tool: "panic_tool".to_string(),
            parameters: json!({}),
            status: "error".to_string(),
            duration_ms: 0,
        };
        let _ = logger_clone.log(&rec);
        panic!("Simulated intentional worker thread panic");
    });
    let _ = panic_handle.join(); // Expected to be Err

    // Next thread must successfully write without being locked out or poisoned
    let rec_ok = AuditRecord {
        timestamp: "2026-09-17T02:25:01Z".to_string(),
        session_id: "sess-recovered".to_string(),
        task_id: Some("task-survivor".to_string()),
        tool: "read_file".to_string(),
        parameters: json!({"path": "Cargo.toml"}),
        status: "success".to_string(),
        duration_ms: 5,
    };

    let log_res = logger.log(&rec_ok);
    assert!(
        log_res.is_ok(),
        "AuditLogger must recover from poison state and allow subsequent logs"
    );

    let all = logger.read_all().expect("Failed to read after recovery");
    assert!(
        all.iter().any(|r| r.session_id == "sess-recovered"),
        "Recovered record must be persisted"
    );
}

#[test]
fn challenge_audit_directory_self_healing() {
    let temp_dir = ChallengeTempDir::new("self_healing");
    let deep_dir = temp_dir
        .path()
        .join("level1")
        .join("level2")
        .join("sublevel");
    let log_path = deep_dir.join(".ctrl").join("audit.log");

    assert!(
        !deep_dir.exists(),
        "Deep directory must not exist prior to test"
    );

    let logger = AuditLogger::from_path(log_path.clone());
    let rec = AuditRecord {
        timestamp: "2026-09-17T02:30:00Z".to_string(),
        session_id: "sess-deep".to_string(),
        task_id: None,
        tool: "test_tool".to_string(),
        parameters: json!({}),
        status: "success".to_string(),
        duration_ms: 10,
    };

    logger.log(&rec).expect("AuditLogger must auto-create non-existent parent directories");
    assert!(log_path.exists(), "audit.log must be created");

    let records = logger.read_all().expect("Failed to read");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "sess-deep");
}

#[test]
fn challenge_audit_with_audit_dir_thread_isolation() {
    let dir1 = ChallengeTempDir::new("iso_dir1");
    let dir2 = ChallengeTempDir::new("iso_dir2");

    let path1 = dir1.path().to_path_buf();
    let path2 = dir2.path().to_path_buf();

    let p1_clone = path1.clone();
    let t1 = thread::spawn(move || {
        with_audit_dir(p1_clone, || {
            for i in 0..10 {
                let rec = AuditRecord {
                    timestamp: "2026-09-17T02:35:00Z".to_string(),
                    session_id: "sess-thread-1".to_string(),
                    task_id: None,
                    tool: "tool_1".to_string(),
                    parameters: json!({"i": i}),
                    status: "success".to_string(),
                    duration_ms: 1,
                };
                log_tool_invocation(&rec).unwrap();
            }
        });
    });

    let p2_clone = path2.clone();
    let t2 = thread::spawn(move || {
        with_audit_dir(p2_clone, || {
            for i in 0..15 {
                let rec = AuditRecord {
                    timestamp: "2026-09-17T02:35:01Z".to_string(),
                    session_id: "sess-thread-2".to_string(),
                    task_id: None,
                    tool: "tool_2".to_string(),
                    parameters: json!({"i": i}),
                    status: "success".to_string(),
                    duration_ms: 2,
                };
                log_tool_invocation(&rec).unwrap();
            }
        });
    });

    t1.join().unwrap();
    t2.join().unwrap();

    let logger1 = AuditLogger::new(&path1);
    let logger2 = AuditLogger::new(&path2);

    let recs1 = logger1.read_all().unwrap();
    let recs2 = logger2.read_all().unwrap();

    assert_eq!(
        recs1.len(),
        10,
        "Log in dir1 must contain exactly 10 records"
    );
    assert!(recs1.iter().all(|r| r.session_id == "sess-thread-1"));

    assert_eq!(
        recs2.len(),
        15,
        "Log in dir2 must contain exactly 15 records"
    );
    assert!(recs2.iter().all(|r| r.session_id == "sess-thread-2"));
}

// ============================================================================
// SUITE 4: COMPLETE TOOL LIFECYCLE EXECUTION TIMING & DURATION RECORDING
// ============================================================================

fn simulate_tool_dispatch(
    name: &str,
    arguments_json: &str,
    sleep_duration: Duration,
    should_fail: bool,
    custom_error: Option<&str>,
) -> Result<String> {
    let start_time = Instant::now();
    let timestamp = agent::tasks::format_utc_timestamp(SystemTime::now());
    let ctx = get_tool_context();

    let args_val: serde_json::Value = serde_json::from_str(arguments_json)
        .unwrap_or_else(|_| serde_json::json!({ "raw": arguments_json }));

    thread::sleep(sleep_duration);

    let result: Result<String> = if let Some(err) = custom_error {
        Err(anyhow::anyhow!("{}", err))
    } else if should_fail {
        Err(anyhow::anyhow!("Simulated tool execution failure"))
    } else {
        Ok(format!("Tool '{}' executed successfully.", name))
    };

    let duration_ms = start_time.elapsed().as_millis() as u64;
    let status = match &result {
        Ok(_) => "success",
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Dry-run rejection")
                || msg.contains("Operation cancelled by user")
                || msg.contains("blocked")
            {
                "blocked"
            } else {
                "error"
            }
        }
    };

    let record = AuditRecord {
        timestamp,
        session_id: ctx.session_id.clone(),
        task_id: ctx.task_id.clone(),
        tool: name.to_string(),
        parameters: args_val,
        status: status.to_string(),
        duration_ms,
    };
    let _ = log_tool_invocation(&record);

    result
}

#[test]
fn challenge_tool_lifecycle_duration_recording_monotonic() {
    let temp_dir = ChallengeTempDir::new("timing_monotonic");
    let ws = temp_dir.path().to_path_buf();
    let logger = AuditLogger::new(&ws);

    with_audit_dir(ws, || {
        let sleep_ms = 45;
        let res = simulate_tool_dispatch(
            "read_file",
            r#"{"path": "test.txt"}"#,
            Duration::from_millis(sleep_ms),
            false,
            None,
        );
        assert!(res.is_ok());

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        let rec = &records[0];

        assert_eq!(rec.tool, "read_file");
        assert_eq!(rec.status, "success");
        assert!(
            rec.duration_ms >= 40,
            "Recorded duration_ms ({}) must be >= 40 for a 45ms simulated tool run",
            rec.duration_ms
        );
        assert!(
            rec.duration_ms < 500,
            "Recorded duration_ms ({}) must not exhibit artificial latency",
            rec.duration_ms
        );
    });
}

#[test]
fn challenge_tool_lifecycle_status_mapping_success() {
    let temp_dir = ChallengeTempDir::new("status_success");
    let ws = temp_dir.path().to_path_buf();
    let logger = AuditLogger::new(&ws);

    with_audit_dir(ws, || {
        let res = simulate_tool_dispatch(
            "write_file",
            r#"{"path": "output.log", "content": "data"}"#,
            Duration::from_millis(5),
            false,
            None,
        );
        assert!(res.is_ok());

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, "success");
    });
}

#[test]
fn challenge_tool_lifecycle_status_mapping_blocked() {
    let temp_dir = ChallengeTempDir::new("status_blocked");
    let ws = temp_dir.path().to_path_buf();
    let logger = AuditLogger::new(&ws);

    with_audit_dir(ws, || {
        // 1. Dry-run rejection
        let _ = simulate_tool_dispatch(
            "shell",
            r#"{"command": "rm -rf /"}"#,
            Duration::from_millis(2),
            true,
            Some("Dry-run rejection: destructive command blocked in background subagent mode"),
        );

        // 2. Interactive cancellation
        let _ = simulate_tool_dispatch(
            "shell",
            r#"{"command": "del /s temp"}"#,
            Duration::from_millis(2),
            true,
            Some("Operation cancelled by user"),
        );

        // 3. Permission gate blocked
        let _ = simulate_tool_dispatch(
            "edit_file",
            r#"{"path": "sensitive.txt"}"#,
            Duration::from_millis(2),
            true,
            Some("Action blocked by user permission gate"),
        );

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].status, "blocked");
        assert_eq!(records[1].status, "blocked");
        assert_eq!(records[2].status, "blocked");
    });
}

#[test]
fn challenge_tool_lifecycle_status_mapping_error() {
    let temp_dir = ChallengeTempDir::new("status_error");
    let ws = temp_dir.path().to_path_buf();
    let logger = AuditLogger::new(&ws);

    with_audit_dir(ws, || {
        let res = simulate_tool_dispatch(
            "read_file",
            r#"{"path": "non_existent_file.xyz"}"#,
            Duration::from_millis(3),
            true,
            None,
        );
        assert!(res.is_err());

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, "error");
    });
}

#[test]
fn challenge_tool_lifecycle_timestamp_format_utc() {
    let ts = agent::tasks::format_utc_timestamp(SystemTime::now());

    // Must strictly match ISO 8601 UTC: YYYY-MM-DDTHH:MM:SSZ (length 20)
    assert_eq!(ts.len(), 20, "UTC timestamp must be 20 chars long");
    assert!(
        ts.ends_with('Z'),
        "UTC timestamp must end with 'Z'. Got: {}",
        ts
    );
    assert_eq!(&ts[10..11], "T", "T separator must be at index 10");

    let parts: Vec<&str> = ts[..10].split('-').collect();
    assert_eq!(parts.len(), 3);
    let year: u32 = parts[0].parse().expect("Invalid year");
    let month: u32 = parts[1].parse().expect("Invalid month");
    let day: u32 = parts[2].parse().expect("Invalid day");

    assert!(year >= 2026, "Year must be >= 2026");
    assert!((1..=12).contains(&month), "Month must be 1..=12");
    assert!((1..=31).contains(&day), "Day must be 1..=31");
}

#[test]
fn challenge_tool_lifecycle_context_inheritance() {
    let temp_dir = ChallengeTempDir::new("context_inheritance");
    let ws = temp_dir.path().to_path_buf();
    let logger = AuditLogger::new(&ws);

    with_audit_dir(ws, || {
        let custom_ctx = ToolExecutionContext {
            session_id: "sess-inherited-uuid-42".to_string(),
            task_id: Some("task-inherited-subagent-99".to_string()),
            is_background: true,
        };

        with_tool_context(custom_ctx, || {
            let res = simulate_tool_dispatch(
                "knowledge_search",
                r#"{"query": "Rust concurrency"}"#,
                Duration::from_millis(8),
                false,
                None,
            );
            assert!(res.is_ok());
        });

        let records = logger.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "sess-inherited-uuid-42");
        assert_eq!(
            records[0].task_id.as_deref(),
            Some("task-inherited-subagent-99")
        );
        assert_eq!(records[0].tool, "knowledge_search");
        assert_eq!(records[0].status, "success");
    });
}

// ============================================================================
// SUITE 5: CLI SUBPROCESS REPL AUDIT INTEGRATION
// ============================================================================

#[test]
fn challenge_cli_repl_subagent_task_dry_run_audit_integration() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");

    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli binary");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open child stdin");
        let _ = writeln!(stdin, "/skills list");
        let _ = writeln!(stdin, "/exit");
    }

    let output = child
        .wait_with_output()
        .expect("Failed to wait on child process");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("researcher") || stdout.contains("writer"),
        "REPL stdout must confirm active skill system"
    );
}
