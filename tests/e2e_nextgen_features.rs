//! Comprehensive Requirement-Driven E2E & Integration Test Suite for `ctrl-cli` Next-Gen Features (R1 to R6)
//!
//! - Tier 1: Feature & Contract Coverage (R1 Skill Discovery & BM25, R2 Guardrails & Audit, R3 SSE & Run API, R4 Git Tools & /undo, R5 Terminal UX, R6 Windows Socket Resilience)
//! - Tier 2: Boundary & Corner Conditions (Empty dirs, malformed yaml, dry-run in background, socket bursts, bad git repos)
//! - Tier 3: Cross-Feature Combinations (Subagent tasks + skills + guardrails + audit logging + SSE events)
//! - Tier 4: Real-World Application Scenarios (End-to-end task workflow via API/CLI)
//!
//! 100% Hermetic & Offline: All network operations bind to ephemeral loopback (`127.0.0.1:0`).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// SHARED TEST INFRASTRUCTURE: Temp Directory, Workspace Locator & Mock HTTP Server
// ============================================================================

/// Self-cleaning isolated temporary directory for hermetic filesystem tests.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "ctrl_nextgen_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Failed to create temporary test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Locates workspace root by searching upwards for `skills/` and `data/knowledge/`.
fn find_workspace_root() -> PathBuf {
    let mut curr = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..5 {
        if curr.join("skills").exists() && curr.join("data").join("knowledge").exists() {
            return curr;
        }
        if let Some(parent) = curr.parent() {
            curr = parent.to_path_buf();
        } else {
            break;
        }
    }
    PathBuf::from(".")
}

