use crate::tools::result_store::ResultStore;
use anyhow::Result;
use std::process::Command;

pub fn execute_shell(command_str: &str, _timeout_secs: Option<u64>) -> Result<String> {
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
