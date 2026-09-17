//! Milestone 2 Phase 4 Empirical Adversarial HTTP & Server Stress Test Suite
//!
//! Target: `GET /api/metrics` and server robustness in `ctrl-cli/src/server.rs`.
//!
//! Objectives:
//! 1. Fire 100 concurrent HTTP GET requests against `GET /api/metrics` on an ephemeral server:
//!    - Verify 100% respond HTTP 200 OK.
//!    - Verify valid JSON payload deserializable into `ProcessMetrics`.
//!    - Verify schema invariants (rss > 0, peak >= rss, cpu in [0, 100], threads >= 1, timestamp > 0).
//!    - Verify CORS headers (`Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods`, etc.).
//!    - Verify zero connection leaks or socket hangs under full concurrency.
//! 2. Test OPTIONS preflight requests to `/api/metrics`:
//!    - Verify HTTP 204 No Content.
//!    - Verify CORS preflight headers and zero body.
//!    - Verify rapid repeated OPTIONS calls.
//! 3. Test non-GET methods (POST, PUT, DELETE, PATCH, TRACE):
//!    - Verify graceful handling (HTTP 404 with JSON error) without crashing or blocking.
//! 4. Test invalid URLs and edge-case paths:
//!    - Trailing slashes, typos, subpaths, query string handling, directory traversal, long URIs.
//! 5. Test adversarial socket behavior:
//!    - Early client disconnects, truncated bodies, empty TCP streams, slow writes.
//! 6. Verify server lifecycle, thread cleanup, and port release.

pub mod agent {
    #[path = "../../src/agent/tasks.rs"]
    pub mod tasks;
}

#[path = "../src/server.rs"]
pub mod server;

pub use server::telemetry;

use agent::tasks::CancellationToken;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use telemetry::ProcessMetrics;

#[test]
fn diagnostic_timing_of_capture_metrics() {
    let t0 = Instant::now();
    for _ in 0..10 {
        let _ = telemetry::capture_metrics(None);
    }
    let elapsed = t0.elapsed();
    println!("10 calls to capture_metrics took: {:?} (avg: {:?})", elapsed, elapsed / 10);
}