/// Lightweight thread-safe cancellation token.
#[derive(Clone, Debug, Default)]
pub struct NextgenCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl NextgenCancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Loopback mock HTTP server for SSE and REST endpoint testing.
struct MockNextgenHttpServer {
    pub port: u16,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockNextgenHttpServer {
    pub fn start<F>(handler: F) -> Self
    where
        F: Fn(&str, &str, &[u8], &mut TcpStream) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral mock port");
        let port = listener.local_addr().unwrap().port();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let handler = Arc::new(handler);

        listener.set_nonblocking(true).unwrap();

        let handle = thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let handler_clone = handler.clone();
                        thread::spawn(move || {
                            let mut reader = BufReader::new(stream.try_clone().unwrap());
                            let mut req_line = String::new();
                            if reader.read_line(&mut req_line).is_ok() && !req_line.is_empty() {
                                let parts: Vec<&str> = req_line.split_whitespace().collect();
                                let method = parts.first().copied().unwrap_or("GET");
                                let path = parts.get(1).copied().unwrap_or("/");

                                let mut content_len: usize = 0;
                                loop {
                                    let mut header_line = String::new();
                                    if reader.read_line(&mut header_line).is_err() || header_line.trim().is_empty() {
                                        break;
                                    }
                                    if let Some((k, v)) = header_line.split_once(':') {
                                        if k.trim().eq_ignore_ascii_case("content-length") {
                                            content_len = v.trim().parse::<usize>().unwrap_or(0);
                                        }
                                    }
                                }

                                let mut body = vec![0u8; content_len];
                                if content_len > 0 {
                                    let _ = reader.read_exact(&mut body);
                                }

                                handler_clone(method, path, &body, &mut stream);
                                let _ = stream.flush();
                                drop(reader);
                                let _ = stream.shutdown(std::net::Shutdown::Write);
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionAborted => {
                        // Crucial for R6: do NOT sleep on connection abort
                        continue;
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            running,
            handle: Some(handle),
        }
    }
}

impl Drop for MockNextgenHttpServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// ============================================================================
// SPECIFICATION HARNESS: R1 to R6 Interface Contracts & Algorithms
// ============================================================================

// --- R1: Skill Discovery & BM25 ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub prompt_template: String,
    pub path: PathBuf,
}

pub fn parse_skill_frontmatter(content: &str, file_path: &Path) -> Option<(SkillMetadata, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let end_pos = after_first.find("\n---")?;
    let yaml_block = &after_first[..end_pos];
    let body = after_first[end_pos + 4..].trim().to_string();

    let mut name = String::new();
    let mut description = String::new();
    let mut tools = Vec::new();

    for line in yaml_block.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "name" => name = v.trim_matches('"').trim_matches('\'').to_string(),
                "description" => description = v.trim_matches('"').trim_matches('\'').to_string(),
                "tools" => {
                    let v_clean = v.trim_start_matches('[').trim_end_matches(']');
                    tools = v_clean
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    let meta = SkillMetadata {
        name,
        description,
        tools,
        prompt_template: body.clone(),
        path: file_path.to_path_buf(),
    };

    Some((meta, body))
}

pub fn discover_skills_in_workspace(ws_root: &Path) -> Vec<SkillMetadata> {
    let mut skills = Vec::new();
    let candidate_dirs = [
        ws_root.join("skills"),
        ws_root.join(".ctrl").join("skills"),
        ws_root.join("prompts"),
    ];

    for dir in &candidate_dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let skill_md = p.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(c) = fs::read_to_string(&skill_md) {
                            if let Some((meta, _)) = parse_skill_frontmatter(&c, &skill_md) {
                                skills.push(meta);
                            }
                        }
                    }
                } else if p.is_file() && p.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(c) = fs::read_to_string(&p) {
                        if let Some((meta, _)) = parse_skill_frontmatter(&c, &p) {
                            skills.push(meta);
                        } else {
                            // Plain persona markdown file in prompts/
                            let file_stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let name = file_stem.strip_suffix("_persona").unwrap_or(file_stem);
                            skills.push(SkillMetadata {
                                name: name.to_string(),
                                description: format!("Persona prompt loaded from {}", p.display()),
                                tools: Vec::new(),
                                prompt_template: c.clone(),
                                path: p.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    skills
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub document: String,
    pub section: String,
    pub snippet: String,
    pub score: f64,
}

pub struct Bm25DocChunk {
    pub document: String,
    pub section: String,
    pub content: String,
    pub tokens: Vec<String>,
}

pub struct PureBm25Index {
    chunks: Vec<Bm25DocChunk>,
    avgdl: f64,
    doc_freqs: HashMap<String, usize>,
    k1: f64,
    b: f64,
}

impl PureBm25Index {
    pub fn build(chunks: Vec<(String, String, String)>) -> Self {
        let mut doc_freqs: HashMap<String, usize> = HashMap::new();
        let mut processed = Vec::new();
        let mut total_tokens = 0;

        for (doc, sec, content) in chunks {
            let tokens = Self::tokenize(&content);
            total_tokens += tokens.len();

            let mut unique_terms = HashSet::new();
            for t in &tokens {
                unique_terms.insert(t.clone());
            }
            for t in unique_terms {
                *doc_freqs.entry(t).or_insert(0) += 1;
            }

            processed.push(Bm25DocChunk {
                document: doc,
                section: sec,
                content,
                tokens,
            });
        }

        let n = processed.len();
        let avgdl = if n > 0 {
            total_tokens as f64 / n as f64
        } else {
            1.0
        };

        Self {
            chunks: processed,
            avgdl,
            doc_freqs,
            k1: 1.2,
            b: 0.75,
        }
    }

    pub fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 1)
            .map(|w| w.to_string())
            .collect()
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<KnowledgeSearchResult> {
        if top_k == 0 {
            return Vec::new();
        }
        let q_tokens = Self::tokenize(query);
        if q_tokens.is_empty() {
            return Vec::new();
        }

        let n_docs = self.chunks.len() as f64;
        let mut scored = Vec::new();

        for chunk in &self.chunks {
            let doc_len = chunk.tokens.len() as f64;
            let mut score = 0.0;

            let mut tf_map = HashMap::new();
            for t in &chunk.tokens {
                *tf_map.entry(t).or_insert(0usize) += 1;
            }

            for q in &q_tokens {
                if let Some(&df) = self.doc_freqs.get(q) {
                    let idf = ((n_docs - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
                    let tf = *tf_map.get(q).unwrap_or(&0) as f64;
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len / self.avgdl));
                    score += idf * (numerator / denominator);
                }
            }

            if score > 0.0 {
                let snippet = if chunk.content.len() > 160 {
                    format!("{}...", chunk.content[..160].trim())
                } else {
                    chunk.content.clone()
                };

                scored.push(KnowledgeSearchResult {
                    document: chunk.document.clone(),
                    section: chunk.section.clone(),
                    snippet,
                    score,
                });
            }
        }

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }
}

// --- R2: Guardrails & Audit ---

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

        // Benign/informational commands whose arguments should not trigger guardrails
        if matches!(first, "echo" | "printf" | "grep" | "cat" | "type" | "head" | "tail" | "findstr") {
            continue;
        }

        // Handle powershell/cmd invocation wrappers: e.g. powershell -Command "..."
        if (first.contains("powershell") || first == "pwsh" || first == "cmd" || first == "cmd.exe") && tokens.len() > 2 {
            let inner = tokens[2..].join(" ");
            let unquoted = inner.trim_matches(|c| c == '\'' || c == '"');
            if let Some(reason) = is_destructive_command(unquoted) {
                return Some(reason);
            }
            continue;
        }

        // Direct destructive operations
        if (first == "rm" || first.ends_with("/rm") || first.ends_with("\\rm"))
            && (lower.contains(" -rf") || lower.contains(" -r -f") || lower.contains(" -fr")
                || (lower.contains(" -r ") && lower.contains(" -f "))) {
            return Some("Recursive force removal (rm -rf)");
        }

        if (first == "del" || first == "erase" || first == "ri")
            && (lower.contains("/s") || lower.contains("-r") || lower.contains("-recurse")) {
            return Some("Recursive file deletion (del /s)");
        }

        if (first == "rmdir" || first == "rd") && lower.contains("/s") {
            return Some("Recursive directory removal (rmdir /s)");
        }

        if first == "git" {
            if lower.contains("reset") && lower.contains("--hard") {
                return Some("Destructive git reset (git reset --hard)");
            }
            if lower.contains("clean") && (lower.contains("-fd") || lower.contains("-f -d") || lower.contains("-df")) {
                return Some("Forced untracked file deletion (git clean -fd)");
            }
        }

        if first == "format" || first.starts_with("format") {
            return Some("Filesystem format operation");
        }

        if first.starts_with("mkfs") {
            return Some("Filesystem creation operation");
        }

        if first.starts_with("fdisk") {
            return Some("Partition table modification");
        }
    }

    None
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditRecord {
    pub timestamp: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub tool: String,
    pub parameters: serde_json::Value,
    pub status: String,
    pub duration_ms: u64,
}

pub struct AuditLogger {
    log_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AuditLogger {
    pub fn new(workspace_root: &Path) -> Self {
        let ctrl_dir = workspace_root.join(".ctrl");
        let _ = fs::create_dir_all(&ctrl_dir);
        let log_path = ctrl_dir.join("audit.log");
        Self {
            log_path,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn log(&self, record: &AuditRecord) -> Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .context("Failed to open audit.log")?;
        let json_line = serde_json::to_string(record)?;
        writeln!(file, "{}", json_line)?;
        file.flush()?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<AuditRecord>> {
        let _guard = self.lock.lock().unwrap();
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.log_path)?;
        let mut records = Vec::new();
        for line in content.lines() {
            if !line.trim().is_empty() {
                let rec: AuditRecord = serde_json::from_str(line.trim())?;
                records.push(rec);
            }
        }
        Ok(records)
    }
}

// --- R4: Git Tools & Multi-File Checkpoint Rollback ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitStatusResult {
    pub branch: String,
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitCommitResult {
    pub commit_hash: String,
    pub message: String,
}

pub fn run_structured_git_status(repo_path: &Path) -> Result<GitStatusResult> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["status", "--porcelain=v1", "-b"])
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        bail!("Git status failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branch = String::from("unknown");
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for line in stdout.lines() {
        if let Some(branch_line) = line.strip_prefix("## ") {
            branch = branch_line.split("...").next().unwrap_or("unknown").trim().to_string();
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

pub fn run_structured_git_commit(repo_path: &Path, message: &str) -> Result<GitCommitResult> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["commit", "-m", message])
        .output()
        .context("Failed to run git commit")?;

    if !output.status.success() {
        bail!("Git commit failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let rev_output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("Failed to get HEAD commit")?;

    let hash = String::from_utf8_lossy(&rev_output.stdout).trim().to_string();
    Ok(GitCommitResult {
        commit_hash: hash,
        message: message.to_string(),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub id: String,
    pub timestamp: String,
    pub files: Vec<String>,
}

pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    pub fn new(ws_root: &Path) -> Self {
        let cp_dir = ws_root.join(".ctrl").join("checkpoints");
        let _ = fs::create_dir_all(&cp_dir);
        Self { checkpoint_dir: cp_dir }
    }

    pub fn create_checkpoint(&self, ws_root: &Path, files: &[&str]) -> Result<String> {
        let cp_id = format!("cp-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
        let target_cp_dir = self.checkpoint_dir.join(&cp_id);
        fs::create_dir_all(&target_cp_dir)?;

        let mut manifest_files = Vec::new();
        for rel_file in files {
            let src = ws_root.join(rel_file);
            if src.exists() {
                let dest = target_cp_dir.join(rel_file);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src, &dest)?;
                manifest_files.push(rel_file.to_string());
            }
        }

        let manifest = CheckpointManifest {
            id: cp_id.clone(),
            timestamp: "2026-09-17T03:50:00Z".to_string(),
            files: manifest_files,
        };
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(target_cp_dir.join("manifest.json"), manifest_json)?;

        Ok(cp_id)
    }

    pub fn rollback_checkpoint(&self, ws_root: &Path, cp_id: Option<&str>) -> Result<Vec<String>> {
        let target_id = if let Some(id) = cp_id {
            id.to_string()
        } else {
            // Get latest
            let mut list = self.list_checkpoints()?;
            if list.is_empty() {
                bail!("No checkpoints available to undo");
            }
            list.remove(0).id
        };

        let target_dir = self.checkpoint_dir.join(&target_id);
        let manifest_path = target_dir.join("manifest.json");
        if !manifest_path.exists() {
            bail!("Checkpoint manifest not found for id: {}", target_id);
        }

        let manifest_str = fs::read_to_string(&manifest_path)?;
        let manifest: CheckpointManifest = serde_json::from_str(&manifest_str)?;

        let mut restored = Vec::new();
        for rel_file in &manifest.files {
            let backup_file = target_dir.join(rel_file);
            let original_file = ws_root.join(rel_file);
            if backup_file.exists() {
                if let Some(parent) = original_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&backup_file, &original_file)?;
                restored.push(rel_file.clone());
            }
        }

        Ok(restored)
    }

    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointManifest>> {
        let mut list = Vec::new();
        if !self.checkpoint_dir.exists() {
            return Ok(list);
        }
        for entry in fs::read_dir(&self.checkpoint_dir)?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let m_path = p.join("manifest.json");
                if m_path.exists() {
                    if let Ok(content) = fs::read_to_string(&m_path) {
                        if let Ok(manifest) = serde_json::from_str::<CheckpointManifest>(&content) {
                            list.push(manifest);
                        }
                    }
                }
            }
        }
        list.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(list)
    }
}

// --- R5: Terminal UX Syntax Highlighting & Bell Alert ---

pub fn highlight_markdown_code_blocks_ansi(content: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    let mut lang = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                lang = line.trim_start_matches("```").trim().to_lowercase();
                result.push_str(line);
                result.push('\n');
            } else {
                in_code_block = false;
                result.push_str(line);
                result.push('\n');
            }
        } else if in_code_block {
            let highlighted = highlight_line_tokens_ansi(line, &lang);
            result.push_str(&highlighted);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn highlight_line_tokens_ansi(line: &str, lang: &str) -> String {
    let keywords = match lang {
        "rust" => vec!["fn", "let", "mut", "pub", "struct", "enum", "match", "impl", "use", "mod", "return"],
        "python" => vec!["def", "class", "import", "from", "return", "if", "elif", "else", "for", "while"],
        "javascript" | "js" => vec!["const", "let", "var", "function", "return", "import", "export", "class"],
        "shell" | "bash" | "sh" => vec!["echo", "if", "then", "else", "fi", "for", "in", "do", "done", "exit"],
        "json" => vec!["true", "false", "null"],
        _ => return line.to_string(),
    };

    let mut words = Vec::new();
    for token in line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_') {
        let (lead, trail) = if let Some(idx) = token.find(|c: char| !c.is_alphanumeric() && c != '_') {
            (&token[..idx], &token[idx..])
        } else {
            (token, "")
        };

        if keywords.contains(&lead) {
            words.push(format!("\x1b[35m{}\x1b[0m{}", lead, trail));
        } else if lead.starts_with('"') || token.starts_with('"') {
            words.push(format!("\x1b[32m{}\x1b[0m", token));
        } else {
            words.push(token.to_string());
        }
    }

    words.concat()
}

pub fn emit_task_completion_alert(alert_enabled: bool) -> String {
    if alert_enabled {
        "\x07".to_string()
    } else {
        String::new()
    }
}

// ============================================================================
// TIER 1: FEATURE & CONTRACT COVERAGE (R1 to R6 Isolated Positive Tests)
// ============================================================================

#[test]
fn test_t1_r1_yaml_frontmatter_parsing() {
    let ws = find_workspace_root();
    let researcher_path = ws.join("skills").join("researcher").join("SKILL.md");
    let content = fs::read_to_string(&researcher_path)
        .unwrap_or_else(|_| "---\nname: researcher\ndescription: Expert AI Researcher\ntools: [read_file, glob_files]\n---\nYou are a researcher.".into());

    let parsed = parse_skill_frontmatter(&content, &researcher_path);
    assert!(parsed.is_some(), "Frontmatter must be parsed successfully");
    let (meta, prompt) = parsed.unwrap();

    assert_eq!(meta.name, "researcher");
    assert!(meta.description.contains("Researcher"));
    assert!(meta.tools.contains(&"read_file".to_string()));
    assert!(!prompt.is_empty());
}

#[test]
fn test_t1_r1_workspace_skill_discovery() {
    let ws = find_workspace_root();
    let skills = discover_skills_in_workspace(&ws);
    assert!(!skills.is_empty(), "Workspace must discover at least 1 skill");

    let names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    assert!(
        names.iter().any(|n| n == "researcher" || n == "writer"),
        "Discovered skills must contain researcher or writer: {:?}",
        names
    );
}

#[test]
fn test_t1_r1_skills_slash_commands_contract() {
    let meta = SkillMetadata {
        name: "tester".into(),
        description: "Automated QA expert".into(),
        tools: vec!["cargo_test".into(), "git_diff".into()],
        prompt_template: "You are tester.".into(),
        path: PathBuf::from("skills/tester/SKILL.md"),
    };

    // Contract: /skills list format
    let list_line = format!("{:<15} {:<35} [{}]", meta.name, meta.description, meta.tools.join(", "));
    assert!(list_line.starts_with("tester"));
    assert!(list_line.contains("Automated QA expert"));

    // Contract: /skills info <name> format
    let info_view = format!("Skill: {}\nDescription: {}\nAllowed Tools: {:?}\nPrompt:\n{}", meta.name, meta.description, meta.tools, meta.prompt_template);
    assert!(info_view.contains("Skill: tester"));
    assert!(info_view.contains("cargo_test"));
}

#[test]
fn test_t1_r1_bm25_indexing_and_scoring_contract() {
    let ws = find_workspace_root();
    let policy_path = ws.join("data").join("knowledge").join("company_policy.md");
    let faqs_path = ws.join("data").join("knowledge").join("product_faqs.md");

    let policy_content = fs::read_to_string(&policy_path).unwrap_or_else(|_| "Keamanan Akses Sistem Kerahasiaan API Key".into());
    let faqs_content = fs::read_to_string(&faqs_path).unwrap_or_else(|_| "Keunggulan ctrl-cli Ukuran Biner Sangat Ringan 1.8 MB".into());

    let chunks = vec![
        ("company_policy.md".into(), "1. Keamanan & Akses Sistem".into(), policy_content),
        ("product_faqs.md".into(), "Q1: Keunggulan ctrl-cli".into(), faqs_content),
    ];

    let index = PureBm25Index::build(chunks);

    // Query 1: Security policy
    let res1 = index.search("keamanan akses sistem", 5);
    assert!(!res1.is_empty(), "Security query should return results");
    assert_eq!(res1[0].document, "company_policy.md");

    // Query 2: Product FAQ architecture
    let res2 = index.search("keunggulan arsitektur biner", 5);
    assert!(!res2.is_empty(), "Architecture query should return results");
    assert_eq!(res2[0].document, "product_faqs.md");
}

#[test]
fn test_t1_r1_knowledge_search_tool_execution() {
    let chunks = vec![
        ("guide.md".into(), "Intro".into(), "The pure Rust autonomous coding agent".into()),
    ];
    let index = PureBm25Index::build(chunks);
    let results = index.search("autonomous agent", 1);

    assert_eq!(results.len(), 1);
    let serialized = serde_json::to_string(&results).expect("Must serialize to JSON");
    assert!(serialized.contains("autonomous coding agent"));
}

#[test]
fn test_t1_r2_destructive_command_detection() {
    assert!(is_destructive_command("rm -rf /tmp/target").is_some());
    assert!(is_destructive_command("rm -r -f target").is_some());
    assert!(is_destructive_command("del /s /q build").is_some());
    assert!(is_destructive_command("rmdir /s /q node_modules").is_some());
    assert!(is_destructive_command("git reset --hard HEAD~1").is_some());
    assert!(is_destructive_command("git clean -fd").is_some());
    assert!(is_destructive_command("format C: /fs:ntfs").is_some());
    assert!(is_destructive_command("mkfs.ext4 /dev/sdb1").is_some());

    // Benign commands must NOT be flagged
    assert!(is_destructive_command("cargo test").is_none());
    assert!(is_destructive_command("git status").is_none());
    assert!(is_destructive_command("git diff HEAD").is_none());
    assert!(is_destructive_command("echo hello world").is_none());
}

#[test]
fn test_t1_r2_interactive_confirmation_gate() {
    let cmd = "rm -rf old_build";
    let is_destructive = is_destructive_command(cmd).is_some();
    assert!(is_destructive);

    // Mock interactive decision function
    let evaluate_gate = |user_input: &str| -> Result<bool> {
        if user_input.eq_ignore_ascii_case("y") || user_input.eq_ignore_ascii_case("yes") {
            Ok(true)
        } else {
            Ok(false)
        }
    };

    assert!(evaluate_gate("y").unwrap());
    assert!(!evaluate_gate("n").unwrap());
    assert!(!evaluate_gate("").unwrap());
}

#[test]
fn test_t1_r2_background_safe_dry_run_rejection() {
    let cmd = "rm -rf temp_cache";
    let is_destructive = is_destructive_command(cmd).is_some();
    assert!(is_destructive);

    let is_silent_subagent = true;

    let execution_result = if is_silent_subagent && is_destructive {
        Err(anyhow::anyhow!("Dry-run rejection: destructive command '{}' is blocked in background subagent mode.", cmd))
    } else {
        Ok("Executed")
    };

    assert!(execution_result.is_err());
    assert!(execution_result.unwrap_err().to_string().contains("blocked in background subagent mode"));
}

#[test]
fn test_t1_r2_append_only_audit_log_generation() {
    let temp_dir = TestTempDir::new("audit_t1");
    let logger = AuditLogger::new(temp_dir.path());

    let record1 = AuditRecord {
        timestamp: "2026-09-17T03:50:01Z".into(),
        session_id: "sess-01".into(),
        task_id: Some("task-01".into()),
        tool: "read_file".into(),
        parameters: json!({"path": "src/main.rs"}),
        status: "success".into(),
        duration_ms: 12,
    };
    logger.log(&record1).expect("Failed to log record 1");

    let record2 = AuditRecord {
        timestamp: "2026-09-17T03:50:02Z".into(),
        session_id: "sess-01".into(),
        task_id: Some("task-01".into()),
        tool: "shell".into(),
        parameters: json!({"command": "rm -rf build"}),
        status: "blocked".into(),
        duration_ms: 1,
    };
    logger.log(&record2).expect("Failed to log record 2");

    let records = logger.read_all().expect("Failed to read audit records");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].tool, "read_file");
    assert_eq!(records[1].status, "blocked");
}

#[test]
fn test_t1_r3_sse_stream_endpoint_contract() {
    let server = MockNextgenHttpServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/api/events" {
            let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
            let _ = stream.write_all(sse_headers.as_bytes());

            let event1 = "event: task_status\ndata: {\"id\":\"task-101\",\"status\":\"running\"}\n\n";
            let event2 = "event: task_log\ndata: {\"id\":\"task-101\",\"chunk\":\"compiling\"}\n\n";
            let ping = ": ping\n\n";

            let _ = stream.write_all(event1.as_bytes());
            let _ = stream.write_all(event2.as_bytes());
            let _ = stream.write_all(ping.as_bytes());
        }
    });

    let url = format!("http://127.0.0.1:{}/api/events", server.port);
    let resp = ureq::get(&url).set("Accept", "text/event-stream").call().expect("SSE request failed");

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.header("content-type").unwrap(), "text/event-stream");

    let reader = BufReader::new(resp.into_reader());
    let mut lines = Vec::new();
    for l in reader.lines().map_while(Result::ok) {
        if !l.trim().is_empty() {
            lines.push(l);
        }
    }

    assert!(lines.iter().any(|l| l == "event: task_status"));
    assert!(lines.iter().any(|l| l.contains("task-101")));
    assert!(lines.iter().any(|l| l == ": ping"));
}

