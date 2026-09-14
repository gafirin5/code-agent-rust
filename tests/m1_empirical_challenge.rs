use std::process::{Command, Stdio};
use std::io::Write;

#[test]
fn challenge_version_flag_exact_match() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let out = Command::new(bin).arg("--version").output().expect("Failed to run ctrl-cli --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "ctrl-cli 0.3.0", "CLI version flag must be exactly 'ctrl-cli 0.3.0'");

    let out_short = Command::new(bin).arg("-V").output().expect("Failed to run ctrl-cli -V");
    assert!(out_short.status.success());
    let stdout_short = String::from_utf8_lossy(&out_short.stdout);
    assert_eq!(stdout_short.trim(), "ctrl-cli 0.3.0", "CLI -V flag must be exactly 'ctrl-cli 0.3.0'");
}

#[test]
fn challenge_help_subcommands_and_flags() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let out = Command::new(bin).arg("--help").output().expect("Failed to run ctrl-cli --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("tui"), "Help must list tui command");
    assert!(stdout.contains("repl"), "Help must list repl command");
    assert!(stdout.contains("--cli"), "Help must list --cli option");
    assert!(stdout.contains("--tui"), "Help must list --tui option");
}

#[test]
fn challenge_repl_banner_version() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to launch ctrl-cli --cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("v0.3.0"),
        "REPL startup banner must advertise version v0.3.0. Output: {}",
        stdout
    );
}

#[test]
fn challenge_repl_help_documents_tui() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");
    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to launch ctrl-cli --cli");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        let _ = writeln!(stdin, "/help");
        let _ = writeln!(stdin, "/exit");
    }

    let out = child.wait_with_output().expect("Failed to wait on child");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("/tui") && stdout.contains("/gui"),
        "/help must document /tui and /gui. Output: {}",
        stdout
    );
}

#[test]
fn challenge_tui_header_version_alignment() {
    // Read src/tui/ui.rs and check line 85 for hardcoded v0.2.0 vs dynamic/0.3.0
    let ui_rs = std::fs::read_to_string("src/tui/ui.rs")
        .or_else(|_| std::fs::read_to_string("ctrl-cli/src/tui/ui.rs"))
        .expect("Must find src/tui/ui.rs");

    let has_stale_v0_2_0 = ui_rs.contains("\"v0.2.0 \"") || ui_rs.contains("\"v0.2.0\"");
    assert!(
        !has_stale_v0_2_0,
        "VULNERABILITY DETECTED: src/tui/ui.rs contains hardcoded stale version 'v0.2.0' in TUI header!"
    );
}
