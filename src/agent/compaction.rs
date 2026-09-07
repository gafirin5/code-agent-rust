use anyhow::Result;
use serde_json::json;
use crate::agent::provider::ApiProtocol;
use crate::types::{ChatMessage, MessageRole};

/// Estimates token count based on standard ~3.8 chars per token approximation.
pub fn estimate_tokens(messages: &[ChatMessage]) -> u64 {
    let mut total_chars: usize = 0;
    for m in messages {
        if let Some(c) = &m.content {
            total_chars += c.len();
        }
        if let Some(r) = &m.reasoning_content {
            total_chars += r.len();
        }
        if let Some(tcs) = &m.tool_calls {
            for tc in tcs {
                total_chars += tc.function.name.len() + tc.function.arguments.len();
            }
        }
    }
    ((total_chars as f64) / 3.8).ceil() as u64
}

/// Automatically compacts context if message count or estimated token threshold is reached.
pub fn maybe_compact_context(
    conversation: &mut Vec<ChatMessage>,
    model: &str,
    api_key: &str,
    base_url: &str,
    protocol: ApiProtocol,
    context_window_limit: u64,
) -> Result<bool> {
    let count = conversation.len();
    let estimated = estimate_tokens(conversation);

    let token_threshold = (context_window_limit as f64 * 0.65) as u64;
    let message_threshold = 16;

    if (count > message_threshold || estimated > token_threshold) && count > 6 {
        compact_conversation(conversation, model, api_key, base_url, protocol)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Force-compacts earlier context turns into a high-density summary.
pub fn force_compact_context(
    conversation: &mut Vec<ChatMessage>,
    model: &str,
    api_key: &str,
    base_url: &str,
    protocol: ApiProtocol,
) -> Result<String> {
    if conversation.len() <= 4 {
        return Ok("Conversation context is too short to compact (< 5 messages).".to_string());
    }

    let before_tokens = estimate_tokens(conversation);
    let before_count = conversation.len();

    compact_conversation(conversation, model, api_key, base_url, protocol)?;

    let after_tokens = estimate_tokens(conversation);
    let after_count = conversation.len();

    let tokens_saved = before_tokens.saturating_sub(after_tokens);
    let msgs_reduced = before_count.saturating_sub(after_count);

    Ok(format!(
        "✔ Context compaction completed!\n  • Messages compacted: {} -> {} ({} turns condensed)\n  • Tokens estimated: ~{} -> ~{} (~{} tokens saved)",
        before_count, after_count, msgs_reduced, before_tokens, after_tokens, tokens_saved
    ))
}

fn compact_conversation(
    conversation: &mut Vec<ChatMessage>,
    model: &str,
    api_key: &str,
    base_url: &str,
    protocol: ApiProtocol,
) -> Result<()> {
    if conversation.len() <= 5 {
        return Ok(());
    }

    // Keep System prompt at index 0 (if present)
    let has_system = conversation.first().map(|m| m.role == MessageRole::System).unwrap_or(false);
    let start_compact_idx = if has_system { 1 } else { 0 };

    // Safe turn-boundary alignment: Never split Assistant tool_calls from their Tool responses,
    // and never let preserved_tail start with a Tool message (which triggers HTTP 400 on LLM APIs).
    let desired_end = conversation.len().saturating_sub(4);
    let end_compact_idx = find_safe_compact_cut_point(conversation, desired_end, start_compact_idx);

    if start_compact_idx >= end_compact_idx {
        return Ok(());
    }

    let slice_to_compact: Vec<ChatMessage> = conversation[start_compact_idx..end_compact_idx].to_vec();

    // Generate summary via LLM or fallback
    let summary_text = match summarize_with_llm(&slice_to_compact, model, api_key, base_url, protocol) {
        Ok(s) => s,
        Err(_) => heuristic_summarize(&slice_to_compact),
    };

    let summary_msg = ChatMessage::user(format!(
        "[System Notice: Context Compaction]\n\
         The earlier conversation turns have been compacted into the following summary to stay within context window limits:\n\n\
         {}\n\n\
         [Continue with current instructions based on the preserved context.]",
        summary_text
    ));

    // Replace the slice with the summary message
    let preserved_tail = conversation[end_compact_idx..].to_vec();
    conversation.truncate(start_compact_idx);
    conversation.push(summary_msg);
    conversation.extend(preserved_tail);

    Ok(())
}

fn summarize_with_llm(
    slice: &[ChatMessage],
    model: &str,
    api_key: &str,
    base_url: &str,
    protocol: ApiProtocol,
) -> Result<String> {
    let mut prompt = String::from(
        "Summarize the following earlier agent conversation history into a concise, high-density structured summary.\n\
         Highlight:\n\
         1. User's original objectives and instructions.\n\
         2. Files read, created, or modified.\n\
         3. Key decisions, errors encountered, and current progress.\n\
         Keep the summary factual and under 250 words.\n\n\
         --- Conversation to Summarize ---\n",
    );

    for m in slice {
        let role_label = match m.role {
            MessageRole::System => "System",
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::Tool => "Tool Result",
        };
        if let Some(content) = &m.content {
            let truncated: String = content.lines().take(6).collect::<Vec<_>>().join("\n");
            prompt.push_str(&format!("[{}]: {}\n", role_label, truncated));
        }
    }

    if protocol == ApiProtocol::Anthropic {
        let base = base_url.trim_end_matches('/');
        let endpoint = if base.ends_with("/messages") {
            base.to_string()
        } else if base.ends_with("/v1") {
            format!("{}/messages", base)
        } else {
            format!("{}/v1/messages", base)
        };

        let body = json!({
            "model": model,
            "system": "You are a technical conversation compactor. Summarize key context concisely.",
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "max_tokens": 400
        });

        let resp = ureq::post(&endpoint)
            .set("x-api-key", api_key)
            .set("anthropic-version", "2023-06-01")
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(30))
            .send_json(body)?;

        let raw: String = resp.into_string()?;
        let val: serde_json::Value = serde_json::from_str(&raw)?;

        if let Some(content_arr) = val.get("content").and_then(|c| c.as_array()) {
            for block in content_arr {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(txt) = block.get("text").and_then(|s| s.as_str()) {
                        return Ok(txt.trim().to_string());
                    }
                }
            }
        }
        anyhow::bail!("No text content in Anthropic summary response");
    }

    let base = base_url.trim_end_matches('/');
    let endpoint = if base.ends_with("/v1") || base.ends_with("/api") {
        format!("{}/chat/completions", base)
    } else {
        format!("{}/v1/chat/completions", base)
    };

    let messages = vec![
        ChatMessage::system("You are a technical conversation compactor. Summarize key context concisely."),
        ChatMessage::user(prompt),
    ];

    let body = json!({
        "model": model,
        "messages": messages,
        "temperature": 0.1,
        "max_tokens": 400
    });

    let mut post_req = ureq::post(&endpoint)
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30));
    if !api_key.is_empty() && api_key != "none" {
        post_req = post_req.set("Authorization", &format!("Bearer {}", api_key));
    }

    let resp = post_req.send_json(body)?;
    let raw: String = resp.into_string()?;
    let val: serde_json::Value = serde_json::from_str(&raw)?;

    let summary = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("Earlier turns completed.")
        .trim()
        .to_string();

    Ok(summary)
}

