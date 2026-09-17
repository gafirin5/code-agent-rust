//! Destructive command guardrails for shell execution safety.
//!
//! Inspects shell commands against high-risk destructive patterns (e.g. recursive deletions,
//! hard git resets, filesystem formatting) and enforces interactive confirmation gates
//! or safe dry-run rejections in background subagent mode.

use anyhow::Result;
use std::io::Write;

/// Analyzes a shell command string and returns a reason if it contains potentially destructive operations.
///
/// Handles command chains (`;`, `&&`, `||`, `|`), unwraps PowerShell/CMD wrappers,
/// and allows benign informational commands (`echo`, `grep`, `cat`, etc.).
pub fn is_destructive_command(cmd: &str) -> Option<&'static str> {
    // Split command sequence by shell chaining operators: ;, &&, ||, |
    let subcommands = cmd.split([';', '|']).flat_map(|seg| seg.split("&&"));

    for sub in subcommands {
        let sub = sub.trim();
        if sub.is_empty() {
            continue;
        }

        let lower = sub.to_lowercase();
        let tokens: Vec<&str> = lower.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let first = tokens[0].trim_matches(|c| c == '\'' || c == '"');
        let base_first = first.rsplit(['/', '\\']).next().unwrap_or(first);

        // Benign/informational commands whose arguments should not trigger guardrails
        if matches!(
            base_first,
            "echo" | "printf" | "grep" | "cat" | "type" | "head" | "tail" | "findstr"
        ) {
            continue;
        }

        // Handle powershell/cmd invocation wrappers: e.g. powershell -Command "..."
        if base_first.contains("powershell")
            || base_first == "pwsh"
            || base_first == "cmd"
            || base_first == "cmd.exe"
        {
            if let Some(pos) = tokens
                .iter()
                .position(|&t| t == "-command" || t == "-c" || t == "/c" || t == "/k")
            {
                if pos + 1 < tokens.len() {
                    let inner = tokens[pos + 1..].join(" ");
                    let unquoted = inner.trim_matches(|c| c == '\'' || c == '"');
                    if let Some(reason) = is_destructive_command(unquoted) {
                        return Some(reason);
                    }
                    continue;
                }
            } else if tokens.len() > 2 {
                let inner = tokens[2..].join(" ");
                let unquoted = inner.trim_matches(|c| c == '\'' || c == '"');
                if let Some(reason) = is_destructive_command(unquoted) {
                    return Some(reason);
                }
                continue;
            }
        }

        // Direct destructive operations
        // 1. Recursive force removal (rm -rf, rm -r -f, rm -fr, rm -r ... -f)
        if base_first == "rm" {
            let has_rf = lower.contains(" -rf")
                || lower.contains(" -fr")
                || lower.contains(" -r -f")
                || lower.contains(" -f -r")
                || ((lower.contains(" -r ") || lower.ends_with(" -r"))
                    && (lower.contains(" -f ") || lower.ends_with(" -f")));
            if has_rf {
                return Some("Recursive force removal (rm -rf)");
            }
        }

        // 2. Recursive file deletion (del /s, erase /s, ri -r, ri -recurse, remove-item -recurse)
        if (base_first == "del"
            || base_first == "erase"
            || base_first == "ri"
            || base_first == "remove-item")
            && (lower.contains("/s") || lower.contains("-r") || lower.contains("-recurse"))
        {
            return Some("Recursive file deletion (del /s)");
        }

        // 3. Recursive directory removal (rmdir /s, rd /s)
        if (base_first == "rmdir" || base_first == "rd")
            && (lower.contains("/s") || lower.contains("-r") || lower.contains("-recurse"))
        {
            return Some("Recursive directory removal (rmdir /s)");
        }

        // 4. Git destructive operations
        if base_first == "git" {
            if lower.contains("reset") && lower.contains("--hard") {
                return Some("Destructive git reset (git reset --hard)");
            }
            if lower.contains("clean")
                && (lower.contains("-fd")
                    || lower.contains("-f -d")
                    || lower.contains("-df")
                    || (lower.contains("-f") && lower.contains("-d")))
            {
                return Some("Forced untracked file deletion (git clean -fd)");
            }
        }

        // 5. Filesystem formatting
        if base_first == "format" || base_first.starts_with("format") {
            return Some("Filesystem format operation");
        }

        // 6. Filesystem creation
        if base_first.starts_with("mkfs") {
            return Some("Filesystem creation operation");
        }

        // 7. Partition table modification
        if base_first.starts_with("fdisk") {
            return Some("Partition table modification");
        }
    }

    None
}

/// Checks whether a command is destructive and enforces confirmation or rejection.
///
/// In background mode (`is_background == true`), returns an immediate dry-run rejection error.
/// In interactive mode, prompts the user via stderr and reads confirmation from stdin.
pub fn check_destructive_guardrail(cmd: &str, is_background: bool) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    check_destructive_guardrail_with_reader(cmd, is_background, &mut stdin)
}

