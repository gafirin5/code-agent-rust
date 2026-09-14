use crate::types::{ChatCompletionTool, FunctionDefinition};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct McpConfigFile {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

pub struct McpManager;

impl McpManager {
    /// Discovers and loads the MCP configuration file from .ctrl/mcp.json or workspace roots.
    pub fn load_config() -> Option<McpConfigFile> {
        let candidates = [
            PathBuf::from(".ctrl/mcp.json"),
            PathBuf::from("mcp.json"),
            PathBuf::from(".mcp.json"),
        ];

        for path in &candidates {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(cfg) = serde_json::from_str::<McpConfigFile>(&content) {
                        return Some(cfg);
                    }
                }
            }
        }
        None
    }

    /// Fetches all available tools across configured MCP servers.
    pub fn get_all_tools() -> Vec<ChatCompletionTool> {
        let cfg = match Self::load_config() {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut all_tools = Vec::new();

        for (server_name, server_cfg) in cfg.mcp_servers {
            match query_server_tools(&server_name, &server_cfg) {
                Ok(tools) => all_tools.extend(tools),
                Err(e) => {
                    eprintln!("⚠️ [MCP Server '{}' Warning]: {}", server_name, e);
                }
            }
        }

        all_tools
    }

    /// Checks if a tool call is an MCP tool invocation (prefixed with `mcp__`).
    pub fn is_mcp_tool(tool_name: &str) -> bool {
        tool_name.starts_with("mcp__")
    }

    /// Dispatches an MCP tool call to the target MCP server.
    pub fn dispatch(full_name: &str, arguments_json: &str) -> Result<String> {
        let parts: Vec<&str> = full_name.splitn(3, "__").collect();
        if parts.len() < 3 || parts[0] != "mcp" {
            anyhow::bail!(
                "Invalid MCP tool name format: '{}'. Expected 'mcp__<server>__<tool>'",
                full_name
            );
        }

        let server_name = parts[1];
        let actual_tool = parts[2];

        let cfg = Self::load_config().context("No MCP configuration found (.ctrl/mcp.json)")?;
        let server_cfg = cfg.mcp_servers.get(server_name).with_context(|| {
            format!(
                "MCP server '{}' is not defined in configuration",
                server_name
            )
        })?;

        let args_value: serde_json::Value =
            serde_json::from_str(arguments_json).unwrap_or_else(|_| json!({}));

        call_server_tool(server_cfg, actual_tool, args_value)
    }

    /// Summarizes status of configured MCP servers for `/mcp` command.
    pub fn get_status_summary() -> String {
        let cfg = match Self::load_config() {
            Some(c) => c,
            None => return "No MCP configuration file found (create `.ctrl/mcp.json` to enable Model Context Protocol servers).".to_string(),
        };

        if cfg.mcp_servers.is_empty() {
            return "MCP configuration loaded, but 0 servers defined.".to_string();
        }

        let mut out = String::new();
        out.push_str("══════════════════════════════════════════════════════════════\n");
        out.push_str(" 🔌 Model Context Protocol (MCP) Servers\n");
        out.push_str("══════════════════════════════════════════════════════════════\n");

        for (name, s_cfg) in &cfg.mcp_servers {
            let tools = query_server_tools(name, s_cfg).unwrap_or_default();
            out.push_str(&format!(" • Server: \x1B[1m{}\x1B[0m\n", name));
            out.push_str(&format!(
                "   Command: {} {}\n",
                s_cfg.command,
                s_cfg.args.join(" ")
            ));
            out.push_str(&format!("   Tools Loaded: {} tool(s)\n", tools.len()));
            for t in &tools {
                out.push_str(&format!(
                    "     - {}: {}\n",
                    t.function.name, t.function.description
                ));
            }
            out.push('\n');
        }

        out
    }
}

fn prepare_command(cmd_name: &str) -> Command {
    #[cfg(windows)]
    {
        let resolved =
            if !cmd_name.contains('.') && !cmd_name.contains('/') && !cmd_name.contains('\\') {
                if let Ok(path_var) = std::env::var("PATH") {
                    let mut found = None;
                    for dir in std::env::split_paths(&path_var) {
                        let cmd_path = dir.join(format!("{}.cmd", cmd_name));
                        if cmd_path.is_file() {
                            found = Some(cmd_path.to_string_lossy().to_string());
                            break;
                        }
                        let exe_path = dir.join(format!("{}.exe", cmd_name));
                        if exe_path.is_file() {
                            found = Some(exe_path.to_string_lossy().to_string());
                            break;
                        }
                    }
                    found.unwrap_or_else(|| cmd_name.to_string())
                } else {
                    cmd_name.to_string()
                }
            } else {
                cmd_name.to_string()
            };
        Command::new(resolved)
    }
    #[cfg(not(windows))]
    {
        Command::new(cmd_name)
    }
}