#[test]
fn challenge_100_concurrent_get_metrics_burst() {
    const CONCURRENCY: usize = 100;

    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");
    assert!(port > 0, "Ephemeral port must be > 0");
    let base_url = Arc::new(format!("http://127.0.0.1:{}", port));

    let barrier = Arc::new(Barrier::new(CONCURRENCY));
    let results = Arc::new(Mutex::new(Vec::with_capacity(CONCURRENCY)));
    let mut client_handles = Vec::with_capacity(CONCURRENCY);

    let start_all = Instant::now();

    for client_id in 0..CONCURRENCY {
        let b = Arc::clone(&barrier);
        let url = Arc::clone(&base_url);
        let res_sink = Arc::clone(&results);

        let h = thread::spawn(move || {
            // Synchronize all 100 threads to burst simultaneously
            b.wait();
            let req_start = Instant::now();

            let resp = ureq::get(&format!("{}/api/metrics", url))
                .timeout(Duration::from_secs(20))
                .call();

            let elapsed = req_start.elapsed();

            match resp {
                Ok(response) => {
                    let status = response.status();
                    let content_type = response.header("content-type").map(|s| s.to_string());
                    let cors_origin = response.header("access-control-allow-origin").map(|s| s.to_string());
                    let cors_methods = response.header("access-control-allow-methods").map(|s| s.to_string());
                    let connection_header = response.header("connection").map(|s| s.to_string());

                    let body_str = response.into_string().unwrap_or_default();
                    let parsed_json: Result<ProcessMetrics, _> = serde_json::from_str(&body_str);

                    let mut guard = res_sink.lock().unwrap();
                    guard.push((client_id, Ok((status, content_type, cors_origin, cors_methods, connection_header, parsed_json, elapsed))));
                }
                Err(e) => {
                    let mut guard = res_sink.lock().unwrap();
                    guard.push((client_id, Err(format!("Client {} error: {:?}", client_id, e))));
                }
            }
        });
        client_handles.push(h);
    }

    for h in client_handles {
        h.join().expect("Client worker thread panicked during 100 concurrent GET burst");
    }

    let total_elapsed = start_all.elapsed();
    println!(
        "100 concurrent GET /api/metrics burst finished in {:.2}ms",
        total_elapsed.as_secs_f64() * 1000.0
    );

    let collected = results.lock().unwrap();
    assert_eq!(
        collected.len(),
        CONCURRENCY,
        "All 100 concurrent client requests must record a result"
    );

    let mut latencies = Vec::with_capacity(CONCURRENCY);

    for (id, res) in collected.iter() {
        match res {
            Ok((status, content_type, cors_origin, cors_methods, connection_header, parsed_json, elapsed)) => {
                latencies.push(*elapsed);

                // 1. Status 200 OK
                assert_eq!(*status, 200, "Client {} expected HTTP 200, got {}", id, status);

                // 2. Content-Type application/json
                assert_eq!(
                    content_type.as_deref(),
                    Some("application/json"),
                    "Client {} expected Content-Type application/json, got {:?}",
                    id,
                    content_type
                );

                // 3. CORS headers
                assert_eq!(
                    cors_origin.as_deref(),
                    Some("*"),
                    "Client {} expected Access-Control-Allow-Origin: *, got {:?}",
                    id,
                    cors_origin
                );
                assert!(
                    cors_methods.as_ref().map(|s| s.contains("GET")).unwrap_or(false),
                    "Client {} expected Access-Control-Allow-Methods to contain GET, got {:?}",
                    id,
                    cors_methods
                );
                assert_eq!(
                    connection_header.as_deref(),
                    Some("close"),
                    "Client {} expected Connection: close, got {:?}",
                    id,
                    connection_header
                );

                // 4. Valid JSON ProcessMetrics
                let metrics = parsed_json
                    .as_ref()
                    .unwrap_or_else(|e| panic!("Client {} failed to parse ProcessMetrics JSON: {}", id, e));

                // Invariants:
                assert!(
                    metrics.memory.rss_bytes > 0,
                    "Client {} rss_bytes must be > 0, got {}",
                    id,
                    metrics.memory.rss_bytes
                );
                assert!(
                    metrics.memory.peak_rss_bytes >= metrics.memory.rss_bytes,
                    "Client {} peak_rss_bytes ({}) must be >= rss_bytes ({})",
                    id,
                    metrics.memory.peak_rss_bytes,
                    metrics.memory.rss_bytes
                );
                assert!(
                    (0.0..=100.0).contains(&metrics.cpu.process_pct),
                    "Client {} cpu pct ({}) must be in [0.0, 100.0]",
                    id,
                    metrics.cpu.process_pct
                );
                assert!(
                    metrics.threads.active_threads >= 1,
                    "Client {} active_threads ({}) must be >= 1",
                    id,
                    metrics.threads.active_threads
                );
                assert!(
                    metrics.timestamp > 0,
                    "Client {} timestamp must be non-zero",
                    id
                );
            }
            Err(e) => panic!("Request failed during concurrency burst: {}", e),
        }
    }

    // 5. Zero connection leaks or socket hangs: verify total time and max latency
    let max_latency = latencies.iter().max().copied().unwrap_or_default();
    let avg_latency = latencies.iter().sum::<Duration>() / (CONCURRENCY as u32);
    println!(
        "Latency distribution across 100 concurrent requests: avg={:?}, max={:?}",
        avg_latency, max_latency
    );
    assert!(
        total_elapsed < Duration::from_secs(20),
        "Total burst time exceeded 20 seconds: {:?}",
        total_elapsed
    );

    // Verify server remains responsive after the 100 requests
    let health_check = ureq::get(&format!("{}/api/metrics", base_url)).call();
    assert!(health_check.is_ok(), "Server must be responsive after 100 concurrent burst");
    assert_eq!(health_check.unwrap().status(), 200);

    // Clean shutdown
    token.cancel();
    let join_res = handle.join();
    assert!(join_res.is_ok(), "Server thread must join cleanly without error");
}

// ============================================================================
// CHALLENGE 2: CORS PREFLIGHT OPTIONS REQUESTS TO /api/metrics
// ============================================================================

