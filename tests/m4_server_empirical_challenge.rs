//! Milestone 4 Empirical Challenge Test Suite: Embedded HTTP Server & REST API
//!
//! Objectives:
//! 1. Verify `GET /` and `GET /index.html` serve HTML with `Content-Type: text/html; charset=utf-8` and CORS.
//! 2. Verify REST endpoints: `GET /api/tasks`, `GET /api/tasks/<id>`, `GET /api/tasks/<id>/logs`, `POST /api/tasks/<id>/cancel`.
//! 3. Verify 404 handling for nonexistent tasks and unknown routes.
//! 4. Verify CORS preflight `OPTIONS *` and endpoint-specific `OPTIONS` returning 204 No Content with required headers.
//! 5. Verify non-blocking shutdown speed (<500ms) and immediate port release for rebind.
//! 6. Verify malformed requests (empty, single token, unknown method, truncated body, path traversal in URI).
//! 7. Verify high-concurrency stress (250 simultaneous requests across 25 worker threads).
//! 8. Verify subprocess CLI execution of `ctrl-cli serve`.

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
}

#[path = "../src/server.rs"]
pub mod server;

use agent::tasks::{CancellationToken, TaskManager};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============================================================================
// CHALLENGE 1: GET / and GET /index.html Static Dashboard Delivery & Headers
// ============================================================================

