use crate::agent::provider::{ApiProtocol, ProviderConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Instant;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeReport {
    pub success: bool,
    pub latency_ms: u128,
    pub endpoint_reachable: bool,
    pub auth_valid: bool,
    pub models_found: Vec<String>,
    pub context_window: u64,
    pub max_output_tokens: Option<u64>,
    pub model_note: String,
    pub status_message: String,
    pub error_detail: Option<String>,
}

/// Resolves known context window and max output limits based on model name family.
pub fn resolve_model_limits(model: &str) -> (u64, Option<u64>, &'static str) {
    let lower = model.to_lowercase();
    if lower.contains("claude-3-7-sonnet") || lower.contains("claude-3.7-sonnet") {
        (
            200_000,
            Some(8_192),
            "Anthropic Claude 3.7 Sonnet (200k context, 8k output)",
        )
    } else if lower.contains("claude-3-5-sonnet") || lower.contains("claude-3.5-sonnet") {
        (
            200_000,
            Some(8_192),
            "Anthropic Claude 3.5 Sonnet (200k context, 8k output)",
        )
    } else if lower.contains("claude-3-5-haiku") || lower.contains("claude-3.5-haiku") {
        (
            200_000,
            Some(8_192),
            "Anthropic Claude 3.5 Haiku (200k context, 8k output)",
        )
    } else if lower.contains("claude-3-opus") || lower.contains("claude-3.0-opus") {
        (
            200_000,
            Some(4_096),
            "Anthropic Claude 3 Opus (200k context, 4k output)",
        )
    } else if lower.contains("claude-3-haiku") {
        (
            200_000,
            Some(4_096),
            "Anthropic Claude 3 Haiku (200k context, 4k output)",
        )
    } else if lower.contains("gemini-2.5-flash")
        || lower.contains("gemini-2.5-pro")
        || lower.contains("gemini-2.5")
    {
        (
            1_000_000,
            Some(8_192),
            "Google Gemini 2.5 (1M context, 8k output)",
        )
    } else if lower.contains("gemini-2.0-flash") || lower.contains("gemini-2.0") {
        (
            1_000_000,
            Some(8_192),
            "Google Gemini 2.0 Flash (1M context, 8k output)",
        )
    } else if lower.contains("gemini-1.5-pro") {
        (
            1_000_000,
            Some(8_192),
            "Google Gemini 1.5 Pro (1M context, 8k output)",
        )
    } else if lower.contains("gemini-1.5-flash") {
        (
            1_000_000,
            Some(8_192),
            "Google Gemini 1.5 Flash (1M context, 8k output)",
        )
    } else if lower.contains("deepseek-reasoner") || lower.contains("deepseek-r1") {
        (
            64_000,
            Some(8_192),
            "DeepSeek R1 Reasoner (64k context, 8k output)",
        )
    } else if lower.contains("deepseek-chat") || lower.contains("deepseek-v3") {
        (64_000, Some(8_192), "DeepSeek V3 (64k context, 8k output)")
    } else if lower.contains("glm-5.3-flash")
        || lower.contains("glm-5.3")
        || lower.contains("glm-5.2")
    {
        (
            1_000_000,
            Some(128_000),
            "Zhipu GLM-5.3 Flash (1M context, 128k output)",
        )
    } else if lower.contains("glm-5") {
        (
            200_000,
            Some(128_000),
            "Zhipu GLM-5 (200k context, 128k output)",
        )
    } else if lower.contains("glm-4-plus") || lower.contains("glm-4-0520") {
        (
            128_000,
            Some(4_096),
            "Zhipu GLM-4 Plus (128k context, 4k output)",
        )
    } else if lower.contains("glm-4") {
        (
            128_000,
            Some(4_096),
            "Zhipu GLM-4 (128k context, 4k output)",
        )
    } else if lower.contains("llama-3.3") || lower.contains("llama-3.1") {
        (
            128_000,
            Some(8_192),
            "Meta Llama 3.1/3.3 (128k context, 8k output)",
        )
    } else if lower.contains("llama-3.2") {
        (
            128_000,
            Some(8_192),
            "Meta Llama 3.2 (128k context, 8k output)",
        )
    } else if lower.contains("llama-3") {
        (8_192, Some(2_048), "Meta Llama 3 (8k context, 2k output)")
    } else if lower.contains("qwen-2.5-coder") || lower.contains("qwen2.5-coder") {
        (
            128_000,
            Some(8_192),
            "Qwen 2.5 Coder (128k context, 8k output)",
        )
    } else if lower.contains("qwen-2.5") || lower.contains("qwen2.5") {
        (128_000, Some(8_192), "Qwen 2.5 (128k context, 8k output)")
    } else if lower.contains("gpt-4o-mini") {
        (
            128_000,
            Some(16_384),
            "OpenAI GPT-4o-mini (128k context, 16k output)",
        )
    } else if lower.contains("gpt-4o") {
        (
            128_000,
            Some(16_384),
            "OpenAI GPT-4o (128k context, 16k output)",
        )
    } else if lower.contains("gpt-4.5") {
        (
            128_000,
            Some(16_384),
            "OpenAI GPT-4.5 (128k context, 16k output)",
        )
    } else if lower.contains("o1-mini") || lower.contains("o3-mini") {
        (
            200_000,
            Some(100_000),
            "OpenAI o-series Mini (200k context, 100k output)",
        )
    } else if lower.contains("o1") {
        (
            200_000,
            Some(100_000),
            "OpenAI o1 Reasoning (200k context, 100k output)",
        )
    } else if lower.contains("gpt-4-turbo") {
        (
            128_000,
            Some(4_096),
            "OpenAI GPT-4 Turbo (128k context, 4k output)",
        )
    } else if lower.contains("gpt-3.5-turbo") {
        (
            16_385,
            Some(4_096),
            "OpenAI GPT-3.5 Turbo (16k context, 4k output)",
        )
    } else if lower.contains("mistral-large") || lower.contains("codestral") {
        (128_000, Some(8_192), "Mistral AI (128k context, 8k output)")
    } else {
        (
            128_000,
            Some(4_096),
            "Standard LLM (estimasi 128k context, 4k output)",
        )
    }
}

/// Probes a provider endpoint to check connectivity, authentication, model availability,
/// and auto-detects Context Window and Max Output Tokens.
pub fn probe_provider_and_model(
    provider: &ProviderConfig,
    model_override: Option<&str>,
) -> ProbeReport {
    let start_time = Instant::now();
    let model = model_override.unwrap_or(&provider.default_model);

    // Initial estimation from catalog
    let (mut detected_ctx, mut detected_out, note) = resolve_model_limits(model);

    // If provider config already has user overrides, honor them as base
    if let Some(c) = provider.context_window {
        detected_ctx = c;
    }
    if let Some(o) = provider.max_output_tokens {
        detected_out = Some(o);
    }

    match provider.protocol {
        ApiProtocol::OpenAi => probe_openai_provider(
            provider,
            model,
            start_time,
            detected_ctx,
            detected_out,
            note,
        ),
        ApiProtocol::Anthropic => probe_anthropic_provider(
            provider,
            model,
            start_time,
            detected_ctx,
            detected_out,
            note,
        ),
        ApiProtocol::Gemini => probe_gemini_provider(
            provider,
            model,
            start_time,
            detected_ctx,
            detected_out,
            note,
        ),
        ApiProtocol::Ollama => probe_ollama_provider(
            provider,
            model,
            start_time,
            detected_ctx,
            detected_out,
            note,
        ),
    }
}

fn extract_u64(val: Option<&serde_json::Value>) -> Option<u64> {
    val.and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    })
}

