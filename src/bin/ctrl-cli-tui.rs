use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Prefer locating the sibling ctrl-cli binary in the same directory
    let current_exe = std::env::current_exe().ok();
    let sibling_ctrl_cli = current_exe.as_ref().and_then(|p| {
        let parent = p.parent()?;
        let exe_ext = if cfg!(windows) { ".exe" } else { "" };
        let candidate = parent.join(format!("ctrl-cli{}", exe_ext));
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    });

    let binary = sibling_ctrl_cli.unwrap_or_else(|| "ctrl-cli".into());

    let mut cmd = Command::new(binary);
    cmd.arg("tui");
    cmd.args(&args);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) => {
            if let Some(code) = status.code() {
                std::process::exit(code);
            }
        }
        Err(e) => {
            eprintln!("Error: Gagal menjalankan ctrl-cli tui: {}", e);
            std::process::exit(1);
        }
    }
}
