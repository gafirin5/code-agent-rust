pub mod filesystem;
pub mod interaction;
pub mod result_store;
pub mod search;
pub mod shell;
pub mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde_json::json;
use crate::types::{ChatCompletionTool, FunctionDefinition};

pub fn is_mutating_tool(name: &str) -> bool {
    matches!(name, "write_file" | "edit_file" | "shell")
}

pub fn get_available_tools() -> Vec<ChatCompletionTool> {
    vec![
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
    ]
}

pub fn dispatch_tool(name: &str, arguments_json: &str) -> Result<String> {
    let args: serde_json::Value = serde_json::from_str(arguments_json)
        .with_context(|| format!("Invalid JSON arguments for tool '{}': {}", name, arguments_json))?;

    match name {
        "read_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let start = args["start_line"].as_u64().map(|v| v as usize);
            let end = args["end_line"].as_u64().map(|v| v as usize);
            filesystem::read_file(path, start, end)
        }
        "write_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let content = args["content"].as_str().context("Missing 'content' argument")?;
            let overwrite = args["overwrite"].as_bool();
            filesystem::write_file(path, content, overwrite)
        }
        "edit_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let target = args["target_content"].as_str().context("Missing 'target_content' argument")?;
            let replacement = args["replacement_content"].as_str().context("Missing 'replacement_content' argument")?;
            let allow_multiple = args["allow_multiple"].as_bool();
            filesystem::edit_file(path, target, replacement, allow_multiple)
        }
        "glob_files" => {
            let pattern = args["pattern"].as_str().context("Missing 'pattern' argument")?;
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
            let cmd = args["command"].as_str().context("Missing 'command' argument")?;
            let timeout = args["timeout_secs"].as_u64();
            shell::execute_shell(cmd, timeout)
        }
        "read_tool_result" => {
            let id = args["result_id"].as_str().context("Missing 'result_id' argument")?;
            let offset = args["offset"].as_u64().context("Missing 'offset' argument")? as usize;
            let limit = args["limit"].as_u64().context("Missing 'limit' argument")? as usize;
            result_store::ResultStore::read_result(id, offset, limit).map_err(|e| anyhow::anyhow!(e))
        }
        "ask_user_question" => {
            let question = args["question"].as_str().context("Missing 'question' argument")?;
            let options = args["options"].as_array().map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
            });
            interaction::ask_user_question(question, options)
        }
        "skill" => {
            let name = args["name"].as_str().context("Missing 'name' argument")?;
            skills::load_skill_content(name)
        }
        "manage_memory" => {
            let action = args["action"].as_str().context("Missing 'action' argument")?;
            let content = args["content"].as_str().unwrap_or("");
            crate::agent::memory::MemoryManager::save_long_term_memory(action, content)
        }
        _ => anyhow::bail!("Unknown tool: '{}'", name),
    }
}
