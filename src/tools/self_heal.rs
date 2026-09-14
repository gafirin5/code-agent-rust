use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Checks compiler or syntax diagnostics for a file or project.
/// Returns Some(diagnostics) if errors or significant warnings are found.
pub fn check_file_diagnostics(file_path_str: &str) -> Option<String> {
    let path = Path::new(file_path_str);
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "rs" => check_rust_diagnostics(path),
        "py" => check_python_diagnostics(path),
        "ts" | "tsx" => check_typescript_diagnostics(path),
        "js" | "jsx" | "mjs" | "cjs" => check_javascript_diagnostics(path),
        "json" => check_json_diagnostics(path),
        _ => None,
    }
}

/// Explicit code check function invoked by the `code_check` tool or `/check` command.
pub fn run_code_check(target: Option<&str>) -> Result<String> {
    if let Some(t) = target {
        let p = Path::new(t);
        if p.is_file() {
            if let Some(diag) = check_file_diagnostics(t) {
                return Ok(format!(
                    "Compiler/Syntax Diagnostics for '{}':\n\n{}",
                    t, diag
                ));
            } else {
                return Ok(format!(
                    "✔ No syntax or compiler errors detected in '{}'.",
                    t
                ));
            }
        }
    }

    // Default: Check workspace project if Cargo.toml exists
    if let Some(cargo_dir) = find_cargo_project_root(Path::new(".")) {
        if let Some(diag) = run_cargo_check(&cargo_dir) {
            return Ok(format!("Cargo check diagnostics:\n\n{}", diag));
        } else {
            return Ok("✔ Cargo check passed with zero errors.".to_string());
        }
    }

    Ok("✔ Code check completed. No compiler errors found in workspace.".to_string())
}

fn find_cargo_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    if current.as_os_str().is_empty() {
        current = std::env::current_dir().ok()?;
    }

    loop {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn run_cargo_check(project_dir: &Path) -> Option<String> {
    let output = Command::new("cargo")
        .arg("check")
        .arg("--message-format=short")
        .current_dir(project_dir)
        .output()
        .ok()?;

    if output.status.success() {
        None
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}\n{}", stdout, stderr);
        let error_lines: Vec<&str> = combined
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.contains("error[")
                    || trimmed.contains("error:")
                    || (trimmed.contains("-->") && !trimmed.contains("Finished"))
            })
            .take(15)
            .collect();

        if !error_lines.is_empty() {
            Some(error_lines.join("\n"))
        } else {
            // If cargo failed but no standard error lines parsed, return first 8 lines of stderr
            let fallback: Vec<&str> = stderr.lines().take(8).collect();
            if !fallback.is_empty() {
                Some(fallback.join("\n"))
            } else {
                Some("Cargo check failed with exit code non-zero.".to_string())
            }
        }
    }
}

fn check_rust_diagnostics(file_path: &Path) -> Option<String> {
    if let Some(root) = find_cargo_project_root(file_path) {
        run_cargo_check(&root)
    } else {
        // Standalone rust file
        let null_dev = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let output = Command::new("rustc")
            .arg("--emit=metadata")
            .arg("-o")
            .arg(null_dev)
            .arg(file_path)
            .output()
            .ok()?;

        if output.status.success() {
            None
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let errors: Vec<&str> = stderr
                .lines()
                .filter(|l| l.contains("error:") || l.contains("error[") || l.contains("-->"))
                .take(12)
                .collect();
            if !errors.is_empty() {
                Some(errors.join("\n"))
            } else {
                Some(stderr.lines().take(6).collect::<Vec<_>>().join("\n"))
            }
        }
    }
}

fn check_python_diagnostics(file_path: &Path) -> Option<String> {
    // Try python then py
    let cmd = if Command::new("python").arg("--version").output().is_ok() {
        "python"
    } else {
        "py"
    };

    let output = Command::new(cmd)
        .arg("-m")
        .arg("py_compile")
        .arg(file_path)
        .output()
        .ok()?;

    if output.status.success() {
        None
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed = stderr.trim();
        if !trimmed.is_empty() {
            Some(format!("Python Syntax Error:\n{}", trimmed))
        } else {
            Some("Python py_compile failed.".to_string())
        }
    }
}

fn check_typescript_diagnostics(file_path: &Path) -> Option<String> {
    let output = Command::new("tsc")
        .arg("--noEmit")
        .arg(file_path)
        .output()
        .ok()?;

    if output.status.success() {
        None
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);
        let lines: Vec<&str> = combined
            .lines()
            .filter(|l| l.contains("error TS"))
            .take(10)
            .collect();
        if !lines.is_empty() {
            Some(lines.join("\n"))
        } else {
            None
        }
    }
}

fn check_javascript_diagnostics(file_path: &Path) -> Option<String> {
    let output = Command::new("node")
        .arg("--check")
        .arg(file_path)
        .output()
        .ok()?;

    if output.status.success() {
        None
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed = stderr.trim();
        if !trimmed.is_empty() {
            Some(trimmed.to_string())
        } else {
            None
        }
    }
}

fn check_json_diagnostics(file_path: &Path) -> Option<String> {
    if let Ok(content) = std::fs::read_to_string(file_path) {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&content) {
            return Some(format!(
                "Invalid JSON syntax in '{}': {}",
                file_path.display(),
                e
            ));
        }
    }
    None
}