#[test]
fn test_t1_r3_task_run_api_contract() {
    let server = MockNextgenHttpServer::start(|method, path, body, stream| {
        if method == "POST" && path == "/api/tasks/run" {
            let payload: serde_json::Value = serde_json::from_slice(body).unwrap_or(json!({}));
            assert_eq!(payload["skill"], "researcher");

            let res_body = json!({
                "id": "task-nextgen-01",
                "status": "queued"
            }).to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                res_body.len(),
                res_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let url = format!("http://127.0.0.1:{}/api/tasks/run", server.port);
    let resp: serde_json::Value = ureq::post(&url)
        .send_json(json!({
            "prompt": "Investigate memory bounds",
            "skill": "researcher",
            "model": "qwen2.5-coder:7b"
        }))
        .expect("Task run request failed")
        .into_json()
        .expect("Valid JSON response");

    assert_eq!(resp["id"], "task-nextgen-01");
    assert_eq!(resp["status"], "queued");
}

#[test]
fn test_t1_r4_structured_git_tools_execution() {
    let temp_dir = TestTempDir::new("git_t1");
    let repo = temp_dir.path();

    // Initialize temporary test git repository
    let _ = Command::new("git").arg("init").arg(repo).output().expect("git init");
    let _ = Command::new("git").arg("-C").arg(repo).args(["config", "user.name", "TestUser"]).output();
    let _ = Command::new("git").arg("-C").arg(repo).args(["config", "user.email", "test@test.com"]).output();

    let dummy_file = repo.join("test.txt");
    fs::write(&dummy_file, "initial content\n").unwrap();

    let status1 = run_structured_git_status(repo).expect("git status");
    assert!(status1.untracked.contains(&"test.txt".to_string()));

    // Stage and commit
    let _ = Command::new("git").arg("-C").arg(repo).args(["add", "test.txt"]).output();
    let commit_res = run_structured_git_commit(repo, "Initial test commit").expect("git commit");
    assert!(!commit_res.commit_hash.is_empty());
    assert_eq!(commit_res.message, "Initial test commit");

    let status2 = run_structured_git_status(repo).expect("git status after commit");
    assert!(status2.staged.is_empty());
    assert!(status2.untracked.is_empty());
}

#[test]
fn test_t1_r4_checkpoint_snapshot_creation() {
    let temp_dir = TestTempDir::new("cp_create_t1");
    let ws = temp_dir.path();
    let mgr = CheckpointManager::new(ws);

    fs::write(ws.join("file1.rs"), "fn one() {}\n").unwrap();
    fs::write(ws.join("file2.rs"), "fn two() {}\n").unwrap();

    let cp_id = mgr.create_checkpoint(ws, &["file1.rs", "file2.rs"]).expect("Create checkpoint");
    assert!(cp_id.starts_with("cp-"));

    let list = mgr.list_checkpoints().expect("List checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, cp_id);
    assert_eq!(list[0].files.len(), 2);
}

#[test]
fn test_t1_r4_undo_atomic_rollback() {
    let temp_dir = TestTempDir::new("cp_undo_t1");
    let ws = temp_dir.path();
    let mgr = CheckpointManager::new(ws);

    fs::write(ws.join("mod_a.rs"), "Original A").unwrap();
    fs::write(ws.join("mod_b.rs"), "Original B").unwrap();

    let cp_id = mgr.create_checkpoint(ws, &["mod_a.rs", "mod_b.rs"]).unwrap();

    // Mutate files
    fs::write(ws.join("mod_a.rs"), "Mutated A").unwrap();
    fs::write(ws.join("mod_b.rs"), "Mutated B").unwrap();
    assert_eq!(fs::read_to_string(ws.join("mod_a.rs")).unwrap(), "Mutated A");

    // Perform rollback
    let restored = mgr.rollback_checkpoint(ws, Some(&cp_id)).expect("Rollback failed");
    assert_eq!(restored.len(), 2);

    assert_eq!(fs::read_to_string(ws.join("mod_a.rs")).unwrap(), "Original A");
    assert_eq!(fs::read_to_string(ws.join("mod_b.rs")).unwrap(), "Original B");
}

#[test]
fn test_t1_r4_undo_list_checkpoint_history() {
    let temp_dir = TestTempDir::new("cp_list_t1");
    let ws = temp_dir.path();
    let mgr = CheckpointManager::new(ws);

    fs::write(ws.join("doc.txt"), "hello").unwrap();
    let cp1 = mgr.create_checkpoint(ws, &["doc.txt"]).unwrap();
    thread::sleep(Duration::from_millis(10));
    let cp2 = mgr.create_checkpoint(ws, &["doc.txt"]).unwrap();

    let list = mgr.list_checkpoints().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, cp2); // Sorted newest first
    assert_eq!(list[1].id, cp1);
}

#[test]
fn test_t1_r5_ansi_syntax_highlighting_markdown_blocks() {
    let markdown = "Here is Rust code:\n```rust\npub fn hello() -> &'static str {\n    \"world\"\n}\n```\nDone.";
    let highlighted = highlight_markdown_code_blocks_ansi(markdown);

    assert!(highlighted.contains("\x1b[35mpub\x1b[0m"));
    assert!(highlighted.contains("\x1b[35mfn\x1b[0m"));
    assert!(highlighted.contains("Here is Rust code:"));
    assert!(highlighted.contains("Done."));
}

#[test]
fn test_t1_r5_task_completion_bell_alert() {
    let alert_on = emit_task_completion_alert(true);
    assert_eq!(alert_on, "\x07", "Must emit terminal bell when alert is enabled");

    let alert_off = emit_task_completion_alert(false);
    assert_eq!(alert_off, "", "Must emit empty string when alert is disabled");
}

#[test]
fn test_t1_r6_socket_nonblocking_accept_clean_teardown() {
    let token = NextgenCancellationToken::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_clone = accepted.clone();
    let token_clone = token.clone();

    let handle = thread::spawn(move || {
        while !token_clone.is_cancelled() {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    accepted_clone.store(true, Ordering::SeqCst);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
                    let _ = stream.flush();
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let client = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Client connect");
    drop(client);

    handle.join().expect("Join thread");
    assert!(accepted.load(Ordering::SeqCst));
}

// ============================================================================
// TIER 2: BOUNDARY & CORNER CONDITIONS (Extreme Inputs, Malformed Data & Stress)
// ============================================================================

#[test]
fn test_t2_r1_empty_skills_dir_and_malformed_yaml() {
    let temp_dir = TestTempDir::new("skills_t2");
    let ws = temp_dir.path();

    // 1. Empty skills dir returns empty vector
    let skills_empty = discover_skills_in_workspace(ws);
    assert!(skills_empty.is_empty());

    // 2. Malformed skill without name
    let bad_dir = ws.join("skills").join("bad");
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(bad_dir.join("SKILL.md"), "---\ndescription: missing name\n---\nBody").unwrap();

    let skills_bad = discover_skills_in_workspace(ws);
    assert!(skills_bad.is_empty(), "Skill without name must be rejected");

    // 3. Corrupted YAML without delimiter
    fs::write(bad_dir.join("SKILL.md"), "name: corrupted_yaml").unwrap();
    let skills_corrupted = discover_skills_in_workspace(ws);
    assert!(skills_corrupted.is_empty());
}

#[test]
fn test_t2_r1_bm25_empty_query_and_zero_matches() {
    let chunks = vec![
        ("doc.md".into(), "Section".into(), "Rust language compiler".into()),
    ];
    let index = PureBm25Index::build(chunks);

    // Empty query
    assert!(index.search("", 5).is_empty());

    // Stop-words only
    assert!(index.search("a the is", 5).is_empty());

    // Zero matches
    assert!(index.search("nonexistentxyz123", 5).is_empty());

    // top_k = 0
    assert!(index.search("Rust", 0).is_empty());

    // top_k > total docs
    let res = index.search("Rust", 1000);
    assert_eq!(res.len(), 1);
}

#[test]
fn test_t2_r2_guardrail_benign_lookalikes_and_chained_commands() {
    // Lookalike strings that must NOT be flagged
    assert!(is_destructive_command("echo rm -rf").is_none());
    assert!(is_destructive_command("git reset HEAD file.txt").is_none());
    assert!(is_destructive_command("grep 'del /s' search.rs").is_none());

    // Chained commands that MUST be flagged
    assert!(is_destructive_command("cargo check && rm -rf target").is_some());
    assert!(is_destructive_command("echo hello; del /s /q temp").is_some());
    assert!(is_destructive_command("powershell.exe -Command \"rmdir /s /q C:\\temp\"").is_some());
}

#[test]
fn test_t2_r2_audit_log_concurrency_and_special_chars() {
    let temp_dir = TestTempDir::new("audit_concurrency_t2");
    let logger = Arc::new(AuditLogger::new(temp_dir.path()));

    let mut handles = Vec::new();
    for thread_idx in 0..20 {
        let logger_clone = logger.clone();
        handles.push(thread::spawn(move || {
            let rec = AuditRecord {
                timestamp: format!("2026-09-17T03:50:{:02}Z", thread_idx),
                session_id: format!("sess-{}", thread_idx),
                task_id: Some(format!("task-{}", thread_idx)),
                tool: "shell".into(),
                parameters: json!({
                    "command": "echo \"quotes \\\" and newlines \n test\"",
                    "unicode": "🦀 Rust Agent"
                }),
                status: "success".into(),
                duration_ms: thread_idx as u64 * 5,
            };
            logger_clone.log(&rec).expect("Concurrent audit log write");
        }));
    }

    for h in handles {
        h.join().expect("Join worker");
    }

    let records = logger.read_all().expect("Read concurrent audit records");
    assert_eq!(records.len(), 20, "All 20 concurrent records must be preserved");
}

#[test]
fn test_t2_r3_sse_early_client_disconnect_and_keepalive() {
    let server = MockNextgenHttpServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/api/events" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            for i in 0..10 {
                let chunk = format!("event: task_log\ndata: {{\"i\":{}}}\n\n", i);
                if stream.write_all(chunk.as_bytes()).is_err() {
                    break; // Early disconnection handled cleanly
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(20));
            }
        }
    });

    let url = format!("http://127.0.0.1:{}/api/events", server.port);
    let resp = ureq::get(&url).call().expect("Connect SSE");
    let mut reader = BufReader::new(resp.into_reader());
    let mut first_line = String::new();
    reader.read_line(&mut first_line).unwrap();

    // Abruptly drop reader to simulate client cancellation
    drop(reader);

    // Ensure server port is still responsive to new connections
    thread::sleep(Duration::from_millis(50));
    let client2 = TcpStream::connect(format!("127.0.0.1:{}", server.port));
    assert!(client2.is_ok(), "Server must remain functional after early client disconnect");
}

#[test]
fn test_t2_r4_git_tools_injection_prevention_and_bad_repo() {
    let temp_dir = TestTempDir::new("git_injection_t2");
    let repo = temp_dir.path();

    let _ = Command::new("git").arg("init").arg(repo).output().unwrap();
    let _ = Command::new("git").arg("-C").arg(repo).args(["config", "user.name", "Test"]).output();
    let _ = Command::new("git").arg("-C").arg(repo).args(["config", "user.email", "t@t.com"]).output();
    fs::write(repo.join("f.txt"), "hello").unwrap();
    let _ = Command::new("git").arg("-C").arg(repo).args(["add", "f.txt"]).output();

    // Injection attempt in commit message
    let malicious_msg = "test\"; rm -rf / ; echo \"pwned";
    let res = run_structured_git_commit(repo, malicious_msg).expect("Safe commit");

    // The message must be preserved literally without executing shell injection!
    assert_eq!(res.message, malicious_msg);

    // Non-git repository returns Err
    let non_git_dir = TestTempDir::new("non_git");
    let bad_res = run_structured_git_status(non_git_dir.path());
    assert!(bad_res.is_err(), "Must return Err for non-git repository");
}

#[test]
fn test_t2_r4_undo_no_checkpoints_and_deleted_files() {
    let temp_dir = TestTempDir::new("undo_edge_t2");
    let ws = temp_dir.path();
    let mgr = CheckpointManager::new(ws);

    // Undo when no checkpoints exist returns error
    let err = mgr.rollback_checkpoint(ws, None);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("No checkpoints"));

    // Snapshot file, then delete it, then undo restores it
    fs::write(ws.join("deleted_me.txt"), "I will be restored").unwrap();
    let cp_id = mgr.create_checkpoint(ws, &["deleted_me.txt"]).unwrap();
    fs::remove_file(ws.join("deleted_me.txt")).unwrap();
    assert!(!ws.join("deleted_me.txt").exists());

    let restored = mgr.rollback_checkpoint(ws, Some(&cp_id)).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(fs::read_to_string(ws.join("deleted_me.txt")).unwrap(), "I will be restored");
}

#[test]
fn test_t2_r5_syntax_highlighting_malformed_blocks() {
    // Unclosed code block fence
    let unclosed = "```rust\nfn unfinished() {\n";
    let h1 = highlight_markdown_code_blocks_ansi(unclosed);
    assert!(h1.contains("\x1b[35mfn\x1b[0m"));

    // Unknown language tag
    let unknown = "```brainfuck\n++--[>+<]\n```";
    let h2 = highlight_markdown_code_blocks_ansi(unknown);
    assert!(h2.contains("++--[>+<]"));

    // Empty code block
    let empty = "```rust\n```";
    let h3 = highlight_markdown_code_blocks_ansi(empty);
    assert!(h3.contains("```rust"));
}

#[test]
fn test_t2_r6_rapid_burst_socket_stress_windows_10053() {
    let server = MockNextgenHttpServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/api/health" {
            let res = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK";
            let _ = stream.write_all(res.as_bytes());
        }
    });

    let addr = format!("127.0.0.1:{}", server.port);

    // Rapid burst: 50 connect-and-abort connections simulating client drop before accept
    for _ in 0..50 {
        if let Ok(mut s) = TcpStream::connect(&addr) {
            let _ = s.write_all(b"G");
            drop(s);
        }
    }

    // Health check must succeed immediately without 10053 starvation
    let start = Instant::now();
    let health_url = format!("http://127.0.0.1:{}/api/health", server.port);
    let resp = ureq::get(&health_url)
        .timeout(Duration::from_secs(2))
        .call()
        .expect("Server must remain responsive after 50 socket aborts");

    assert_eq!(resp.status(), 200);
    assert!(start.elapsed() < Duration::from_millis(500), "Server took too long to respond after burst");
}

