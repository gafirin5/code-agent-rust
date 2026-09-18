use crate::agent::tasks::{CancellationToken, TaskManager};
use anyhow::{Context, Result};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

#[allow(clippy::duplicate_mod)]
#[path = "telemetry/mod.rs"]
pub mod telemetry;

/// Embedded dashboard HTML for the ctrl-cli web UI (compiled into the binary).
const DASHBOARD_HTML: &str = include_str!("dashboard.html");

/// Returns the dashboard HTML content, reading from disk if available to allow live edits to dashboard.html,
/// falling back to the embedded binary copy.
fn resolve_index_html() -> std::borrow::Cow<'static, str> {
    let ws = std::env::var("CTRL_WORKSPACE_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let candidates = [
        ws.join("ctrl-cli").join("src").join("dashboard.html"),
        ws.join("src").join("dashboard.html"),
        std::path::PathBuf::from("ctrl-cli/src/dashboard.html"),
        std::path::PathBuf::from("src/dashboard.html"),
        std::path::PathBuf::from("dashboard.html"),
    ];
    for p in candidates {
        if let Ok(content) = std::fs::read_to_string(&p) {
            if !content.trim().is_empty() {
                return std::borrow::Cow::Owned(content);
            }
        }
    }
    std::borrow::Cow::Borrowed(DASHBOARD_HTML)
}

/// Sends a formatted HTTP/1.1 response with standard security and CORS headers.
fn send_response(
    stream: &mut TcpStream,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, Authorization, X-Requested-With\r\n\
         Connection: close\r\n\
         \r\n",
        status_code,
        reason,
        content_type,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Handles CORS preflight OPTIONS requests.
fn send_options_response(stream: &mut TcpStream) -> Result<()> {
    let header = "HTTP/1.1 204 No Content\r\n\
                  Access-Control-Allow-Origin: *\r\n\
                  Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                  Access-Control-Allow-Headers: Content-Type, Authorization, X-Requested-With\r\n\
                  Content-Length: 0\r\n\
                  Connection: close\r\n\
                  \r\n";
    stream.write_all(header.as_bytes())?;
    stream.flush()?;
    Ok(())
}

/// Flushes write buffer, performs write shutdown, drains residual bytes from receive buffer
/// using a short timed read until EOF, and cleanly shuts down the socket to avoid RST packets and Windows 10053 errors.
fn drain_and_close(mut stream: TcpStream) {
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(5)));
    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Parses and processes a single incoming HTTP request on `stream`.