#[test]
fn challenge_options_preflight_to_metrics_endpoint() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");

    let addr = format!("127.0.0.1:{}", port);

    // 2.1 Standard CORS preflight request with Origin and Request Headers
    {
        let stream = TcpStream::connect(&addr).expect("TCP connect failed");
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        let req = "OPTIONS /api/metrics HTTP/1.1\r\n\
                   Host: 127.0.0.1\r\n\
                   Origin: https://dashboard.example.com\r\n\
                   Access-Control-Request-Method: GET\r\n\
                   Access-Control-Request-Headers: Content-Type, Authorization\r\n\
                   \r\n";
        writer.write_all(req.as_bytes()).unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("204 No Content"),
            "OPTIONS /api/metrics must return 204 No Content, got: {}",
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

        let header_blob = headers.join("\n").to_lowercase();
        assert!(
            header_blob.contains("access-control-allow-origin: *"),
            "OPTIONS response missing Access-Control-Allow-Origin: *"
        );
        assert!(
            header_blob.contains("access-control-allow-methods:"),
            "OPTIONS response missing Access-Control-Allow-Methods"
        );
        assert!(
            header_blob.contains("get") && header_blob.contains("options"),
            "OPTIONS response methods must include GET and OPTIONS"
        );
        assert!(
            header_blob.contains("access-control-allow-headers:"),
            "OPTIONS response missing Access-Control-Allow-Headers"
        );
        assert!(
            header_blob.contains("content-length: 0"),
            "OPTIONS response must have Content-Length: 0"
        );
        assert!(
            header_blob.contains("connection: close"),
            "OPTIONS response must have Connection: close"
        );

        // Verify body is empty (EOF reached immediately after headers)
        let mut body = Vec::new();
        reader.read_to_end(&mut body).unwrap();
        assert!(
            body.is_empty(),
            "OPTIONS 204 No Content response must have an empty body"
        );
    }

    // 2.2 Preflight with query params: OPTIONS /api/metrics?cors_probe=true
    {
        let stream = TcpStream::connect(&addr).expect("TCP connect failed");
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"OPTIONS /api/metrics?cors_probe=true HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("204 No Content"),
            "OPTIONS /api/metrics?query must return 204 No Content, got: {}",
            status_line
        );
    }

    // 2.3 Rapid burst of 25 OPTIONS requests in succession
    for i in 0..25 {
        let stream = TcpStream::connect(&addr)
            .unwrap_or_else(|e| panic!("Rapid OPTIONS iteration {} connect failed: {}", i, e));
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"OPTIONS /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(status_line.contains("204 No Content"));
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 3: NON-GET HTTP METHODS ON /api/metrics (POST, PUT, DELETE, ETC.)
// ============================================================================

#[test]
fn challenge_non_get_methods_handled_gracefully() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");

    let base_url = format!("http://127.0.0.1:{}", port);
    let addr = format!("127.0.0.1:{}", port);

    // 3.1 POST /api/metrics should return 404 Not Found (method not mapped to this route)
    let post_err = ureq::post(&format!("{}/api/metrics", base_url))
        .set("Content-Type", "application/json")
        .send_string(r#"{"action":"reset"}"#);

    match post_err {
        Err(ureq::Error::Status(404, resp)) => {
            assert_eq!(resp.header("content-type"), Some("application/json"));
            assert_eq!(resp.header("access-control-allow-origin"), Some("*"));
            let body = resp.into_string().unwrap();
            assert!(body.contains("Not Found"), "Body should contain Not Found, got: {}", body);
        }
        other => panic!("Expected 404 for POST /api/metrics, got: {:?}", other),
    }

    // 3.2 PUT /api/metrics should return 404 Not Found
    let put_err = ureq::put(&format!("{}/api/metrics", base_url))
        .send_string("invalid_body");

    match put_err {
        Err(ureq::Error::Status(404, resp)) => {
            assert_eq!(resp.status(), 404);
            let body = resp.into_string().unwrap();
            assert!(body.contains("Not Found"));
        }
        other => panic!("Expected 404 for PUT /api/metrics, got: {:?}", other),
    }

    // 3.3 DELETE /api/metrics should return 404 Not Found
    let delete_err = ureq::delete(&format!("{}/api/metrics", base_url)).call();
    match delete_err {
        Err(ureq::Error::Status(404, resp)) => {
            assert_eq!(resp.status(), 404);
            let body = resp.into_string().unwrap();
            assert!(body.contains("Not Found"));
        }
        other => panic!("Expected 404 for DELETE /api/metrics, got: {:?}", other),
    }

    // 3.4 PATCH /api/metrics via raw TCP
    {
        let stream = TcpStream::connect(&addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"PATCH /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("404 Not Found"),
            "PATCH /api/metrics must return 404, got: {}",
            status_line
        );
    }

    // 3.5 TRACE /api/metrics via raw TCP
    {
        let stream = TcpStream::connect(&addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"TRACE /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("404 Not Found"),
            "TRACE /api/metrics must return 404, got: {}",
            status_line
        );
    }

    // 3.6 HEAD /api/metrics via raw TCP
    {
        let stream = TcpStream::connect(&addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"HEAD /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(
            status_line.contains("404 Not Found"),
            "HEAD /api/metrics must return 404 (unmapped method), got: {}",
            status_line
        );
    }

    // 3.7 Verify server remains healthy and serves GET /api/metrics with 200
    let valid = ureq::get(&format!("{}/api/metrics", base_url)).call();
    assert!(valid.is_ok(), "Server must continue serving GET /api/metrics after non-GET verbs");
    assert_eq!(valid.unwrap().status(), 200);

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 4: INVALID URLS, QUERY STRINGS, AND ROUTING EDGE CASES
// ============================================================================

#[test]
fn challenge_invalid_urls_and_routing_edge_cases() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");

    let base_url = format!("http://127.0.0.1:{}", port);
    let addr = format!("127.0.0.1:{}", port);

    // 4.1 Trailing slash: /api/metrics/
    let err_trailing = ureq::get(&format!("{}/api/metrics/", base_url)).call();
    match err_trailing {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for /api/metrics/, got: {:?}", other),
    }

    // 4.2 Typo in endpoint name: /api/metric (singular)
    let err_singular = ureq::get(&format!("{}/api/metric", base_url)).call();
    match err_singular {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for /api/metric, got: {:?}", other),
    }

    // 4.3 Subpath under metrics: /api/metrics/cpu
    let err_subpath = ureq::get(&format!("{}/api/metrics/cpu", base_url)).call();
    match err_subpath {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for /api/metrics/cpu, got: {:?}", other),
    }

    // 4.4 Case sensitivity: /API/METRICS
    let err_case = ureq::get(&format!("{}/API/METRICS", base_url)).call();
    match err_case {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for /API/METRICS, got: {:?}", other),
    }

    // 4.5 Path traversal attempt: /api/metrics/../../Cargo.toml
    let err_traversal = ureq::get(&format!("{}/api/metrics/../../Cargo.toml", base_url)).call();
    match err_traversal {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for path traversal attempt, got: {:?}", other),
    }

    // 4.6 Query parameters: /api/metrics?format=json&interval=5s
    // Query string should be ignored by the path splitter and successfully serve metrics!
    let resp_query = ureq::get(&format!("{}/api/metrics?format=json&interval=5s", base_url))
        .call()
        .expect("GET /api/metrics with query parameters must succeed");
    assert_eq!(resp_query.status(), 200);
    let val: serde_json::Value = resp_query.into_json().expect("Must parse JSON");
    assert!(val.get("memory").is_some());
    assert!(val.get("cpu").is_some());

    // 4.7 Empty query string: /api/metrics?
    let resp_empty_query = ureq::get(&format!("{}/api/metrics?", base_url))
        .call()
        .expect("GET /api/metrics? must succeed");
    assert_eq!(resp_empty_query.status(), 200);

    // 4.8 Extremely long URL (8192 characters)
    let long_path = format!("{}/api/metrics/{}", base_url, "a".repeat(8192));
    let err_long = ureq::get(&long_path).call();
    match err_long {
        Err(ureq::Error::Status(404, _)) => {}
        other => panic!("Expected 404 for oversized URI path, got: {:?}", other),
    }

    // 4.9 Double slashes: //api//metrics
    {
        let stream = TcpStream::connect(&addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        writer
            .write_all(b"GET //api//metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();

        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(status_line.contains("404 Not Found"));
    }

    token.cancel();
    let _ = handle.join();
}

// ============================================================================
// CHALLENGE 5: ADVERSARIAL SOCKETS, PREMATURE DISCONNECTS & DURABILITY
// ============================================================================

#[test]
fn challenge_socket_resilience_and_no_leaks_or_hangs() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");

    let addr = format!("127.0.0.1:{}", port);
    let base_url = format!("http://127.0.0.1:{}", port);

    // 5.1 Empty TCP connection (immediate close upon connect)
    for _ in 0..10 {
        let stream = TcpStream::connect(&addr).unwrap();
        drop(stream);
    }
    thread::sleep(Duration::from_millis(50));

    // 5.2 Truncated HTTP request line (sends partial bytes then closes)
    for _ in 0..10 {
        let mut stream = TcpStream::connect(&addr).unwrap();
        let _ = stream.write_all(b"GET /api/met");
        drop(stream);
    }
    thread::sleep(Duration::from_millis(50));

    // 5.3 Request line without HTTP version (single-word malformed line)
    {
        let mut stream = TcpStream::connect(&addr).unwrap();
        stream.write_all(b"INVALID_NO_VERSION\r\n\r\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        let _ = reader.read_line(&mut resp);
        assert!(
            resp.contains("400 Bad Request"),
            "Single-word request must yield 400 Bad Request, got: {}",
            resp
        );
    }

    // 5.4 Truncated body with Content-Length claim
    for _ in 0..5 {
        let mut stream = TcpStream::connect(&addr).unwrap();
        let _ = stream.write_all(b"POST /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 5000\r\n\r\nTruncated");
        drop(stream);
    }
    thread::sleep(Duration::from_millis(50));

    // 5.5 Rapid burst of 50 connect-and-drop iterations
    for _ in 0..50 {
        if let Ok(mut s) = TcpStream::connect(&addr) {
            let _ = s.write_all(b"GET /");
            drop(s);
        }
    }
    thread::sleep(Duration::from_millis(250));

    // 5.6 Verify server is completely healthy and responsive
    let mut health = ureq::get(&format!("{}/api/metrics", base_url))
        .timeout(Duration::from_secs(5))
        .call();
    if health.is_err() {
        thread::sleep(Duration::from_millis(300));
        health = ureq::get(&format!("{}/api/metrics", base_url))
            .timeout(Duration::from_secs(5))
            .call();
    }
    assert!(health.is_ok(), "Server must remain functional after socket abuse: {:?}", health.as_ref().err());
    let resp = health.unwrap();
    assert_eq!(resp.status(), 200);
    let metrics: ProcessMetrics = resp.into_json().expect("Valid ProcessMetrics");
    assert!(metrics.memory.rss_bytes > 0);

    // 5.7 Fast shutdown assertion (< 500ms)
    let t_shutdown = Instant::now();
    token.cancel();
    let join_res = handle.join();
    let shutdown_dur = t_shutdown.elapsed();

    assert!(join_res.is_ok(), "Server thread must join without panic");
    assert!(
        shutdown_dur < Duration::from_millis(500),
        "Shutdown took too long: {:?}",
        shutdown_dur
    );

    // 5.8 Immediate port rebinding verification (zero socket leaks)
    let rebind = TcpListener::bind(format!("127.0.0.1:{}", port));
    assert!(
        rebind.is_ok(),
        "Port {} must be immediately free for rebinding after shutdown",
        port
    );
}

// ============================================================================
// CHALLENGE 6: OS HANDLE LIFECYCLE & NO RESOURCE LEAKS UNDER STRESS
// ============================================================================

#[test]
fn challenge_metrics_server_handle_lifecycle_stability() {
    let token = CancellationToken::new();
    let (port, handle) = server::spawn_server("127.0.0.1", 0, token.clone())
        .expect("Failed to bind ephemeral server");
    let base_url = format!("http://127.0.0.1:{}", port);

    // Warm up the server with 5 requests
    for _ in 0..5 {
        let _ = ureq::get(&format!("{}/api/metrics", base_url)).call();
    }

    // Capture initial handles if supported on platform
    let initial_handles = telemetry::capture_metrics(None).threads.process_handles;

    // Run 50 successive mixed requests
    for i in 0..50 {
        match i % 3 {
            0 => {
                let resp = ureq::get(&format!("{}/api/metrics", base_url)).call();
                assert!(resp.is_ok());
            }
            1 => {
                let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut writer = stream;
                writer.write_all(b"OPTIONS /api/metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
                let mut status = String::new();
                reader.read_line(&mut status).unwrap();
                assert!(status.contains("204 No Content"));
            }
            _ => {
                let _ = ureq::post(&format!("{}/api/metrics", base_url)).call();
            }
        }
    }

    // Allow worker threads to terminate and sockets to clear
    thread::sleep(Duration::from_millis(150));

    let final_handles = telemetry::capture_metrics(None).threads.process_handles;

    if let (Some(init), Some(fin)) = (initial_handles, final_handles) {
        println!("Handle count before: {}, after 50 mixed requests: {}", init, fin);
        // Ensure handle count did not explode (allow small runtime margin < 25)
        let delta = (fin as isize) - (init as isize);
        assert!(
            delta < 25,
            "Handle count grew excessively by {} (from {} to {}) indicating handle/socket leak",
            delta,
            init,
            fin
        );
    }

    token.cancel();
    let _ = handle.join();
}
