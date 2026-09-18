# Project: ctrl-cli Next-Gen Modernization & Feature Development

## Architecture
- **Language & Runtime**: Pure Rust (edition 2021) without heavyweight async runtimes (`tokio`, `async-std`). Bounded memory and release binary footprint (< 10 MB target, currently ~3.10 MB). Thread-safe concurrency using `std::thread`, `std::sync::mpsc`, standard synchronization primitives (`Arc`, `Mutex`, `RwLock`, `Condvar`, `AtomicBool`, `AtomicUsize`), and synchronous `ureq 2.10`.
- **Dynamic Skill Auto-Discovery**: Automatic recursive scanning of workspace skill directories (`skills/`, `.ctrl/skills/`, and `prompts/`), parsing YAML frontmatter (`name`, `description`, `tools`), and exposing interactive slash commands (`/skills list`, `/skills info <name>`) and agent capabilities.
- **In-Process BM25 RAG Engine**: Zero-external-dependency, pure Rust Okapi BM25 ranking algorithm indexing Markdown documents in `data/knowledge/` (`company_policy.md`, `product_faqs.md`) with tokenization, stop-words, and scoring ($k_1=1.2, b=0.75$), exposed via `knowledge_search(query, top_k)` agent tool.
- **Destructive Command Guardrails**: Syntax-aware safety interceptor detecting high-risk commands (`rm -rf`, `del /s`, `git reset --hard`, `mkfs`, etc.). Prompts user confirmation `(y/N)` in interactive REPL mode, while enforcing safe dry-run rejection in silent background subagent execution.
- **Immutable Thread-Safe Audit Trail**: Append-only structured JSONL logging at `.ctrl/audit.log` recording every tool invocation timestamp, tool name, arguments, execution duration, and termination status across main and background subagent worker threads.
- **Real-Time Web Dashboard SSE Streaming**: Embedded HTTP server (`std::net::TcpListener`) extended with a Server-Sent Events (SSE) endpoint (`GET /api/events`) broadcasting live task status transitions, execution log chunks, and telemetry snapshots to browser clients via `mpsc::sync_channel` with periodic keep-alive pings.
- **Interactive Task Run API**: REST endpoint (`POST /api/tasks/run`) accepting prompt, skill, and model overrides to dispatch background agent tasks via `TaskManager::global().spawn_task_with_sink(...)`.
- **Windows Socket Host Reliability (Error 10053 Fix)**: Resolves `WSAECONNABORTED (10053)` during rapid connection bursts by properly ignoring `ErrorKind::ConnectionAborted` in the listener loop without starvation sleeps, avoiding unnecessary socket handle duplication (`try_clone`), and properly draining streams prior to close.
- **Structured Git Tools**: Native structured tools (`git_status`, `git_diff`, `git_commit`) executing Git commands directly via argument slices without shell interpolation or quote escaping vulnerabilities.
- **Multi-File Checkpoint & `/undo` Rollback**: Workspace snapshotting mechanism storing atomic turn snapshots in `.ctrl/checkpoints/<id>/` with `manifest.json`, enabling multi-file edits rollback via `/undo` and history listing via `/undo list`.
- **Terminal UX Syntax Highlighting & Completion Bell**: Pure Rust ANSI and Ratatui syntax highlighter for Markdown code blocks (Rust, Python, JS, Shell, JSON) in terminal output, and terminal bell (`\x07`) notification on task completion when configured (`ALERT_ON_TASK_DONE=true`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | YAML Frontmatter Skill Parsing | Parse `name`, `description`, `tools` from `skills/` and `.ctrl/skills/` | M1 | Survey R1 |
| 2 | Workspace Skill Discovery Engine | Recursive discovery across `skills/`, `.ctrl/skills/`, and `prompts/` | M1 | Survey R1 |
| 3 | `/skills list` Slash Command | Interactive CLI command listing available skills with descriptions | M1 | Survey R1 |
| 4 | `/skills info <name>` Slash Command | Detailed inspect CLI command showing metadata and allowed tools | M1 | Survey R1 |
| 5 | In-Process BM25 Ranking Algorithm | Pure Rust Okapi BM25 scoring with term frequency and IDF | M1 | Survey R1 |
| 6 | Knowledge Base Indexing | Tokenizing and indexing markdown documents in `data/knowledge/` | M1 | Survey R1 |
| 7 | `knowledge_search` Agent Tool | Structured agent tool returning ranked text snippets from knowledge base | M1 | Survey R1 |
| 8 | Destructive Shell Command Detector | Regex/syntax matcher for `rm -rf`, `del /s`, `git reset --hard`, `format` | M2 | Survey R2 |
| 9 | Interactive Confirmation Gate | Interactive `(y/N)` confirmation prompt in REPL for destructive commands | M2 | Survey R2 |
| 10| Background Safe Dry-Run Rejection | Safe dry-run rejection in background subagents without freezing | M2 | Survey R2 |
| 11| Append-Only Audit Logger | Thread-safe single-line JSONL logging to `.ctrl/audit.log` | M2 | Survey R2 |
| 12| Complete Tool Invocation Auditing | Logging timestamp, tool name, arguments, duration, and status | M2 | Survey R2 |
| 13| Web Dashboard SSE Stream (`GET /api/events`)| Server-Sent Events delivering live task transitions and logs | M3 | Survey R3 |
| 14| EventBroadcaster Mechanism | Thread-safe fan-out broadcaster using bounded `mpsc::sync_channel` | M3 | Survey R3 |
| 15| Interactive Task Run API (`POST /api/tasks/run`)| REST API endpoint initiating agent tasks from dashboard | M3 | Survey R3 |
| 16| Windows Socket 10053 Resilience | Fix accept loop handling of `ErrorKind::ConnectionAborted` | M3 | Survey R6 |
| 17| Clean Socket Teardown & Buffer Draining | Prevent RST aborts and socket handle leaks on Windows | M3 | Survey R6 |
| 18| Structured `git_status` Tool | Native Git status tool returning typed branch, staged, unstaged files | M4 | Survey R4 |
| 19| Structured `git_diff` Tool | Native Git diff tool returning typed patch/diff output | M4 | Survey R4 |
| 20| Structured `git_commit` Tool | Native Git commit tool safely passing commit messages | M4 | Survey R4 |
| 21| Multi-File Checkpoint Directory | Directory-based snapshotting at `.ctrl/checkpoints/<id>/` with manifest | M4 | Survey R4 |
| 22| `/undo` Multi-File Atomic Rollback | Reverting multi-file edits in a single turn to snapshot state | M4 | Survey R4 |
| 23| `/undo list` Checkpoint History | Listing available checkpoints with timestamps and affected files | M4 | Survey R4 |
| 24| Markdown Code Block ANSI Highlighting | Pure Rust token syntax highlighter for Rust, Python, JS, Shell, JSON | M5 | Survey R5 |
| 25| TUI Code Block Color Spans | Colored spans rendering in Ratatui chat view | M5 | Survey R5 |
| 26| Task Completion Audio/Bell Alert | Emitting `\x07` terminal bell on background task completion | M5 | Survey R5 |
| 27| 100% E2E Test Suite Pass (Tiers 1-4) | Comprehensive offline test suite validating all R1-R6 features | M6 | Survey / Dual Track |
| 28| Tier 5 Adversarial Coverage Hardening | White-box adversarial testing verifying edge cases and stress limits | M6 | Project Pattern |
| 29| Zero Clippy Warnings | Strict `cargo clippy --all-targets -- -D warnings` compliance | M6 | Acceptance |
| 30| Binary Footprint Compliance | Release binary remains < 10 MB on Windows | M6 | Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Skill Discovery & In-Process BM25 (R1) | `src/tools/skills.rs`, `src/tools/knowledge.rs`, `src/tools/mod.rs`, `src/main.rs` | none | PLANNED |
| 2 | Destructive Command Guardrails & Audit Trail (R2) | `src/tools/guardrails.rs`, `src/tools/audit.rs`, `src/tools/shell.rs`, `src/tools/mod.rs` | none | PLANNED |
| 3 | SSE Streaming, Run API & Socket 10053 Fix (R3 & R6) | `src/server.rs`, `src/agent/tasks.rs` | none | PLANNED |
| 4 | Structured Git Tools & /undo Checkpoint Rollback (R4) | `src/tools/git.rs`, `src/tools/mod.rs`, `src/agent/checkpoint.rs`, `src/main.rs` | M1, M2 | PLANNED |
| 5 | Terminal UX Syntax Highlighting & Completion Bell (R5) | `src/tui/highlight.rs`, `src/tui/ui.rs`, `src/main.rs`, `src/agent/tasks.rs` | M1, M3 | PLANNED |
| 6 | Final Acceptance: 100% E2E Pass & Hardening (All) | Full test verification, clippy clean, binary check, docs | M1-M5, E2E | PLANNED |

## Interface Contracts

### `src/tools/skills.rs`
- `pub struct SkillMetadata { pub name: String, pub description: String, pub tools: Vec<String>, pub prompt_template: String, pub path: PathBuf }`
- `pub fn parse_skill_frontmatter(content: &str) -> Option<(SkillMetadata, String)>`
- `pub fn discover_skills(workspace_root: &Path) -> Vec<SkillMetadata>`
- `pub fn get_skill_by_name(name: &str, workspace_root: &Path) -> Option<SkillMetadata>`

### `src/tools/knowledge.rs`
- `pub struct SearchResult { pub document: String, pub section: String, pub snippet: String, pub score: f64 }`
- `pub struct Bm25Index { ... }`
- `impl Bm25Index { pub fn build_from_dir(dir: &Path) -> Result<Self>; pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult>; }`
- Tool: `knowledge_search { query: String, top_k: Option<usize> } -> Result<String>`

### `src/tools/guardrails.rs`
- `pub fn is_destructive_command(cmd: &str) -> Option<&'static str>`
- Intercepts destructive patterns: `rm -rf`, `rmdir /s`, `del /s`, `git reset --hard`, `git clean -fd`, `format`, `mkfs`, `fdisk`.
- Interactive execution prompts `(y/N)`. Background/silent execution rejects with diagnostic error.

### `src/tools/audit.rs`
- `pub struct AuditRecord { pub timestamp: String, pub session_id: String, pub task_id: Option<String>, pub tool: String, pub parameters: serde_json::Value, pub status: String, pub duration_ms: u64 }`
- `pub fn log_tool_invocation(record: &AuditRecord) -> Result<()>`
- File location: `.ctrl/audit.log` (append-only JSON Lines).

### `src/server.rs`
- Route `GET /api/events`: SSE stream with headers `Content-Type: text/event-stream`, `Cache-Control: no-cache`, `Connection: keep-alive`. Emits `event: task_status\ndata: ...\n\n`, `event: task_log\ndata: ...\n\n`, keepalive `: ping\n\n`.
- Route `POST /api/tasks/run`: Body `{ "prompt": "...", "skill": "...", "model": "..." }`, triggers `TaskManager::global().spawn_task_with_sink(...)`, returns HTTP 200 `{ "id": task_id, "status": "queued" }`.
- Accept loop: Catch `ErrorKind::ConnectionAborted` without sleeping 20ms.

### `src/tools/git.rs`
- `pub fn tool_git_status(repo_path: Option<&str>) -> Result<GitStatusResult>`
- `pub fn tool_git_diff(repo_path: Option<&str>, staged: bool, file: Option<&str>) -> Result<String>`
- `pub fn tool_git_commit(message: &str, repo_path: Option<&str>, all: bool) -> Result<GitCommitResult>`
- Invoked via `std::process::Command::new("git").args(...)` without shell wrapper.

### `src/agent/checkpoint.rs`
- Directory structure: `.ctrl/checkpoints/<checkpoint_id>/`
- `manifest.json`: `{ "id": "cp-...", "timestamp": "...", "files": ["src/main.rs", ...] }`
- Backed up originals preserved in checkpoint folder.
- `pub fn create_checkpoint(files: &[PathBuf]) -> Result<String>`
- `pub fn rollback_checkpoint(checkpoint_id: Option<&str>) -> Result<RollbackReport>`
- `pub fn list_checkpoints() -> Result<Vec<CheckpointInfo>>`

### `src/tui/highlight.rs`
- `pub fn highlight_markdown_code_blocks_ansi(content: &str) -> String`
- `pub fn highlight_code_tokens_ansi(code: &str, language: &str) -> String`
- Lexical token highlighting for Rust, Python, JavaScript, Shell/Bash, JSON.

## Code Layout
- `ctrl-cli/src/tools/skills.rs`: Skill discovery & frontmatter parsing.
- `ctrl-cli/src/tools/knowledge.rs`: In-process BM25 indexer & scoring.
- `ctrl-cli/src/tools/guardrails.rs`: Destructive command pattern matching.
- `ctrl-cli/src/tools/audit.rs`: Thread-safe audit logger (`.ctrl/audit.log`).
- `ctrl-cli/src/tools/git.rs`: Native structured Git tools.
- `ctrl-cli/src/tools/shell.rs`: Shell execution with guardrail integration.
- `ctrl-cli/src/tools/mod.rs`: Tool registry & dispatch routing.
- `ctrl-cli/src/server.rs`: Embedded HTTP server (SSE streaming, run API, Windows socket fix).
- `ctrl-cli/src/agent/checkpoint.rs`: Multi-file checkpoint snapshotting & `/undo`.
- `ctrl-cli/src/agent/tasks.rs`: Task manager, background task bell alert on completion.
- `ctrl-cli/src/tui/highlight.rs`: ANSI & Ratatui code block syntax highlighting.
- `ctrl-cli/src/tui/ui.rs`: TUI chat rendering with syntax highlights.
- `ctrl-cli/src/main.rs`: CLI commands (`/skills list`, `/skills info`, `/undo list`, `/undo`).
- `ctrl-cli/tests/`: Unit and integration test suites.