fn read_jsonrpc_response(
    reader: &mut BufReader<std::process::ChildStdout>,
    expected_id: Option<u64>,
    max_lines: usize,
) -> Result<serde_json::Value> {
    let mut line = String::new();
    let mut lines_read = 0;

    while lines_read < max_lines {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            anyhow::bail!("MCP server closed connection unexpectedly");
        }
        lines_read += 1;

        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(target_id) = expected_id {
                if let Some(resp_id) = val.get("id").and_then(|id| id.as_u64()) {
                    if resp_id == target_id {
                        return Ok(val);
                    }
                }
            } else {
                return Ok(val);
            }
        }
    }

    anyhow::bail!(
        "Exceeded limit ({} lines) waiting for JSON-RPC response id {:?}",
        max_lines,
        expected_id
    )
}

fn query_server_tools(server_name: &str, cfg: &McpServerConfig) -> Result<Vec<ChatCompletionTool>> {
    let mut cmd = prepare_command(&cfg.command);
    cmd.args(&cfg.args);
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn MCP server '{}' ({})",
            server_name, cfg.command
        )
    })?;

    let mut stdin = child
        .stdin
        .take()
        .context("Failed to open stdin for MCP server")?;
    let stdout = child
        .stdout
        .take()
        .context("Failed to open stdout for MCP server")?;
    let mut reader = BufReader::new(stdout);

    // 1. Initialize Handshake
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "ctrl-cli",
                "version": "0.2.0"
            }
        }
    });

    let _ = writeln!(stdin, "{}", serde_json::to_string(&init_req)?);
    let _ = stdin.flush();

    let _ = read_jsonrpc_response(&mut reader, Some(1), 50)?;

    // Send initialized notification
    let notify = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = writeln!(stdin, "{}", serde_json::to_string(&notify)?);
    let _ = stdin.flush();

    // 2. Request tools/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let _ = writeln!(stdin, "{}", serde_json::to_string(&list_req)?);
    let _ = stdin.flush();

    let resp_val = read_jsonrpc_response(&mut reader, Some(2), 50)?;
    let _ = child.kill();

    let mut tools = Vec::new();
    if let Some(items) = resp_val["result"]["tools"].as_array() {
        for t in items {
            let t_name = t["name"].as_str().unwrap_or("unknown");
            let t_desc = t["description"].as_str().unwrap_or("MCP external tool");
            let schema = t["inputSchema"].clone();

            let tool_id = format!("mcp__{}__{}", server_name, t_name);
            tools.push(ChatCompletionTool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: tool_id,
                    description: format!("[MCP: {}] {}", server_name, t_desc),
                    parameters: schema,
                },
            });
        }
    }

    Ok(tools)
}

fn call_server_tool(
    cfg: &McpServerConfig,
    tool_name: &str,
    args: serde_json::Value,
) -> Result<String> {
    let mut cmd = prepare_command(&cfg.command);
    cmd.args(&cfg.args);
    for (k, v) in &cfg.env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn MCP server ({})", cfg.command))?;

    let mut stdin = child
        .stdin
        .take()
        .context("Failed to open stdin for MCP server")?;
    let stdout = child
        .stdout
        .take()
        .context("Failed to open stdout for MCP server")?;
    let mut reader = BufReader::new(stdout);

    // Initialize (id: 1)
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "ctrl-cli", "version": "0.2.0" }
        }
    });
    let _ = writeln!(stdin, "{}", serde_json::to_string(&init_req)?);
    let _ = stdin.flush();

    let _ = read_jsonrpc_response(&mut reader, Some(1), 50)?;

    // Call tool (id: 2)
    let call_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": args
        }
    });

    let _ = writeln!(stdin, "{}", serde_json::to_string(&call_req)?);
    let _ = stdin.flush();

    let resp_val = read_jsonrpc_response(&mut reader, Some(2), 100)?;
    let _ = child.kill();

    if let Some(err) = resp_val.get("error") {
        anyhow::bail!("MCP Tool error: {}", err);
    }

    let mut output = String::new();
    if let Some(contents) = resp_val["result"]["content"].as_array() {
        for item in contents {
            if let Some(txt) = item["text"].as_str() {
                output.push_str(txt);
                output.push('\n');
            }
        }
    } else {
        output = serde_json::to_string_pretty(&resp_val["result"]).unwrap_or_default();
    }

    Ok(output.trim().to_string())
}
