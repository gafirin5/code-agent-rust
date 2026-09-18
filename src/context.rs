use crate::agent::probe::resolve_model_limits;
use crate::agent::provider::ProviderConfig;
use crate::types::Usage;

#[derive(Clone, Debug)]
pub struct ModelContextInfo {
    pub context_window: u64,
    pub max_output: Option<u64>,
    pub note: String,
}

pub fn get_model_context_info(model: &str, provider: Option<&ProviderConfig>) -> ModelContextInfo {
    let (cat_ctx, cat_out, cat_note) = resolve_model_limits(model);

    let context_window = provider.and_then(|p| p.context_window).unwrap_or(cat_ctx);
    let max_output = provider.and_then(|p| p.max_output_tokens).or(cat_out);

    ModelContextInfo {
        context_window,
        max_output,
        note: cat_note.to_string(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionTokenTracker {
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub query_count: u64,
    pub last_usage: Option<Usage>,
    pub last_model: Option<String>,
    pub last_content: Option<String>,
}

impl SessionTokenTracker {
    pub fn record(&mut self, usage: Option<&Usage>, model: &str, content: &str) {
        self.query_count += 1;
        self.last_model = Some(model.to_string());
        self.last_content = Some(content.to_string());
        if let Some(u) = usage {
            let p = u.prompt_tokens.unwrap_or(0);
            let c = u.completion_tokens.unwrap_or(0);
            let t = u.total_tokens.unwrap_or(p + c);
            self.total_prompt_tokens += p;
            self.total_completion_tokens += c;
            self.total_tokens += t;
            self.last_usage = Some(u.clone());
        }
    }
}