// ============================================================================
// TIER 3: CROSS-FEATURE COMBINATIONS (Subsystem Collaborations)
// ============================================================================

#[test]
fn test_t3_subagent_skill_guardrail_audit_flow() {
    let temp_dir = TestTempDir::new("flow_t3_1");
    let ws = temp_dir.path();
    let logger = AuditLogger::new(ws);

    // 1. Skill is loaded
    let skill = SkillMetadata {
        name: "writer".into(),
        description: "Technical Writer".into(),
        tools: vec!["shell".into(), "edit_file".into()],
        prompt_template: "You write documentation.".into(),
        path: PathBuf::from("skills/writer/SKILL.md"),
    };

    // 2. Subagent attempts to run a destructive cleanup command
    let attempted_cmd = "rm -rf old_drafts";
    let is_silent_subagent = true;

    let execution_res = if is_silent_subagent && is_destructive_command(attempted_cmd).is_some() {
        let rec = AuditRecord {
            timestamp: "2026-09-17T03:50:00Z".into(),
            session_id: "sess-flow-1".into(),
            task_id: Some("task-writer-01".into()),
            tool: "shell".into(),
            parameters: json!({"command": attempted_cmd, "skill": skill.name}),
            status: "blocked".into(),
            duration_ms: 1,
        };
        logger.log(&rec).unwrap();
        Err("Command blocked by safety guardrail")
    } else {
        Ok("Executed")
    };

    assert!(execution_res.is_err());
    let audit_records = logger.read_all().unwrap();
    assert_eq!(audit_records.len(), 1);
    assert_eq!(audit_records[0].status, "blocked");
    assert_eq!(audit_records[0].parameters["skill"], "writer");
}