/// Internal helper supporting custom `BufRead` for deterministic test verification.
pub fn check_destructive_guardrail_with_reader<R: std::io::BufRead>(
    cmd: &str,
    is_background: bool,
    reader: &mut R,
) -> Result<()> {
    if let Some(reason) = is_destructive_command(cmd) {
        if is_background {
            return Err(anyhow::anyhow!(
                "Dry-run rejection: destructive command '{}' is blocked in background subagent mode. Reason: {}",
                cmd,
                reason
            ));
        }

        eprintln!("⚠ Destructive command detected: '{}' ({})", cmd, reason);
        eprint!("Proceed? (y/N): ");
        let _ = std::io::stderr().flush();

        let mut input = String::new();
        if reader.read_line(&mut input).is_err() {
            return Err(anyhow::anyhow!("Operation cancelled by user"));
        }

        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes") {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Operation cancelled by user"))
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destructive_patterns_rm_variations() {
        assert_eq!(
            is_destructive_command("rm -rf /tmp/target"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("rm -r -f target"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("rm -fr target"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("rm -r target -f"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("/bin/rm -rf /var/log"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("RM -RF /tmp/foo"),
            Some("Recursive force removal (rm -rf)")
        );
    }

    #[test]
    fn test_destructive_patterns_windows_deletions() {
        assert_eq!(
            is_destructive_command("del /s /q build"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("DEL /S BUILD"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("erase /s temp"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("ri -r ./dist"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("ri -recurse ./dist"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("remove-item -recurse node_modules"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("rmdir /s /q node_modules"),
            Some("Recursive directory removal (rmdir /s)")
        );
        assert_eq!(
            is_destructive_command("rd /s /q cache"),
            Some("Recursive directory removal (rmdir /s)")
        );
    }

    #[test]
    fn test_destructive_patterns_git_and_system() {
        assert_eq!(
            is_destructive_command("git reset --hard HEAD~1"),
            Some("Destructive git reset (git reset --hard)")
        );
        assert_eq!(
            is_destructive_command("git clean -fd"),
            Some("Forced untracked file deletion (git clean -fd)")
        );
        assert_eq!(
            is_destructive_command("git clean -f -d"),
            Some("Forced untracked file deletion (git clean -fd)")
        );
        assert_eq!(
            is_destructive_command("git clean -df"),
            Some("Forced untracked file deletion (git clean -fd)")
        );
        assert_eq!(
            is_destructive_command("format C: /fs:ntfs"),
            Some("Filesystem format operation")
        );
        assert_eq!(
            is_destructive_command("mkfs.ext4 /dev/sdb1"),
            Some("Filesystem creation operation")
        );
        assert_eq!(
            is_destructive_command("fdisk /dev/sda"),
            Some("Partition table modification")
        );
    }

    #[test]
    fn test_benign_commands_and_lookalikes() {
        assert!(is_destructive_command("echo rm -rf").is_none());
        assert!(is_destructive_command("printf 'del /s /q'").is_none());
        assert!(is_destructive_command("grep 'del /s' search.rs").is_none());
        assert!(is_destructive_command("cat instructions.txt").is_none());
        assert!(is_destructive_command("type config.json").is_none());
        assert!(is_destructive_command("head -n 10 file.txt").is_none());
        assert!(is_destructive_command("tail -f log.txt").is_none());
        assert!(is_destructive_command("findstr /i 'error' file.log").is_none());

        // Safe git commands
        assert!(is_destructive_command("git reset HEAD file.txt").is_none());
        assert!(is_destructive_command("git status").is_none());
        assert!(is_destructive_command("git diff HEAD").is_none());
        assert!(is_destructive_command("git log -n 5").is_none());

        // Standard tool commands
        assert!(is_destructive_command("cargo test").is_none());
        assert!(is_destructive_command("cargo build --release").is_none());
    }

    #[test]
    fn test_chained_commands() {
        assert_eq!(
            is_destructive_command("cargo check && rm -rf target"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("echo hello; del /s /q temp"),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("test -d build || rm -rf build"),
            Some("Recursive force removal (rm -rf)")
        );
        assert_eq!(
            is_destructive_command("cat file.txt | rm -rf"),
            Some("Recursive force removal (rm -rf)")
        );
    }

    #[test]
    fn test_shell_wrappers() {
        assert_eq!(
            is_destructive_command("powershell.exe -Command \"rmdir /s /q C:\\temp\""),
            Some("Recursive directory removal (rmdir /s)")
        );
        assert_eq!(
            is_destructive_command("cmd /c \"del /s /q build\""),
            Some("Recursive file deletion (del /s)")
        );
        assert_eq!(
            is_destructive_command("pwsh -c \"git reset --hard\""),
            Some("Destructive git reset (git reset --hard)")
        );
    }

    #[test]
    fn test_check_guardrail_background_rejection() {
        let res = check_destructive_guardrail("rm -rf target", true);
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("Dry-run rejection: destructive command 'rm -rf target' is blocked in background subagent mode"));
        assert!(err_msg.contains("Recursive force removal (rm -rf)"));

        // Safe command in background mode passes
        assert!(check_destructive_guardrail("cargo check", true).is_ok());
    }

    #[test]
    fn test_check_guardrail_interactive_decisions() {
        // Confirmation accepted: "y", "yes", "Y", "YES"
        let mut reader_y = "y\n".as_bytes();
        assert!(check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader_y).is_ok());

        let mut reader_yes = "YES\n".as_bytes();
        assert!(check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader_yes).is_ok());

        // Confirmation rejected: "n", "", "no", arbitrary
        let mut reader_n = "n\n".as_bytes();
        let res_n = check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader_n);
        assert!(res_n.is_err());
        assert_eq!(res_n.unwrap_err().to_string(), "Operation cancelled by user");

        let mut reader_empty = "\n".as_bytes();
        let res_empty = check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader_empty);
        assert!(res_empty.is_err());

        let mut reader_cancel = "cancel\n".as_bytes();
        let res_cancel = check_destructive_guardrail_with_reader("rm -rf target", false, &mut reader_cancel);
        assert!(res_cancel.is_err());
    }
}
