use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CheckpointRecord {
    pub id: usize,
    pub timestamp: String,
    pub file_path: String,
    pub action: String,
    pub snapshot_rel_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CheckpointIndex {
    pub next_id: usize,
    pub records: Vec<CheckpointRecord>,
}

impl Default for CheckpointIndex {
    fn default() -> Self {
        Self {
            next_id: 1,
            records: Vec::new(),
        }
    }
}

thread_local! {
    static CUSTOM_SNAPSHOT_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

pub struct CheckpointManager;

impl CheckpointManager {
    pub fn get_snapshot_dir() -> PathBuf {
        let custom = CUSTOM_SNAPSHOT_DIR.with(|c| c.borrow().clone());
        if let Some(dir) = custom {
            dir
        } else {
            let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            current.join(".ctrl").join("snapshots")
        }
    }

    #[allow(dead_code)]
    pub fn with_snapshot_dir<F, R>(dir: PathBuf, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        CUSTOM_SNAPSHOT_DIR.with(|c| {
            *c.borrow_mut() = Some(dir);
        });
        let result = f();
        CUSTOM_SNAPSHOT_DIR.with(|c| {
            *c.borrow_mut() = None;
        });
        result
    }

    fn get_index_path() -> PathBuf {
        Self::get_snapshot_dir().join("index.json")
    }

    fn load_index() -> CheckpointIndex {
        let path = Self::get_index_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(idx) = serde_json::from_str::<CheckpointIndex>(&content) {
                    return idx;
                }
            }
        }
        CheckpointIndex::default()
    }

    fn save_index(index: &CheckpointIndex) -> Result<()> {
        let dir = Self::get_snapshot_dir();
        fs::create_dir_all(&dir)?;
        let path = Self::get_index_path();
        let json = serde_json::to_string_pretty(index)?;
        fs::write(path, json.as_bytes())?;
        Ok(())
    }

    /// Records a snapshot of a file prior to a mutating operation.
    pub fn record_checkpoint(file_path_str: &str, action: &str) -> Result<usize> {
        let dir = Self::get_snapshot_dir();
        fs::create_dir_all(&dir)?;

        let mut index = Self::load_index();
        let id = index.next_id;
        index.next_id += 1;

        let path = Path::new(file_path_str);
        let exists = path.exists();

        let snapshot_rel_path = if exists {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let snap_name = format!("{}_{}", id, file_name);
            let snap_path = dir.join(&snap_name);

            let content = fs::read(path)
                .with_context(|| format!("Failed to read existing file for snapshot: {}", file_path_str))?;
            fs::write(&snap_path, content)?;
            Some(snap_name)
        } else {
            None
        };

        let now = chrono_or_fallback_timestamp();

        let record = CheckpointRecord {
            id,
            timestamp: now,
            file_path: file_path_str.to_string(),
            action: action.to_string(),
            snapshot_rel_path,
        };

        index.records.push(record);
        Self::save_index(&index)?;

        Ok(id)
    }

    /// Reverts the most recent checkpoint.
    pub fn undo_last() -> Result<String> {
        let mut index = Self::load_index();
        let record = match index.records.pop() {
            Some(r) => r,
            None => anyhow::bail!("No checkpoints available to undo."),
        };

        let target_path = Path::new(&record.file_path);

        match &record.snapshot_rel_path {
            Some(rel_snap) => {
                let snap_path = Self::get_snapshot_dir().join(rel_snap);
                if snap_path.exists() {
                    let prev_bytes = fs::read(&snap_path)?;
                    if let Some(parent) = target_path.parent() {
                        if !parent.as_os_str().is_empty() {
                            fs::create_dir_all(parent)?;
                        }
                    }
                    fs::write(target_path, prev_bytes)?;
                    let _ = fs::remove_file(&snap_path);
                } else {
                    anyhow::bail!("Snapshot file not found: {}", snap_path.display());
                }
                Self::save_index(&index)?;
                Ok(format!(
                    "✔ Undid {} on '{}'. Restored previous state (checkpoint #{})",
                    record.action, record.file_path, record.id
                ))
            }
            None => {
                // File did not exist before this operation -> delete it
                if target_path.exists() {
                    fs::remove_file(target_path)?;
                }
                Self::save_index(&index)?;
                Ok(format!(
                    "✔ Undid {} on '{}'. Deleted newly created file (checkpoint #{})",
                    record.action, record.file_path, record.id
                ))
            }
        }
    }

    /// Lists recent checkpoints.
    pub fn list_checkpoints() -> Vec<CheckpointRecord> {
        Self::load_index().records
    }

    /// Computes unified diff of the most recent change, or a specified file.
    pub fn get_diff(file_filter: Option<&str>) -> Result<String> {
        // Try git diff first if git is available and active
        if let Ok(git_diff) = run_git_diff(file_filter) {
            if !git_diff.trim().is_empty() {
                return Ok(format_color_diff(&git_diff));
            }
        }

        // Fallback to internal snapshot comparison
        let index = Self::load_index();
        let target_record = if let Some(filter) = file_filter {
            index.records.iter().rev().find(|r| r.file_path.contains(filter))
        } else {
            index.records.last()
        };

        let record = match target_record {
            Some(r) => r,
            None => anyhow::bail!("No snapshot checkpoints found to diff."),
        };

        let old_content = match &record.snapshot_rel_path {
            Some(snap_rel) => {
                let p = Self::get_snapshot_dir().join(snap_rel);
                fs::read_to_string(&p).unwrap_or_default()
            }
            None => String::new(),
        };

        let current_content = fs::read_to_string(&record.file_path).unwrap_or_default();

        let diff_text = generate_unified_diff(
            &record.file_path,
            &old_content,
            &current_content,
        );

        if diff_text.trim().is_empty() {
            Ok(format!("No diff detected for '{}' compared to checkpoint #{}", record.file_path, record.id))
        } else {
            Ok(format_color_diff(&diff_text))
        }
    }
}