fn probe_openai_provider(
    provider: &ProviderConfig,
    model: &str,
    start_time: Instant,
    mut detected_ctx: u64,
    mut detected_out: Option<u64>,
    note: &'static str,
) -> ProbeReport {
    let base = provider.base_url.trim_end_matches('/');
    let (models_endpoint, comp_endpoint) = if base.ends_with("/v1") || base.ends_with("/api") {
        (
            format!("{}/models", base),
            format!("{}/chat/completions", base),
        )
    } else {
        (
            format!("{}/v1/models", base),
            format!("{}/v1/chat/completions", base),
        )
    };

    let mut request = ureq::get(&models_endpoint).timeout(std::time::Duration::from_secs(12));
    if !provider.api_key.is_empty() && provider.api_key != "none" {
        request = request.set("Authorization", &format!("Bearer {}", provider.api_key));
    }

    let res = request.call();
    let elapsed = start_time.elapsed().as_millis();

    match res {
        Ok(resp) => {
            let mut models_found = Vec::new();
            if let Ok(json_val) = resp.into_json::<serde_json::Value>() {
                let items_opt = json_val
                    .get("data")
                    .and_then(|d| d.as_array())
                    .or_else(|| json_val.get("models").and_then(|m| m.as_array()));

                if let Some(arr) = items_opt {
                    for item in arr {
                        let id_str_opt = item
                            .get("id")
                            .or_else(|| item.get("name"))
                            .and_then(|s| s.as_str());

                        if let Some(id_str) = id_str_opt {
                            models_found.push(id_str.to_string());

                            // Check if API response supplies explicit context length attributes
                            let matches_target = id_str.eq_ignore_ascii_case(model)
                                || id_str.to_lowercase().contains(&model.to_lowercase())
                                || model.to_lowercase().contains(&id_str.to_lowercase());

                            if matches_target {
                                if let Some(ctx) = extract_u64(item.get("context_length"))
                                    .or_else(|| extract_u64(item.get("max_context_length")))
                                    .or_else(|| extract_u64(item.get("context_window")))
                                    .or_else(|| extract_u64(item.get("max_model_len")))
                                    .or_else(|| {
                                        extract_u64(
                                            item.get("top_provider")
                                                .and_then(|tp| tp.get("context_length")),
                                        )
                                    })
                                {
                                    detected_ctx = ctx;
                                }
                                if let Some(max_t) = extract_u64(item.get("max_tokens"))
                                    .or_else(|| extract_u64(item.get("max_completion_tokens")))
                                    .or_else(|| extract_u64(item.get("max_output_tokens")))
                                {
                                    detected_out = Some(max_t);
                                }
                            }
                        }
                    }
                }
            }

            ProbeReport {
                success: true,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: true,
                models_found,
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: format!("OK: Connected successfully ({}ms)", elapsed),
                error_detail: None,
            }
        }
        Err(ureq::Error::Status(401, resp)) | Err(ureq::Error::Status(403, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            ProbeReport {
                success: false,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: false,
                models_found: Vec::new(),
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: "Authentication Failed: Invalid or missing API key (HTTP 401/403)"
                    .to_string(),
                error_detail: Some(err_body),
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            // Some proxies/servers don't implement /models (e.g. 404 or 405), fallback to minimal completion probe
            let fallback_err = resp.into_string().unwrap_or_default();
            let ping_body = json!({
                "model": model,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1
            });

            let mut post_req = ureq::post(&comp_endpoint)
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(12));
            if !provider.api_key.is_empty() && provider.api_key != "none" {
                post_req = post_req.set("Authorization", &format!("Bearer {}", provider.api_key));
            }

            let comp_res = post_req.send_json(ping_body);
            let comp_elapsed = start_time.elapsed().as_millis();

            match comp_res {
                Ok(_) => ProbeReport {
                    success: true,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: true,
                    models_found: vec![model.to_string()],
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "OK: Chat completions active (HTTP {} on /models bypassed, {}ms)",
                        code, comp_elapsed
                    ),
                    error_detail: None,
                },
                Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
                    ProbeReport {
                        success: false,
                        latency_ms: comp_elapsed,
                        endpoint_reachable: true,
                        auth_valid: false,
                        models_found: Vec::new(),
                        context_window: detected_ctx,
                        max_output_tokens: detected_out,
                        model_note: note.to_string(),
                        status_message: "Authentication Failed on test completion".to_string(),
                        error_detail: Some(fallback_err),
                    }
                }
                Err(e) => ProbeReport {
                    success: false,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: false,
                    models_found: Vec::new(),
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "Endpoint responded with HTTP {}: {}",
                        code, fallback_err
                    ),
                    error_detail: Some(e.to_string()),
                },
            }
        }
        Err(e) => ProbeReport {
            success: false,
            latency_ms: elapsed,
            endpoint_reachable: false,
            auth_valid: false,
            models_found: Vec::new(),
            context_window: detected_ctx,
            max_output_tokens: detected_out,
            model_note: note.to_string(),
            status_message: format!("Connection failed: {}", e),
            error_detail: Some(e.to_string()),
        },
    }
}

