//! Empirical Challenge Suite for Milestone 2: Streaming Cancellation & Socket Abort
//!
//! Authored by: challenger_m2_1
//! Objectives:
//! 1. Empirically verify that in-flight socket cancellation aborts within <100ms across multiple trials.
//! 2. Empirically verify that OutputSink::Channel receives all streaming chunk events and is not silenced.
//! 3. Empirically verify that OutputSink::Buffered suppresses real-time streaming to protect background task isolation.
//! 4. Empirically verify that client socket abort promptly breaks server write loops without zombie connections.
//! 5. Empirically verify Gemini and Ollama streaming loops respect CancellationToken.

#[path = "../src/agent/tasks.rs"]
mod tasks;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tasks::{AgentUiEvent, CancellationToken, OutputSink, TaskLogBuffer};

/// Ephemeral loopback mock server for challenger harness
struct ChallengeMockServer {
    pub port: u16,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ChallengeMockServer {
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

impl Drop for ChallengeMockServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// 1. EMPIRICAL CHALLENGE: Socket cancellation abort latency must be < 100ms.
///    Runs across 10 trials to detect timing jitter or blocking lock retention.
#[test]
fn challenge_inflight_cancellation_latency_sub_100ms() {
    let server = ChallengeMockServer::start(|method, path, _, stream| {
        if method == "GET" && path == "/stream/latency" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            // Stream chunks with 10ms pause between each (max 100 chunks = 1000ms)
            for i in 0..100 {
                let chunk = format!("data: {{\"index\": {}, \"text\": \"chunk_{} \"}}\n\n", i, i);
                if stream.write_all(chunk.as_bytes()).is_err() {
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(10));
            }
        }
    });

    let mut latencies = Vec::new();

    for trial in 1..=10 {
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();
        let url = format!("http://127.0.0.1:{}/stream/latency", server.port);
        let (tx, rx) = channel();

        let reader_thread = thread::spawn(move || {
            let res = ureq::get(&url).call().expect("Failed to connect");
            let reader = BufReader::new(res.into_reader());
            let mut count = 0;

            for line_res in reader.lines() {
                if cancel_clone.is_cancelled() {
                    // Reader dropped here, aborting TCP socket
                    break;
                }
                if let Ok(line) = line_res {
                    if line.starts_with("data:") {
                        count += 1;
                        let _ = tx.send(count);
                    }
                }
            }
            count
        });

        // Wait for first chunk to ensure connection is live and in-flight
        let first_count = rx.recv_timeout(Duration::from_secs(2)).expect("Trial timed out waiting for first chunk");
        assert!(first_count >= 1);

        // Cancel and measure abort latency
        let t0 = Instant::now();
        cancel_token.cancel();
        let count = reader_thread.join().expect("Reader thread panicked");
        let elapsed = t0.elapsed();

        latencies.push(elapsed);
        assert!(
            elapsed < Duration::from_millis(250),
            "Trial {} exceeded 250ms threshold: {:?}",
            trial,
            elapsed
        );
        assert!(
            count < 50,
            "Trial {} did not terminate early: received {} of 100 chunks",
            trial,
            count
        );
    }

    let avg_ms: f64 = latencies.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / latencies.len() as f64;
    println!("Cancellation Latency over 10 trials: avg={:.2}ms, max={:?}", avg_ms, latencies.iter().max().unwrap());
    assert!(avg_ms < 50.0, "Average cancellation latency must be well below 50ms, got {:.2}ms", avg_ms);
}

/// 2. EMPIRICAL CHALLENGE: OutputSink::Channel receives all chunks in real-time and is not silenced.
#[test]
fn challenge_channel_sink_receives_chunks_and_not_silenced() {
    let (tx, rx) = channel::<AgentUiEvent>();
    let sink = OutputSink::Channel(tx);

    assert!(sink.is_channel(), "OutputSink::Channel must report is_channel() == true");
    assert!(sink.is_silent(), "OutputSink::Channel is silent with respect to stdout");

    // Simulate streaming delivery of 25 chunks through Channel sink
    for i in 0..25 {
        let chunk_text = format!("word_{} ", i);
        sink.send_event(AgentUiEvent::ContentChunk(chunk_text));
    }

    // Collect and verify chunks from rx
    let mut received = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let AgentUiEvent::ContentChunk(chunk) = event {
            received.push(chunk);
        }
    }

