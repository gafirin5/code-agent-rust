//! Milestone 3 Empirical Challenge Suite: Windows Socket 10053 Resilience & Connection Lifecycle
//!
//! Empirical Verification Coverage:
//! 1. Rapid bursts of connect-and-abort/reset (simulating WSAECONNABORTED 10053 and WSAECONNRESET 10054)
//!    do NOT cause thread starvation, loop sleep, or latency spikes above 500ms.
//! 2. `drain_and_close` prevents unbuffered TCP RST packets and socket handle leaks under high request rates.
//! 3. CORS preflight `OPTIONS` requests return 204 No Content with all required headers.
//! 4. Invalid or missing prompt in `POST /api/tasks/run` returns 400 Bad Request.
//! 5. SSE stream lifecycle, task status fanout, and abrupt client disconnect teardown.

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
}

#[path = "../src/server.rs"]
pub mod server;

use agent::tasks::{CancellationToken, TaskManager};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
fn get_current_process_handle_count() -> u32 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessHandleCount(hProcess: *mut std::ffi::c_void, pdwHandleCount: *mut u32) -> i32;
    }
    unsafe {
        let mut count = 0u32;
        let proc = GetCurrentProcess();
        let ret = GetProcessHandleCount(proc, &mut count);
        assert_ne!(ret, 0, "GetProcessHandleCount failed");
        count
    }
}

#[cfg(not(windows))]
fn get_current_process_handle_count() -> u32 {
    0
}

/// Configures Winsock SO_LINGER (l_onoff=1, l_linger=0) to force a hard TCP RST on socket close.
#[cfg(windows)]
fn set_socket_rst(stream: &TcpStream) {
    use std::os::windows::io::AsRawSocket;
    #[repr(C)]
    struct Linger {
        l_onoff: u16,
        l_linger: u16,
    }
    let linger = Linger {
        l_onoff: 1,
        l_linger: 0,
    };
    #[link(name = "ws2_32")]
    extern "system" {
        fn setsockopt(
            s: usize,
            level: i32,
            optname: i32,
            optval: *const std::ffi::c_void,
            optlen: i32,
        ) -> i32;
    }
    const SOL_SOCKET: i32 = 0xffff;
    const SO_LINGER: i32 = 0x0080;
    unsafe {
        let raw = stream.as_raw_socket() as usize;
        let _ = setsockopt(
            raw,
            SOL_SOCKET,
            SO_LINGER,
            &linger as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<Linger>() as i32,
        );
    }
}

#[cfg(not(windows))]
fn set_socket_rst(_stream: &TcpStream) {}

// ============================================================================
// CHALLENGE 1: Windows Socket 10053 / 10054 Resilience & Latency Spike Gate
// ============================================================================