fn probe_anthropic_provider(
    provider: &ProviderConfig,
    model: &str,
    start_time: Instant,
    detected_ctx: u64,
    detected_out: Option<u64>,
    note: &'static str,
) -> ProbeReport {
    let base = provider.base_url.trim_end_matches('/');
    let models_endpoint = if base.ends_with("/v1") {
        format!("{}/models", base)
    } else {
        format!("{}/v1/models", base)
    };

    let request = ureq::get(&models_endpoint)
        .set("x-api-key", &provider.api_key)
        .set("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(12));

    let res = request.call();
    let elapsed = start_time.elapsed().as_millis();

    match res {
        Ok(resp) => {
            let mut models_found = Vec::new();
            if let Ok(json_val) = resp.into_json::<serde_json::Value>() {
                let items_opt = json_val
                    .get("data")
                    .and_then(|d| d.as_array())
                    .or_else(|| json_val.get("models").and_then(|m| m.as_array()));

                if let Some(arr) = items_opt {
                    for item in arr {
                        if let Some(id_str) = item
                            .get("id")
                            .or_else(|| item.get("name"))
                            .and_then(|s| s.as_str())
                        {
                            models_found.push(id_str.to_string());
                        }
                    }
                }
            }
            ProbeReport {
                success: true,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: true,
                models_found,
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: format!("OK: Anthropic API connected ({}ms)", elapsed),
                error_detail: None,
            }
        }
        Err(ureq::Error::Status(401, resp)) | Err(ureq::Error::Status(403, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            ProbeReport {
                success: false,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: false,
                models_found: Vec::new(),
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: "Authentication Failed: Invalid Anthropic x-api-key".to_string(),
                error_detail: Some(err_body),
            }
        }
        Err(_) => {
            // Fallback: Test minimal Anthropic messages call
            let msg_endpoint = if base.ends_with("/messages") {
                base.to_string()
            } else if base.ends_with("/v1") {
                format!("{}/messages", base)
            } else {
                format!("{}/v1/messages", base)
            };

            let ping_body = json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}],
                "max_tokens": 1
            });

            let test_res = ureq::post(&msg_endpoint)
                .set("x-api-key", &provider.api_key)
                .set("anthropic-version", "2023-06-01")
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(12))
                .send_json(ping_body);

            let test_elapsed = start_time.elapsed().as_millis();
            match test_res {
                Ok(_) => ProbeReport {
                    success: true,
                    latency_ms: test_elapsed,
                    endpoint_reachable: true,
                    auth_valid: true,
                    models_found: vec![model.to_string()],
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "OK: Anthropic Messages endpoint active ({}ms)",
                        test_elapsed
                    ),
                    error_detail: None,
                },
                Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
                    ProbeReport {
                        success: false,
                        latency_ms: test_elapsed,
                        endpoint_reachable: true,
                        auth_valid: false,
                        models_found: Vec::new(),
                        context_window: detected_ctx,
                        max_output_tokens: detected_out,
                        model_note: note.to_string(),
                        status_message: "Authentication Failed: Invalid Anthropic API key"
                            .to_string(),
                        error_detail: None,
                    }
                }
                Err(e) => ProbeReport {
                    success: false,
                    latency_ms: test_elapsed,
                    endpoint_reachable: false,
                    auth_valid: false,
                    models_found: Vec::new(),
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!("Anthropic endpoint error: {}", e),
                    error_detail: Some(e.to_string()),
                },
            }
        }
    }
}

