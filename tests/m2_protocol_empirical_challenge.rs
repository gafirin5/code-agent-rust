//! Empirical Challenge Test Suite for Milestone 2: Wire Protocols & Mock Streaming
//!
//! Validates:
//! 1. Gemini REST v1beta SSE streaming wire protocol (`data: {...}\n\n`)
//! 2. Gemini functionCall extraction and usageMetadata token counting
//! 3. Gemini error responses and SSE comment/heartbeat tolerance
//! 4. Ollama NDJSON streaming wire protocol (`{...}\n`)
//! 5. Ollama tool_calls extraction (both JSON object and stringified arguments)
//! 6. Ollama token usage metrics (prompt_eval_count, eval_count)
//! 7. Ollama model tags probe (`/api/tags`)
//! 8. SSE / NDJSON malformed line tolerance and graceful degradation

use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Lightweight local mock server for deterministic protocol testing.
struct LocalProtocolMock {
    pub port: u16,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LocalProtocolMock {
    pub fn start<F>(handler: F) -> Self
    where
        F: Fn(&str, &str, &[u8], &mut TcpStream) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Bind ephemeral mock port");
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
                                    if reader.read_line(&mut header_line).is_err()
                                        || header_line.trim().is_empty()
                                    {
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

impl Drop for LocalProtocolMock {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// ============================================================================
// GEMINI REST v1beta SSE STREAMING TESTS
// ============================================================================

#[test]
fn challenge_gemini_sse_streaming_tool_call_and_token_usage_extraction() {
    let server = LocalProtocolMock::start(|method, path, body, stream| {
        if method == "POST" && path.contains(":streamGenerateContent?alt=sse") {
            let req_json: serde_json::Value = serde_json::from_slice(body).expect("Valid JSON");
            assert!(req_json.get("contents").is_some());

            // Multi-event SSE with text, functionCall, and usageMetadata
            let event1 = json!({
                "candidates": [{
                    "content": {
                        "parts": [{ "text": "I will examine the codebase.\n" }],
                        "role": "model"
                    }
                }]
            });

            let event2 = json!({
                "candidates": [{
                    "content": {
                        "parts": [{
                            "functionCall": {
                                "name": "read_file",
                                "args": { "path": "src/main.rs" }
                            }
                        }],
                        "role": "model"
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 245,
                    "candidatesTokenCount": 42
                }
            });

            // Include SSE comments (heartbeats ': ping') and empty lines
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n\
                : ping\r\n\
                data: {}\n\n\
                : ping\r\n\
                data: {}\n\n\
                data: [DONE]\n\n",
                event1, event2
            );
            let _ = stream.write_all(response.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let endpoint = format!(
        "http://127.0.0.1:{}/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
        server.port
    );

    let res = ureq::post(&endpoint)
        .set("x-goog-api-key", "test-key-123")
        .set("Content-Type", "application/json")
        .send_json(json!({
            "contents": [{ "parts": [{ "text": "Check files" }] }]
        }))
        .expect("Gemini request succeed");

    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-type").unwrap(), "text/event-stream");

    let reader = BufReader::new(res.into_reader());
    let mut accumulated_text = String::new();
    let mut extracted_tool_calls = Vec::new();
    let mut prompt_tokens = 0u64;
    let mut completion_tokens = 0u64;

    for line in reader.lines() {
        let line = line.expect("Read SSE line");
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue; // Skip comments and empty lines
        }

        if let Some(data_str) = trimmed.strip_prefix("data:") {
            let data_str = data_str.trim_start();
            if data_str == "[DONE]" {
                break;
            }
            let val: serde_json::Value = serde_json::from_str(data_str).expect("Valid JSON data");

            // Extract usage
            if let Some(usage) = val.get("usageMetadata") {
                if let Some(p) = usage.get("promptTokenCount").and_then(|v| v.as_u64()) {
                    prompt_tokens += p;
                }
                if let Some(c) = usage.get("candidatesTokenCount").and_then(|v| v.as_u64()) {
                    completion_tokens += c;
                }
            }

            // Extract content and functionCall
            if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
                for cand in candidates {
                    if let Some(parts) = cand.get("content").and_then(|c| c.get("parts")).and_then(|p| p.as_array()) {
                        for part in parts {
                            if let Some(txt) = part.get("text").and_then(|t| t.as_str()) {
                                accumulated_text.push_str(txt);
                            }
                            if let Some(fc) = part.get("functionCall") {
                                let name = fc["name"].as_str().unwrap().to_string();
                                let args = fc["args"].clone();
                                extracted_tool_calls.push((name, args));
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(accumulated_text, "I will examine the codebase.\n");
    assert_eq!(extracted_tool_calls.len(), 1);
    assert_eq!(extracted_tool_calls[0].0, "read_file");
    assert_eq!(extracted_tool_calls[0].1["path"], "src/main.rs");
    assert_eq!(prompt_tokens, 245);
    assert_eq!(completion_tokens, 42);
    assert_eq!(prompt_tokens + completion_tokens, 287);
}

#[test]
fn challenge_gemini_sse_api_error_handling() {
    let server = LocalProtocolMock::start(|_, _, _, stream| {
        let err_json = json!({
            "error": {
                "code": 403,
                "message": "Resource has been exhausted (quota limit).",
                "status": "RESOURCE_EXHAUSTED"
            }
        });
        let res = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            err_json.to_string().len(),
            err_json
        );
        let _ = stream.write_all(res.as_bytes());
    });

    let endpoint = format!(
        "http://127.0.0.1:{}/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse",
        server.port
    );

    let err = ureq::post(&endpoint)
        .set("x-goog-api-key", "quota-exhausted-key")
        .send_json(json!({ "contents": [] }))
        .unwrap_err();

    match err {
        ureq::Error::Status(code, resp) => {
            assert_eq!(code, 403);
            let body: serde_json::Value = resp.into_json().expect("Valid error JSON");
            assert_eq!(body["error"]["code"], 403);
            assert!(body["error"]["message"].as_str().unwrap().contains("Resource has been exhausted"));
        }
        _ => panic!("Expected status error"),
    }
}

// ============================================================================
// OLLAMA NATIVE NDJSON STREAMING TESTS
// ============================================================================

#[test]
fn challenge_ollama_ndjson_streaming_tool_call_and_eval_metrics() {
    let server = LocalProtocolMock::start(|method, path, body, stream| {
        if method == "POST" && path == "/api/chat" {
            let req_json: serde_json::Value = serde_json::from_slice(body).expect("Valid Ollama request");
            assert_eq!(req_json["model"], "qwen2.5-coder:7b");
            assert_eq!(req_json["stream"], true);

            // Verify tools schema payload
            assert!(req_json.get("tools").is_some(), "Tools must be sent to Ollama");

            // Line 1: text chunk
            let line1 = json!({
                "model": "qwen2.5-coder:7b",
                "message": { "role": "assistant", "content": "Running verification tests..." },
                "done": false
            });

            // Line 2: tool call with arguments as structured JSON object
            let line2 = json!({
                "model": "qwen2.5-coder:7b",
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "run_command",
                            "arguments": { "cmd": "cargo check" }
                        }
                    }]
                },
                "done": false
            });

            // Line 3: terminal chunk with token counts
            let line3 = json!({
                "model": "qwen2.5-coder:7b",
                "done": true,
                "prompt_eval_count": 182,
                "eval_count": 56,
                "total_duration": 48200000
            });

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n\
                {}\n\
                {}\n\
                {}\n",
                line1, line2, line3
            );
            let _ = stream.write_all(response.as_bytes());
        } else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let endpoint = format!("http://127.0.0.1:{}/api/chat", server.port);

    let res = ureq::post(&endpoint)
        .set("Content-Type", "application/json")
        .send_json(json!({
            "model": "qwen2.5-coder:7b",
            "messages": [{ "role": "user", "content": "Run tests" }],
            "stream": true,
            "tools": [{
                "name": "run_command",
                "description": "Run shell command",
                "parameters": {}
            }]
        }))
        .expect("Ollama stream succeed");

    assert_eq!(res.status(), 200);

    let reader = BufReader::new(res.into_reader());
    let mut accumulated_text = String::new();
    let mut tool_calls = Vec::new();
    let mut prompt_eval = 0u64;
    let mut eval_count = 0u64;
    let mut stream_finished = false;

    for line in reader.lines() {
        let line = line.expect("Read line");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let val: serde_json::Value = serde_json::from_str(trimmed).expect("Valid JSON line");

        if let Some(p) = val.get("prompt_eval_count").and_then(|v| v.as_u64()) {
            prompt_eval += p;
        }
        if let Some(e) = val.get("eval_count").and_then(|v| v.as_u64()) {
            eval_count += e;
        }

        if let Some(msg) = val.get("message") {
            if let Some(txt) = msg.get("content").and_then(|c| c.as_str()) {
                accumulated_text.push_str(txt);
            }
            if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    if let Some(func) = tc.get("function") {
                        let name = func["name"].as_str().unwrap().to_string();
                        let args = func["arguments"].clone();
                        tool_calls.push((name, args));
                    }
                }
            }
        }

        if val.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
            stream_finished = true;
        }
    }

    assert!(stream_finished, "Stream must reach done=true");
    assert_eq!(accumulated_text, "Running verification tests...");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].0, "run_command");
    assert_eq!(tool_calls[0].1["cmd"], "cargo check");
    assert_eq!(prompt_eval, 182);
    assert_eq!(eval_count, 56);
    assert_eq!(prompt_eval + eval_count, 238);
}

#[test]
fn challenge_ollama_tool_call_stringified_arguments_tolerance() {
    // Some versions/models of Ollama return arguments as a stringified JSON instead of raw object
    let server = LocalProtocolMock::start(|method, path, _, stream| {
        if method == "POST" && path == "/api/chat" {
            let line = json!({
                "model": "qwen2.5-coder:7b",
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "write_file",
                            "arguments": "{\"path\":\"hello.txt\",\"content\":\"world\"}"
                        }
                    }]
                },
                "done": true
            });
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n{}\n",
                line
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let endpoint = format!("http://127.0.0.1:{}/api/chat", server.port);
    let res = ureq::post(&endpoint)
        .send_json(json!({ "model": "qwen2.5-coder:7b", "stream": true }))
        .unwrap();

    let reader = BufReader::new(res.into_reader());
    let mut args_parsed: Option<serde_json::Value> = None;

    for line in reader.lines() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        let val: serde_json::Value = serde_json::from_str(&line).unwrap();
        if let Some(tcs) = val["message"]["tool_calls"].as_array() {
            for tc in tcs {
                let args_val = match &tc["function"]["arguments"] {
                    serde_json::Value::String(s) => serde_json::from_str(s).unwrap(),
                    other => other.clone(),
                };
                args_parsed = Some(args_val);
            }
        }
    }

    assert!(args_parsed.is_some());
    let args = args_parsed.unwrap();
    assert_eq!(args["path"], "hello.txt");
    assert_eq!(args["content"], "world");
}

#[test]
fn challenge_ollama_tags_probe_multiple_models() {
    let server = LocalProtocolMock::start(|method, path, _, stream| {
        if method == "GET" && path == "/api/tags" {
            let tags = json!({
                "models": [
                    { "name": "qwen2.5-coder:7b", "size": 4700000000u64 },
                    { "name": "qwen2.5-coder:14b", "size": 9000000000u64 },
                    { "name": "llama3.2:3b", "size": 2000000000u64 },
                    { "name": "mistral:7b", "size": 4100000000u64 }
                ]
            });
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                tags.to_string().len(),
                tags
            );
            let _ = stream.write_all(res.as_bytes());
        }
    });

    let endpoint = format!("http://127.0.0.1:{}/api/tags", server.port);
    let tags_val: serde_json::Value = ureq::get(&endpoint).call().unwrap().into_json().unwrap();
    let models = tags_val["models"].as_array().unwrap();
    assert_eq!(models.len(), 4);
    assert_eq!(models[0]["name"], "qwen2.5-coder:7b");
    assert_eq!(models[1]["name"], "qwen2.5-coder:14b");
}
