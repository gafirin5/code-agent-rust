# Test Infrastructure Architecture: `ctrl-cli` Modernization & Next-Gen Features (v0.3.0)

## 1. Executive Summary & Design Principles

The `ctrl-cli` testing infrastructure ensures rigorous validation of the autonomous coding agent while preserving its core architectural tenets:
- **Pure Rust Runtime**: Zero reliance on heavyweight asynchronous runtimes (`tokio`, `async-std`). Networking, threading, and streaming are managed via `std::thread`, standard synchronization primitives (`Arc`, `Mutex`, `RwLock`, `Condvar`, `AtomicBool`, `AtomicUsize`), and synchronous `ureq 2.10`.
- **100% Offline Determinism**: Every integration and E2E test runs completely hermetic and offline. Network interactions (Gemini AI Studio, Ollama, HTTP dashboard, SSE stream) are intercepted by local ephemeral mock servers bound to dynamic loopback ports (`127.0.0.1:0`), ensuring zero flake, zero external API key requirements, and zero network latency variability.
- **Strict Isolation & Concurrency Safety**: Subagent outputs, task persistence, filesystem access, and REPL/TUI state transitions are verified for thread-safety, race-freedom, deadlock-freedom, and output isolation.
- **Sandboxed Security & Guardrails**: Comprehensive boundary verification enforcing workspace root confinement, rejection of traversal attacks, interactive confirmation gates, and safe dry-run rejections for destructive operations.
- **Immutable Auditability**: Append-only structured JSON Lines logging capturing every tool call, parameters, duration, and status.

---

## 2. The 4-Tier Test Architecture

The `ctrl-cli` quality framework is organized into four complementary testing tiers:

```
+-------------------------------------------------------------------------+
|                  Tier 4: Real-World Application Scenarios               |
|      Full E2E workflows, lifecycle transitions, web dashboard sync      |
+-------------------------------------------------------------------------+
                                    ^
+-------------------------------------------------------------------------+
|                 Tier 3: Cross-Feature Combinations                      |
|      DAG + Persistence + Streaming + Skills + Guardrails + Checkpoints  |
+-------------------------------------------------------------------------+
                                    ^
+-------------------------------------------------------------------------+
|                 Tier 2: Boundary & Corner Conditions                    |
|      Zero timeouts, cycle detection, path traversals, socket bursts     |
+-------------------------------------------------------------------------+
                                    ^
+-------------------------------------------------------------------------+
|                 Tier 1: Feature & Contract Coverage                     |
|      CLI versioning, Wire protocols, BM25, Guardrails, Git Tools, SSE   |
+-------------------------------------------------------------------------+
```

---

### Tier 1: Feature Coverage (Unit & Functional Validation)

Validates discrete correctness and interface contracts of each subsystem:

1. **CLI Version Alignment & Command Dispatch**:
   - `Cargo.toml` specifies `version = "0.3.0"`.
   - CLI executable `--version` and `-V` flags emit `ctrl-cli 0.3.0`.
   - CLI `--help` exposes subcommands (`--cli`, `--tui`, `serve`, `--web`).
2. **AI Provider Wire Protocols (`ApiProtocol`)**:
   - **Gemini REST v1beta**: Request payload formatting (`contents` array, `role`, `parts`), header validation (`x-goog-api-key`), and SSE streaming response parsing (`candidates[].content.parts[].text`).
   - **Ollama Native**: Request formatting (`model`, `messages`, `stream`), NDJSON stream parsing (`message.content`, `done: bool`), and model discovery (`GET /api/tags`).
3. **Dynamic Skill Auto-Discovery & YAML Frontmatter (R1)**:
   - Scanning `skills/`, `.ctrl/skills/`, and `prompts/`.
   - YAML frontmatter parsing (`name`, `description`, `tools`, prompt template body).
   - Slash command contracts: `/skills list` and `/skills info <name>`.
4. **In-Process BM25 Knowledge Base Indexing (R1)**:
   - Pure Rust Okapi BM25 ranking algorithm ($k_1=1.2, b=0.75$) with term frequency and IDF scoring.
   - Relevance retrieval across markdown files in `data/knowledge/` (`company_policy.md`, `product_faqs.md`).
   - Structured `knowledge_search` tool execution returning ranked snippets.