pub fn probe_gemini_provider(
    provider: &ProviderConfig,
    model: &str,
    start_time: Instant,
    mut detected_ctx: u64,
    mut detected_out: Option<u64>,
    note: &'static str,
) -> ProbeReport {
    let base = provider.base_url.trim_end_matches('/');
    let clean_model = model.strip_prefix("models/").unwrap_or(model);

    let models_endpoint = if base.contains("/v1beta") {
        format!("{}/models", base)
    } else {
        format!("{}/v1beta/models", base)
    };

    let mut request = ureq::get(&models_endpoint).timeout(std::time::Duration::from_secs(12));
    if !provider.api_key.is_empty() && provider.api_key != "none" {
        request = request.set("x-goog-api-key", &provider.api_key);
    }

    let res = request.call();
    let elapsed = start_time.elapsed().as_millis();

    match res {
        Ok(resp) => {
            let mut models_found = Vec::new();
            if let Ok(json_val) = resp.into_json::<serde_json::Value>() {
                if let Some(arr) = json_val.get("models").and_then(|m| m.as_array()) {
                    for item in arr {
                        if let Some(raw_name) = item.get("name").and_then(|n| n.as_str()) {
                            let name = raw_name
                                .strip_prefix("models/")
                                .unwrap_or(raw_name)
                                .to_string();
                            models_found.push(name.clone());

                            let matches_target = name.eq_ignore_ascii_case(clean_model)
                                || name.to_lowercase().contains(&clean_model.to_lowercase())
                                || clean_model.to_lowercase().contains(&name.to_lowercase());

                            if matches_target {
                                if let Some(ctx) = extract_u64(item.get("inputTokenLimit")) {
                                    detected_ctx = ctx;
                                }
                                if let Some(max_t) = extract_u64(item.get("outputTokenLimit")) {
                                    detected_out = Some(max_t);
                                }
                            }
                        }
                    }
                }
            }

            ProbeReport {
                success: true,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: true,
                models_found,
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: format!("OK: Connected successfully to Gemini API ({}ms)", elapsed),
                error_detail: None,
            }
        }
        Err(ureq::Error::Status(400, resp))
        | Err(ureq::Error::Status(401, resp))
        | Err(ureq::Error::Status(403, resp)) => {
            let err_body = resp.into_string().unwrap_or_default();
            ProbeReport {
                success: false,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: false,
                models_found: Vec::new(),
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message:
                    "Authentication Failed: Invalid Gemini API key (HTTP 400/401/403)".to_string(),
                error_detail: Some(err_body),
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let fallback_err = resp.into_string().unwrap_or_default();
            let gen_endpoint = if base.contains("/v1beta") {
                format!("{}/models/{}:generateContent", base, clean_model)
            } else {
                format!("{}/v1beta/models/{}:generateContent", base, clean_model)
            };

            let ping_body = json!({
                "contents": [
                    {
                        "parts": [
                            {"text": "ping"}
                        ]
                    }
                ],
                "generationConfig": {
                    "maxOutputTokens": 1
                }
            });

            let mut post_req = ureq::post(&gen_endpoint)
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(12));
            if !provider.api_key.is_empty() && provider.api_key != "none" {
                post_req = post_req.set("x-goog-api-key", &provider.api_key);
            }

            let comp_res = post_req.send_json(ping_body);
            let comp_elapsed = start_time.elapsed().as_millis();

            match comp_res {
                Ok(_) => ProbeReport {
                    success: true,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: true,
                    models_found: vec![clean_model.to_string()],
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "OK: Gemini generateContent active (HTTP {} on /models bypassed, {}ms)",
                        code, comp_elapsed
                    ),
                    error_detail: None,
                },
                Err(ureq::Error::Status(400, _))
                | Err(ureq::Error::Status(401, _))
                | Err(ureq::Error::Status(403, _)) => ProbeReport {
                    success: false,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: false,
                    models_found: Vec::new(),
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: "Authentication Failed on Gemini generateContent".to_string(),
                    error_detail: Some(fallback_err),
                },
                Err(e) => ProbeReport {
                    success: false,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: false,
                    models_found: Vec::new(),
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "Gemini endpoint responded with HTTP {}: {}",
                        code, fallback_err
                    ),
                    error_detail: Some(e.to_string()),
                },
            }
        }
        Err(e) => ProbeReport {
            success: false,
            latency_ms: elapsed,
            endpoint_reachable: false,
            auth_valid: false,
            models_found: Vec::new(),
            context_window: detected_ctx,
            max_output_tokens: detected_out,
            model_note: note.to_string(),
            status_message: format!("Connection failed: {}", e),
            error_detail: Some(e.to_string()),
        },
    }
}

