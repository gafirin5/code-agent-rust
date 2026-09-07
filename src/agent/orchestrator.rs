use anyhow::{Context, Result};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use crate::agent::permissions::PermissionGate;
use crate::agent::provider::ApiProtocol;
use crate::agent::tasks::{CancellationToken, OutputSink};
use crate::tools::{dispatch_tool, get_available_tools};
use crate::types::{
    ChatCompletionStreamChunk, ChatCompletionTool, ChatMessage, ChatResponse, FunctionCall,
    MessageRole, ToolCall, Usage,
};

#[derive(Debug, Clone)]
pub struct AgentTurnResult {
    pub final_content: String,
    pub total_usage: Usage,
    pub tools_executed: usize,
}

fn extract_json_slice(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start <= end {
            return &trimmed[start..=end];
        }
    }
    trimmed
}

fn format_tool_args_summary(name: &str, args_json: &str) -> String {
    let _ = name;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args_json) {
        if let Some(obj) = v.as_object() {
            let mut parts = Vec::new();
            for (k, val) in obj {
                let val_str = match val {
                    serde_json::Value::String(s) => {
                        if s.len() > 38 {
                            format!("\"{}...\"", &s[..35])
                        } else {
                            format!("\"{}\"", s)
                        }
                    }
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                parts.push(format!("{}={}", k, val_str));
                if parts.len() >= 2 {
                    break;
                }
            }
            if !parts.is_empty() {
                return format!("({})", parts.join(", "));
            }
        }
    }
    String::new()
}

/// Formats standard ChatMessage conversation turns into Anthropic Messages API format.
pub fn build_anthropic_messages(conversation: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut anthropic_msgs: Vec<serde_json::Value> = Vec::new();

    for msg in conversation {
        match msg.role {
            MessageRole::System => {
                // System message is sent at top-level "system" parameter in Anthropic API
                continue;
            }
            MessageRole::User => {
                let content_str = msg.content.clone().unwrap_or_default();
                anthropic_msgs.push(json!({
                    "role": "user",
                    "content": content_str
                }));
            }
            MessageRole::Assistant => {
                let mut content_blocks: Vec<serde_json::Value> = Vec::new();
                if let Some(c) = &msg.content {
                    if !c.trim().is_empty() {
                        content_blocks.push(json!({
                            "type": "text",
                            "text": c
                        }));
                    }
                }
                if let Some(tcs) = &msg.tool_calls {
                    for tc in tcs {
                        let parsed_input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or_else(|_| json!({}));
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": parsed_input
                        }));
                    }
                }
                if content_blocks.is_empty() {
                    content_blocks.push(json!({
                        "type": "text",
                        "text": "(empty assistant turn)"
                    }));
                }
                anthropic_msgs.push(json!({
                    "role": "assistant",
                    "content": content_blocks
                }));
            }
            MessageRole::Tool => {
                let tool_use_id = msg.tool_call_id.clone().unwrap_or_default();
                let tool_content = msg.content.clone().unwrap_or_default();
                let tool_block = json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": tool_content
                });

                // Coalesce into previous user message if adjacent to keep alternating turns
                let merged = if let Some(last) = anthropic_msgs.last_mut() {
                    if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                            arr.push(tool_block.clone());
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !merged {
                    anthropic_msgs.push(json!({
                        "role": "user",
                        "content": vec![tool_block]
                    }));
                }
            }
        }
    }

    anthropic_msgs
}

/// Converts internal tool definitions to Anthropic's expected tool format.
pub fn build_anthropic_tools(tools_schema: &[ChatCompletionTool]) -> Vec<serde_json::Value> {
    tools_schema
        .iter()
        .map(|t| {
            json!({
                "name": t.function.name,
                "description": t.function.description,
                "input_schema": t.function.parameters
            })
        })
        .collect()
}

