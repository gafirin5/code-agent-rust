pub mod filesystem;
pub mod interaction;
pub mod mcp;
pub mod result_store;
pub mod search;
pub mod self_heal;
pub mod shell;
pub mod skills;
#[cfg(test)]
mod tests;
pub mod web;

use crate::types::{ChatCompletionTool, FunctionDefinition};
use anyhow::{Context, Result};
use serde_json::json;

pub fn is_mutating_tool(name: &str) -> bool {
    matches!(name, "write_file" | "edit_file" | "shell")
}

pub fn get_available_tools() -> Vec<ChatCompletionTool> {
    let mut tools = vec![
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "read_file".to_string(),
                description: "Read content of a file from the workspace. Supports specifying start_line and end_line (1-indexed) to read specific line ranges.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read (relative to workspace or absolute)."
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "Optional start line number (1-indexed, inclusive)."
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "Optional end line number (1-indexed, inclusive)."
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "write_file".to_string(),
                description: "Write content to a file. Automatically creates any necessary parent directories.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write."
                        },
                        "content": {
                            "type": "string",
                            "description": "The exact full text content to write into the file."
                        },
                        "overwrite": {
                            "type": "boolean",
                            "description": "Whether to overwrite the file if it already exists (default: true)."
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "edit_file".to_string(),
                description: "Edit an existing file by replacing a unique chunk (target_content) with new content (replacement_content). Avoids rewriting entire files.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit."
                        },
                        "target_content": {
                            "type": "string",
                            "description": "The exact substring currently in the file that must be replaced. Include unique surrounding lines if needed."
                        },
                        "replacement_content": {
                            "type": "string",
                            "description": "The replacement content to insert in place of target_content."
                        },
                        "allow_multiple": {
                            "type": "boolean",
                            "description": "Whether to replace multiple occurrences (default: false)."
                        }
                    },
                    "required": ["path", "target_content", "replacement_content"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "glob_files".to_string(),
                description: "Find file paths matching a glob pattern (e.g. '*.rs', 'src/**/*.ts'). Supports mode='count' for quick counts.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern to search for (e.g. '*.rs', '*.toml', 'src/*')."
                        },
                        "path": {
                            "type": "string",
                            "description": "Root directory to search within (default: '.')."
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["list", "count"],
                            "description": "Mode of output: 'list' (default) or 'count'."
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "grep_files".to_string(),
                description: "Search text files for a literal substring across the workspace. Returns matching lines with line numbers and optional context.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Literal substring to search for."
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search within (default: '.')."
                        },
                        "include": {
                            "type": "string",
                            "description": "File filter pattern, e.g. '*.rs' or '*.json'."
                        },
                        "case_insensitive": {
                            "type": "boolean",
                            "description": "Case insensitive search (default: false)."
                        },
                        "head_limit": {
                            "type": "integer",
                            "description": "Maximum number of matches to return (default: 50)."
                        },
                        "offset": {
                            "type": "integer",
                            "description": "1-indexed offset of matches to skip (default: 1)."
                        },
                        "context_lines": {
                            "type": "integer",
                            "description": "Number of context lines before and after each match."
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "shell".to_string(),
                description: "Execute a command in the local shell/terminal and return its exit code, stdout, and stderr.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The exact shell command to execute."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 60)."
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "read_tool_result".to_string(),
                description: "Paginate and view more lines of an oversized tool result that was previously truncated.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "result_id": {
                            "type": "string",
                            "description": "The result ID returned when the output was truncated (e.g. 'tr_1')."
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Starting line number (1-indexed) to read from."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Number of lines to read."
                        }
                    },
                    "required": ["result_id", "offset", "limit"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "ask_user_question".to_string(),
                description: "Ask the user an interactive question in the terminal when clarification or decisions are needed.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question prompt to present to the user."
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of predefined choices for the user to select from."
                        }
                    },
                    "required": ["question"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "skill".to_string(),
                description: "Load workflow instructions for a specialized skill (e.g. from local SKILL.md).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the skill to load (e.g. 'code-reviewer', 'rust-expert')."
                        }
                    },
                    "required": ["name"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "manage_memory".to_string(),
                description: "Read, append, or update important notes, architectural decisions, and file paths in the workspace persistent memory (.ctrl/MEMORY.md). Use this to remember context across turns.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["read", "append", "set"],
                            "description": "Action to perform: 'read', 'append' (add a note), or 'set' (overwrite)."
                        },
                        "content": {
                            "type": "string",
                            "description": "The note or information to save into memory."
                        }
                    },
                    "required": ["action"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "code_check".to_string(),
                description: "Run compiler or syntax diagnostic checks (e.g. cargo check, python syntax, tsc) on a specific file or the whole workspace.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Optional file path or directory to check (defaults to checking workspace project)."
                        }
                    }
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "web_fetch".to_string(),
                description: "Fetch web content from an HTTP(S) URL and convert HTML into clean readable Markdown text.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The full HTTP/HTTPS URL to fetch."
                        }
                    },
                    "required": ["url"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "web_search".to_string(),
                description: "Search the web (via DuckDuckGo) for documentation, programming solutions, and technical references.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search keywords or question."
                        },
                        "num_results": {
                            "type": "integer",
                            "description": "Number of search results to return (default: 5, max: 10)."
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "subagent".to_string(),
                description: "Delegate an isolated research or execution sub-task to an autonomous subagent. Set background=true to run non-blocking and get a task_id for tracking.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "The specific mission or investigation prompt for the subagent."
                        },
                        "skill": {
                            "type": "string",
                            "description": "Optional skill specialization to assign (e.g. 'rust-expert', 'code-reviewer', 'debugger')."
                        },
                        "model": {
                            "type": "string",
                            "description": "Optional model override for the subagent."
                        },
                        "max_turns": {
                            "type": "integer",
                            "description": "Maximum tool-call turns for the subagent (default: 8)."
                        },
                        "background": {
                            "type": "boolean",
                            "description": "If true, run the subagent in the background and return a task_id immediately without blocking (default: false)."
                        },
                        "dependencies": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of prerequisite task IDs that must complete before this subagent executes."
                        }
                    },
                    "required": ["task"]
                }),
            },
        },
        ChatCompletionTool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "manage_task".to_string(),
                description: "Inspect, await, cancel, or retrieve logs for background tasks spawned by subagent(background=true). Actions: 'list' (all tasks), 'status' (single task with recent logs), 'await' (wait for completion), 'cancel' (stop a running task), 'logs' (get captured task log output).".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "status", "await", "cancel", "logs"],
                            "description": "Action to perform on background tasks."
                        },
                        "task_id": {
                            "type": "string",
                            "description": "The task ID to query/await/cancel/fetch logs (required for 'status', 'await', 'cancel', 'logs')."
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional timeout in seconds for 'await' action (default: no timeout)."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Optional maximum number of log lines to retrieve for 'logs' action (default: 100)."
                        }
                    },
                    "required": ["action"]
                }),
            },
        },
    ];

    tools.extend(mcp::McpManager::get_all_tools());
    tools
}