#[test]
fn test_empirical_socket_10053_burst_latency_under_500ms() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port");
    let base_url = format!("http://127.0.0.1:{}", port);
    let addr = format!("127.0.0.1:{}", port);

    // Warm-up request
    let warm = ureq::get(&format!("{}/api/health", base_url))
        .call()
        .expect("Warm-up request must succeed");
    assert_eq!(warm.status(), 200);

    // BURST 1: 100 Connect-and-Immediate-Drop cycles (simulates WSAECONNABORTED 10053)
    let b1_start = Instant::now();
    for _ in 0..100 {
        if let Ok(stream) = TcpStream::connect(&addr) {
            drop(stream);
        }
    }
    let b1_duration = b1_start.elapsed();
    println!("Burst 1 (100 immediate drops) finished in {:?}", b1_duration);

    // Measure latency immediately after Burst 1
    let t0 = Instant::now();
    let r1 = ureq::get(&format!("{}/api/health", base_url))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("Server must answer after Burst 1");
    let l1 = t0.elapsed();
    assert_eq!(r1.status(), 200);
    assert!(
        l1 < Duration::from_millis(500),
        "Burst 1 latency spike: took {:?}",
        l1
    );
    println!("Post-Burst 1 health latency: {:?}", l1);

    // BURST 2: 100 Partial-Write Abort cycles (write "G" and drop)
    let b2_start = Instant::now();
    for _ in 0..100 {
        if let Ok(mut stream) = TcpStream::connect(&addr) {
            let _ = stream.write_all(b"G");
            drop(stream);
        }
    }
    let b2_duration = b2_start.elapsed();
    println!("Burst 2 (100 partial-write drops) finished in {:?}", b2_duration);

    // Measure latency immediately after Burst 2
    let t0 = Instant::now();
    let r2 = ureq::get(&format!("{}/api/health", base_url))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("Server must answer after Burst 2");
    let l2 = t0.elapsed();
    assert_eq!(r2.status(), 200);
    assert!(
        l2 < Duration::from_millis(500),
        "Burst 2 latency spike: took {:?}",
        l2
    );
    println!("Post-Burst 2 health latency: {:?}", l2);

    // BURST 3: 50 Hard TCP RST Bursts with SO_LINGER=0 (simulates WSAECONNRESET 10054)
    let b3_start = Instant::now();
    for _ in 0..50 {
        if let Ok(stream) = TcpStream::connect(&addr) {
            set_socket_rst(&stream);
            drop(stream);
        }
    }
    let b3_duration = b3_start.elapsed();
    println!("Burst 3 (50 hard RST drops) finished in {:?}", b3_duration);

    // Measure latency immediately after Burst 3
    let t0 = Instant::now();
    let r3 = ureq::get(&format!("{}/api/health", base_url))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("Server must answer after Burst 3");
    let l3 = t0.elapsed();
    assert_eq!(r3.status(), 200);
    assert!(
        l3 < Duration::from_millis(500),
        "Burst 3 latency spike: took {:?}",
        l3
    );
    println!("Post-Burst 3 health latency: {:?}", l3);

    // BURST 4: Concurrent Multi-Threaded Abort Storm
    // 10 client threads simultaneously performing 10 aborts each (100 total concurrent aborts)
    let mut thread_handles = Vec::new();
    for _ in 0..10 {
        let addr_clone = addr.clone();
        thread_handles.push(thread::spawn(move || {
            for _ in 0..10 {
                if let Ok(mut s) = TcpStream::connect(&addr_clone) {
                    let _ = s.write_all(b"GET /");
                    drop(s);
                }
            }
        }));
    }
    for th in thread_handles {
        let _ = th.join();
    }

    // Measure latency immediately after concurrent storm
    let t0 = Instant::now();
    let r4 = ureq::get(&format!("{}/api/health", base_url))
        .timeout(Duration::from_secs(2))
        .call()
        .expect("Server must answer after Burst 4");
    let l4 = t0.elapsed();
    assert_eq!(r4.status(), 200);
    assert!(
        l4 < Duration::from_millis(500),
        "Burst 4 latency spike: took {:?}",
        l4
    );
    println!("Post-Burst 4 health latency: {:?}", l4);

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 2: `drain_and_close` Prevention of Unbuffered RST & Handle Leaks
// ============================================================================

