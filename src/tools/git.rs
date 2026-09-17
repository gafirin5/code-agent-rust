//! Native Structured Git Version Control Agent Tools.
//!
//! Exposes typed, safe Git operations (`git_status`, `git_diff`, `git_commit`)
//! executed directly via process argument slices without shell interpolation.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Structured output of `git status`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatusResult {
    pub branch: String,
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}

/// Structured output of `git commit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitResult {
    pub commit_hash: String,
    pub message: String,
}

/// Runs `git status` on `repo_path` (or current directory) and returns typed repository status.
pub fn tool_git_status(repo_path: Option<&str>) -> Result<GitStatusResult> {
    let target_dir = repo_path.map(Path::new).unwrap_or_else(|| Path::new("."));

    let output = Command::new("git")
        .arg("-C")
        .arg(target_dir)
        .args(["status", "--porcelain=v1", "-b"])
        .output()
        .context("Failed to run git status command")?;

    if !output.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branch = String::from("unknown");
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for line in stdout.lines() {
        if let Some(branch_line) = line.strip_prefix("## ") {
            branch = branch_line
                .split("...")
                .next()
                .unwrap_or("unknown")
                .trim()
                .to_string();
        } else if line.len() >= 3 {
            let index_stat = line.chars().next().unwrap_or(' ');
            let work_stat = line.chars().nth(1).unwrap_or(' ');
            let path = line[3..].trim().to_string();

            if index_stat == '?' && work_stat == '?' {
                untracked.push(path);
            } else {
                if index_stat != ' ' && index_stat != '?' {
                    staged.push(path.clone());
                }
                if work_stat != ' ' && work_stat != '?' {
                    unstaged.push(path);
                }
            }
        }
    }

    Ok(GitStatusResult {
        branch,
        staged,
        unstaged,
        untracked,
    })
}

/// Runs `git diff` on `repo_path` (or current directory).
///
/// If `staged` is true, passes `--staged`.
/// If `file` is provided, restricts diff to that path.
pub fn tool_git_diff(
    repo_path: Option<&str>,
    staged: bool,
    file: Option<&str>,
) -> Result<String> {
    let target_dir = repo_path.map(Path::new).unwrap_or_else(|| Path::new("."));

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(target_dir).arg("diff");

    if staged {
        cmd.arg("--staged");
    }

    if let Some(f) = file {
        cmd.arg("--").arg(f);
    }

    let output = cmd.output().context("Failed to run git diff command")?;

    if !output.status.success() {
        bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Commits staged (or all modified if `all` is true) files with `message`.
pub fn tool_git_commit(
    message: &str,
    repo_path: Option<&str>,
    all: bool,
) -> Result<GitCommitResult> {
    if message.trim().is_empty() {
        bail!("Commit message cannot be empty");
    }

    let target_dir = repo_path.map(Path::new).unwrap_or_else(|| Path::new("."));

    let mut commit_cmd = Command::new("git");
    commit_cmd.arg("-C").arg(target_dir).arg("commit");

    if all {
        commit_cmd.arg("-a");
    }

    commit_cmd.arg("-m").arg(message);

    let output = commit_cmd.output().context("Failed to run git commit command")?;

    if !output.status.success() {
        bail!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let rev_output = Command::new("git")
        .arg("-C")
        .arg(target_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("Failed to get HEAD commit hash")?;

    let commit_hash = String::from_utf8_lossy(&rev_output.stdout).trim().to_string();

    Ok(GitCommitResult {
        commit_hash,
        message: message.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestRepo {
        path: std::path::PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let unique = format!(
                "test_git_{}_{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();

            let _ = Command::new("git").arg("init").arg(&path).output().unwrap();
            let _ = Command::new("git").arg("-C").arg(&path).args(["config", "user.name", "Test"]).output().unwrap();
            let _ = Command::new("git").arg("-C").arg(&path).args(["config", "user.email", "test@example.com"]).output().unwrap();

            Self { path }
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_git_status_and_commit_flow() {
        let repo = TestRepo::new();
        let repo_str = repo.path.to_str().unwrap();

        let file1 = repo.path.join("file1.txt");
        fs::write(&file1, "line 1\n").unwrap();

        let status = tool_git_status(Some(repo_str)).unwrap();
        assert!(status.untracked.contains(&"file1.txt".to_string()));

        let _ = Command::new("git").arg("-C").arg(&repo.path).args(["add", "file1.txt"]).output().unwrap();
        let status_staged = tool_git_status(Some(repo_str)).unwrap();
        assert!(status_staged.staged.contains(&"file1.txt".to_string()));

        let commit_res = tool_git_commit("feat: initial commit", Some(repo_str), false).unwrap();
        assert!(!commit_res.commit_hash.is_empty());
        assert_eq!(commit_res.message, "feat: initial commit");

        let status_clean = tool_git_status(Some(repo_str)).unwrap();
        assert!(status_clean.staged.is_empty());
        assert!(status_clean.untracked.is_empty());
    }

    #[test]
    fn test_git_diff_execution() {
        let repo = TestRepo::new();
        let repo_str = repo.path.to_str().unwrap();

        let file1 = repo.path.join("file1.txt");
        fs::write(&file1, "line 1\n").unwrap();
        let _ = Command::new("git").arg("-C").arg(&repo.path).args(["add", "file1.txt"]).output().unwrap();
        let _ = tool_git_commit("initial", Some(repo_str), false).unwrap();

        fs::write(&file1, "line 1\nline 2\n").unwrap();
        let diff = tool_git_diff(Some(repo_str), false, None).unwrap();
        assert!(diff.contains("+line 2"));

        let diff_file = tool_git_diff(Some(repo_str), false, Some("file1.txt")).unwrap();
        assert!(diff_file.contains("+line 2"));
    }
}