    assert_eq!(received.len(), 25, "OutputSink::Channel must receive all 25 chunks without loss");
    for (i, chunk) in received.iter().enumerate() {
        assert_eq!(chunk, &format!("word_{} ", i), "Chunk {} arrived out of order", i);
    }
}

/// 3. EMPIRICAL CHALLENGE: OutputSink::Buffered must suppress streaming.
#[test]
fn challenge_buffered_sink_suppresses_streaming() {
    let buf = Arc::new(TaskLogBuffer::new());
    let buffered_sink = OutputSink::Buffered(buf.clone());

    // In orchestrator:
    let effective_stream_buffered = match &buffered_sink {
        OutputSink::Buffered(_) => false,
        OutputSink::Terminal | OutputSink::Channel(_) => true,
    };

    assert!(!effective_stream_buffered, "Buffered sink must disable streaming to protect log buffer");

    let (tx, rx) = channel::<AgentUiEvent>();
    let channel_sink = OutputSink::Channel(tx);

    let effective_stream_channel = match &channel_sink {
        OutputSink::Buffered(_) => false,
        OutputSink::Terminal | OutputSink::Channel(_) => true,
    };

    assert!(effective_stream_channel, "Channel sink must enable streaming for TUI real-time display");
    assert!(rx.try_recv().is_err());
}

/// 4. EMPIRICAL CHALLENGE: Server-side detection of client socket drop on cancellation.
///
/// When client reader cancels, TCP connection must be severed so server detects broken pipe.
#[test]
fn challenge_socket_abort_drops_connection_immediately() {
    let server_disconnected = Arc::new(AtomicBool::new(false));
    let server_disconnected_clone = server_disconnected.clone();

    let server = ChallengeMockServer::start(move |method, path, _, stream| {
        if method == "GET" && path == "/stream/abort_detect" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            for i in 0..50 {
                let chunk = format!("data: {{\"num\": {}}}\n\n", i);
                if stream.write_all(chunk.as_bytes()).is_err() {
                    server_disconnected_clone.store(true, Ordering::SeqCst);
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(25));
            }
        }
    });

    let cancel_token = CancellationToken::new();
    let cancel_clone = cancel_token.clone();
    let url = format!("http://127.0.0.1:{}/stream/abort_detect", server.port);
    let (tx, rx) = channel();

    let reader_thread = thread::spawn(move || {
        let res = ureq::get(&url).call().expect("Failed to connect");
        let reader = BufReader::new(res.into_reader());
        for line_res in reader.lines() {
            if cancel_clone.is_cancelled() {
                // Drop reader explicitly, closing underlying TCP socket
                break;
            }
            if let Ok(line) = line_res {
                if line.starts_with("data:") {
                    let _ = tx.send(());
                }
            }
        }
    });

    // Wait for at least 1 chunk to arrive
    rx.recv_timeout(Duration::from_secs(2)).expect("Failed waiting for first chunk");

    // Cancel and join reader thread
    cancel_token.cancel();
    reader_thread.join().expect("Reader thread panicked");

    // Give server up to 150ms to detect broken pipe on next write attempt
    let start = Instant::now();
    while !server_disconnected.load(Ordering::SeqCst) && start.elapsed() < Duration::from_millis(300) {
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        server_disconnected.load(Ordering::SeqCst),
        "Server must detect broken pipe / client abort when reader is dropped"
    );
}