5. **Destructive Command Guardrails & Audit Trail (R2)**:
   - Regex and syntax matcher intercepting high-risk operations (`rm -rf`, `del /s`, `rmdir /s`, `git reset --hard`, `git clean -fd`, `format`, `mkfs`, `fdisk`).
   - Interactive `(y/N)` confirmation in interactive mode.
   - Immediate safe dry-run rejection in silent background subagent execution.
   - Append-only structured JSONL logging to `.ctrl/audit.log` capturing timestamp, session_id, task_id, tool, parameters, status, and duration_ms.
6. **Real-Time Web Dashboard SSE Streaming & Run API (R3)**:
   - `GET /api/events` delivering SSE stream (`text/event-stream`) for live `task_status`, `task_log`, and keep-alive `: ping` comments.
   - `POST /api/tasks/run` accepting prompt, skill, model overrides and queueing background tasks.
7. **Structured Git Tools & Checkpoint Rollback (R4)**:
   - Native `git_status`, `git_diff`, `git_commit` tools executing via argument slices without shell interpolation or quote escaping bugs.
   - Checkpoint snapshotting to `.ctrl/checkpoints/<id>/` with `manifest.json`.
   - Atomic `/undo` rollback restoring pre-modification file states.
   - Checkpoint history listing via `/undo list`.
8. **Terminal UX & Completion Alert (R5)**:
   - Pure Rust lexical token syntax highlighter for Markdown code blocks (Rust, Python, JS, Shell, JSON) producing ANSI escape codes.
   - Terminal bell (`\x07`) notification triggered upon task completion when configured (`ALERT_ON_TASK_DONE=true`).
9. **Windows Host Socket Reliability (R6)**:
   - Listener accept loop gracefully ignores `ErrorKind::ConnectionAborted` (`WSAECONNABORTED (10053)`) without starvation sleeps.

---

### Tier 2: Boundary & Corner Conditions (Adversarial & Stress)

Ensures resilience under extreme inputs, invalid configurations, and hardware contention:

1. **Path Traversal & Sandboxing Attacks**:
   - Relative parent traversal sequences: `../`, `../../`, `subdir/../../outside.txt`.
   - Absolute paths targeting outside workspace: `C:\Windows\System32`, `/etc/shadow`.
   - Windows UNC prefix normalization (`\\?\C:\...`).
2. **DAG Dependency Validation**:
   - Direct cycles (`A <-> B`), self-cycles (`A -> A`), 3-node transitive cycles (`A -> B -> C -> A`), and dangling dependencies (`A -> Z`).
3. **In-Flight Stream Cancellation & Socket Abort**:
   - Immediate socket closure when `CancellationToken` transitions to cancelled.
   - Verifying client drops connection in <100ms without blocking.
4. **Skill Discovery & BM25 Edge Cases**:
   - Empty skill directories, malformed YAML frontmatter, missing `name` attribute.
   - Empty search queries, queries with stop-words only, non-matching terms, `top_k = 0`, and `top_k` exceeding total corpus size.
5. **Guardrail Edge Cases & Lookalike Filtering**:
   - Safe lookalike commands containing destructive substrings (e.g. `echo rm -rf`, `git reset HEAD file.txt`, `grep 'del /s'`) are NOT blocked.
   - Chained commands (`cargo check && rm -rf target`, `echo hello; del /s /q temp`) ARE detected and intercepted.
6. **Multithreaded Audit Log Stress**:
   - High concurrency (20–50 simultaneous threads) logging audit records with special characters, quotes, and unicode.
7. **Git Injection Prevention & Checkpoint Edge Cases**:
   - Commit messages containing shell injection payloads (`test"; rm -rf / ;`) passed safely via argument slices.
   - `/undo` executed when no checkpoints exist, and rollback of deleted or newly added files.
8. **Windows Socket Rapid Burst Stress (Error 10053)**:
   - 50 rapid connect-and-abort iterations on embedded server followed by health verification in <500ms.

---

### Tier 3: Cross-Feature Combinations (Subsystem Interactions)

Verifies that disparate subsystems collaborate cleanly without side-effects:

1. **DAG Dependency Cascades**:
   - Diamond graph execution with cascading failure and cancellation.
2. **Subagent + Skill + Guardrail + Audit Flow**:
   - Subagent executing a designated skill attempts a destructive cleanup command; guardrail intercepts in silent background mode, rejects with dry run error, and logs record with `status: "blocked"` to `.ctrl/audit.log`.
3. **Checkpoint + Mutating Tool + Undo + Audit Flow**:
   - Checkpoint manager snapshots workspace files before tool execution; tool mutates files; validation fails; agent triggers `/undo`; workspace is restored and all events are recorded in audit log.
4. **Task Run + SSE Streaming + Completion Bell Alert**:
   - Task dispatched via `POST /api/tasks/run`; real-time transitions streamed via `GET /api/events`; task completion emits terminal bell `\x07`.
5. **Knowledge Search + Git Commit Pipeline**:
   - In-process BM25 queries technical guidelines; agent generates compliant code; structured `git_commit` commits changes without shell escaping; audit trail captures both invocations.

---

### Tier 4: Real-World Application Scenarios (End-to-End Workflows)

Replicates complete user and agent workflows end-to-end:

1. **Autonomous Development Pipeline**:
   - CLI boots, probes providers, streams generation, executes DAG tasks, persists snapshots, and serves web dashboard.
2. **Full Autonomous Next-Gen Agent Workflow**:
   - Discovers skills dynamically (`skills/researcher`).
   - Retrieves guidelines from `data/knowledge/` via BM25 RAG.
   - Takes multi-file checkpoint snapshot before editing.
   - Intercepts and blocks unsafe destructive command.
   - Performs compliant edit with ANSI syntax-highlighted code blocks.
   - Commits changes safely via structured Git tool.
   - Reverts workspace to snapshot via `/undo`.
   - Streams status over SSE and triggers terminal completion alert.
   - Verifies sequential, immutable `.ctrl/audit.log` trail.

---

## 3. Test Suite Inventory

| Test Binary / Module | Primary Focus | Tier Coverage | Test Count Target | Status |
|---|---|---|:---:|:---:|
| `ctrl-cli/tests/e2e_nextgen_features.rs` | R1-R6 Next-Gen Features (Skills, BM25, Guardrails, Audit, SSE, Git, Undo, Bell, Sockets) | Tier 1, 2, 3, 4 | 32 | **100% PASS** |
| `ctrl-cli/tests/e2e_modernization.rs` | CLI 0.3.0, Gemini/Ollama SSE/NDJSON, DAG, Persistence, Web, Sandbox | Tier 1, 2, 3, 4 | 13 | **100% PASS** |
| `ctrl-cli/tests/boundary_stress.rs` | Zero-durations, rapid cancellations, lock recovery | Tier 2 | 58 | **100% PASS** |
| `ctrl-cli/tests/concurrency_stress.rs` | 100 concurrent tasks, peak parallelism, stampedes | Tier 2, 3 | 54 | **100% PASS** |
| `ctrl-cli/tests/output_isolation_stress.rs` | Subprocess silence, zero escape codes, 100k log lines | Tier 1, 2 | 58 | **100% PASS** |
| `ctrl-cli/tests/ux_notification_concurrency_stress.rs` | Deduplication, inter-turn prompt drain, TTY parity | Tier 1, 3 | 54 | **100% PASS** |
| `ctrl-cli` Unit Tests (`src/**`) | Model limits, token compaction, tool dispatch, inquire completers | Tier 1 | 96 | **100% PASS** |
| **Total Automated Suite** | **Comprehensive Full-Spectrum Coverage** | **All Tiers** | **365** | **100% PASS** |

---

## 4. Execution Commands & Quality Gates

The test suite is enforced by strict automated verification commands:

```bash
# 1. Run all unit and integration test suites
cargo test

# 2. Run specifically the Next-Gen Features E2E integration test suite
cargo test --test e2e_nextgen_features -- --nocapture

# 3. Run the Modernization E2E integration test suite
cargo test --test e2e_modernization -- --nocapture

# 4. Verify zero clippy linter warnings across all targets and tests
cargo clippy --all-targets -- -D warnings

# 5. Check release binary build footprint (< 10 MB target)
cargo build --release
```