fn heuristic_summarize(slice: &[ChatMessage]) -> String {
    let mut files_touched = Vec::new();
    let mut user_requests = Vec::new();

    for m in slice {
        if m.role == MessageRole::User {
            if let Some(c) = &m.content {
                user_requests.push(c.lines().next().unwrap_or("").trim().to_string());
            }
        } else if let Some(tcs) = &m.tool_calls {
            for tc in tcs {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                    if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                        if !files_touched.contains(&p.to_string()) {
                            files_touched.push(p.to_string());
                        }
                    }
                }
            }
        }
    }

    format!(
        "Summary of earlier {} turns:\n\
         • User queries: {}\n\
         • Files involved: {}\n\
         • Actions: Autonomous exploration and tool executions completed.",
        slice.len(),
        if user_requests.is_empty() { "None recorded".to_string() } else { user_requests.join("; ") },
        if files_touched.is_empty() { "None".to_string() } else { files_touched.join(", ") }
    )
}

fn find_safe_compact_cut_point(conversation: &[ChatMessage], desired_end: usize, start_idx: usize) -> usize {
    let mut cut = desired_end.min(conversation.len());

    // 1. Never cut in the middle of a tool call / response block:
    // Step backward if cut points directly to a Tool message
    while cut > start_idx && conversation.get(cut).map(|m| m.role == MessageRole::Tool).unwrap_or(false) {
        cut -= 1;
    }

    // 2. If the message right before cut is an Assistant with tool_calls,
    // its tool responses would end up in preserved_tail without the assistant,
    // so step before the assistant as well.
    if cut > start_idx {
        if let Some(prev) = conversation.get(cut - 1) {
            if prev.role == MessageRole::Assistant && prev.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false) {
                cut -= 1;
            }
        }
    }

    // 3. Search backwards within a reasonable window for a clean User message boundary
    let mut user_cut = cut;
    while user_cut > start_idx {
        if conversation[user_cut].role == MessageRole::User {
            return user_cut;
        }
        user_cut -= 1;
    }

    // Fallback: Ensure cut never leaves preserved_tail starting with Tool
    while cut > start_idx && conversation.get(cut).map(|m| m.role == MessageRole::Tool).unwrap_or(false) {
        cut -= 1;
    }

    cut
}