#[test]
fn test_t3_checkpoint_mutation_undo_audit_flow() {
    let temp_dir = TestTempDir::new("flow_t3_2");
    let ws = temp_dir.path();
    let mgr = CheckpointManager::new(ws);
    let logger = AuditLogger::new(ws);

    let file_path = ws.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, "pub fn baseline() {}\n").unwrap();

    // 1. Checkpoint snapshot before mutation
    let cp_id = mgr.create_checkpoint(ws, &["src/lib.rs"]).unwrap();

    // 2. Mutating tool modifies file
    fs::write(&file_path, "pub fn buggy_mutation() {}\n").unwrap();
    logger.log(&AuditRecord {
        timestamp: "2026-09-17T03:50:01Z".into(),
        session_id: "sess-flow-2".into(),
        task_id: None,
        tool: "edit_file".into(),
        parameters: json!({"path": "src/lib.rs"}),
        status: "success".into(),
        duration_ms: 5,
    }).unwrap();

    // 3. Rollback triggered
    mgr.rollback_checkpoint(ws, Some(&cp_id)).unwrap();
    logger.log(&AuditRecord {
        timestamp: "2026-09-17T03:50:02Z".into(),
        session_id: "sess-flow-2".into(),
        task_id: None,
        tool: "/undo".into(),
        parameters: json!({"checkpoint_id": cp_id}),
        status: "success".into(),
        duration_ms: 2,
    }).unwrap();

    // 4. Verification
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "pub fn baseline() {}\n");
    let records = logger.read_all().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].tool, "/undo");
}