pub fn run_agent_loop(
    prompt: &str,
    conversation: &mut Vec<ChatMessage>,
    model: &str,
    api_key: &str,
    base_url: &str,
    protocol: ApiProtocol,
    permission_gate: &mut PermissionGate,
    system_prompt: &str,
    max_turns: usize,
    stream: bool,
    context_window_limit: Option<u64>,
    output_sink: Option<&OutputSink>,
    cancel_token: Option<&CancellationToken>,
) -> Result<AgentTurnResult> {
    if conversation.is_empty() {
        conversation.push(ChatMessage::system(system_prompt));
    } else {
        // Ensure first message is the latest system prompt
        if let Some(first) = conversation.first_mut() {
            if first.role == MessageRole::System {
                first.content = Some(system_prompt.to_string());
            }
        }
    }

    conversation.push(ChatMessage::user(prompt));

    let default_sink = OutputSink::Terminal;
    let sink = output_sink.unwrap_or(&default_sink);

    // If output is silent/buffered, force streaming to false to prevent raw chunk leakage to stdout
    let effective_stream = if sink.is_silent() { false } else { stream };

    let tools_schema = get_available_tools();
    let effective_ctx_limit = context_window_limit.unwrap_or(128_000);

    let mut accumulated_prompt_tokens: u64 = 0;
    let mut accumulated_completion_tokens: u64 = 0;
    let mut accumulated_total_tokens: u64 = 0;
    let mut total_tools_executed: usize = 0;

    let mut turn_count = 0;

    loop {
        // Cooperative cancellation check before starting turn
        if let Some(token) = cancel_token {
            token.check().map_err(|e| anyhow::anyhow!("Task cancelled: {}", e))?;
        }

        turn_count += 1;
        if turn_count > max_turns {
            anyhow::bail!(
                "Exceeded maximum agent loop turns ({}). Stopping to prevent runaway cycle.",
                max_turns
            );
        }

        // Feature 4: Smart Context Compaction check
        if let Ok(true) = crate::agent::compaction::maybe_compact_context(
            conversation,
            model,
            api_key,
            base_url,
            protocol,
            effective_ctx_limit,
        ) {
            if sink.is_silent() {
                sink.emit("🧹 [Context Compaction] Older conversation turns were automatically compacted to save context window.");
            } else {
                println!("\x1B[33m🧹 [Context Compaction]\x1B[0m Older conversation turns were automatically compacted to save context window.\n");
            }
        }

        let (final_text, tool_calls_result) = match protocol {
            ApiProtocol::OpenAi => {
                let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
                execute_openai_turn(
                    &endpoint,
                    model,
                    api_key,
                    conversation,
                    &tools_schema,
                    effective_stream,
                    &mut accumulated_prompt_tokens,
                    &mut accumulated_completion_tokens,
                    &mut accumulated_total_tokens,
                    sink,
                )?
            }
            ApiProtocol::Anthropic => {
                let base = base_url.trim_end_matches('/');
                let endpoint = if base.ends_with("/messages") {
                    base.to_string()
                } else if base.ends_with("/v1") {
                    format!("{}/messages", base)
                } else {
                    format!("{}/v1/messages", base)
                };
                execute_anthropic_turn(
                    &endpoint,
                    model,
                    api_key,
                    system_prompt,
                    conversation,
                    &tools_schema,
                    effective_stream,
                    &mut accumulated_prompt_tokens,
                    &mut accumulated_completion_tokens,
                    &mut accumulated_total_tokens,
                    sink,
                )?
            }
        };

        if let Some(tool_calls) = tool_calls_result {
            if !tool_calls.is_empty() {
                for call in &tool_calls {
                    // Check cancellation before each tool call
                    if let Some(token) = cancel_token {
                        token.check().map_err(|e| anyhow::anyhow!("Task cancelled: {}", e))?;
                    }

                    let name = &call.function.name;
                    let args = &call.function.arguments;

                    let arg_summary = format_tool_args_summary(name, args);
                    if sink.is_silent() {
                        sink.emit(&format!("  ⚡ Tool: {}{}", name, arg_summary));
                    } else if arg_summary.is_empty() {
                        println!("  \x1B[36m⚡ Tool:\x1B[0m \x1B[1;37m{}\x1B[0m", name);
                    } else {
                        println!("  \x1B[36m⚡ Tool:\x1B[0m \x1B[1;37m{}\x1B[0m \x1B[90m{}\x1B[0m", name, arg_summary);
                    }

                    // Defense-in-depth: if running in silent background mode, reject interactive prompts
                    let authorized = if sink.is_silent() && permission_gate.mode == crate::agent::permissions::PermissionMode::Ask {
                        sink.emit(&format!("  └─ ⚠ Mutating tool '{}' rejected (interactive permission prompt disabled in background mode)", name));
                        false
                    } else {
                        permission_gate.check_and_authorize(name, args)?
                    };

                    let tool_result_text = if !authorized {
                        if sink.is_silent() {
                            sink.emit("  └─ ⚠ Action rejected by user permission policy.");
                        } else {
                            println!("  \x1B[33m└─ ⚠\x1B[0m \x1B[33mAction rejected by user permission policy.\x1B[0m\n");
                        }
                        "Action was rejected by user permission policy.".to_string()
                    } else if name == "ask_user_question" && sink.is_silent() {
                        // INTERACTIVE TOOL GUARD:
                        // Prevent background subagents from hijacking terminal stdin.
                        let err_msg = "Interactive tool 'ask_user_question' is disabled in background subagent mode. Proceed autonomously without interactive clarification.";
                        sink.emit(&format!("  └─ ✖ Error: {}", err_msg));
                        format!("Error: {}", err_msg)
                    } else {
                        match dispatch_tool(name, args) {
                            Ok(res) => {
                                total_tools_executed += 1;
                                let first_line = res.lines().next().unwrap_or("Completed.");
                                if sink.is_silent() {
                                    sink.emit(&format!("  └─ ✔ {}", first_line));
                                } else {
                                    println!("  \x1B[32m└─ ✔\x1B[0m \x1B[37m{}\x1B[0m\n", first_line);
                                }
                                res
                            }
                            Err(e) => {
                                if sink.is_silent() {
                                    sink.emit(&format!("  └─ ✖ Error: {}", e));
                                } else {
                                    println!("  \x1B[31m└─ ✖ Error:\x1B[0m \x1B[31m{}\x1B[0m\n", e);
                                }
                                format!("Tool execution failed: {}", e)
                            }
                        }
                    };

                    conversation.push(ChatMessage::tool_result(
                        call.id.clone(),
                        name.clone(),
                        tool_result_text,
                    ));
                }
                continue;
            }
        }

        return Ok(AgentTurnResult {
            final_content: final_text,
            total_usage: Usage {
                prompt_tokens: Some(accumulated_prompt_tokens),
                completion_tokens: Some(accumulated_completion_tokens),
                total_tokens: Some(accumulated_total_tokens),
            },
            tools_executed: total_tools_executed,
        });
    }
}