#[test]
fn test_empirical_drain_and_close_and_socket_handle_leak_resistance() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port");
    let base_url = format!("http://127.0.0.1:{}", port);
    let addr = format!("127.0.0.1:{}", port);

    // Warm up to stabilize initial handles
    for _ in 0..5 {
        let _ = ureq::get(&format!("{}/api/health", base_url)).call();
    }
    thread::sleep(Duration::from_millis(100));

    let initial_handles = get_current_process_handle_count();

    // 2.1 Send requests with unbuffered trailing data
    // Server must drain receive buffer cleanly without sending RST back to client
    for _ in 0..50 {
        let mut stream = TcpStream::connect(&addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // Send request with trailing unread data
        let req = format!(
            "GET /api/health HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\nTRAILING_GARBAGE_BYTES_12345",
            addr
        );
        stream.write_all(req.as_bytes()).unwrap();

        let mut resp = String::new();
        let read_res = stream.read_to_string(&mut resp);
        assert!(read_res.is_ok(), "Client read failed: {:?}", read_res.err());
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        drop(stream);
    }

    // 2.2 High Request Rate (150 sequential requests across endpoints)
    for i in 0..150 {
        let path = match i % 3 {
            0 => "/api/health",
            1 => "/",
            _ => "/api/tasks",
        };
        let resp = ureq::get(&format!("{}{}", base_url, path))
            .timeout(Duration::from_secs(2))
            .call()
            .expect("High rate request must succeed");
        assert_eq!(resp.status(), 200);
    }

    // Settle handle reclamation
    thread::sleep(Duration::from_millis(200));
    let final_handles = get_current_process_handle_count();
    let delta = (final_handles as i64) - (initial_handles as i64);
    println!(
        "Handle Check after 200 requests: Initial={}, Final={}, Delta={}",
        initial_handles, final_handles, delta
    );

    #[cfg(windows)]
    {
        // Allow at most +2 handles for OS background runtime fluctuation
        assert!(
            delta <= 2,
            "SOCKET HANDLE LEAK DETECTED: 200 requests leaked {} handles (Initial={}, Final={})",
            delta,
            initial_handles,
            final_handles
        );
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 3: CORS Preflight OPTIONS Compliance (204 No Content & Headers)
// ============================================================================

#[test]
fn test_empirical_cors_preflight_options_all_endpoints() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port");
    let addr = format!("127.0.0.1:{}", port);

    let test_paths = [
        "*",
        "/",
        "/api/tasks/run",
        "/api/events",
        "/api/tasks",
        "/api/metrics",
        "/api/tasks/xyz-123/cancel",
    ];

    for path in test_paths {
        let mut stream = TcpStream::connect(&addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let req = format!(
            "OPTIONS {} HTTP/1.1\r\nHost: {}\r\nOrigin: http://localhost:3000\r\nAccess-Control-Request-Method: POST\r\n\r\n",
            path, addr
        );
        stream.write_all(req.as_bytes()).unwrap();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();

        assert!(
            status_line.contains("204 No Content"),
            "OPTIONS {} must return 204 No Content, got: {}",
            path,
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

        let headers_lower = headers.join("\n").to_lowercase();

        // Validate required CORS headers
        assert!(
            headers_lower.contains("access-control-allow-origin: *"),
            "OPTIONS {} missing Access-Control-Allow-Origin: *",
            path
        );
        assert!(
            headers_lower.contains("access-control-allow-methods: get, post, options"),
            "OPTIONS {} missing Access-Control-Allow-Methods",
            path
        );
        assert!(
            headers_lower.contains("access-control-allow-headers: content-type, authorization, x-requested-with"),
            "OPTIONS {} missing Access-Control-Allow-Headers",
            path
        );
        assert!(
            headers_lower.contains("content-length: 0"),
            "OPTIONS {} missing Content-Length: 0",
            path
        );
        assert!(
            headers_lower.contains("connection: close"),
            "OPTIONS {} missing Connection: close",
            path
        );

        // Verify body is strictly 0 bytes
        let mut body = Vec::new();
        let _ = reader.read_to_end(&mut body);
        assert_eq!(
            body.len(),
            0,
            "OPTIONS {} must have empty body, got {} bytes",
            path,
            body.len()
        );
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 4: `POST /api/tasks/run` Robust Validation & Error Codes
// ============================================================================

#[test]
fn test_empirical_post_tasks_run_validation_adversarial() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port");
    let run_url = format!("http://127.0.0.1:{}/api/tasks/run", port);

    // 4.1 Missing prompt field -> 400 Bad Request
    let resp1 = ureq::post(&run_url).send_json(json!({
        "skill": "researcher",
        "model": "qwen2.5-coder"
    }));
    match resp1 {
        Err(ureq::Error::Status(400, r)) => {
            let body = r.into_string().unwrap();
            assert!(
                body.contains("Missing or empty required field: 'prompt'"),
                "Body: {}",
                body
            );
        }
        other => panic!("Expected 400 for missing prompt, got {:?}", other),
    }

    // 4.2 Empty string prompt -> 400 Bad Request
    let resp2 = ureq::post(&run_url).send_json(json!({
        "prompt": ""
    }));
    match resp2 {
        Err(ureq::Error::Status(400, r)) => {
            let body = r.into_string().unwrap();
            assert!(body.contains("Missing or empty required field: 'prompt'"));
        }
        other => panic!("Expected 400 for empty prompt, got {:?}", other),
    }

    // 4.3 Whitespace-only prompt -> 400 Bad Request
    let resp3 = ureq::post(&run_url).send_json(json!({
        "prompt": "   \t\r\n   "
    }));
    match resp3 {
        Err(ureq::Error::Status(400, r)) => {
            let body = r.into_string().unwrap();
            assert!(body.contains("Missing or empty required field: 'prompt'"));
        }
        other => panic!("Expected 400 for whitespace-only prompt, got {:?}", other),
    }

    // 4.4 Non-string prompt (integer, array, bool) -> 400 Bad Request
    for invalid_val in [json!(12345), json!(["run", "task"]), json!(true)] {
        let resp4 = ureq::post(&run_url).send_json(json!({
            "prompt": invalid_val
        }));
        match resp4 {
            Err(ureq::Error::Status(400, r)) => {
                let body = r.into_string().unwrap();
                assert!(body.contains("Missing or empty required field: 'prompt'"));
            }
            other => panic!("Expected 400 for non-string prompt, got {:?}", other),
        }
    }

    // 4.5 Malformed JSON payload -> 400 Bad Request
    let resp5 = ureq::post(&run_url)
        .set("Content-Type", "application/json")
        .send_string("INVALID_NON_JSON_DATA{{");
    match resp5 {
        Err(ureq::Error::Status(400, r)) => {
            let body = r.into_string().unwrap();
            assert!(body.contains("Invalid JSON"));
        }
        other => panic!("Expected 400 for invalid JSON, got {:?}", other),
    }

    // 4.6 Empty body with Content-Length: 0 -> 400 Bad Request
    let resp6 = ureq::post(&run_url)
        .set("Content-Type", "application/json")
        .send_string("");
    match resp6 {
        Err(ureq::Error::Status(400, r)) => {
            let body = r.into_string().unwrap();
            assert!(body.contains("Invalid JSON"));
        }
        other => panic!("Expected 400 for empty body, got {:?}", other),
    }

    // 4.7 Valid prompt -> 200 OK with {"id": "...", "status": "queued"}
    let valid_resp = ureq::post(&run_url)
        .send_json(json!({
            "prompt": "Run empirical memory check",
            "skill": "profiler"
        }))
        .expect("Valid task run must succeed");
    assert_eq!(valid_resp.status(), 200);
    assert_eq!(valid_resp.header("content-type").unwrap(), "application/json");

    let val: serde_json::Value = valid_resp.into_json().expect("Valid JSON response");
    assert_eq!(val["status"], "queued");
    let task_id = val["id"].as_str().expect("Task id must be string");
    assert!(!task_id.is_empty(), "Task id must not be empty");

    // Verify task actually registered in TaskManager
    let tm = TaskManager::global();
    let snap = tm.get_task(task_id);
    assert!(snap.is_some(), "Spawned task must exist in TaskManager");
    let task_snap = snap.unwrap();
    assert_eq!(task_snap.id, task_id);
    assert_eq!(task_snap.name, "profiler");

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 5: SSE Stream Lifecycle, Event Fanout & Abrupt Disconnect Teardown
// ============================================================================

#[test]
fn test_empirical_sse_stream_fanout_and_abrupt_disconnect() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Server must bind to ephemeral port");
    let base_url = format!("http://127.0.0.1:{}", port);
    let events_url = format!("{}/api/events", base_url);

    // Initial subscriber count
    let initial_subs = TaskManager::global().broadcaster().subscriber_count();

    // 5.1 Connect SSE Client using ureq
    let sse_resp = ureq::get(&events_url)
        .set("Accept", "text/event-stream")
        .call()
        .expect("GET /api/events must succeed");
    assert_eq!(sse_resp.status(), 200);
    assert_eq!(sse_resp.header("content-type").unwrap(), "text/event-stream");

    let mut reader = BufReader::new(sse_resp.into_reader());

    // Read initial ping keepalive
    let mut ping_line = String::new();
    reader.read_line(&mut ping_line).unwrap();
    assert!(
        ping_line.contains(": ping"),
        "First frame must be ping, got: {}",
        ping_line
    );

    // Verify subscriber was registered
    let subs_during = TaskManager::global().broadcaster().subscriber_count();
    assert_eq!(subs_during, initial_subs + 1);

    // 5.2 Dispatch a task to trigger SSE broadcast
    let run_resp = ureq::post(&format!("{}/api/tasks/run", base_url))
        .send_json(json!({"prompt": "Stream verification task"}))
        .expect("task run");
    assert_eq!(run_resp.status(), 200);
    let run_json: serde_json::Value = run_resp.into_json().unwrap();
    let task_id = run_json["id"].as_str().unwrap();

    // 5.3 Read task_status or task_log frame from SSE
    let mut received_event = false;
    for _ in 0..20 {
        let mut frame_line = String::new();
        if reader.read_line(&mut frame_line).is_ok() && !frame_line.trim().is_empty()
            && (frame_line.contains("task_status") || frame_line.contains(task_id)) {
                received_event = true;
                break;
        }
    }
    assert!(
        received_event,
        "Must receive task_status event frame over SSE"
    );

    // 5.4 Abrupt Client Disconnect
    // Drop reader immediately
    drop(reader);

    // Trigger another broadcast to let broadcaster detect disconnected channel
    TaskManager::global()
        .broadcaster()
        .broadcast_status("dummy-id", "test");

    // Wait briefly for server loop to detect write error and unsubscribe
    thread::sleep(Duration::from_millis(300));
    let subs_after = TaskManager::global().broadcaster().subscriber_count();
    assert_eq!(
        subs_after, initial_subs,
        "Broadcaster must prune disconnected subscriber (was {}, expected {})",
        subs_after, initial_subs
    );

    token.cancel();
    let _ = handle.join();
}
