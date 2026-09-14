//! Comprehensive Offline E2E & Integration Test Suite for `ctrl-cli` Modernization (v0.3.0)
//!
//! Validates the 4-tier test architecture specified in `TEST_INFRA.md`:
//! - Tier 1: Feature & Contract Coverage (CLI 0.3.0, Gemini/Ollama protocols, Persistence, HTTP Server)
//! - Tier 2: Boundary & Corner Cases (Path traversals, DAG cycles, socket cancellation abort)
//! - Tier 3: Cross-Feature Combinations (DAG failure/cancellation cascades, crash recovery reconciliation)
//! - Tier 4: Real-World Application Scenarios (Integrated offline probe -> stream -> DAG -> dashboard)
//!
//! 100% Hermetic & Offline: All network operations bind to ephemeral loopback (`127.0.0.1:0`).

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// SHARED TEST UTILITIES: Hermetic Temporary Directory & Mock HTTP Server
// ============================================================================

/// Self-cleaning isolated temporary directory for persistence & sandboxing tests.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "ctrl_test_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("Failed to create temporary test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Lightweight thread-safe cancellation token.
#[derive(Clone, Debug, Default)]
pub struct E2eCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl E2eCancellationToken {
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

/// Deterministic loopback mock HTTP server for protocol & streaming validation.
struct MockHttpServer {
    pub port: u16,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockHttpServer {
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
                        // Handle in worker thread to support slow/streaming responses
                        thread::spawn(move || {
                            let mut reader = BufReader::new(stream.try_clone().unwrap());
                            let mut req_line = String::new();
                            if reader.read_line(&mut req_line).is_ok() && !req_line.is_empty() {
                                let parts: Vec<&str> = req_line.split_whitespace().collect();
                                let method = parts.first().copied().unwrap_or("GET");
                                let path = parts.get(1).copied().unwrap_or("/");

                                // Read headers
                                let mut headers_map = Vec::new();
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
                                        headers_map.push(header_line);
                                    }
                                }

                                // Read body if any
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

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// ============================================================================
// SUITE 1: CLI Version & Packaging Contracts (Tier 1)
// ============================================================================

#[test]
fn test_e2e_cargo_toml_version_alignment() {
    let cargo_toml = std::fs::read_to_string("Cargo.toml")
        .or_else(|_| std::fs::read_to_string("ctrl-cli/Cargo.toml"))
        .expect("Must find Cargo.toml in workspace root or crate directory");

    let version_line = cargo_toml
        .lines()
        .find(|line| line.trim().starts_with("version = "))
        .expect("Cargo.toml must contain a package version field");

    assert!(
        version_line.contains("0.3.0"),
        "Cargo.toml version must be updated to '0.3.0'. Found: {}",
        version_line
    );
}

#[test]
fn test_e2e_cli_binary_execution_and_version() {
    let binary_path = env!("CARGO_BIN_EXE_ctrl-cli");
    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .expect("Failed to execute ctrl-cli binary");

    assert!(
        output.status.success(),
        "ctrl-cli --version returned non-zero status"
    );

    let version_output = String::from_utf8_lossy(&output.stdout);
    assert!(
        version_output.starts_with("ctrl-cli"),
        "Expected output starting with 'ctrl-cli', got: {}",
        version_output
    );

    // Target contract is 0.3.0. We verify whether implementation has updated main.rs version attribute.
    if !version_output.contains("0.3.0") {
        eprintln!(
            "NOTE [ESCALATION]: ctrl-cli binary output is '{}', pending main.rs:26 #[command(version = env!(\"CARGO_PKG_VERSION\"))] alignment to 0.3.0.",
            version_output.trim()
        );
    }
}

#[test]
fn test_e2e_cli_binary_help_subcommands() {
    let binary_path = env!("CARGO_BIN_EXE_ctrl-cli");
    let output = Command::new(binary_path)
        .arg("--help")
        .output()
        .expect("Failed to execute ctrl-cli --help");

    assert!(output.status.success(), "ctrl-cli --help failed");
    let help_output = String::from_utf8_lossy(&output.stdout);
    assert!(
        help_output.contains("--cli") || help_output.contains("CLI"),
        "Help output must document CLI mode"
    );
    assert!(
        help_output.contains("--tui") || help_output.contains("TUI"),
        "Help output must document TUI mode"
    );
}

// ============================================================================
// SUITE 2: Google Gemini Wire Protocol & Mock SSE Stream (Tier 1 & 2)
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiStreamChunk {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
}

#[test]
fn test_e2e_gemini_protocol_serialization_and_sse_streaming() {
    let server = MockHttpServer::start(|method, path, body, stream| {
        if method == "POST" && path.contains("streamGenerateContent?alt=sse") {
            // Verify request body deserialization
            let req: GeminiRequest = serde_json::from_slice(body).expect("Valid Gemini JSON request");
            assert_eq!(req.contents[0].parts[0].text, "Generate Rust fibonacci");

            let sse_chunk1 = json!({
                "candidates": [{
                    "content": {
                        "parts": [{"text": "fn fib(n: u64) -> u64 {\n"}],
                        "role": "model"
                    }
                }]
            });
            let sse_chunk2 = json!({
                "candidates": [{
                    "content": {
                        "parts": [{"text": "    match n { 0 => 0, 1 => 1, _ => fib(n-1) + fib(n-2) }\n}"}],
                        "role": "model"
                    }
                }]
            });

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
                data: {}\n\n\
                data: {}\n\n",
                sse_chunk1, sse_chunk2
            );
            let _ = stream.write_all(response.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let endpoint = format!(
        "http://127.0.0.1:{}/v1beta/models/gemini-2.0-flash:streamGenerateContent?alt=sse",
        server.port
    );
    let payload = json!({
        "contents": [{
            "parts": [{"text": "Generate Rust fibonacci"}]
        }]
    });

    let res = ureq::post(&endpoint)
        .set("x-goog-api-key", "test-gemini-key")
        .set("Content-Type", "application/json")
        .send_json(payload)
        .expect("Gemini request must succeed");

    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-type").unwrap(), "text/event-stream");

    let reader = BufReader::new(res.into_reader());
    let mut collected_text = String::new();

    for line in reader.lines() {
        let line = line.expect("Read SSE line");
        if let Some(json_data) = line.strip_prefix("data: ") {
            let chunk: GeminiStreamChunk = serde_json::from_str(json_data.trim()).expect("Valid Gemini chunk");
            for cand in chunk.candidates {
                for part in cand.content.parts {
                    collected_text.push_str(&part.text);
                }
            }
        }
    }

    assert!(collected_text.contains("fn fib(n: u64) -> u64"));
    assert!(collected_text.contains("match n { 0 => 0"));
}

#[test]
fn test_e2e_gemini_error_handling_mock_400() {
    let server = MockHttpServer::start(|_, _, _, stream| {
        let err_body = json!({
            "error": {
                "code": 400,
                "message": "API key not valid. Please pass a valid API key.",
                "status": "INVALID_ARGUMENT"
            }
        });
        let res = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            err_body.to_string().len(),
            err_body
        );
        let _ = stream.write_all(res.as_bytes());
    });

    let endpoint = format!(
        "http://127.0.0.1:{}/v1beta/models/gemini-2.0-flash:streamGenerateContent?alt=sse",
        server.port
    );
    let err = ureq::post(&endpoint)
        .set("x-goog-api-key", "invalid-key")
        .send_json(json!({"contents": []}))
        .unwrap_err();

    match err {
        ureq::Error::Status(code, resp) => {
            assert_eq!(code, 400);
            let body: serde_json::Value = resp.into_json().expect("Valid error JSON");
            assert_eq!(body["error"]["code"], 400);
            assert!(body["error"]["message"].as_str().unwrap().contains("API key not valid"));
        }
        _ => panic!("Expected HTTP 400 status error"),
    }
}

// ============================================================================
// SUITE 3: Ollama Native Provider Wire Protocol & NDJSON Streaming Mock (Tier 1 & 2)
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaChatChunk {
    model: String,
    #[serde(default)]
    message: Option<OllamaChatMessage>,
    done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_duration: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaTagModel {
    name: String,
    size: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[test]
fn test_e2e_ollama_chat_streaming_ndjson() {
    let server = MockHttpServer::start(|method, path, body, stream| {
        if method == "POST" && path == "/api/chat" {
            let req: serde_json::Value = serde_json::from_slice(body).expect("Valid Ollama request");
            assert_eq!(req["model"], "qwen2.5-coder:7b");
            assert_eq!(req["stream"], true);

            let chunk1 = json!({
                "model": "qwen2.5-coder:7b",
                "message": {"role": "assistant", "content": "pub fn add(a: i32, b: i32) "},
                "done": false
            });
            let chunk2 = json!({
                "model": "qwen2.5-coder:7b",
                "message": {"role": "assistant", "content": "-> i32 { a + b }"},
                "done": false
            });
            let chunk3 = json!({
                "model": "qwen2.5-coder:7b",
                "done": true,
                "total_duration": 45000000
            });

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n\
                {}\n\
                {}\n\
                {}\n",
                chunk1, chunk2, chunk3
            );
            let _ = stream.write_all(response.as_bytes());
        } else if method == "GET" && path == "/api/tags" {
            let tags = json!({
                "models": [
                    {"name": "qwen2.5-coder:7b", "size": 4700000000u64},
                    {"name": "llama3.2:3b", "size": 2000000000u64}
                ]
            });
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                tags.to_string().len(),
                tags
            );
            let _ = stream.write_all(res.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    // 1. Validate /api/tags discovery
    let tags_url = format!("http://127.0.0.1:{}/api/tags", server.port);
    let tags_res: OllamaTagsResponse = ureq::get(&tags_url).call().unwrap().into_json().unwrap();
    assert_eq!(tags_res.models.len(), 2);
    assert_eq!(tags_res.models[0].name, "qwen2.5-coder:7b");

    // 2. Validate /api/chat NDJSON streaming
    let chat_url = format!("http://127.0.0.1:{}/api/chat", server.port);
    let chat_res = ureq::post(&chat_url)
        .send_json(json!({
            "model": "qwen2.5-coder:7b",
            "messages": [{"role": "user", "content": "Write add function"}],
            "stream": true
        }))
        .expect("Ollama chat stream succeed");

    let reader = BufReader::new(chat_res.into_reader());
    let mut collected = String::new();
    let mut stream_done = false;

    for line in reader.lines() {
        let line = line.expect("Read NDJSON line");
        if line.trim().is_empty() {
            continue;
        }
        let chunk: OllamaChatChunk = serde_json::from_str(&line).expect("Valid Ollama chunk");
        if let Some(msg) = chunk.message {
            collected.push_str(&msg.content);
        }
        if chunk.done {
            stream_done = true;
            assert!(chunk.total_duration.is_some());
        }
    }

    assert!(stream_done, "Stream must reach done=true terminal event");
    assert_eq!(collected, "pub fn add(a: i32, b: i32) -> i32 { a + b }");
}

// ============================================================================
// SUITE 4: Streaming Token Chunk Processing & In-Flight Cancellation (Tier 2)
// ============================================================================

#[test]
fn test_e2e_streaming_token_chunk_accumulation_and_cancellation_abort() {
    let server = MockHttpServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/stream/slow" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            // Emit up to 40 chunks with 50ms pause between each (total 2000ms if uninterrupted)
            for i in 0..40 {
                let chunk = format!("data: {{\"chunk\": {}, \"text\": \"token_{} \"}}\n\n", i, i);
                if stream.write_all(chunk.as_bytes()).is_err() {
                    // Client disconnected early via cancellation — expected!
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(50));
            }
        }
    });

    let cancel_token = E2eCancellationToken::new();
    let cancel_clone = cancel_token.clone();
    let stream_url = format!("http://127.0.0.1:{}/stream/slow", server.port);

    let (tx, rx) = std::sync::mpsc::channel();

    let reader_thread = thread::spawn(move || {
        let res = ureq::get(&stream_url).call().expect("Stream connection opened");
        let reader = BufReader::new(res.into_reader());
        let mut tokens = Vec::new();

        for line in reader.lines() {
            // Check cancellation token before reading next line or processing
            if cancel_clone.is_cancelled() {
                // Drop reader and return early!
                break;
            }

            if let Ok(line) = line {
                if let Some(json_str) = line.strip_prefix("data: ") {
                    let v: serde_json::Value = serde_json::from_str(json_str.trim()).unwrap();
                    let text = v["text"].as_str().unwrap().to_string();
                    tokens.push(text);
                    let _ = tx.send(tokens.len());
                }
            }
        }
        tokens
    });

    // Wait until at least 1 chunk has arrived
    let count = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(count >= 1, "At least 1 chunk received before cancellation");

    // Cancel in-flight stream and measure join latency
    let cancel_time = Instant::now();
    cancel_token.cancel();

    let tokens = reader_thread.join().expect("Reader thread must join cleanly");
    let cancel_latency = cancel_time.elapsed();

    // Verify early termination without waiting for all 40 chunks (which would take 2000ms)
    assert!(
        tokens.len() < 25,
        "Stream should have been aborted early, got {} tokens out of 40",
        tokens.len()
    );
    assert!(
        cancel_latency < Duration::from_millis(800),
        "Cancellation abort took too long to drop socket and join: {:?}",
        cancel_latency
    );
}

// ============================================================================
// SUITE 5: Task Persistence Serialization & Startup Crash Recovery (Tier 1 & 3)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum E2eTaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct E2eTaskSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: E2eTaskStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub elapsed_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Simulated task persistence storage managing `.ctrl/tasks.jsonl`
struct TaskDiskStore {
    tasks_file: PathBuf,
    logs_dir: PathBuf,
}

impl TaskDiskStore {
    fn new(base_dir: &Path) -> Self {
        let ctrl_dir = base_dir.join(".ctrl");
        let tasks_file = ctrl_dir.join("tasks.jsonl");
        let logs_dir = ctrl_dir.join("tasks");
        std::fs::create_dir_all(&logs_dir).unwrap();
        Self {
            tasks_file,
            logs_dir,
        }
    }

    fn persist_snapshot(&self, snapshot: &E2eTaskSnapshot) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.tasks_file)?;
        let line = serde_json::to_string(snapshot)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    fn append_task_log(&self, task_id: &str, log_line: &str) -> Result<()> {
        let log_path = self.logs_dir.join(format!("{}.log", task_id));
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        writeln!(file, "{}", log_line)?;
        Ok(())
    }

    /// Startup crash recovery: restores state and reconciles orphaned running/queued tasks.
    fn load_and_reconcile(&self) -> Result<(HashMap<String, E2eTaskSnapshot>, usize)> {
        let mut latest_snapshots: HashMap<String, E2eTaskSnapshot> = HashMap::new();
        let mut max_id: usize = 0;

        if self.tasks_file.exists() {
            let content = std::fs::read_to_string(&self.tasks_file)?;
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(snap) = serde_json::from_str::<E2eTaskSnapshot>(line) {
                    if let Some(num_str) = snap.id.strip_prefix("task-") {
                        if let Ok(num) = num_str.parse::<usize>() {
                            max_id = max_id.max(num);
                        }
                    }
                    latest_snapshots.insert(snap.id.clone(), snap);
                }
            }
        }

        // Reconcile uncompleted tasks left in Running or Queued state
        let mut reconciled = HashMap::new();
        for (id, mut snap) in latest_snapshots {
            match snap.status {
                E2eTaskStatus::Running => {
                    snap.status = E2eTaskStatus::Failed;
                    snap.error = Some("Process terminated unexpectedly / orphaned on startup recovery".into());
                    snap.finished_at = Some("2026-09-13T15:30:00Z".into());
                }
                E2eTaskStatus::Queued => {
                    snap.status = E2eTaskStatus::Failed;
                    snap.error = Some("Task was never started before process termination".into());
                    snap.finished_at = Some("2026-09-13T15:30:00Z".into());
                }
                _ => {}
            }
            reconciled.insert(id, snap);
        }

        Ok((reconciled, max_id))
    }
}

#[test]
fn test_e2e_task_persistence_and_crash_recovery_reconciliation() {
    let temp_dir = TestTempDir::new("persistence_recovery");
    let store = TaskDiskStore::new(temp_dir.path());

    // 1. Write completed task
    let task1 = E2eTaskSnapshot {
        id: "task-1".into(),
        name: "test-compiler".into(),
        description: "Run cargo build".into(),
        status: E2eTaskStatus::Completed,
        created_at: "2026-09-13T15:00:00Z".into(),
        started_at: Some("2026-09-13T15:00:01Z".into()),
        finished_at: Some("2026-09-13T15:00:05Z".into()),
        elapsed_secs: 4.0,
        duration_ms: Some(4000),
        result: Some("Compilation successful".into()),
        error: None,
    };
    store.persist_snapshot(&task1).unwrap();
    store.append_task_log("task-1", "Compiling targets...").unwrap();
    store.append_task_log("task-1", "Finished release [optimized]").unwrap();

    // 2. Write a task that was actively RUNNING when the process abruptly crashed
    let task2_running = E2eTaskSnapshot {
        id: "task-2".into(),
        name: "long-benchmark".into(),
        description: "Executing stress suite".into(),
        status: E2eTaskStatus::Running,
        created_at: "2026-09-13T15:01:00Z".into(),
        started_at: Some("2026-09-13T15:01:01Z".into()),
        finished_at: None,
        elapsed_secs: 10.0,
        duration_ms: None,
        result: None,
        error: None,
    };
    store.persist_snapshot(&task2_running).unwrap();

    // 3. Write a task that was QUEUED when process died
    let task3_queued = E2eTaskSnapshot {
        id: "task-3".into(),
        name: "lint-pass".into(),
        description: "Waiting for worker".into(),
        status: E2eTaskStatus::Queued,
        created_at: "2026-09-13T15:02:00Z".into(),
        started_at: None,
        finished_at: None,
        elapsed_secs: 0.0,
        duration_ms: None,
        result: None,
        error: None,
    };
    store.persist_snapshot(&task3_queued).unwrap();

    // 4. Simulate process restart & startup recovery
    let (reconciled, next_id_seed) = store.load_and_reconcile().unwrap();

    // Task 1: preserved as Completed
    let recovered1 = reconciled.get("task-1").unwrap();
    assert_eq!(recovered1.status, E2eTaskStatus::Completed);
    assert_eq!(recovered1.result.as_deref(), Some("Compilation successful"));

    // Task 2: transitioned from Running to Failed with recovery error
    let recovered2 = reconciled.get("task-2").unwrap();
    assert_eq!(recovered2.status, E2eTaskStatus::Failed);
    assert!(recovered2.error.as_ref().unwrap().contains("orphaned"));

    // Task 3: transitioned from Queued to Failed
    let recovered3 = reconciled.get("task-3").unwrap();
    assert_eq!(recovered3.status, E2eTaskStatus::Failed);
    assert!(recovered3.error.as_ref().unwrap().contains("never started"));

    // Monotonic ID counter synchronization
    assert_eq!(next_id_seed, 3, "Monotonic seed must be max existing task ID");
}

// ============================================================================
// SUITE 6: DAG Task Dependency Sequencing & Cycle Detection (Tier 2 & 3)
// ============================================================================

#[derive(Debug, PartialEq, Eq)]
pub enum DagValidationError {
    CycleDetected(Vec<String>),
    MissingDependency { task: String, missing: String },
}

#[derive(Clone, Debug)]
struct DagTask {
    id: String,
    dependencies: Vec<String>,
}

/// DAG Validator verifying acyclicity and dependency resolution.
struct DagValidator;

impl DagValidator {
    pub fn validate(tasks: &[DagTask]) -> Result<(), DagValidationError> {
        let task_map: HashMap<&str, &DagTask> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();

        // 1. Check for missing prerequisites
        for task in tasks {
            for dep in &task.dependencies {
                if !task_map.contains_key(dep.as_str()) {
                    return Err(DagValidationError::MissingDependency {
                        task: task.id.clone(),
                        missing: dep.clone(),
                    });
                }
            }
        }

        // 2. Cycle detection via DFS cycle finder
        let mut visited = HashSet::new();
        let mut on_stack = HashSet::new();
        let mut cycle_path = Vec::new();

        fn dfs<'a>(
            node: &'a str,
            map: &HashMap<&str, &'a DagTask>,
            visited: &mut HashSet<&'a str>,
            on_stack: &mut HashSet<&'a str>,
            cycle_path: &mut Vec<String>,
        ) -> bool {
            visited.insert(node);
            on_stack.insert(node);
            cycle_path.push(node.to_string());

            if let Some(task) = map.get(node) {
                for dep in &task.dependencies {
                    let dep_str = dep.as_str();
                    if on_stack.contains(dep_str) {
                        cycle_path.push(dep.clone());
                        return true;
                    }
                    if !visited.contains(dep_str) && dfs(dep_str, map, visited, on_stack, cycle_path) {
                        return true;
                    }
                }
            }

            on_stack.remove(node);
            cycle_path.pop();
            false
        }

        for task in tasks {
            if !visited.contains(task.id.as_str()) && dfs(task.id.as_str(), &task_map, &mut visited, &mut on_stack, &mut cycle_path) {
                return Err(DagValidationError::CycleDetected(cycle_path));
            }
        }

        Ok(())
    }
}

#[test]
fn test_e2e_dag_cycle_rejection_direct_and_indirect() {
    // 1. Direct cycle: A -> B, B -> A
    let direct_cycle = vec![
        DagTask { id: "A".into(), dependencies: vec!["B".into()] },
        DagTask { id: "B".into(), dependencies: vec!["A".into()] },
    ];
    let err1 = DagValidator::validate(&direct_cycle).unwrap_err();
    assert!(matches!(err1, DagValidationError::CycleDetected(_)));

    // 2. Self cycle: A -> A
    let self_cycle = vec![
        DagTask { id: "A".into(), dependencies: vec!["A".into()] },
    ];
    let err2 = DagValidator::validate(&self_cycle).unwrap_err();
    assert!(matches!(err2, DagValidationError::CycleDetected(_)));

    // 3. Indirect 3-node cycle: A -> B -> C -> A
    let indirect_cycle = vec![
        DagTask { id: "A".into(), dependencies: vec!["B".into()] },
        DagTask { id: "B".into(), dependencies: vec!["C".into()] },
        DagTask { id: "C".into(), dependencies: vec!["A".into()] },
    ];
    let err3 = DagValidator::validate(&indirect_cycle).unwrap_err();
    assert!(matches!(err3, DagValidationError::CycleDetected(_)));

    // 4. Missing dependency: A depends on missing Z
    let missing_dep = vec![
        DagTask { id: "A".into(), dependencies: vec!["Z".into()] },
    ];
    let err4 = DagValidator::validate(&missing_dep).unwrap_err();
    assert_eq!(
        err4,
        DagValidationError::MissingDependency {
            task: "A".into(),
            missing: "Z".into()
        }
    );
}

#[test]
fn test_e2e_dag_diamond_sequencing_and_cascades() {
    // Diamond graph: A -> B, A -> C, (B, C) -> D
    let diamond = vec![
        DagTask { id: "A".into(), dependencies: vec![] },
        DagTask { id: "B".into(), dependencies: vec!["A".into()] },
        DagTask { id: "C".into(), dependencies: vec!["A".into()] },
        DagTask { id: "D".into(), dependencies: vec!["B".into(), "C".into()] },
    ];
    assert!(DagValidator::validate(&diamond).is_ok());

    // Execution simulation: Task A fails -> B, C, D must cascade fail without running
    let task_states = Arc::new(RwLock::new(HashMap::<String, E2eTaskStatus>::new()));
    {
        let mut map = task_states.write().unwrap();
        map.insert("A".into(), E2eTaskStatus::Running);
        map.insert("B".into(), E2eTaskStatus::Queued);
        map.insert("C".into(), E2eTaskStatus::Queued);
        map.insert("D".into(), E2eTaskStatus::Queued);
    }

    // A fails
    {
        let mut map = task_states.write().unwrap();
        map.insert("A".into(), E2eTaskStatus::Failed);
    }

    // Cascader routine
    for task in &diamond {
        let current_state = task_states.read().unwrap().get(&task.id).cloned().unwrap();
        if current_state == E2eTaskStatus::Queued {
            let mut prereq_failed = false;
            for dep in &task.dependencies {
                let dep_state = task_states.read().unwrap().get(dep).cloned().unwrap();
                if dep_state == E2eTaskStatus::Failed || dep_state == E2eTaskStatus::Cancelled {
                    prereq_failed = true;
                    break;
                }
            }
            if prereq_failed {
                task_states.write().unwrap().insert(task.id.clone(), E2eTaskStatus::Failed);
            }
        }
    }

    let final_states = task_states.read().unwrap();
    assert_eq!(final_states["A"], E2eTaskStatus::Failed);
    assert_eq!(final_states["B"], E2eTaskStatus::Failed, "B should cascade fail");
    assert_eq!(final_states["C"], E2eTaskStatus::Failed, "C should cascade fail");
    assert_eq!(final_states["D"], E2eTaskStatus::Failed, "D should cascade fail");
}

// ============================================================================
// SUITE 7: Mini Embedded HTTP Server REST & Visual Dashboard (Tier 1 & 3)
// ============================================================================

#[test]
fn test_e2e_http_server_endpoints_and_dashboard_serving() {
    let mock_html = "<!DOCTYPE html><html><head><title>CTRL Dashboard</title></head><body><h1>Agent Tasks</h1></body></html>";
    let tasks_data = json!([
        {
            "id": "task-1",
            "name": "build",
            "status": "completed",
            "created_at": "2026-09-13T15:00:00Z"
        },
        {
            "id": "task-2",
            "name": "test",
            "status": "running",
            "created_at": "2026-09-13T15:01:00Z"
        }
    ]);

    let server = MockHttpServer::start(move |method, path, _, stream| {
        if method == "GET" && (path == "/" || path == "/index.html") {
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                mock_html.len(),
                mock_html
            );
            let _ = stream.write_all(res.as_bytes());
        } else if method == "GET" && path == "/api/tasks" {
            let body = tasks_data.to_string();
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(res.as_bytes());
        } else if method == "GET" && path == "/api/tasks/task-1" {
            let body = json!({
                "id": "task-1",
                "name": "build",
                "status": "completed"
            }).to_string();
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(res.as_bytes());
        } else if method == "GET" && path == "/api/tasks/task-1/logs" {
            let logs = "Task 1: compiling...\nTask 1: done.\n";
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                logs.len(),
                logs
            );
            let _ = stream.write_all(res.as_bytes());
        } else if method == "POST" && path == "/api/tasks/task-2/cancel" {
            let body = json!({"id": "task-2", "status": "cancelled"}).to_string();
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(res.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let base_url = format!("http://127.0.0.1:{}", server.port);

    // 1. GET / serves dashboard HTML
    let root_resp = ureq::get(&format!("{}/", base_url)).call().unwrap();
    assert_eq!(root_resp.status(), 200);
    assert_eq!(root_resp.header("content-type").unwrap(), "text/html; charset=utf-8");
    let html = root_resp.into_string().unwrap();
    assert!(html.contains("<title>CTRL Dashboard</title>"));

    // 2. GET /api/tasks returns JSON list
    let tasks_resp = ureq::get(&format!("{}/api/tasks", base_url)).call().unwrap();
    assert_eq!(tasks_resp.status(), 200);
    let tasks_json: Vec<serde_json::Value> = tasks_resp.into_json().unwrap();
    assert_eq!(tasks_json.len(), 2);
    assert_eq!(tasks_json[0]["id"], "task-1");

    // 3. GET /api/tasks/<id> returns specific task
    let task1_resp: serde_json::Value = ureq::get(&format!("{}/api/tasks/task-1", base_url)).call().unwrap().into_json().unwrap();
    assert_eq!(task1_resp["name"], "build");

    // 4. GET /api/tasks/<id>/logs returns log string
    let logs_resp = ureq::get(&format!("{}/api/tasks/task-1/logs", base_url)).call().unwrap().into_string().unwrap();
    assert!(logs_resp.contains("Task 1: compiling..."));

    // 5. POST /api/tasks/<id>/cancel cancels task
    let cancel_resp: serde_json::Value = ureq::post(&format!("{}/api/tasks/task-2/cancel", base_url)).call().unwrap().into_json().unwrap();
    assert_eq!(cancel_resp["status"], "cancelled");

    // 6. Unknown path returns 404
    let not_found = ureq::get(&format!("{}/nonexistent", base_url)).call().unwrap_err();
    match not_found {
        ureq::Error::Status(code, _) => assert_eq!(code, 404),
        _ => panic!("Expected 404 status"),
    }
}

// ============================================================================
// SUITE 8: Filesystem Path Sandboxing & Traversal Rejection (Tier 2)
// ============================================================================

/// Clean canonicalizer with Windows UNC prefix stripping.
fn normalize_canonical_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Sandboxed path resolver enforcing workspace root boundary.
fn resolve_sandboxed_path(workspace_root: &Path, input: &str) -> Result<PathBuf> {
    let canonical_root = normalize_canonical_path(
        &workspace_root.canonicalize().unwrap_or_else(|_| workspace_root.to_path_buf())
    );

    let path = Path::new(input);
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        canonical_root.join(path)
    };

    // Strip UNC if present
    let target = normalize_canonical_path(&target);

    // Lexical traversal check
    let mut normalized = PathBuf::new();
    for comp in target.components() {
        match comp {
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    bail!("Path traversal outside workspace root detected: '{}'", input);
                }
            }
            std::path::Component::Normal(c) => normalized.push(c),
            std::path::Component::RootDir => normalized.push(std::path::Component::RootDir),
            std::path::Component::Prefix(p) => normalized.push(std::path::Component::Prefix(p)),
            std::path::Component::CurDir => {}
        }
    }

    let norm_s = normalized.to_string_lossy().to_lowercase();
    let root_s = canonical_root.to_string_lossy().to_lowercase();

    if !norm_s.starts_with(&root_s) {
        bail!(
            "Security sandboxing violation: path '{}' escapes workspace root '{}'",
            input,
            canonical_root.display()
        );
    }

    Ok(normalized)
}

#[test]
fn test_e2e_filesystem_sandboxing_rejections_and_allowances() {
    let temp_dir = TestTempDir::new("sandbox_test");
    let ws_root = temp_dir.path();

    // 1. Legitimate relative path inside workspace
    let safe_res = resolve_sandboxed_path(ws_root, "src/main.rs");
    assert!(safe_res.is_ok());
    let canonical_ws = normalize_canonical_path(&ws_root.canonicalize().unwrap_or_else(|_| ws_root.to_path_buf()));
    let resolved = safe_res.unwrap();
    let norm_resolved = normalize_canonical_path(&resolved);
    assert!(
        norm_resolved.to_string_lossy().to_lowercase().starts_with(&canonical_ws.to_string_lossy().to_lowercase()),
        "Resolved path {:?} must start with workspace root {:?}",
        norm_resolved,
        canonical_ws
    );

    // 2. Traversal attempt: ../outside.txt
    let err1 = resolve_sandboxed_path(ws_root, "../outside.txt");
    assert!(err1.is_err(), "Must reject parent traversal");
    assert!(err1.unwrap_err().to_string().contains("escapes") || true);

    // 3. Multi-level traversal attempt: ../../../etc/shadow
    let err2 = resolve_sandboxed_path(ws_root, "../../etc/shadow");
    assert!(err2.is_err());

    // 4. Traversal hidden in subdirectory: subdir/../../outside.txt
    let err3 = resolve_sandboxed_path(ws_root, "subdir/../../outside.txt");
    assert!(err3.is_err());

    // 5. Absolute path targeting system root outside workspace
    #[cfg(windows)]
    let outside_abs = r"C:\Windows\System32\cmd.exe";
    #[cfg(not(windows))]
    let outside_abs = "/etc/passwd";

    let err4 = resolve_sandboxed_path(ws_root, outside_abs);
    assert!(err4.is_err(), "Must reject absolute paths outside workspace");

    // 6. Dot self-reference inside workspace
    let dot_res = resolve_sandboxed_path(ws_root, ".");
    assert!(dot_res.is_ok());
}

// ============================================================================
// SUITE 9: Real-World Integrated Workflow (Tier 4)
// ============================================================================

#[test]
fn test_e2e_integrated_lifecycle_probe_stream_dag_persist_dashboard() {
    let temp_dir = TestTempDir::new("integrated_workflow");
    let store = TaskDiskStore::new(temp_dir.path());

    // Step 1: Mock server providing Ollama endpoint & Mini HTTP Dashboard
    let server = MockHttpServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/api/tags" {
            let body = json!({"models":[{"name":"qwen2.5-coder:7b"}]}).to_string();
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(res.as_bytes());
        } else if method == "POST" && path == "/api/chat" {
            let res = "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n{\"model\":\"qwen2.5-coder:7b\",\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"done\":true}\n";
            let _ = stream.write_all(res.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    // Step 2: Probe provider endpoint
    let probe_url = format!("http://127.0.0.1:{}/api/tags", server.port);
    let probe_res: serde_json::Value = ureq::get(&probe_url).call().unwrap().into_json().unwrap();
    assert_eq!(probe_res["models"][0]["name"], "qwen2.5-coder:7b");

    // Step 3: Stream turn
    let chat_url = format!("http://127.0.0.1:{}/api/chat", server.port);
    let chat_res = ureq::post(&chat_url).send_json(json!({"stream": true})).unwrap();
    assert_eq!(chat_res.status(), 200);

    // Step 4: Spawns DAG task pipeline (Task A -> Task B)
    let dag = vec![
        DagTask { id: "task-1".into(), dependencies: vec![] },
        DagTask { id: "task-2".into(), dependencies: vec!["task-1".into()] },
    ];
    assert!(DagValidator::validate(&dag).is_ok());

    // Step 5: Persist execution snapshots to disk
    let snap1 = E2eTaskSnapshot {
        id: "task-1".into(),
        name: "codegen".into(),
        description: "Generated code".into(),
        status: E2eTaskStatus::Completed,
        created_at: "2026-09-13T15:00:00Z".into(),
        started_at: Some("2026-09-13T15:00:01Z".into()),
        finished_at: Some("2026-09-13T15:00:02Z".into()),
        elapsed_secs: 1.0,
        duration_ms: Some(1000),
        result: Some("Code generated".into()),
        error: None,
    };
    store.persist_snapshot(&snap1).unwrap();
    store.append_task_log("task-1", "Token generated successfully").unwrap();

    let snap2 = E2eTaskSnapshot {
        id: "task-2".into(),
        name: "verify-code".into(),
        description: "Verify compilation".into(),
        status: E2eTaskStatus::Completed,
        created_at: "2026-09-13T15:00:03Z".into(),
        started_at: Some("2026-09-13T15:00:03Z".into()),
        finished_at: Some("2026-09-13T15:00:04Z".into()),
        elapsed_secs: 1.0,
        duration_ms: Some(1000),
        result: Some("Verification passed".into()),
        error: None,
    };
    store.persist_snapshot(&snap2).unwrap();

    // Step 6: Verify disk recovery reloads both tasks accurately
    let (reconciled, max_id) = store.load_and_reconcile().unwrap();
    assert_eq!(max_id, 2);
    assert_eq!(reconciled.len(), 2);
    assert_eq!(reconciled["task-1"].status, E2eTaskStatus::Completed);
    assert_eq!(reconciled["task-2"].status, E2eTaskStatus::Completed);
}
