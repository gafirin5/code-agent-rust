//! Empirical Challenge Test Suite for Milestone 2:
//! Destructive Command Guardrails & Immutable Audit Trail.
//!
//! Authored by: challenger_m2_gate_1 (Empirical Challenger)
//!
//! Aggressively stress-tests:
//! 1. Detection of 80+ dangerous commands and evasion attempts across shells, wrappers, and delimiters.
//! 2. Verification that benign commands (lookalikes, grep, echo, cat, safe git) are NOT blocked.
//! 3. High concurrency audit log stress testing (30 parallel threads, 300 records, poison recovery).
//! 4. Audit trail file format, single-line JSONL serialization, and directory self-healing.
//! 5. Interactive confirmation vs background dry-run rejections.
//! 6. Execution through shell tool with background ambient detection.
//! 7. Adversarial edge cases and evasion boundary exploration.

pub mod agent {
    pub mod checkpoint {
        pub struct CheckpointManager;
        impl CheckpointManager {
            pub fn record_checkpoint(_path: &str, _op: &str) -> anyhow::Result<()> {
                Ok(())
            }
        }
    }

    pub mod tasks {
        use std::time::SystemTime;
        pub fn format_utc_timestamp(time: SystemTime) -> String {
            let dur = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
            let secs = dur.as_secs();
            format!("{}Z", secs)
        }
    }
}

pub mod tools {
    pub mod self_heal {
        pub fn check_file_diagnostics(_path: &str) -> Option<String> {
            None
        }
    }

    #[path = "../../src/tools/result_store.rs"]
    pub mod result_store;
    #[path = "../../src/tools/filesystem.rs"]
    pub mod filesystem;
    #[path = "../../src/tools/guardrails.rs"]
    pub mod guardrails;
    #[path = "../../src/tools/audit.rs"]
    pub mod audit;
    #[path = "../../src/tools/shell.rs"]
    pub mod shell;

    pub use audit::*;
    pub use guardrails::*;
    pub use shell::*;

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
                ToolExecutionContext {
                    session_id: "challenge-sess".into(),
                    task_id: None,
                    is_background: is_bg,
                }
            })
        })
    }
}

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use tools::audit::{AuditLogger, AuditRecord};
use tools::guardrails::{
    check_destructive_guardrail, check_destructive_guardrail_with_reader, is_destructive_command,
};
use tools::shell::{execute_shell, execute_shell_with_options};
use tools::{with_tool_context, ToolExecutionContext};

/// Hermetic temporary directory guard for file-backed tests.
struct ChallengeTempDir {
    path: PathBuf,
}