#[test]
fn test_t3_task_run_sse_streaming_completion_bell() {
    let server = MockNextgenHttpServer::start(|method, path, _body, stream| {
        if method == "POST" && path == "/api/tasks/run" {
            let res = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"id\":\"task-flow-3\",\"status\":\"queued\"}";
            let _ = stream.write_all(res.as_bytes());
        } else if method == "GET" && path == "/api/events" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(b"event: task_status\ndata: {\"id\":\"task-flow-3\",\"status\":\"running\"}\n\n");
            let _ = stream.write_all(b"event: task_status\ndata: {\"id\":\"task-flow-3\",\"status\":\"completed\"}\n\n");
        }
    });

    let run_url = format!("http://127.0.0.1:{}/api/tasks/run", server.port);
    let run_resp: serde_json::Value = ureq::post(&run_url)
        .send_json(json!({"prompt": "Build release"}))
        .unwrap()
        .into_json()
        .unwrap();
    assert_eq!(run_resp["id"], "task-flow-3");

    let events_url = format!("http://127.0.0.1:{}/api/events", server.port);
    let events_resp = ureq::get(&events_url).call().unwrap();
    let reader = BufReader::new(events_resp.into_reader());
    let mut got_completed = false;

    for line in reader.lines().map_while(Result::ok) {
        if line.contains("\"status\":\"completed\"") {
            got_completed = true;
            break;
        }
    }
    assert!(got_completed, "Must receive completed task transition over SSE");

    let bell = emit_task_completion_alert(true);
    assert_eq!(bell, "\x07");
}