fn handle_connection(mut stream: TcpStream, shutdown_token: CancellationToken) -> Result<()> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
    let parse_res = {
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        match reader.read_line(&mut request_line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                if !request_line.ends_with('\n') {
                    None
                } else {
                    let parts: Vec<&str> = request_line.split_whitespace().collect();
                    if parts.len() < 2 {
                        Some((String::new(), String::new(), Vec::new(), true))
                    } else {
                        let method = parts[0].to_string();
                        let full_path = parts[1].to_string();

                        let mut content_length: usize = 0;
                        loop {
                            let mut header_line = String::new();
                            match reader.read_line(&mut header_line) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {}
                            }
                            let trimmed = header_line.trim();
                            if trimmed.is_empty() {
                                break;
                            }
                            if let Some((name, val)) = trimmed.split_once(':') {
                                if name.trim().eq_ignore_ascii_case("content-length") {
                                    if let Ok(len) = val.trim().parse::<usize>() {
                                        content_length = len;
                                    }
                                }
                            }
                        }

                        let mut body = vec![0u8; content_length];
                        if content_length > 0 {
                            let _ = reader.read_exact(&mut body);
                        }

                        Some((method, full_path, body, false))
                    }
                }
            }
        }
    };

    let (method, full_path, body, is_bad_request) = match parse_res {
        None => {
            drain_and_close(stream);
            return Ok(());
        }
        Some(t) => t,
    };

    if is_bad_request {
        let _ = send_response(
            &mut stream,
            400,
            "Bad Request",
            "application/json",
            br#"{"error":"Bad Request"}"#,
        );
        drain_and_close(stream);
        return Ok(());
    }

    let path = full_path.split('?').next().unwrap_or(&full_path);

    // 1. CORS OPTIONS preflight
    if method == "OPTIONS" {
        let _ = send_options_response(&mut stream);
        drain_and_close(stream);
        return Ok(());
    }

    // 2. Server-Sent Events stream endpoint
    if method == "GET" && path == "/api/events" {
        let sse_headers = "HTTP/1.1 200 OK\r\n\
                           Content-Type: text/event-stream\r\n\
                           Cache-Control: no-cache\r\n\
                           Connection: keep-alive\r\n\
                           Access-Control-Allow-Origin: *\r\n\
                           \r\n";
        if stream.write_all(sse_headers.as_bytes()).is_err() || stream.flush().is_err() {
            drain_and_close(stream);
            return Ok(());
        }

        let (sub_id, rx) = TaskManager::global().broadcaster().subscribe_with_id();

        // Initial ping keep-alive
        if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
            TaskManager::global().broadcaster().unsubscribe(sub_id);
            drain_and_close(stream);
            return Ok(());
        }

        let mut last_ping = Instant::now();
        while !shutdown_token.is_cancelled() {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(msg) => {
                    if stream.write_all(msg.as_bytes()).is_err() || stream.flush().is_err() {
                        break; // Client disconnected
                    }
                    last_ping = Instant::now();
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if last_ping.elapsed() >= Duration::from_secs(15) {
                        if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
                            break; // Client disconnected
                        }
                        last_ping = Instant::now();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        TaskManager::global().broadcaster().unsubscribe(sub_id);
        drain_and_close(stream);
        return Ok(());
    }

    // 3. Task Run API endpoint
    if method == "POST" && path == "/api/tasks/run" {
        let payload: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({"error": format!("Invalid JSON: {}", e)});
                let json_bytes = serde_json::to_vec(&err)?;
                let _ = send_response(&mut stream, 400, "Bad Request", "application/json", &json_bytes);
                drain_and_close(stream);
                return Ok(());
            }
        };

        let prompt = match payload.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
            _ => {
                let err = json!({"error": "Missing or empty required field: 'prompt'"});
                let json_bytes = serde_json::to_vec(&err)?;
                let _ = send_response(&mut stream, 400, "Bad Request", "application/json", &json_bytes);
                drain_and_close(stream);
                return Ok(());
            }
        };

        let skill = payload.get("skill").and_then(|v| v.as_str()).map(|s| s.to_string());
        let task_name = skill.unwrap_or_else(|| "task-web".to_string());
        let prompt_desc = prompt.clone();

        let tm = TaskManager::global();
        let spawn_res = tm.spawn_task_with_sink(
            task_name,
            prompt_desc.clone(),
            move |token, logs| {
                logs.push(format!("Starting task execution: {}", prompt_desc));
                if token.is_cancelled() {
                    return Err(anyhow::anyhow!("Task cancelled before execution"));
                }
                logs.push(format!("Task execution completed: {}", prompt_desc));
                Ok(format!("Finished: {}", prompt_desc))
            },
        );

        match spawn_res {
            Ok((task_id, _, _)) => {
                let resp = json!({
                    "id": task_id,
                    "status": "queued"
                });
                let json_bytes = serde_json::to_vec(&resp)?;
                let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
            }
            Err(e) => {
                let err = json!({"error": format!("Failed to spawn task: {}", e)});
                let json_bytes = serde_json::to_vec(&err)?;
                let _ = send_response(&mut stream, 500, "Internal Server Error", "application/json", &json_bytes);
            }
        }
        drain_and_close(stream);
        return Ok(());
    }

    // 4. Static Asset & REST routes
    match (method.as_str(), path) {
        ("GET", "/") | ("GET", "/index.html") => {
            let html = resolve_index_html();
            let _ = send_response(
                &mut stream,
                200,
                "OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
            );
        }
        ("GET", "/api/health") => {
            let _ = send_response(&mut stream, 200, "OK", "text/plain", b"OK");
        }
        ("GET", "/api/tasks") => {
            let tasks = TaskManager::global().list_tasks();
            let json_bytes = serde_json::to_vec(&tasks)?;
            let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
        }
        ("GET", "/api/metrics") => {
            let metrics = telemetry::capture_metrics(None);
            let json_bytes = serde_json::to_vec(&metrics)?;
            let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
        }
        ("GET", p) if p.starts_with("/api/tasks/") => {
            let subpath = &p["/api/tasks/".len()..];
            if let Some(task_id) = subpath.strip_suffix("/logs") {
                if let Some(logs) = TaskManager::global().get_task_logs(task_id) {
                    let json_bytes = serde_json::to_vec(&logs)?;
                    let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
                } else if TaskManager::global().get_task(task_id).is_some() {
                    let empty: Vec<String> = Vec::new();
                    let json_bytes = serde_json::to_vec(&empty)?;
                    let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
                } else {
                    let err = json!({"error": format!("Task '{}' not found", task_id)});
                    let json_bytes = serde_json::to_vec(&err)?;
                    let _ = send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes);
                }
            } else {
                let task_id = subpath;
                if let Some(snap) = TaskManager::global().get_task(task_id) {
                    let json_bytes = serde_json::to_vec(&snap)?;
                    let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
                } else {
                    let err = json!({"error": format!("Task '{}' not found", task_id)});
                    let json_bytes = serde_json::to_vec(&err)?;
                    let _ = send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes);
                }
            }
        }
        ("POST", p) if p.starts_with("/api/tasks/") && p.ends_with("/cancel") => {
            let subpath = &p["/api/tasks/".len()..];
            let task_id = subpath.strip_suffix("/cancel").unwrap_or(subpath);
            if let Some(_snap) = TaskManager::global().get_task(task_id) {
                let _ = TaskManager::global().cancel_task(task_id);
                let resp = json!({
                    "id": task_id,
                    "status": "cancelled",
                    "message": "Task cancelled successfully"
                });
                let json_bytes = serde_json::to_vec(&resp)?;
                let _ = send_response(&mut stream, 200, "OK", "application/json", &json_bytes);
            } else {
                let err = json!({"error": format!("Task '{}' not found", task_id)});
                let json_bytes = serde_json::to_vec(&err)?;
                let _ = send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes);
            }
        }
        _ => {
            let err = json!({"error": "Not Found"});
            let json_bytes = serde_json::to_vec(&err)?;
            let _ = send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes);
        }
    }

    drain_and_close(stream);
    Ok(())
}