fn execute_openai_turn(
    endpoint: &str,
    model: &str,
    api_key: &str,
    conversation: &mut Vec<ChatMessage>,
    tools_schema: &[ChatCompletionTool],
    stream: bool,
    accumulated_prompt: &mut u64,
    accumulated_completion: &mut u64,
    accumulated_total: &mut u64,
    sink: &OutputSink,
) -> Result<(String, Option<Vec<ToolCall>>)> {
    if stream {
        let body = json!({
            "model": model,
            "messages": conversation,
            "tools": tools_schema,
            "tool_choice": "auto",
            "temperature": 0.2,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        sink.emit_spinner("\r\x1B[36m✦\x1B[0m \x1B[90mThinking...\x1B[0m");

        let mut request = ureq::post(endpoint)
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(120));
        if !api_key.is_empty() && api_key != "none" {
            request = request.set("Authorization", &format!("Bearer {}", api_key));
        }

        let response = request.send_json(body);
        let res = match response {
            Ok(r) => {
                sink.clear_spinner();
                r
            }
            Err(ureq::Error::Status(code, resp)) => {
                sink.clear_spinner();
                let err_body = resp.into_string().unwrap_or_default();
                anyhow::bail!("OpenAI-Compatible Provider returned HTTP {}: {}", code, err_body);
            }
            Err(e) => {
                sink.clear_spinner();
                anyhow::bail!("Network request error: {}", e);
            }
        };

        let reader = BufReader::new(res.into_reader());
        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut first_reasoning = true;
        let mut first_content = true;
        let mut tool_indicator_shown = false;

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') {
                continue;
            }
            let data_str = if let Some(rest) = trimmed.strip_prefix("data:") {
                rest.trim_start()
            } else {
                continue;
            };

            if data_str == "[DONE]" {
                break;
            }
            if let Ok(chunk) = serde_json::from_str::<ChatCompletionStreamChunk>(data_str) {
                if let Some(u) = &chunk.usage {
                    *accumulated_prompt += u.prompt_tokens.unwrap_or(0);
                    *accumulated_completion += u.completion_tokens.unwrap_or(0);
                    *accumulated_total += u.total_tokens.unwrap_or(0);
                }
                if let Some(choices) = &chunk.choices {
                    for ch in choices {
                        if let Some(delta) = &ch.delta {
                            if let Some(reasoning) = &delta.reasoning_content {
                                if !reasoning.is_empty() {
                                    if !sink.is_silent() {
                                        if first_reasoning {
                                            print!("\x1B[90m┌─ 💭 Reasoning\n│ ");
                                            first_reasoning = false;
                                        }
                                        print!("{}", reasoning.replace('\n', "\n│ "));
                                        let _ = std::io::stdout().flush();
                                    }
                                    accumulated_reasoning.push_str(reasoning);
                                }
                            }
                            if let Some(content) = &delta.content {
                                if !content.is_empty() {
                                    if !sink.is_silent() {
                                        if !accumulated_reasoning.is_empty() && first_content {
                                            println!("\n└────────────────────────────────────────────────\x1B[0m\n");
                                        }
                                        first_content = false;
                                        print!("{}", content);
                                        let _ = std::io::stdout().flush();
                                    } else {
                                        first_content = false;
                                    }
                                    accumulated_content.push_str(content);
                                }
                            }
                            if let Some(tool_deltas) = &delta.tool_calls {
                                for td in tool_deltas {
                                    let idx = td.index.unwrap_or(0);
                                    while accumulated_tool_calls.len() <= idx {
                                        accumulated_tool_calls.push(ToolCall {
                                            id: String::new(),
                                            call_type: "function".to_string(),
                                            function: FunctionCall {
                                                name: String::new(),
                                                arguments: String::new(),
                                            },
                                        });
                                    }
                                    if let Some(id) = &td.id {
                                        accumulated_tool_calls[idx].id.push_str(id);
                                    }
                                    if let Some(ct) = &td.call_type {
                                        accumulated_tool_calls[idx].call_type = ct.clone();
                                    }
                                    if let Some(f) = &td.function {
                                        if let Some(name) = &f.name {
                                            if !name.is_empty() && !tool_indicator_shown {
                                                if !sink.is_silent() {
                                                    if !accumulated_reasoning.is_empty() && first_content {
                                                        println!("\n└────────────────────────────────────────────────\x1B[0m\n");
                                                        first_content = false;
                                                    }
                                                }
                                                tool_indicator_shown = true;
                                            }
                                            accumulated_tool_calls[idx].function.name.push_str(name);
                                        }
                                        if let Some(args) = &f.arguments {
                                            accumulated_tool_calls[idx].function.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !sink.is_silent() {
            print!("\x1B[0m");
            let _ = std::io::stdout().flush();
            if !first_reasoning && first_content {
                println!("\n└────────────────────────────────────────────────\x1B[0m\n");
            }
            if !first_content {
                println!();
            }
        }

        accumulated_tool_calls.retain(|tc| !tc.function.name.trim().is_empty());

        if !accumulated_tool_calls.is_empty() {
            let assistant_msg = ChatMessage {
                role: MessageRole::Assistant,
                content: if accumulated_content.is_empty() { None } else { Some(accumulated_content) },
                tool_calls: Some(accumulated_tool_calls.clone()),
                tool_call_id: None,
                name: None,
                reasoning_content: if accumulated_reasoning.is_empty() { None } else { Some(accumulated_reasoning) },
            };
            conversation.push(assistant_msg);
            return Ok((String::new(), Some(accumulated_tool_calls)));
        }

        let final_text = if !accumulated_content.trim().is_empty() {
            accumulated_content.trim().to_string()
        } else if !accumulated_reasoning.trim().is_empty() {
            accumulated_reasoning.trim().to_string()
        } else {
            "(No response content)".to_string()
        };

        let assistant_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: Some(final_text.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: if accumulated_reasoning.is_empty() { None } else { Some(accumulated_reasoning) },
        };
        conversation.push(assistant_msg);

        Ok((final_text, None))
    } else {
        let body = json!({
            "model": model,
            "messages": conversation,
            "tools": tools_schema,
            "tool_choice": "auto",
            "temperature": 0.2
        });

        sink.emit_spinner("🤖 [Agent Thinking...] ");

        let mut request = ureq::post(endpoint)
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(120));
        if !api_key.is_empty() && api_key != "none" {
            request = request.set("Authorization", &format!("Bearer {}", api_key));
        }

        let res = match request.send_json(body) {
            Ok(r) => {
                sink.clear_spinner();
                r
            }
            Err(ureq::Error::Status(code, resp)) => {
                sink.clear_spinner();
                let err_body = resp.into_string().unwrap_or_default();
                anyhow::bail!("AI Provider returned HTTP {}: {}", code, err_body);
            }
            Err(e) => {
                sink.clear_spinner();
                anyhow::bail!("Network request error: {}", e);
            }
        };

        let raw_text = res.into_string().context("Failed to read response body from AI provider")?;
        let clean_json = extract_json_slice(&raw_text);

        let chat_res: ChatResponse = serde_json::from_str(clean_json)
            .with_context(|| format!("Failed to parse AI provider JSON response: {}", clean_json))?;

        if let Some(err) = chat_res.error {
            let msg = err.message.unwrap_or_else(|| "Unknown API error".to_string());
            anyhow::bail!("AI Provider error: {}", msg);
        }

        if let Some(u) = &chat_res.usage {
            *accumulated_prompt += u.prompt_tokens.unwrap_or(0);
            *accumulated_completion += u.completion_tokens.unwrap_or(0);
            *accumulated_total += u.total_tokens.unwrap_or(0);
        }

        let choice = chat_res
            .choices
            .as_ref()
            .and_then(|c| c.first())
            .context("No choices returned in AI provider response")?;

        let msg = choice.message.as_ref().context("No message in choice")?;

        if let Some(reasoning) = &msg.reasoning_content {
            if !reasoning.trim().is_empty() {
                if sink.is_silent() {
                    sink.emit("┌─ 💭 Reasoning");
                    for r_line in reasoning.trim().lines() {
                        sink.emit(&format!("│ {}", r_line));
                    }
                    sink.emit("└────────────────────────────────────────────────");
                } else {
                    println!("\x1B[90m┌─ 💭 Reasoning");
                    for r_line in reasoning.trim().lines() {
                        println!("│ {}", r_line);
                    }
                    println!("└────────────────────────────────────────────────\x1B[0m\n");
                }
            }
        }

        if let Some(text) = &msg.content {
            if !text.trim().is_empty() && msg.tool_calls.is_some() {
                if sink.is_silent() {
                    sink.emit(&format!("💬 {}", text.trim()));
                } else {
                    println!("💬 {}", text.trim());
                }
            }
        }

        if let Some(tool_calls) = &msg.tool_calls {
            if !tool_calls.is_empty() {
                conversation.push(msg.clone());
                return Ok((String::new(), Some(tool_calls.clone())));
            }
        }

        let final_text = msg
            .content
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| msg.reasoning_content.clone())
            .unwrap_or_else(|| "(No response content)".to_string());

        conversation.push(msg.clone());
        Ok((final_text.trim().to_string(), None))
    }
}

fn execute_anthropic_turn(
    endpoint: &str,
    model: &str,
    api_key: &str,
    system_prompt: &str,
    conversation: &mut Vec<ChatMessage>,
    tools_schema: &[ChatCompletionTool],
    stream: bool,
    accumulated_prompt: &mut u64,
    accumulated_completion: &mut u64,
    accumulated_total: &mut u64,
    sink: &OutputSink,
) -> Result<(String, Option<Vec<ToolCall>>)> {
    let anthropic_msgs = build_anthropic_messages(conversation);
    let anthropic_tools = build_anthropic_tools(tools_schema);

    let (_, cat_out, _) = crate::agent::probe::resolve_model_limits(model);
    let effective_max_tokens = cat_out.unwrap_or(8192).min(8192);

    let body = json!({
        "model": model,
        "system": system_prompt,
        "messages": anthropic_msgs,
        "tools": anthropic_tools,
        "max_tokens": effective_max_tokens,
        "stream": stream
    });

    sink.emit_spinner("\r\x1B[36m✦\x1B[0m \x1B[90mThinking...\x1B[0m");

    let request = ureq::post(endpoint)
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(120));

    let response = request.send_json(body);
    let res = match response {
        Ok(r) => {
            sink.clear_spinner();
            r
        }
        Err(ureq::Error::Status(code, resp)) => {
            sink.clear_spinner();
            let err_body = resp.into_string().unwrap_or_default();
            anyhow::bail!("Anthropic API returned HTTP {}: {}", code, err_body);
        }
        Err(e) => {
            sink.clear_spinner();
            anyhow::bail!("Anthropic request error: {}", e);
        }
    };

    if stream {
        let reader = BufReader::new(res.into_reader());
        let mut accumulated_content = String::new();
        let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();
        let mut first_content = true;
        let mut tool_indicator_shown = false;

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') {
                continue;
            }
            let data_str = if let Some(rest) = trimmed.strip_prefix("data:") {
                rest.trim_start()
            } else {
                continue;
            };

            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match event_type {
                    "message_start" => {
                        if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
                            if let Some(in_tok) = usage.get("input_tokens").and_then(|n| n.as_u64()) {
                                *accumulated_prompt += in_tok;
                            }
                        }
                    }
                    "content_block_start" => {
                        let idx = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        if let Some(block) = val.get("content_block") {
                            let b_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if b_type == "tool_use" {
                                let id = block.get("id").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                let name = block.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                while accumulated_tool_calls.len() <= idx {
                                    accumulated_tool_calls.push(ToolCall {
                                        id: String::new(),
                                        call_type: "function".to_string(),
                                        function: FunctionCall {
                                            name: String::new(),
                                            arguments: String::new(),
                                        },
                                    });
                                }
                                accumulated_tool_calls[idx].id = id;
                                accumulated_tool_calls[idx].function.name = name;

                                if !tool_indicator_shown {
                                    if !sink.is_silent() {
                                        print!("\x1B[33m⚡ Streaming Tool Call...\x1B[0m\n");
                                        let _ = std::io::stdout().flush();
                                    }
                                    tool_indicator_shown = true;
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        let idx = val.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        if let Some(delta) = val.get("delta") {
                            let d_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if d_type == "text_delta" {
                                if let Some(txt) = delta.get("text").and_then(|s| s.as_str()) {
                                    if !sink.is_silent() {
                                        if first_content {
                                            first_content = false;
                                        }
                                        print!("{}", txt);
                                        let _ = std::io::stdout().flush();
                                    } else {
                                        first_content = false;
                                    }
                                    accumulated_content.push_str(txt);
                                }
                            } else if d_type == "input_json_delta" {
                                if let Some(pj) = delta.get("partial_json").and_then(|s| s.as_str()) {
                                    while accumulated_tool_calls.len() <= idx {
                                        accumulated_tool_calls.push(ToolCall {
                                            id: String::new(),
                                            call_type: "function".to_string(),
                                            function: FunctionCall {
                                                name: String::new(),
                                                arguments: String::new(),
                                            },
                                        });
                                    }
                                    accumulated_tool_calls[idx].function.arguments.push_str(pj);
                                }
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(usage) = val.get("usage") {
                            if let Some(out_tok) = usage.get("output_tokens").and_then(|n| n.as_u64()) {
                                *accumulated_completion += out_tok;
                            }
                        }
                    }
                    "error" => {
                        let err_msg = val
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("Anthropic streaming error");
                        anyhow::bail!("Anthropic stream error: {}", err_msg);
                    }
                    _ => {}
                }
            }
        }

        if !sink.is_silent() {
            print!("\x1B[0m");
            let _ = std::io::stdout().flush();
            if !first_content {
                println!();
            }
        }

        *accumulated_total = *accumulated_prompt + *accumulated_completion;
        accumulated_tool_calls.retain(|tc| !tc.function.name.trim().is_empty());

        if !accumulated_tool_calls.is_empty() {
            let assistant_msg = ChatMessage {
                role: MessageRole::Assistant,
                content: if accumulated_content.is_empty() { None } else { Some(accumulated_content) },
                tool_calls: Some(accumulated_tool_calls.clone()),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            };
            conversation.push(assistant_msg);
            return Ok((String::new(), Some(accumulated_tool_calls)));
        }

        let final_text = if !accumulated_content.trim().is_empty() {
            accumulated_content.trim().to_string()
        } else {
            "(No response content)".to_string()
        };

        let assistant_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: Some(final_text.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };
        conversation.push(assistant_msg);

        Ok((final_text, None))
    } else {
        let raw_text = res.into_string().context("Failed to read Anthropic response body")?;
        let clean_json = extract_json_slice(&raw_text);
        let val: serde_json::Value = serde_json::from_str(clean_json)
            .with_context(|| format!("Failed to parse Anthropic JSON: {}", clean_json))?;

        if let Some(err) = val.get("error") {
            let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Anthropic API error");
            anyhow::bail!("Anthropic error: {}", msg);
        }

        if let Some(usage) = val.get("usage") {
            if let Some(in_tok) = usage.get("input_tokens").and_then(|n| n.as_u64()) {
                *accumulated_prompt += in_tok;
            }
            if let Some(out_tok) = usage.get("output_tokens").and_then(|n| n.as_u64()) {
                *accumulated_completion += out_tok;
            }
            *accumulated_total = *accumulated_prompt + *accumulated_completion;
        }

        let mut text_acc = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        if let Some(content_arr) = val.get("content").and_then(|c| c.as_array()) {
            for block in content_arr {
                let b_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if b_type == "text" {
                    if let Some(txt) = block.get("text").and_then(|s| s.as_str()) {
                        text_acc.push_str(txt);
                    }
                } else if b_type == "tool_use" {
                    let id = block.get("id").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let name = block.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let input_val = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name,
                            arguments: serde_json::to_string(&input_val).unwrap_or_default(),
                        },
                    });
                }
            }
        }

        if !tool_calls.is_empty() {
            let assistant_msg = ChatMessage {
                role: MessageRole::Assistant,
                content: if text_acc.is_empty() { None } else { Some(text_acc) },
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            };
            conversation.push(assistant_msg);
            return Ok((String::new(), Some(tool_calls)));
        }

        let final_text = if !text_acc.trim().is_empty() {
            text_acc.trim().to_string()
        } else {
            "(No response content)".to_string()
        };

        let assistant_msg = ChatMessage {
            role: MessageRole::Assistant,
            content: Some(final_text.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };
        conversation.push(assistant_msg);

        Ok((final_text, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::agent::tasks::TaskLogBuffer;

    #[test]
    fn test_output_sink_silence_contract() {
        let buf = Arc::new(TaskLogBuffer::new());
        let sink = OutputSink::Buffered(buf.clone());
        assert!(sink.is_silent());
        assert!(sink.buffer().is_some());

        sink.emit_spinner("Thinking...");
        sink.clear_spinner();
        assert_eq!(buf.len(), 0);

        sink.emit("Permanent log line");
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.lines()[0], "Permanent log line");
    }

    #[test]
    fn test_tool_args_summary_truncation() {
        let long_arg = json!({
            "path": "very_long_file_name_exceeding_thirty_five_characters.rs"
        }).to_string();
        let summary = format_tool_args_summary("read_file", &long_arg);
        assert!(summary.contains("..."));
        assert!(summary.starts_with('('));
        assert!(summary.ends_with(')'));
    }

    #[test]
    fn test_cooperative_cancellation_pre_check() {
        let token = CancellationToken::new();
        token.cancel();

        let mut conversation = Vec::new();
        let mut permission_gate = PermissionGate {
            mode: crate::agent::permissions::PermissionMode::AutoApprove,
        };

        let res = run_agent_loop(
            "test prompt",
            &mut conversation,
            "mock-model",
            "mock-key",
            "http://127.0.0.1:9999",
            ApiProtocol::OpenAi,
            &mut permission_gate,
            "system prompt",
            5,
            false,
            None,
            None,
            Some(&token),
        );

        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("Task cancelled"));
    }
}