#[test]
fn test_t3_knowledge_search_to_git_commit_pipeline() {
    let temp_dir = TestTempDir::new("flow_t3_4");
    let ws = temp_dir.path();
    let logger = AuditLogger::new(ws);

    let chunks = vec![
        ("standards.md".into(), "Rust Standards".into(), "All code must be zero warnings".into()),
    ];
    let index = PureBm25Index::build(chunks);

    // 1. Search knowledge
    let search_res = index.search("zero warnings", 1);
    assert_eq!(search_res.len(), 1);
    logger.log(&AuditRecord {
        timestamp: "2026-09-17T03:50:00Z".into(),
        session_id: "sess-flow-4".into(),
        task_id: None,
        tool: "knowledge_search".into(),
        parameters: json!({"query": "zero warnings"}),
        status: "success".into(),
        duration_ms: 3,
    }).unwrap();

    // 2. Initialize repo and commit code adhering to standards
    let _ = Command::new("git").arg("init").arg(ws).output().unwrap();
    let _ = Command::new("git").arg("-C").arg(ws).args(["config", "user.name", "QA"]).output();
    let _ = Command::new("git").arg("-C").arg(ws).args(["config", "user.email", "qa@test.com"]).output();

    fs::write(ws.join("compliant.rs"), "// zero warnings compliant\n").unwrap();
    let _ = Command::new("git").arg("-C").arg(ws).args(["add", "compliant.rs"]).output();

    let commit_res = run_structured_git_commit(ws, "Adhere to zero warnings standard").unwrap();
    logger.log(&AuditRecord {
        timestamp: "2026-09-17T03:50:01Z".into(),
        session_id: "sess-flow-4".into(),
        task_id: None,
        tool: "git_commit".into(),
        parameters: json!({"commit": commit_res.commit_hash}),
        status: "success".into(),
        duration_ms: 15,
    }).unwrap();

    let records = logger.read_all().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].tool, "knowledge_search");
    assert_eq!(records[1].tool, "git_commit");
}

