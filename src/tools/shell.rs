use crate::tools::result_store::ResultStore;
use anyhow::Result;
use std::process::Command;

/// Executes a shell command using powershell.exe (Windows) or sh (Unix).
pub fn execute_shell(command_str: &str, timeout_secs: Option<u64>) -> Result<String> {
    execute_shell_with_options(command_str, timeout_secs, None)
}

/// Executes a shell command with explicit background mode override.
pub fn execute_shell_with_options(
    command_str: &str,
    _timeout_secs: Option<u64>,
    is_background: Option<bool>,
) -> Result<String> {
    let bg = is_background.unwrap_or_else(|| {
        if crate::tools::get_tool_context().is_background {
            return true;
        }
        std::thread::current().name().is_some_and(|name| {
            name.contains("task-worker")
                || name.contains("subagent")
                || name.contains("background")
        })
    });

    crate::tools::guardrails::check_destructive_guardrail(command_str, bg)?;

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-NonInteractive", "-Command", command_str]);
        c
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.args(["-c", command_str]);
        c
    };

    let output = cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    let mut combined = format!(
        "--- Command: '{}' (Exit Code: {}) ---\n",
        command_str, exit_code
    );
    if !stdout.is_empty() {
        combined.push_str("\n[STDOUT]\n");
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        combined.push_str("\n[STDERR]\n");
        combined.push_str(&stderr);
    }
    if stdout.is_empty() && stderr.is_empty() {
        combined.push_str("\n(no output emitted)\n");
    }

    Ok(ResultStore::process_output(combined, 200, 15000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_shell_rejects_destructive_in_background() {
        let res = execute_shell_with_options("rm -rf target", None, Some(true));
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Dry-run rejection: destructive command 'rm -rf target' is blocked in background subagent mode"));
    }

    #[test]
    fn test_execute_shell_runs_benign_command() {
        let res = execute_shell("echo hello_guardrail", None);
        assert!(res.is_ok());
        let out = res.unwrap();
        assert!(out.contains("hello_guardrail"));
    }
}