pub fn dispatch_tool(name: &str, arguments_json: &str) -> Result<String> {
    let args: serde_json::Value = serde_json::from_str(arguments_json).with_context(|| {
        format!(
            "Invalid JSON arguments for tool '{}': {}",
            name, arguments_json
        )
    })?;

    match name {
        "read_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let start = args["start_line"].as_u64().map(|v| v as usize);
            let end = args["end_line"].as_u64().map(|v| v as usize);
            filesystem::read_file(path, start, end)
        }
        "write_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let content = args["content"]
                .as_str()
                .context("Missing 'content' argument")?;
            let overwrite = args["overwrite"].as_bool();
            filesystem::write_file(path, content, overwrite)
        }
        "edit_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let target = args["target_content"]
                .as_str()
                .context("Missing 'target_content' argument")?;
            let replacement = args["replacement_content"]
                .as_str()
                .context("Missing 'replacement_content' argument")?;
            let allow_multiple = args["allow_multiple"].as_bool();
            filesystem::edit_file(path, target, replacement, allow_multiple)
        }
        "glob_files" => {
            let pattern = args["pattern"]
                .as_str()
                .context("Missing 'pattern' argument")?;
            let path = args["path"].as_str();
            let mode = args["mode"].as_str();
            search::glob_files(pattern, path, mode)
        }
        "grep_files" => {
            let query = args["query"].as_str().context("Missing 'query' argument")?;
            let path = args["path"].as_str();
            let include = args["include"].as_str();
            let ci = args["case_insensitive"].as_bool();
            let limit = args["head_limit"].as_u64().map(|v| v as usize);
            let offset = args["offset"].as_u64().map(|v| v as usize);
            let ctx = args["context_lines"].as_u64().map(|v| v as usize);
            search::grep_files(query, path, include, ci, limit, offset, ctx)
        }
        "shell" => {
            let cmd = args["command"]
                .as_str()
                .context("Missing 'command' argument")?;
            let timeout = args["timeout_secs"].as_u64();
            shell::execute_shell(cmd, timeout)
        }
        "read_tool_result" => {
            let id = args["result_id"]
                .as_str()
                .context("Missing 'result_id' argument")?;
            let offset = args["offset"]
                .as_u64()
                .context("Missing 'offset' argument")? as usize;
            let limit = args["limit"].as_u64().context("Missing 'limit' argument")? as usize;
            result_store::ResultStore::read_result(id, offset, limit)
                .map_err(|e| anyhow::anyhow!(e))
        }
        "ask_user_question" => {
            let question = args["question"]
                .as_str()
                .context("Missing 'question' argument")?;
            let options = args["options"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });
            interaction::ask_user_question(question, options)
        }
        "skill" => {
            let name = args["name"].as_str().context("Missing 'name' argument")?;
            skills::load_skill_content(name)
        }
        "manage_memory" => {
            let action = args["action"]
                .as_str()
                .context("Missing 'action' argument")?;
            let content = args["content"].as_str().unwrap_or("");
            crate::agent::memory::MemoryManager::save_long_term_memory(action, content)
        }
        "code_check" => {
            let target = args["target"].as_str();
            self_heal::run_code_check(target)
        }
        "web_fetch" => {
            let url = args["url"].as_str().context("Missing 'url' argument")?;
            web::web_fetch(url)
        }
        "web_search" => {
            let query = args["query"].as_str().context("Missing 'query' argument")?;
            let num = args["num_results"].as_u64().map(|n| n as usize);
            web::web_search(query, num)
        }
        "subagent" => {
            let task = args["task"].as_str().context("Missing 'task' argument")?;
            let skill = args["skill"].as_str();
            let model = args["model"].as_str();
            let max_turns = args["max_turns"].as_u64().map(|n| n as usize);
            let background = args["background"].as_bool().unwrap_or(false);
            let dependencies: Option<Vec<String>> = args["dependencies"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });
            let api_key = std::env::var("AI_API_KEY").unwrap_or_default();
            let base_url = std::env::var("AI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            let default_model =
                std::env::var("AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
            if background {
                crate::agent::subagent::run_subagent_background_with_dependencies(
                    task,
                    skill,
                    model,
                    max_turns,
                    &api_key,
                    &base_url,
                    &default_model,
                    dependencies,
                )
            } else {
                crate::agent::subagent::run_subagent(
                    task,
                    skill,
                    model,
                    max_turns,
                    &api_key,
                    &base_url,
                    &default_model,
                )
            }
        }
        "manage_task" => {
            let action = args["action"]
                .as_str()
                .context("Missing 'action' argument")?;
            let task_id = args["task_id"].as_str();
            let timeout_secs = args["timeout_secs"].as_u64();
            let limit = args["limit"].as_u64().map(|n| n as usize);
            dispatch_manage_task(action, task_id, timeout_secs, limit)
        }
        _ => {
            if mcp::McpManager::is_mcp_tool(name) {
                mcp::McpManager::dispatch(name, arguments_json)
            } else {
                anyhow::bail!("Unknown tool: '{}'", name)
            }
        }
    }
}

