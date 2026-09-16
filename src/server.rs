use crate::agent::tasks::{CancellationToken, TaskManager};
use anyhow::{Context, Result};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[path = "telemetry/mod.rs"]
pub mod telemetry;

/// Compile-time fallback for the dashboard HTML if file cannot be read from disk.
const EMBEDDED_INDEX_HTML: &str = include_str!("../../index.html");

/// Resolves the repository root `index.html` from filesystem candidates, falling back to embedded HTML.
fn resolve_index_html() -> String {
    let candidates = [
        PathBuf::from("index.html"),
        PathBuf::from("../index.html"),
        PathBuf::from("../../index.html"),
        PathBuf::from(r"C:\Users\Administrator\code-agent-rust\index.html"),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(candidate) {
                return content;
            }
        }
    }

    EMBEDDED_INDEX_HTML.to_string()
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
    let _ = stream.shutdown(std::net::Shutdown::Write);
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
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(())
}

/// Parses and processes a single incoming HTTP request on `stream`.
fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_response(
            &mut stream,
            400,
            "Bad Request",
            "application/json",
            br#"{"error":"Bad Request"}"#,
        )?;
        return Ok(());
    }

    let method = parts[0];
    let full_path = parts[1];
    let path = full_path.split('?').next().unwrap_or(full_path);

    // Read headers to determine Content-Length if any
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line)? == 0 {
            break;
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

    // Read request body if Content-Length > 0
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    drop(reader);

    // 1. CORS OPTIONS preflight
    if method == "OPTIONS" {
        send_options_response(&mut stream)?;
        return Ok(());
    }

    // 2. Static Asset & REST routes
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            let html = resolve_index_html();
            send_response(
                &mut stream,
                200,
                "OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
            )?;
        }
        ("GET", "/api/tasks") => {
            let tasks = TaskManager::global().list_tasks();
            let json_bytes = serde_json::to_vec(&tasks)?;
            send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
        }
        ("GET", "/api/metrics") => {
            let metrics = telemetry::capture_metrics(None);
            let json_bytes = serde_json::to_vec(&metrics)?;
            send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
        }
        ("GET", p) if p.starts_with("/api/tasks/") => {
            let subpath = &p["/api/tasks/".len()..];
            if let Some(task_id) = subpath.strip_suffix("/logs") {
                if let Some(logs) = TaskManager::global().get_task_logs(task_id) {
                    let json_bytes = serde_json::to_vec(&logs)?;
                    send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
                } else if TaskManager::global().get_task(task_id).is_some() {
                    let empty: Vec<String> = Vec::new();
                    let json_bytes = serde_json::to_vec(&empty)?;
                    send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
                } else {
                    let err = json!({"error": format!("Task '{}' not found", task_id)});
                    let json_bytes = serde_json::to_vec(&err)?;
                    send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes)?;
                }
            } else {
                let task_id = subpath;
                if let Some(snap) = TaskManager::global().get_task(task_id) {
                    let json_bytes = serde_json::to_vec(&snap)?;
                    send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
                } else {
                    let err = json!({"error": format!("Task '{}' not found", task_id)});
                    let json_bytes = serde_json::to_vec(&err)?;
                    send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes)?;
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
                send_response(&mut stream, 200, "OK", "application/json", &json_bytes)?;
            } else {
                let err = json!({"error": format!("Task '{}' not found", task_id)});
                let json_bytes = serde_json::to_vec(&err)?;
                send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes)?;
            }
        }
        _ => {
            let err = json!({"error": "Not Found"});
            let json_bytes = serde_json::to_vec(&err)?;
            send_response(&mut stream, 404, "Not Found", "application/json", &json_bytes)?;
        }
    }

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
                        thread::spawn(move || {
                            let _ = handle_connection(stream);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => {
                        if token_clone.is_cancelled() {
                            break;
                        }
                        eprintln!("Warning: Server accept error: {}", e);
                        thread::sleep(Duration::from_millis(20));
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
        assert!(html.contains("MyWeb") || html.contains("CTRL") || html.contains("<html"));
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
}