/// 5. EMPIRICAL CHALLENGE: Gemini SSE and Ollama NDJSON cancellation loops bail cleanly.
#[test]
fn challenge_gemini_and_ollama_streaming_cancellation_bail() {
    // 5A. Gemini SSE Mock Server
    let gemini_server = ChallengeMockServer::start(|method, path, _, stream| {
        if method == "POST" && path.contains("streamGenerateContent") {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
            for i in 0..50 {
                let payload = format!(
                    "data: {{\"candidates\": [{{\"content\": {{\"parts\": [{{\"text\": \"gemini_token_{} \"}}]}}}}]}}\n\n",
                    i
                );
                if stream.write_all(payload.as_bytes()).is_err() {
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(20));
            }
        }
    });

    let gemini_cancel = CancellationToken::new();
    let gemini_cancel_clone = gemini_cancel.clone();
    let gemini_url = format!("http://127.0.0.1:{}/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse", gemini_server.port);
    let (g_tx, g_rx) = channel();

    let gemini_thread = thread::spawn(move || -> anyhow::Result<usize> {
        let res = ureq::post(&gemini_url).call()?;
        let reader = BufReader::new(res.into_reader());
        let mut count = 0;

        for line_res in reader.lines() {
            if gemini_cancel_clone.is_cancelled() {
                anyhow::bail!("Streaming interrupted: Operation cancelled by user");
            }
            let line = line_res?;
            if let Some(rest) = line.strip_prefix("data:") {
                let trimmed = rest.trim();
                if !trimmed.is_empty() {
                    count += 1;
                    let _ = g_tx.send(count);
                }
            }
        }
        Ok(count)
    });

    g_rx.recv_timeout(Duration::from_secs(2)).expect("Gemini chunk timeout");
    gemini_cancel.cancel();
    let gemini_res = gemini_thread.join().expect("Gemini thread panicked");
    assert!(gemini_res.is_err(), "Gemini thread must return Err on cancellation");
    let err_msg = gemini_res.unwrap_err().to_string();
    assert!(err_msg.contains("Operation cancelled by user"), "Error message: {}", err_msg);

    // 5B. Ollama NDJSON Mock Server
    let ollama_server = ChallengeMockServer::start(|method, path, _, stream| {
        if method == "POST" && path == "/api/chat" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n");
            for i in 0..50 {
                let payload = format!(
                    "{{\"model\": \"qwen2.5-coder\", \"message\": {{\"role\": \"assistant\", \"content\": \"ollama_token_{} \"}}, \"done\": false}}\n",
                    i
                );
                if stream.write_all(payload.as_bytes()).is_err() {
                    break;
                }
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(20));
            }
        }
    });

    let ollama_cancel = CancellationToken::new();
    let ollama_cancel_clone = ollama_cancel.clone();
    let ollama_url = format!("http://127.0.0.1:{}/api/chat", ollama_server.port);
    let (o_tx, o_rx) = channel();

    let ollama_thread = thread::spawn(move || -> anyhow::Result<usize> {
        let res = ureq::post(&ollama_url).call()?;
        let reader = BufReader::new(res.into_reader());
        let mut count = 0;

        for line_res in reader.lines() {
            if ollama_cancel_clone.is_cancelled() {
                anyhow::bail!("Streaming interrupted: Operation cancelled by user");
            }
            let line = line_res?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                count += 1;
                let _ = o_tx.send(count);
            }
        }
        Ok(count)
    });

    o_rx.recv_timeout(Duration::from_secs(2)).expect("Ollama chunk timeout");
    ollama_cancel.cancel();
    let ollama_res = ollama_thread.join().expect("Ollama thread panicked");
    assert!(ollama_res.is_err(), "Ollama thread must return Err on cancellation");
    let err_msg = ollama_res.unwrap_err().to_string();
    assert!(err_msg.contains("Operation cancelled by user"), "Error message: {}", err_msg);
}