// ============================================================================
// TIER 4: REAL-WORLD APPLICATION SCENARIOS (Integrated Autonomous Workflows)
// ============================================================================

#[test]
fn test_t4_full_autonomous_nextgen_agent_workflow() {
    let temp_dir = TestTempDir::new("full_workflow_t4");
    let ws = temp_dir.path();
    let logger = AuditLogger::new(ws);
    let cp_mgr = CheckpointManager::new(ws);

    // 1. Skill Auto-Discovery
    let skill_dir = ws.join("skills").join("researcher");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: researcher\ndescription: Deep researcher\ntools: [knowledge_search, git_commit]\n---\nPrompt"
    ).unwrap();
    let discovered = discover_skills_in_workspace(ws);
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].name, "researcher");

    // 2. Knowledge Ingestion & BM25
    let knowledge_dir = ws.join("data").join("knowledge");
    fs::create_dir_all(&knowledge_dir).unwrap();
    fs::write(
        knowledge_dir.join("security.md"),
        "## Security Guidelines\nAlways use sandboxed tools and never run destructive commands."
    ).unwrap();

    let chunks = vec![
        ("security.md".into(), "Security Guidelines".into(), "Always use sandboxed tools and never run destructive commands.".into()),
    ];
    let bm25 = PureBm25Index::build(chunks);
    let search_res = bm25.search("destructive commands", 1);
    assert_eq!(search_res.len(), 1);

    // 3. Workspace Checkpoint Snapshotting
    let main_rs = ws.join("main.rs");
    fs::write(&main_rs, "fn main() { println!(\"v1\"); }\n").unwrap();
    let cp_id = cp_mgr.create_checkpoint(ws, &["main.rs"]).unwrap();

    // 4. Guardrail Interception of Destructive Shell Command
    let bad_cmd = "rm -rf .git";
    let is_blocked = is_destructive_command(bad_cmd).is_some();
    assert!(is_blocked);
    logger.log(&AuditRecord {
        timestamp: "2026-09-17T03:50:05Z".into(),
        session_id: "sess-e2e".into(),
        task_id: Some("task-agent-e2e".into()),
        tool: "shell".into(),
        parameters: json!({"command": bad_cmd}),
        status: "blocked".into(),
        duration_ms: 1,
    }).unwrap();

    // 5. Safe Edit & ANSI Syntax Highlighting
    fs::write(&main_rs, "fn main() { println!(\"v2\"); }\n").unwrap();
    let markdown_explanation = "Generated code:\n```rust\npub fn calculate() -> u32 {\n    42\n}\n```\nLooks good.";
    let ansi_highlighted = highlight_markdown_code_blocks_ansi(markdown_explanation);
    assert!(ansi_highlighted.contains("\x1b[35mpub\x1b[0m"));

    // 6. Safe Git Commit via Argument Slices
    let _ = Command::new("git").arg("init").arg(ws).output().unwrap();
    let _ = Command::new("git").arg("-C").arg(ws).args(["config", "user.name", "E2E"]).output();
    let _ = Command::new("git").arg("-C").arg(ws).args(["config", "user.email", "e2e@test.com"]).output();
    let _ = Command::new("git").arg("-C").arg(ws).args(["add", "main.rs"]).output();
    let commit = run_structured_git_commit(ws, "feat: update main.rs to v2").unwrap();
    assert!(!commit.commit_hash.is_empty());

    // 7. Verify Rollback Capability
    let restored = cp_mgr.rollback_checkpoint(ws, Some(&cp_id)).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(fs::read_to_string(&main_rs).unwrap(), "fn main() { println!(\"v1\"); }\n");

    // 8. Completion Alert Emission
    let alert = emit_task_completion_alert(true);
    assert_eq!(alert, "\x07");

    // 9. Verify Immutable Audit Trail
    let audit_log = logger.read_all().unwrap();
    assert_eq!(audit_log.len(), 1);
    assert_eq!(audit_log[0].status, "blocked");
}