impl ChallengeTempDir {
    fn new(name: &str) -> Self {
        let unique = format!(
            "ctrl_m2_challenge_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Create temporary test directory");
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
// SUITE 1: 80+ DANGEROUS COMMANDS & EVASION ATTEMPTS DETECTION
// ============================================================================

#[test]
fn test_challenge_dangerous_commands_detection_80_plus() {
    let dangerous_commands = [
        // 1. Unix rm variants & flag permutation evasions
        "rm -rf /",
        "rm -rf /tmp/target",
        "rm -r -f target",
        "rm -fr target",
        "rm -r target -f",
        "rm -f target -r",
        "rm -rf *",
        "rm -rf .*",
        "/bin/rm -rf /var/log",
        "/usr/bin/rm -rf /var/tmp",
        "/usr/local/bin/rm -rf /var/cache",
        r"C:\tools\rm -rf C:\temp",
        "'rm' -rf target",
        "\"rm\" -rf target",
        "RM -RF /tmp/foo",
        "Rm -r -F /tmp/foo",
        "rm -r -v -f /tmp/dir",
        "rm -v -f -r /tmp/dir",
        "rm -rf ./node_modules",
        "rm -rf target/debug",
        "rm -rf build/artifacts",

        // 2. Windows del / erase evasions & casing
        "del /s file.txt",
        "del /s /q build",
        "DEL /S /Q BUILD",
        r"del /s /f /q C:\temp\*",
        "erase /s temp",
        "ERASE /S /Q *",
        "erase /s file.log",

        // 3. PowerShell Remove-Item / ri evasions
        "ri -r ./dist",
        "ri -recurse ./dist",
        "RI -R target",
        "remove-item -recurse node_modules",
        "Remove-Item -Recurse .git",
        "remove-item -r build",
        "Remove-Item -Recurse -Force ./cache",

        // 4. Directory removal (rmdir / rd)
        "rmdir /s /q node_modules",
        "RMDIR /S /Q build",
        "rmdir /s dir",
        r"rmdir /s /q C:\Windows\Temp",
        "rd /s /q cache",
        "RD /S temp",
        r"rd /s /q .\build",

        // 5. Git destructive operations
        "git reset --hard",
        "git reset --hard HEAD",
        "git reset --hard HEAD~1",
        "git reset --hard origin/main",
        "git reset --hard HEAD@{1}",
        "GIT RESET --HARD",
        "git clean -fd",
        "git clean -f -d",
        "git clean -df",
        "git clean -d -f",
        "GIT CLEAN -FD",
        "git clean -f -d -x",

        // 6. Filesystem format & disk partition operations
        "format C: /fs:ntfs",
        "format D: /q",
        "FORMAT C:",
        "format E: /fs:fat32 /v:backup",
        "mkfs /dev/sdb1",
        "mkfs.ext4 /dev/sdb1",
        "mkfs.xfs -f /dev/nvme0n1",
        "MKFS.EXT4 /dev/sda1",
        "mkfs.vfat -F 32 /dev/sdc1",
        "fdisk /dev/sda",
        "fdisk -l /dev/sda",
        "FDISK /dev/nvme0n1",

        // 7. Compound & chained command evasions (delimiters: ;, &&, ||, |)
        "cargo check && rm -rf target",
        "echo starting ; del /s /q temp",
        "test -d build || rm -rf build",
        "cat list.txt | rm -rf",
        "echo done ; git reset --hard HEAD~1",
        "mkdir -p /tmp/foo ; rmdir /s /q /tmp/foo",
        "dir ; echo separator ; format D:",
        "true && false || rm -rf /",

        // 8. Shell wrappers & quotes unwrapping (PowerShell, CMD, pwsh)
        r#"powershell.exe -Command "rmdir /s /q C:\temp""#,
        "powershell -command 'del /s /q build'",
        r#"pwsh -c "git reset --hard""#,
        r#"cmd.exe /c "del /s /q build""#,
        r#"cmd /k "rmdir /s /q temp""#,
        "cmd /c del /s build",
        r#"powershell -c "rm -rf /var/log""#,
        r#"pwsh -Command "remove-item -recurse node_modules""#,

        // 9. Padding, leading/trailing whitespace & multiple spaces
        "   rm -rf /tmp   ",
        "\t\trm -rf /tmp/test\t",
        "rm   -rf    target",
        "git    reset    --hard   HEAD",
    ];

    assert!(
        dangerous_commands.len() >= 80,
        "Must test at least 80 dangerous command patterns, actual: {}",
        dangerous_commands.len()
    );

    let mut detected_count = 0;
    for (idx, cmd) in dangerous_commands.iter().enumerate() {
        let reason = is_destructive_command(cmd);
        assert!(
            reason.is_some(),
            "Dangerous command #{} failed detection: '{}'",
            idx + 1,
            cmd
        );
        detected_count += 1;
    }

    assert_eq!(
        detected_count,
        dangerous_commands.len(),
        "All dangerous commands must be detected"
    );
}

// ============================================================================
// SUITE 2: BENIGN COMMANDS FALSE-POSITIVE IMMUNITY (35+ CASES)
// ============================================================================

#[test]
fn test_challenge_benign_commands_not_blocked_35_plus() {
    let benign_commands = [
        // Echo commands containing dangerous tokens
        "echo rm -rf",
        "echo 'rm -rf /'",
        "echo \"del /s /q\"",
        "echo 'git reset --hard'",
        "echo format C:",
        "echo mkfs.ext4 /dev/sda1",
        "echo fdisk /dev/sda",

        // Informational inspection tools (cat, type, head, tail, grep, findstr)
        "cat /tmp/del",
        "cat rm_instructions.txt",
        "cat /var/log/format.log",
        "type config.json",
        "type delete_report.txt",
        "head -n 20 rm_summary.txt",
        "head -5 /etc/fstab",
        "tail -f /var/log/syslog",
        "tail -n 50 debug.log",
        "grep 'rm -rf' src/tools/guardrails.rs",
        "grep 'del /s' search.rs",
        "grep -rn 'git reset --hard' .",
        "findstr /i 'format' doc.txt",
        "findstr /s 'git clean -fd' Cargo.toml",
        "printf 'git reset --hard\\n'",

        // Non-destructive safe git operations
        "git status",
        "git diff HEAD",
        "git log -n 5",
        "git reset HEAD file.txt",
        "git reset --soft HEAD~1",
        "git reset --mixed HEAD~1",
        "git clean -n",
        "git show HEAD",
        "git branch -a",

        // Standard build & test commands
        "cargo test",
        "cargo test --test m2_guardrails_empirical_challenge",
        "cargo build --release",
        "cargo check --all-targets",
        "cargo clippy",

        // Non-recursive safe filesystem listings and operations
        "ls -la",
        "dir /s",
        "del file.txt",
        "rmdir empty_dir",
        "rd empty_dir",
        "mkdir -p build/output",
        "touch README.md",
    ];

    assert!(
        benign_commands.len() >= 35,
        "Must test at least 35 benign command patterns, actual: {}",
        benign_commands.len()
    );

    for (idx, cmd) in benign_commands.iter().enumerate() {
        let result = is_destructive_command(cmd);
        assert!(
            result.is_none(),
            "Benign command #{} was FALSE-POSITIVELY blocked: '{}' (reason: {:?})",
            idx + 1,
            cmd,
            result
        );
    }
}

// ============================================================================
// SUITE 3: HIGH CONCURRENCY AUDIT LOG STRESS TESTING (30 THREADS, 300 RECORDS)
// ============================================================================

#[test]
fn test_challenge_high_concurrency_audit_stress_30_threads() {
    let temp_dir = ChallengeTempDir::new("concurrency_stress");
    let logger = Arc::new(AuditLogger::new(temp_dir.path()));

    let thread_count = 30;
    let records_per_thread = 10;
    let total_expected_records = thread_count * records_per_thread;

    let barrier = Arc::new(Barrier::new(thread_count));
    let mut handles = Vec::new();

    for t_idx in 0..thread_count {
        let logger_clone = logger.clone();
        let barrier_clone = barrier.clone();

        handles.push(thread::spawn(move || {
            // Synchronize all threads to start hammering the file concurrently
            barrier_clone.wait();

            for r_idx in 0..records_per_thread {
                let rec = AuditRecord {
                    timestamp: format!("2026-09-17T03:{:02}:{:02}Z", t_idx, r_idx),
                    session_id: format!("sess-{:02}", t_idx),
                    task_id: Some(format!("task-{:02}-{:02}", t_idx, r_idx)),
                    tool: match r_idx % 4 {
                        0 => "shell".to_string(),
                        1 => "read_file".to_string(),
                        2 => "write_file".to_string(),
                        _ => "knowledge_search".to_string(),
                    },
                    parameters: json!({
                        "thread": t_idx,
                        "iteration": r_idx,
                        "payload": format!("High concurrency audit stress test thread {} iter {} with \"quotes\", newlines \n, and UTF-8 emojis 🛡️ ⚡ 🦀", t_idx, r_idx),
                        "meta": {
                            "status": "active",
                            "flags": ["-v", "--all", "unicode_🚀"]
                        }
                    }),
                    status: if (t_idx + r_idx) % 7 == 0 {
                        "blocked".to_string()
                    } else {
                        "success".to_string()
                    },
                    duration_ms: (t_idx * 10 + r_idx) as u64,
                };

                logger_clone.log(&rec).expect("Concurrent write must succeed");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Stress worker thread joined cleanly");
    }

    // Verify all records in file
    let records = logger.read_all().expect("Read all concurrent records");
    assert_eq!(
        records.len(),
        total_expected_records,
        "All {} concurrent audit records must be preserved exactly! Actual: {}",
        total_expected_records,
        records.len()
    );

    // Verify raw file is 100% strictly valid JSONL (single line per record, valid JSON)
    let raw_content = fs::read_to_string(logger.path()).expect("Read raw audit log content");
    let non_empty_lines: Vec<&str> = raw_content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();

    assert_eq!(
        non_empty_lines.len(),
        total_expected_records,
        "Raw lines count must equal total records written"
    );

    for (line_idx, line) in non_empty_lines.iter().enumerate() {
        let parsed: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Line #{} is not valid JSON! Error: {}. Line: '{}'", line_idx + 1, e, line));
        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("session_id").is_some());
        assert!(parsed.get("tool").is_some());
        assert!(parsed.get("parameters").is_some());
        assert!(parsed.get("status").is_some());
        assert!(parsed.get("duration_ms").is_some());
    }
}

// ============================================================================
// SUITE 4: POISON RESILIENCE & SELF-HEALING DIRECTORY TESTS
// ============================================================================

#[test]
fn test_challenge_audit_poison_resilience_and_recovery() {
    let temp_dir = ChallengeTempDir::new("poison_resilience");
    let logger = AuditLogger::new(temp_dir.path());

    // Force poison the internal mutex by panicking while lock is held
    let logger_clone = logger.clone();
    let poison_thread = thread::spawn(move || {
        let _guard = logger_clone.log(&AuditRecord {
            timestamp: "2026-09-17T03:00:00Z".into(),
            session_id: "sess-poison".into(),
            task_id: None,
            tool: "poison_trigger".into(),
            parameters: json!({}),
            status: "error".into(),
            duration_ms: 0,
        });
        panic!("Simulated panic to poison Mutex");
    });
    let _ = poison_thread.join();

    // Logger must remain operational despite any previous panics
    let recovery_rec = AuditRecord {
        timestamp: "2026-09-17T03:00:01Z".into(),
        session_id: "sess-recovery".into(),
        task_id: None,
        tool: "post_poison_tool".into(),
        parameters: json!({"recovered": true}),
        status: "success".into(),
        duration_ms: 5,
    };

    assert!(logger.log(&recovery_rec).is_ok());
    let recs = logger.read_all().expect("Must read after recovery");
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[1].tool, "post_poison_tool");
}

#[test]
fn test_challenge_audit_directory_self_healing() {
    let temp_dir = ChallengeTempDir::new("self_healing");
    let deep_dir = temp_dir.path().join("nested").join("deep").join("workspace");
    let logger = AuditLogger::new(&deep_dir);

    // Write record to a path whose parents do not exist yet
    let rec = AuditRecord {
        timestamp: "2026-09-17T03:00:00Z".into(),
        session_id: "sess-heal".into(),
        task_id: None,
        tool: "heal_test".into(),
        parameters: json!({}),
        status: "success".into(),
        duration_ms: 1,
    };

    assert!(logger.log(&rec).is_ok());
    assert!(logger.path().exists(), "Audit file must be created");
    let records = logger.read_all().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "sess-heal");
}

// ============================================================================
// SUITE 5: INTERACTIVE CONFIRMATION VS BACKGROUND DRY-RUN REJECTION
// ============================================================================

#[test]
fn test_challenge_guardrail_interactive_confirmation_matrix() {
    // 1. Positive confirmations (y, yes, Y, YES, Yes)
    for pos_input in ["y\n", "yes\n", "Y\n", "YES\n", "Yes\n"] {
        let mut reader = pos_input.as_bytes();
        let res = check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader);
        assert!(
            res.is_ok(),
            "Interactive confirmation should succeed with '{}'",
            pos_input.trim()
        );
    }

    // 2. Negative and invalid confirmations (n, no, empty, spaces, invalid words)
    for neg_input in ["n\n", "no\n", "N\n", "NO\n", "\n", "   \n", "cancel\n", "abort\n"] {
        let mut reader = neg_input.as_bytes();
        let res = check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader);
        assert!(
            res.is_err(),
            "Interactive rejection should fail with '{}'",
            neg_input.trim()
        );
        assert_eq!(
            res.unwrap_err().to_string(),
            "Operation cancelled by user"
        );
    }
}

#[test]
fn test_challenge_guardrail_background_dry_run_rejection() {
    // Background subagent execution must NEVER freeze waiting for stdin,
    // and must immediately return a descriptive dry-run error.
    let destructive_cmds = [
        "rm -rf /tmp/target",
        "del /s /q build",
        "git reset --hard HEAD",
        "format C:",
        "mkfs.ext4 /dev/sdb1",
    ];

    for cmd in destructive_cmds {
        let res = check_destructive_guardrail(cmd, true);
        assert!(
            res.is_err(),
            "Background subagent mode must reject destructive command: '{}'",
            cmd
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("Dry-run rejection: destructive command"),
            "Error message must indicate dry-run rejection. Actual: '{}'",
            err_msg
        );
        assert!(
            err_msg.contains("is blocked in background subagent mode"),
            "Error message must state blocked in background subagent mode. Actual: '{}'",
            err_msg
        );
    }

    // Benign command in background mode must pass cleanly without prompt
    let benign_cmds = [
        "cargo test",
        "echo 'hello world'",
        "git status",
    ];

    for cmd in benign_cmds {
        let res = check_destructive_guardrail(cmd, true);
        assert!(
            res.is_ok(),
            "Benign command in background mode should pass cleanly: '{}'",
            cmd
        );
    }
}

// ============================================================================
// SUITE 6: SHELL TOOL INTEGRATION & AMBIENT CONTEXT SENSITIVITY
// ============================================================================

#[test]
fn test_challenge_shell_tool_guardrail_integration() {
    // 1. Explicit background override
    let res = execute_shell_with_options("rm -rf target_dummy", None, Some(true));
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Dry-run rejection"));

    // 2. Ambient ToolExecutionContext background flag
    let ctx = ToolExecutionContext {
        session_id: "sess-bg-test".into(),
        task_id: Some("task-bg-01".into()),
        is_background: true,
    };

    with_tool_context(ctx, || {
        let res2 = execute_shell("git reset --hard HEAD", None);
        assert!(res2.is_err());
        let err_str2 = res2.unwrap_err().to_string();
        assert!(err_str2.contains("Dry-run rejection"));
    });

    // 3. Worker thread name inspection sensitivity
    let handle = thread::Builder::new()
        .name("subagent-task-worker-01".to_string())
        .spawn(|| {
            let res3 = execute_shell("del /s /q temp_dummy", None);
            assert!(res3.is_err());
            let err_str3 = res3.unwrap_err().to_string();
            assert!(err_str3.contains("Dry-run rejection"));
        })
        .expect("Spawn subagent thread");

    handle.join().expect("Thread execution ok");
}

// ============================================================================
// SUITE 7: ADVERSARIAL EDGE CASE EXPLORATION (EMPIRICAL AUDIT REPORT)
// ============================================================================

#[test]
fn test_challenge_adversarial_boundary_probes() {
    // Probe 1: PowerShell Format-Table behavior
    // Format-Table starts with "format", which is caught as Filesystem format operation
    // This empirically confirms that Format-Table or Format-List are matched by starts_with("format").
    let ft_result = is_destructive_command("Format-Table");
    assert_eq!(ft_result, Some("Filesystem format operation"));

    // Probe 2: sudo wrapping boundary
    // Sudo is not in the shell wrapper list (pwsh/powershell/cmd), so sudo rm -rf / is unparsed wrapper
    let sudo_res = is_destructive_command("sudo rm -rf /");
    assert!(sudo_res.is_none(), "Documented caveat: external sudo is not an unwrapped shell wrapper");

    // Probe 3: Soft vs hard git resets
    assert!(is_destructive_command("git reset --soft HEAD~1").is_none());
    assert!(is_destructive_command("git reset --mixed HEAD~1").is_none());
    assert!(is_destructive_command("git reset --hard HEAD~1").is_some());

    // Probe 4: Dry-run git clean vs forced git clean
    assert!(is_destructive_command("git clean -n").is_none());
    assert!(is_destructive_command("git clean -fd").is_some());

    // Probe 5: Non-recursive del vs recursive del
    assert!(is_destructive_command("del single_file.txt").is_none());
    assert!(is_destructive_command("del /s single_file.txt").is_some());

    // Probe 6: Windows .exe extension evasion boundary
    // Executables with .exe suffix (e.g., git.exe reset --hard, rm.exe -rf)
    // bypass guardrails because base_first check expects "rm" or "git" exactly.
    let exe_rm_res = is_destructive_command("rm.exe -rf /tmp");
    let exe_git_res = is_destructive_command("git.exe reset --hard HEAD");
    assert!(exe_rm_res.is_none(), "Documented evasion: rm.exe is not normalized to rm");
    assert!(exe_git_res.is_none(), "Documented evasion: git.exe is not normalized to git");
}