fn chrono_or_fallback_timestamp() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let mins = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    format!("{:02}:{:02}:{:02} UTC", hours, mins, secs % 60)
}

fn run_git_diff(file_filter: Option<&str>) -> Result<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("diff");
    if let Some(f) = file_filter {
        cmd.arg("--").arg(f);
    }
    let output = cmd.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        anyhow::bail!("git diff failed");
    }
}

pub fn generate_unified_diff(filename: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = if old.is_empty() { Vec::new() } else { old.lines().collect() };
    let new_lines: Vec<&str> = if new.is_empty() { Vec::new() } else { new.lines().collect() };

    if old_lines == new_lines {
        return String::new();
    }

    let n = old_lines.len();
    let m = new_lines.len();

    // Standard LCS dynamic programming table
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    enum DiffOp<'a> {
        Equal(&'a str),
        Delete(&'a str),
        Insert(&'a str),
    }

    let mut ops = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            ops.push(DiffOp::Equal(old_lines[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(DiffOp::Insert(new_lines[j - 1]));
            j -= 1;
        } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
            ops.push(DiffOp::Delete(old_lines[i - 1]));
            i -= 1;
        }
    }
    ops.reverse();

    let mut out = String::new();
    out.push_str(&format!("--- a/{}\n+++ b/{}\n", filename, filename));
    out.push_str(&format!("@@ -1,{} +1,{} @@\n", n.max(1), m.max(1)));

    for op in ops {
        match op {
            DiffOp::Equal(line) => out.push_str(&format!(" {}\n", line)),
            DiffOp::Delete(line) => out.push_str(&format!("-{}\n", line)),
            DiffOp::Insert(line) => out.push_str(&format!("+{}\n", line)),
        }
    }

    out
}

pub fn format_color_diff(raw_diff: &str) -> String {
    let mut out = String::new();
    for line in raw_diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            out.push_str(&format!("\x1B[32m{}\x1B[0m\n", line));
        } else if line.starts_with('-') && !line.starts_with("---") {
            out.push_str(&format!("\x1B[31m{}\x1B[0m\n", line));
        } else if line.starts_with('@') {
            out.push_str(&format!("\x1B[36m{}\x1B[0m\n", line));
        } else if line.starts_with("diff ") || line.starts_with("--- ") || line.starts_with("+++ ") {
            out.push_str(&format!("\x1B[1m{}\x1B[0m\n", line));
        } else {
            out.push_str(&format!("{}\n", line));
        }
    }
    out
}