/// Dispatches manage_task actions against the global TaskManager.
fn dispatch_manage_task(
    action: &str,
    task_id: Option<&str>,
    timeout_secs: Option<u64>,
    limit: Option<usize>,
) -> Result<String> {
    use crate::agent::tasks::TaskManager;
    let tm = TaskManager::global();

    match action {
        "list" => {
            let snapshots = tm.list_tasks();
            if snapshots.is_empty() {
                return Ok("No background tasks found.".to_string());
            }
            let mut out = format!("Background Tasks ({} total):\n\n", snapshots.len());
            for snap in &snapshots {
                out.push_str(&format!(
                    "  {} {} — {} ({})\n",
                    snap.status.badge(),
                    snap.id,
                    snap.name,
                    snap.elapsed_human
                ));
                if !snap.dependencies.is_empty() {
                    out.push_str(&format!("      Prerequisites: {}\n", snap.dependencies.join(", ")));
                }
                if !snap.description.is_empty() {
                    out.push_str(&format!("      {}\n", snap.description));
                }
            }
            Ok(out)
        }
        "status" => {
            let id = task_id.context("'task_id' is required for 'status' action")?;
            match tm.get_task(id) {
                Some(snap) => {
                    let mut out = format!(
                        "Task: {}\nName: {}\nStatus: {}\nElapsed: {}\nCreated: {}\n",
                        snap.id,
                        snap.name,
                        snap.status.as_str(),
                        snap.elapsed_human,
                        snap.created_at
                    );
                    if !snap.dependencies.is_empty() {
                        out.push_str(&format!("Prerequisites: {}\n", snap.dependencies.join(", ")));
                    }
                    if let Some(ref started) = snap.started_at {
                        out.push_str(&format!("Started: {}\n", started));
                    }
                    if let Some(ref finished) = snap.finished_at {
                        out.push_str(&format!("Finished: {}\n", finished));
                    }
                    if let Some(ref result) = snap.result {
                        let preview = if result.len() > 500 {
                            format!(
                                "{}...\n(truncated, {} chars total)",
                                &result[..497],
                                result.len()
                            )
                        } else {
                            result.clone()
                        };
                        out.push_str(&format!("\nResult:\n{}\n", preview));
                    }
                    if let Some(ref error) = snap.error {
                        out.push_str(&format!("\nError: {}\n", error));
                    }

                    // Retrieve captured logs from TaskLogBuffer
                    if let Some(logs) = tm.get_task_logs(id) {
                        if !logs.is_empty() {
                            let total = logs.len();
                            let max_lines = 15;
                            let start = total.saturating_sub(max_lines);
                            out.push_str(&format!(
                                "\nRecent Logs (showing {} of {} lines):\n",
                                total - start,
                                total
                            ));
                            for line in &logs[start..] {
                                out.push_str(&format!("  {}\n", line));
                            }
                            if start > 0 {
                                out.push_str(&format!("  ... ({} earlier lines omitted)\n", start));
                            }
                        } else {
                            out.push_str("\nLogs: (none recorded)\n");
                        }
                    }

                    Ok(out)
                }
                None => Ok(format!("Task '{}' not found.", id)),
            }
        }
        "logs" => {
            let id = task_id.context("'task_id' is required for 'logs' action")?;
            match tm.get_task(id) {
                Some(snap) => match tm.get_task_logs(id) {
                    Some(logs) => {
                        if logs.is_empty() {
                            Ok(format!(
                                "Task '{}' has no recorded logs (status: {}).",
                                id,
                                snap.status.as_str()
                            ))
                        } else {
                            let max_lines = limit.unwrap_or(100);
                            let total = logs.len();
                            let start = total.saturating_sub(max_lines);
                            let mut out = format!(
                                "Task '{}' Logs (showing {} of {} lines, status: {}):\n",
                                id,
                                total - start,
                                total,
                                snap.status.as_str()
                            );
                            for line in &logs[start..] {
                                out.push_str(&format!("  {}\n", line));
                            }
                            if start > 0 {
                                out.push_str(&format!("  ... ({} earlier lines omitted)\n", start));
                            }
                            Ok(out)
                        }
                    }
                    None => Ok(format!("No log buffer found for task '{}'.", id)),
                },
                None => Ok(format!("Task '{}' not found.", id)),
            }
        }
        "await" => {
            let id = task_id.context("'task_id' is required for 'await' action")?;
            let timeout = timeout_secs.map(std::time::Duration::from_secs);
            match tm.await_task(id, timeout) {
                Ok(snap) => {
                    let mut out = format!(
                        "Task {} finished.\nStatus: {}\nElapsed: {}\n",
                        snap.id,
                        snap.status.as_str(),
                        snap.elapsed_human
                    );
                    if let Some(ref result) = snap.result {
                        out.push_str(&format!("\nResult:\n{}\n", result));
                    }
                    if let Some(ref error) = snap.error {
                        out.push_str(&format!("\nError: {}\n", error));
                    }
                    Ok(out)
                }
                Err(e) => Ok(format!("Await failed: {}", e)),
            }
        }
        "cancel" => {
            let id = task_id.context("'task_id' is required for 'cancel' action")?;
            match tm.cancel_task(id) {
                Ok(()) => Ok(format!("Task '{}' has been cancelled.", id)),
                Err(e) => Ok(format!("Cancel failed: {}", e)),
            }
        }
        _ => anyhow::bail!(
            "Unknown manage_task action: '{}'. Valid: list, status, await, cancel, logs",
            action
        ),
    }
}