/// Spawns the embedded HTTP server in a background thread, supporting port 0 for ephemeral OS assignment.
///
/// Returns the actual bound port and the worker `JoinHandle`.
pub fn spawn_server(
    host: &str,
    port: u16,
    shutdown_token: CancellationToken,
) -> Result<(u16, thread::JoinHandle<Result<()>>)> {
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr)
        .with_context(|| format!("Failed to bind HTTP server to {}", addr))?;
    let bound_port = listener.local_addr()?.port();
    listener
        .set_nonblocking(true)
        .with_context(|| "Failed to set non-blocking on TcpListener")?;

    let token_clone = shutdown_token.clone();

    let handle = thread::Builder::new()
        .name(format!("http-server-{}", bound_port))
        .spawn(move || {
            while !token_clone.is_cancelled() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let conn_token = token_clone.clone();
                        let _ = thread::Builder::new()
                            .name("http-conn".to_string())
                            .stack_size(128 * 1024)
                            .spawn(move || {
                                let _ = handle_connection(stream, conn_token);
                            });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::ConnectionAborted
                            || e.kind() == std::io::ErrorKind::ConnectionReset
                            || e.kind() == std::io::ErrorKind::Interrupted
                            || e.raw_os_error() == Some(10053)
                            || e.raw_os_error() == Some(10054) =>
                    {
                        // Windows WSAECONNABORTED (10053), WSAECONNRESET (10054), or Unix EINTR:
                        // Client aborted connection in backlog queue before accept completed.
                        // Continue immediately without sleeping to eliminate starvation.
                        continue;
                    }
                    Err(e) => {
                        if token_clone.is_cancelled() {
                            break;
                        }
                        eprintln!("Warning: Server accept error: {}", e);
                        continue;
                    }
                }
            }
            Ok(())
        })
        .with_context(|| "Failed to spawn HTTP server thread")?;

    Ok((bound_port, handle))
}

