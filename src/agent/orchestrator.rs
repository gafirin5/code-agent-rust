use anyhow::{Context, Result};
use serde_json::json;
use std::io::Write;
use crate::agent::permissions::PermissionGate;
use crate::tools::{dispatch_tool, get_available_tools};
use crate::types::{ChatMessage, ChatResponse, Usage};

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

pub fn run_agent_loop(
    prompt: &str,
    conversation: &mut Vec<ChatMessage>,
    model: &str,
    api_key: &str,
    base_url: &str,
    permission_gate: &mut PermissionGate,
    system_prompt: &str,
    max_turns: usize,
) -> Result<AgentTurnResult> {
    if conversation.is_empty() {
        conversation.push(ChatMessage::system(system_prompt));
    } else {
        // Ensure first message is the latest system prompt
        if let Some(first) = conversation.first_mut() {
            if first.role == crate::types::MessageRole::System {
                first.content = Some(system_prompt.to_string());
            }
        }
    }

    conversation.push(ChatMessage::user(prompt));

    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let tools_schema = get_available_tools();

    let mut accumulated_prompt_tokens: u64 = 0;
    let mut accumulated_completion_tokens: u64 = 0;
    let mut accumulated_total_tokens: u64 = 0;
    let mut total_tools_executed: usize = 0;

    let mut turn_count = 0;

    loop {
        turn_count += 1;
        if turn_count > max_turns {
            anyhow::bail!("Exceeded maximum agent loop turns ({}). Stopping to prevent runaway cycle.", max_turns);
        }

        let body = json!({
            "model": model,
            "messages": conversation,
            "tools": tools_schema,
            "tool_choice": "auto",
            "temperature": 0.2
        });

        print!("🤖 [Agent Thinking...] ");
        let _ = std::io::stdout().flush();

        let response = ureq::post(&endpoint)
            .set("Authorization", &format!("Bearer {}", api_key))
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(120))
            .send_json(body);

        let res = match response {
            Ok(r) => r,
            Err(ureq::Error::Status(code, resp)) => {
                let err_body = resp.into_string().unwrap_or_default();
                anyhow::bail!("AI Provider returned HTTP {}: {}", code, err_body);
            }
            Err(e) => anyhow::bail!("Network request error: {}", e),
        };

        // Clear the thinking indicator
        print!("\r\x1B[K");
        let _ = std::io::stdout().flush();

        let raw_text = res.into_string().context("Failed to read response body from AI provider")?;
        let clean_json = extract_json_slice(&raw_text);

        let chat_res: ChatResponse = serde_json::from_str(clean_json)
            .with_context(|| format!("Failed to parse AI provider JSON response: {}", clean_json))?;

        if let Some(err) = chat_res.error {
            let msg = err.message.unwrap_or_else(|| "Unknown API error".to_string());
            anyhow::bail!("AI Provider error: {}", msg);
        }

        if let Some(u) = &chat_res.usage {
            accumulated_prompt_tokens += u.prompt_tokens.unwrap_or(0);
            accumulated_completion_tokens += u.completion_tokens.unwrap_or(0);
            accumulated_total_tokens += u.total_tokens.unwrap_or(0);
        }

        let choice = chat_res
            .choices
            .as_ref()
            .and_then(|c| c.first())
            .context("No choices returned in AI provider response")?;

        let msg = choice.message.as_ref().context("No message in choice")?;

        // If the model provided reasoning or preamble text before tool calls
        if let Some(reasoning) = &msg.reasoning_content {
            if !reasoning.trim().is_empty() {
                println!("\x1B[90m💭 Reasoning: {}\x1B[0m\n", reasoning.trim());
            }
        }

        if let Some(text) = &msg.content {
            if !text.trim().is_empty() && msg.tool_calls.is_some() {
                println!("💬 {}", text.trim());
            }
        }

        // Check if tool calls were requested
        if let Some(tool_calls) = &msg.tool_calls {
            if !tool_calls.is_empty() {
                // Record assistant message with tool calls into history
                conversation.push(msg.clone());

                for call in tool_calls {
                    let name = &call.function.name;
                    let args = &call.function.arguments;

                    println!("⚙️  [Tool Call] \x1B[1m{}\x1B[0m", name);
                    let authorized = permission_gate.check_and_authorize(name, args)?;

                    let tool_result_text = if authorized {
                        match dispatch_tool(name, args) {
                            Ok(res) => {
                                total_tools_executed += 1;
                                let first_line = res.lines().next().unwrap_or("Completed.");
                                println!("   ✔ \x1B[32m{}\x1B[0m\n", first_line);
                                res
                            }
                            Err(e) => {
                                println!("   ❌ \x1B[31mError: {}\x1B[0m\n", e);
                                format!("Tool execution failed: {}", e)
                            }
                        }
                    } else {
                        "Action was rejected by user permission policy.".to_string()
                    };

                    conversation.push(ChatMessage::tool_result(
                        call.id.clone(),
                        name.clone(),
                        tool_result_text,
                    ));
                }

                // Loop to provide tool results back to the model
                continue;
            }
        }

        // Final answer from model without tool calls
        let final_text = msg
            .content
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| msg.reasoning_content.clone())
            .unwrap_or_else(|| "(No response content)".to_string());

        conversation.push(msg.clone());

        return Ok(AgentTurnResult {
            final_content: final_text.trim().to_string(),
            total_usage: Usage {
                prompt_tokens: Some(accumulated_prompt_tokens),
                completion_tokens: Some(accumulated_completion_tokens),
                total_tokens: Some(accumulated_total_tokens),
            },
            tools_executed: total_tools_executed,
        });
    }
}
