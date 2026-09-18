# Dead Ends Log

| Iteration | Approach Tried | Why It Failed | Files Touched |
|---|---|---|---|
| M1 Iter 1 | Arbitrary `thread::sleep(25ms)` before issuing cancellation | Raced with OS worker thread startup latency on Windows; task was cancelled while still Queued, returning None duration | `src/agent/tasks.rs` |
| M1 Iter 2 | Narrow wall-clock assertion thresholds (`total_elapsed < 600ms`, `dur < 180ms`, `elapsed >= 25ms`) | Windows OS thread creation latency and timer resolution jitter under parallel test execution routinely exceeded narrow millisecond bounds (e.g. 628ms to 2.2s for 40 threads) | `src/agent/tasks.rs` |