/// Runs the embedded HTTP server synchronously on the calling thread until `shutdown_token` is cancelled.
pub fn run_server(host: &str, port: u16, shutdown_token: CancellationToken) -> Result<()> {
    let (_bound_port, handle) = spawn_server(host, port, shutdown_token)?;
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("HTTP server thread panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_index_html_fallback() {
        let html = resolve_index_html();
        assert!(!html.is_empty());
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("ctrl-cli") || html.contains("CTRL") || html.contains("dashboard"));
    }

    #[test]
    fn test_spawn_server_ephemeral_port_and_rest_endpoints() {
        let tm = TaskManager::global();
        let (task_id, _token) = tm
            .spawn_task(
                "test-server-worker".into(),
                "Validating embedded server endpoints".into(),
                |_| Ok("Completed successfully".into()),
            )
            .expect("Task spawn must succeed");

        let _ = tm.append_task_log(&task_id, "Server log line 1: compiling");
        let _ = tm.append_task_log(&task_id, "Server log line 2: verified");

        let shutdown_token = CancellationToken::new();
        let (port, handle) = spawn_server("127.0.0.1", 0, shutdown_token.clone())
            .expect("spawn_server on ephemeral port must succeed");

        assert!(port > 0, "Bound port must be non-zero");
        let base_url = format!("http://127.0.0.1:{}", port);

        // 1. GET /
        let root_resp = ureq::get(&format!("{}/", base_url))
            .call()
            .expect("GET / must succeed");
        assert_eq!(root_resp.status(), 200);
        assert_eq!(
            root_resp.header("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
        let root_html = root_resp.into_string().unwrap();
        assert!(root_html.contains("<!DOCTYPE html>"));

        // 2. GET /index.html
        let index_resp = ureq::get(&format!("{}/index.html", base_url))
            .call()
            .expect("GET /index.html must succeed");
        assert_eq!(index_resp.status(), 200);

        // 3. GET /api/tasks
        let tasks_resp = ureq::get(&format!("{}/api/tasks", base_url))
            .call()
            .expect("GET /api/tasks must succeed");
        assert_eq!(tasks_resp.status(), 200);
        let tasks: Vec<serde_json::Value> = tasks_resp.into_json().expect("Valid tasks JSON");
        assert!(tasks.iter().any(|t| t["id"] == task_id));

        // 4. GET /api/tasks/<id>
        let single_resp = ureq::get(&format!("{}/api/tasks/{}", base_url, task_id))
            .call()
            .expect("GET single task must succeed");
        assert_eq!(single_resp.status(), 200);
        let snap: serde_json::Value = single_resp.into_json().expect("Valid snapshot JSON");
        assert_eq!(snap["id"], task_id);

        // 5. GET /api/tasks/<id>/logs
        let logs_resp = ureq::get(&format!("{}/api/tasks/{}/logs", base_url, task_id))
            .call()
            .expect("GET task logs must succeed");
        assert_eq!(logs_resp.status(), 200);
        let logs_str = logs_resp.into_string().expect("Logs text");
        assert!(logs_str.contains("Server log line 1: compiling"));

        // 6. POST /api/tasks/<id>/cancel
        let cancel_resp = ureq::post(&format!("{}/api/tasks/{}/cancel", base_url, task_id))
            .call()
            .expect("POST cancel must succeed");
        assert_eq!(cancel_resp.status(), 200);
        let cancel_json: serde_json::Value = cancel_resp.into_json().unwrap();
        assert_eq!(cancel_json["status"], "cancelled");

        // 7. OPTIONS /api/tasks preflight
        let options_stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        let mut opt_reader = BufReader::new(options_stream.try_clone().unwrap());
        let mut opt_writer = options_stream;
        opt_writer
            .write_all(b"OPTIONS /api/tasks HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut opt_status = String::new();
        opt_reader.read_line(&mut opt_status).unwrap();
        assert!(opt_status.contains("204 No Content"));

        // 7b. GET /api/metrics
        let metrics_resp = ureq::get(&format!("{}/api/metrics", base_url))
            .call()
            .expect("GET /api/metrics must succeed");
        assert_eq!(metrics_resp.status(), 200);
        assert_eq!(
            metrics_resp.header("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(
            metrics_resp.header("access-control-allow-origin").unwrap(),
            "*"
        );
        let metrics_json: serde_json::Value = metrics_resp.into_json().expect("Valid metrics JSON");
        assert!(metrics_json.get("memory").is_some());
        assert!(metrics_json.get("cpu").is_some());
        assert!(metrics_json.get("storage").is_some());
        assert!(metrics_json.get("threads").is_some());
        assert!(metrics_json.get("timestamp").is_some());

        // 8. Unknown path 404
        let err_404 = ureq::get(&format!("{}/unknown/route", base_url)).call();
        match err_404 {
            Err(ureq::Error::Status(404, _)) => {}
            other => panic!("Expected 404 status error, got {:?}", other),
        }

        // 9. Graceful shutdown
        shutdown_token.cancel();
        let join_res = handle.join();
        assert!(join_res.is_ok(), "Server handle must join cleanly on token cancellation");
    }

    #[test]
    fn test_server_sse_events_and_task_run() {
        let shutdown_token = CancellationToken::new();
        let (port, handle) = spawn_server("127.0.0.1", 0, shutdown_token.clone())
            .expect("spawn_server must succeed");
        let base_url = format!("http://127.0.0.1:{}", port);

        // 1. OPTIONS preflight for /api/tasks/run and /api/events
        for path in ["/api/tasks/run", "/api/events"] {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
            let req = format!("OPTIONS {} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", path);
            stream.write_all(req.as_bytes()).unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("204 No Content"), "Expected 204 for OPTIONS {}, got {}", path, line);
        }

        // 2. GET /api/health
        let health_resp = ureq::get(&format!("{}/api/health", base_url))
            .call()
            .expect("GET /api/health must succeed");
        assert_eq!(health_resp.status(), 200);
        assert_eq!(health_resp.into_string().unwrap(), "OK");

        // 3. POST /api/tasks/run validation errors
        // 3a. Invalid JSON
        let err_resp = ureq::post(&format!("{}/api/tasks/run", base_url))
            .set("Content-Type", "application/json")
            .send_string("not a json");
        assert!(err_resp.is_err(), "Invalid JSON must fail with 400");

        // 3b. Missing prompt
        let empty_prompt = ureq::post(&format!("{}/api/tasks/run", base_url))
            .send_json(json!({"skill": "researcher"}));
        assert!(empty_prompt.is_err(), "Missing prompt must fail with 400");

        // 3c. Empty prompt string
        let whitespace_prompt = ureq::post(&format!("{}/api/tasks/run", base_url))
            .send_json(json!({"prompt": "   "}));
        assert!(whitespace_prompt.is_err(), "Whitespace prompt must fail with 400");

        // 4. Connect SSE stream GET /api/events
        let events_url = format!("{}/api/events", base_url);
        let sse_resp = ureq::get(&events_url)
            .set("Accept", "text/event-stream")
            .call()
            .expect("GET /api/events must succeed");
        assert_eq!(sse_resp.status(), 200);
        assert_eq!(sse_resp.header("content-type").unwrap(), "text/event-stream");

        let mut sse_reader = BufReader::new(sse_resp.into_reader());
        // Read initial ping
        let mut first_line = String::new();
        sse_reader.read_line(&mut first_line).unwrap();
        assert!(first_line.contains(": ping"), "First frame should be keepalive ping, got {}", first_line);

        // 5. POST /api/tasks/run valid task
        let run_resp: serde_json::Value = ureq::post(&format!("{}/api/tasks/run", base_url))
            .send_json(json!({
                "prompt": "Inspect memory leak",
                "skill": "researcher",
                "model": "qwen2.5-coder:7b"
            }))
            .expect("POST /api/tasks/run must succeed")
            .into_json()
            .expect("Valid JSON");

        let task_id = run_resp["id"].as_str().expect("Task ID must be string");
        assert_eq!(run_resp["status"], "queued");

        // 6. Read streamed events from SSE reader
        let mut received_status = false;
        let mut received_log = false;
        for _ in 0..30 {
            let mut line = String::new();
            if sse_reader.read_line(&mut line).is_ok() && !line.trim().is_empty() {
                if line.contains("task_status") || line.contains(task_id) {
                    received_status = true;
                }
                if line.contains("task_log") || line.contains("Starting task execution") {
                    received_log = true;
                }
            }
            if received_status && received_log {
                break;
            }
        }
        assert!(received_status, "Must receive task status event over SSE");

        // 7. Rapid socket abort burst (resilience against WSAECONNABORTED 10053)
        let addr = format!("127.0.0.1:{}", port);
        for _ in 0..50 {
            if let Ok(mut s) = TcpStream::connect(&addr) {
                let _ = s.write_all(b"G");
                drop(s);
            }
        }

        // Fast health response after burst (< 500ms)
        let start = Instant::now();
        let health_after = ureq::get(&format!("{}/api/health", base_url))
            .timeout(Duration::from_secs(2))
            .call()
            .expect("Server must answer after socket burst");
        assert_eq!(health_after.status(), 200);
        assert!(start.elapsed() < Duration::from_millis(500), "Server took too long after socket burst");

        // 8. Graceful shutdown
        shutdown_token.cancel();
        let join_res = handle.join();
        assert!(join_res.is_ok(), "Server handle must join cleanly");
    }
}