pub fn probe_ollama_provider(
    provider: &ProviderConfig,
    model: &str,
    start_time: Instant,
    detected_ctx: u64,
    detected_out: Option<u64>,
    note: &'static str,
) -> ProbeReport {
    let base = provider.base_url.trim_end_matches('/');
    let base = if base.ends_with("/v1") {
        base.trim_end_matches("/v1")
    } else {
        base
    };

    let tags_endpoint = format!("{}/api/tags", base);
    let request = ureq::get(&tags_endpoint).timeout(std::time::Duration::from_secs(12));

    let res = request.call();
    let elapsed = start_time.elapsed().as_millis();

    match res {
        Ok(resp) => {
            let mut models_found = Vec::new();
            if let Ok(json_val) = resp.into_json::<serde_json::Value>() {
                if let Some(arr) = json_val.get("models").and_then(|m| m.as_array()) {
                    for item in arr {
                        if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                            models_found.push(name.to_string());
                        } else if let Some(model_str) = item.get("model").and_then(|m| m.as_str()) {
                            models_found.push(model_str.to_string());
                        }
                    }
                }
            }

            ProbeReport {
                success: true,
                latency_ms: elapsed,
                endpoint_reachable: true,
                auth_valid: true,
                models_found,
                context_window: detected_ctx,
                max_output_tokens: detected_out,
                model_note: note.to_string(),
                status_message: format!("OK: Connected successfully to Ollama API ({}ms)", elapsed),
                error_detail: None,
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let fallback_err = resp.into_string().unwrap_or_default();
            let chat_endpoint = format!("{}/api/chat", base);
            let ping_body = json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}],
                "stream": false
            });

            let post_req = ureq::post(&chat_endpoint)
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(12));

            let comp_res = post_req.send_json(ping_body);
            let comp_elapsed = start_time.elapsed().as_millis();

            match comp_res {
                Ok(_) => ProbeReport {
                    success: true,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: true,
                    models_found: vec![model.to_string()],
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "OK: Ollama chat endpoint active (HTTP {} on /api/tags bypassed, {}ms)",
                        code, comp_elapsed
                    ),
                    error_detail: None,
                },
                Err(e) => ProbeReport {
                    success: false,
                    latency_ms: comp_elapsed,
                    endpoint_reachable: true,
                    auth_valid: false,
                    models_found: Vec::new(),
                    context_window: detected_ctx,
                    max_output_tokens: detected_out,
                    model_note: note.to_string(),
                    status_message: format!(
                        "Ollama endpoint responded with HTTP {}: {}",
                        code, fallback_err
                    ),
                    error_detail: Some(e.to_string()),
                },
            }
        }
        Err(e) => ProbeReport {
            success: false,
            latency_ms: elapsed,
            endpoint_reachable: false,
            auth_valid: false,
            models_found: Vec::new(),
            context_window: detected_ctx,
            max_output_tokens: detected_out,
            model_note: note.to_string(),
            status_message: format!("Connection failed: {}", e),
            error_detail: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_limits_gemini_variants() {
        let (ctx_flash, out_flash, note_flash) = resolve_model_limits("gemini-2.5-flash");
        assert_eq!(ctx_flash, 1_000_000);
        assert_eq!(out_flash, Some(8_192));
        assert!(note_flash.contains("Gemini 2.5"));

        let (ctx_pro, out_pro, note_pro) = resolve_model_limits("gemini-2.5-pro");
        assert_eq!(ctx_pro, 1_000_000);
        assert_eq!(out_pro, Some(8_192));
        assert!(note_pro.contains("Gemini 2.5"));

        let (ctx_20, out_20, note_20) = resolve_model_limits("gemini-2.0-flash");
        assert_eq!(ctx_20, 1_000_000);
        assert_eq!(out_20, Some(8_192));
        assert!(note_20.contains("Gemini 2.0"));
    }

    #[test]
    fn test_resolve_model_limits_qwen_coder() {
        let (ctx, out, note) = resolve_model_limits("qwen2.5-coder:7b");
        assert_eq!(ctx, 128_000);
        assert_eq!(out, Some(8_192));
        assert!(note.contains("Qwen 2.5 Coder"));
    }

    #[test]
    fn test_probe_gemini_unreachable_endpoint() {
        let provider = ProviderConfig {
            id: "gemini".to_string(),
            name: "Gemini".to_string(),
            protocol: ApiProtocol::Gemini,
            base_url: "http://127.0.0.1:59998".to_string(),
            api_key: "test-key".to_string(),
            default_model: "gemini-2.5-flash".to_string(),
            context_window: None,
            max_output_tokens: None,
        };

        let report = probe_provider_and_model(&provider, None);
        assert!(!report.success);
        assert!(!report.endpoint_reachable);
        assert!(report.status_message.contains("Connection failed"));
    }

    #[test]
    fn test_probe_ollama_unreachable_endpoint() {
        let provider = ProviderConfig {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            protocol: ApiProtocol::Ollama,
            base_url: "http://127.0.0.1:59999".to_string(),
            api_key: "ollama".to_string(),
            default_model: "qwen2.5-coder:7b".to_string(),
            context_window: None,
            max_output_tokens: None,
        };

        let report = probe_provider_and_model(&provider, None);
        assert!(!report.success);
        assert!(!report.endpoint_reachable);
        assert!(report.status_message.contains("Connection failed"));
    }
}