#[test]
fn test_challenge_get_root_and_index_content_type_and_cors() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    assert!(port > 0, "Bound port must be non-zero");
    let base_url = format!("http://127.0.0.1:{}", port);

    // 1.1 GET /
    let resp_root = ureq::get(&format!("{}/", base_url)).call().expect("GET / failed");
    assert_eq!(resp_root.status(), 200);
    assert_eq!(
        resp_root.header("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        resp_root.header("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(resp_root.header("connection").unwrap(), "close");
    let html_root = resp_root.into_string().unwrap();
    assert!(
        html_root.contains("<!DOCTYPE html>") || html_root.contains("<html"),
        "GET / must return valid HTML content"
    );

    // 1.2 GET /index.html
    let resp_index = ureq::get(&format!("{}/index.html", base_url))
        .call()
        .expect("GET /index.html failed");
    assert_eq!(resp_index.status(), 200);
    assert_eq!(
        resp_index.header("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let html_index = resp_index.into_string().unwrap();
    assert_eq!(html_root, html_index, "GET / and GET /index.html must return identical content");

    // 1.3 GET with query params: /?cache_bust=12345
    let resp_query = ureq::get(&format!("{}/?cache_bust=12345", base_url))
        .call()
        .expect("GET /?cache_bust=12345 failed");
    assert_eq!(resp_query.status(), 200);
    assert_eq!(
        resp_query.header("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 2: REST Task Lifecycle Endpoints (/api/tasks, logs, cancel)
// ============================================================================

#[test]
fn test_challenge_tasks_rest_api_lifecycle() {
    let tm = TaskManager::global();
    let (task_id, _child_token) = tm
        .spawn_task(
            "challenger-worker".into(),
            "Milestone 4 REST endpoint stress".into(),
            |_| {
                // Sleep briefly to simulate in-flight running task
                thread::sleep(Duration::from_millis(500));
                Ok("Finished".into())
            },
        )
        .expect("Task spawn must succeed");

    let _ = tm.append_task_log(&task_id, "Log item 1: initialized");
    let _ = tm.append_task_log(&task_id, "Log item 2: executing challenge");

    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");
    let base_url = format!("http://127.0.0.1:{}", port);

    // 2.1 GET /api/tasks
    let tasks_resp = ureq::get(&format!("{}/api/tasks", base_url))
        .call()
        .expect("GET /api/tasks failed");
    assert_eq!(tasks_resp.status(), 200);
    assert_eq!(tasks_resp.header("content-type").unwrap(), "application/json");
    assert_eq!(tasks_resp.header("access-control-allow-origin").unwrap(), "*");
    let tasks: Vec<serde_json::Value> = tasks_resp.into_json().unwrap();
    assert!(
        tasks.iter().any(|t| t["id"] == task_id),
        "Spawned task must appear in /api/tasks listing"
    );

    // 2.2 GET /api/tasks with query params: /api/tasks?filter=all
    let query_tasks_resp = ureq::get(&format!("{}/api/tasks?filter=all", base_url))
        .call()
        .expect("GET /api/tasks?filter=all failed");
    assert_eq!(query_tasks_resp.status(), 200);

    // 2.3 GET /api/tasks/<id>
    let single_resp = ureq::get(&format!("{}/api/tasks/{}", base_url, task_id))
        .call()
        .expect("GET single task failed");
    assert_eq!(single_resp.status(), 200);
    let snap: serde_json::Value = single_resp.into_json().unwrap();
    assert_eq!(snap["id"], task_id);
    assert_eq!(snap["name"], "challenger-worker");

    // 2.4 GET /api/tasks/<id>/logs
    let logs_resp = ureq::get(&format!("{}/api/tasks/{}/logs", base_url, task_id))
        .call()
        .expect("GET task logs failed");
    assert_eq!(logs_resp.status(), 200);
    let logs_text = logs_resp.into_string().unwrap();
    assert!(logs_text.contains("Log item 1: initialized"));
    assert!(logs_text.contains("Log item 2: executing challenge"));

    // 2.5 POST /api/tasks/<id>/cancel
    let cancel_resp = ureq::post(&format!("{}/api/tasks/{}/cancel", base_url, task_id))
        .call()
        .expect("POST cancel failed");
    assert_eq!(cancel_resp.status(), 200);
    let cancel_json: serde_json::Value = cancel_resp.into_json().unwrap();
    assert_eq!(cancel_json["id"], task_id);
    assert_eq!(cancel_json["status"], "cancelled");

    // 2.6 Verify task state in TaskManager is now cancelled
    let snap_after = tm.get_task(&task_id).expect("Task must still exist");
    assert_eq!(snap_after.status.as_str(), "cancelled");

    // 2.7 Duplicate cancellation (idempotent / already cancelled)
    let cancel_repeat = ureq::post(&format!("{}/api/tasks/{}/cancel", base_url, task_id))
        .call()
        .expect("POST cancel repeat should succeed with status 200");
    assert_eq!(cancel_repeat.status(), 200);

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 3: 404 Error Handling & Edge Cases
// ============================================================================

#[test]
fn test_challenge_tasks_rest_api_404_and_edge_cases() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");
    let base_url = format!("http://127.0.0.1:{}", port);

    // 3.1 Nonexistent task ID GET
    let err_task = ureq::get(&format!("{}/api/tasks/nonexistent-task-9999", base_url)).call();
    match err_task {
        Err(ureq::Error::Status(404, resp)) => {
            let body = resp.into_string().unwrap();
            assert!(body.contains("Task 'nonexistent-task-9999' not found"));
        }
        other => panic!("Expected 404, got {:?}", other),
    }

    // 3.2 Nonexistent task logs GET
    let err_logs = ureq::get(&format!("{}/api/tasks/nonexistent-task-9999/logs", base_url)).call();
    match err_logs {
        Err(ureq::Error::Status(404, resp)) => {
            let body = resp.into_string().unwrap();
            assert!(body.contains("Task 'nonexistent-task-9999' not found"));
        }
        other => panic!("Expected 404, got {:?}", other),
    }

    // 3.3 Nonexistent task cancel POST
    let err_cancel = ureq::post(&format!("{}/api/tasks/nonexistent-task-9999/cancel", base_url)).call();
    match err_cancel {
        Err(ureq::Error::Status(404, resp)) => {
            let body = resp.into_string().unwrap();
            assert!(body.contains("Task 'nonexistent-task-9999' not found"));
        }
        other => panic!("Expected 404, got {:?}", other),
    }

    // 3.4 Empty task ID: /api/tasks/
    let err_empty = ureq::get(&format!("{}/api/tasks/", base_url)).call();
    match err_empty {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for empty task ID, got {:?}", other),
    }

    // 3.5 Completely unknown route: /unknown/path
    let err_unknown = ureq::get(&format!("{}/unknown/path", base_url)).call();
    match err_unknown {
        Err(ureq::Error::Status(404, resp)) => {
            let body = resp.into_string().unwrap();
            assert!(body.contains("Not Found"));
        }
        other => panic!("Expected 404 for unknown route, got {:?}", other),
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 4: CORS Preflight (OPTIONS *) & Security Headers
// ============================================================================

#[test]
fn test_challenge_cors_preflight_and_headers() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    let stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .expect("TcpStream connect must succeed");
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    // 4.1 Send raw OPTIONS * request
    writer
        .write_all(b"OPTIONS * HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .unwrap();

    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    assert!(
        status_line.contains("204 No Content"),
        "OPTIONS * must return 204 No Content, got: {}",
        status_line
    );

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        headers.push(trimmed.to_string());
    }

    let joined_headers = headers.join("\n").to_lowercase();
    assert!(
        joined_headers.contains("access-control-allow-origin: *"),
        "Must have Access-Control-Allow-Origin: *"
    );
    assert!(
        joined_headers.contains("access-control-allow-methods: get, post, options"),
        "Must have Access-Control-Allow-Methods"
    );
    assert!(
        joined_headers.contains("access-control-allow-headers: content-type, authorization, x-requested-with"),
        "Must have Access-Control-Allow-Headers"
    );
    assert!(
        joined_headers.contains("content-length: 0"),
        "Must have Content-Length: 0"
    );

    // 4.2 Endpoint-specific OPTIONS: OPTIONS /api/tasks/task-1/cancel
    let stream2 = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
    let mut writer2 = stream2;
    writer2
        .write_all(b"OPTIONS /api/tasks/task-1/cancel HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .unwrap();
    let mut status2 = String::new();
    reader2.read_line(&mut status2).unwrap();
    assert!(status2.contains("204 No Content"));

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 5: Non-Blocking Shutdown & Immediate Port Rebinding
// ============================================================================

#[test]
fn test_challenge_non_blocking_shutdown_and_port_rebind() {
    let token = CancellationToken::new();
    let (bound_port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    // Verify server is alive
    let check = ureq::get(&format!("http://127.0.0.1:{}/api/tasks", bound_port)).call();
    assert!(check.is_ok(), "Server must be responsive");

    // Signal shutdown and time the thread join
    let start = Instant::now();
    token.cancel();
    let join_res = handle.join();
    let elapsed = start.elapsed();

    assert!(join_res.is_ok(), "Server thread must join without panic");
    assert!(
        elapsed < Duration::from_millis(500),
        "Shutdown must complete non-blockingly (< 500ms), took {:?}",
        elapsed
    );

    // Verify port was released: re-bind to the exact same port must succeed
    let rebind_res = TcpListener::bind(format!("127.0.0.1:{}", bound_port));
    assert!(
        rebind_res.is_ok(),
        "Port {} must be immediately released and re-bindable after shutdown",
        bound_port
    );
}

// ============================================================================
// CHALLENGE 6: Malformed & Adversarial Request Handling
// ============================================================================

#[test]
fn test_challenge_malformed_and_adversarial_requests() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    let base_addr = format!("127.0.0.1:{}", port);

    // 6.1 Empty TCP connection (immediate EOF)
    {
        let stream = TcpStream::connect(&base_addr).unwrap();
        drop(stream); // Close connection immediately
        thread::sleep(Duration::from_millis(20));
    }

    // 6.2 Single-word malformed request line
    {
        let mut stream = TcpStream::connect(&base_addr).unwrap();
        stream.write_all(b"GARBAGE_NO_PATH\r\n\r\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp_line = String::new();
        let _ = reader.read_line(&mut resp_line);
        assert!(
            resp_line.contains("400 Bad Request"),
            "Malformed request line must yield 400 Bad Request, got: {}",
            resp_line
        );
    }

    // 6.3 Unsupported HTTP method: PUT
    {
        let mut stream = TcpStream::connect(&base_addr).unwrap();
        stream.write_all(b"PUT /api/tasks HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp_line = String::new();
        let _ = reader.read_line(&mut resp_line);
        assert!(
            resp_line.contains("404 Not Found"),
            "Unsupported method must yield 404, got: {}",
            resp_line
        );
    }

    // 6.4 URI Directory Traversal attempt: GET /../../Cargo.toml
    {
        let mut stream = TcpStream::connect(&base_addr).unwrap();
        stream.write_all(b"GET /../../Cargo.toml HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp_line = String::new();
        let _ = reader.read_line(&mut resp_line);
        assert!(
            resp_line.contains("404 Not Found"),
            "URI directory traversal must yield 404, got: {}",
            resp_line
        );
    }

    // 6.5 Content-Length header with truncated body (client drops socket early)
    {
        let mut stream = TcpStream::connect(&base_addr).unwrap();
        stream.write_all(b"POST /api/tasks/fake/cancel HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 100\r\n\r\nShort").unwrap();
        drop(stream); // Prematurely disconnect
        thread::sleep(Duration::from_millis(30));
    }

    // 6.6 Verify server is STILL healthy after all malformed requests
    let health_check = ureq::get(&format!("http://{}/api/tasks", base_addr)).call();
    assert!(health_check.is_ok(), "Server must remain functional after adversarial requests");

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 7: High Concurrency Stress (250 simultaneous requests)
// ============================================================================

#[test]
fn test_challenge_high_concurrency_stress() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port 0");

    let base_url = Arc::new(format!("http://127.0.0.1:{}", port));
    let num_threads = 25;
    let requests_per_thread = 10;
    let mut workers = Vec::new();

    for t_idx in 0..num_threads {
        let url = Arc::clone(&base_url);
        let worker = thread::spawn(move || {
            for i in 0..requests_per_thread {
                match (t_idx + i) % 4 {
                    0 => {
                        let resp = ureq::get(&format!("{}/", url)).call();
                        assert!(resp.is_ok(), "Concurrent GET / failed");
                        assert_eq!(resp.unwrap().status(), 200);
                    }
                    1 => {
                        let resp = ureq::get(&format!("{}/api/tasks", url)).call();
                        assert!(resp.is_ok(), "Concurrent GET /api/tasks failed");
                        assert_eq!(resp.unwrap().status(), 200);
                    }
                    2 => {
                        let resp = ureq::get(&format!("{}/api/tasks/nonexistent-{}", url, i)).call();
                        match resp {
                            Err(ureq::Error::Status(404, _)) => {}
                            other => panic!("Expected 404 for concurrent nonexistent, got {:?}", other),
                        }
                    }
                    _ => {
                        // Raw OPTIONS
                        let stream = TcpStream::connect(url.trim_start_matches("http://")).unwrap();
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut writer = stream;
                        writer.write_all(b"OPTIONS * HTTP/1.1\r\n\r\n").unwrap();
                        let mut status = String::new();
                        reader.read_line(&mut status).unwrap();
                        assert!(status.contains("204 No Content"));
                    }
                }
            }
        });
        workers.push(worker);
    }

    for w in workers {
        w.join().expect("Worker thread panicked during concurrency stress");
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 8: CLI Subprocess Integration (ctrl-cli serve)
// ============================================================================

#[test]
fn test_challenge_cli_serve_subprocess() {
    let binary_path = env!("CARGO_BIN_EXE_ctrl-cli");

    // Find free ephemeral port
    let ephemeral_listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral");
    let port = ephemeral_listener.local_addr().unwrap().port();
    drop(ephemeral_listener); // Release so subprocess can bind it

    let child = Command::new(binary_path)
        .args(["serve", "--port", &port.to_string(), "--host", "127.0.0.1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli serve child process");

    let _guard = ChildGuard { child };
    let base_url = format!("http://127.0.0.1:{}", port);

    // Wait up to 5s for server readiness
    let start = Instant::now();
    let mut ready = false;
    while start.elapsed() < Duration::from_secs(5) {
        if ureq::get(&format!("{}/api/tasks", base_url)).call().is_ok() {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ready, "Subprocess server did not become ready within 5s");

    // Verify GET / serves HTML
    let root_res = ureq::get(&format!("{}/", base_url)).call().unwrap();
    assert_eq!(root_res.status(), 200);
    assert_eq!(root_res.header("content-type").unwrap(), "text/html; charset=utf-8");

    // Verify GET /api/tasks returns JSON
    let tasks_res = ureq::get(&format!("{}/api/tasks", base_url)).call().unwrap();
    assert_eq!(tasks_res.status(), 200);
    assert_eq!(tasks_res.header("content-type").unwrap(), "application/json");
}
