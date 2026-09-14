use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApiProtocol {
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
}

impl std::fmt::Display for ApiProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiProtocol::OpenAi => write!(f, "OpenAI-Compatible"),
            ApiProtocol::Anthropic => write!(f, "Anthropic Messages API"),
            ApiProtocol::Gemini => write!(f, "Google Gemini AI Studio"),
            ApiProtocol::Ollama => write!(f, "Ollama Native API"),
        }
    }
}

impl std::str::FromStr for ApiProtocol {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        if lower.contains("anthropic") || lower.contains("claude") {
            Ok(ApiProtocol::Anthropic)
        } else if lower.contains("gemini") || lower.contains("google") {
            Ok(ApiProtocol::Gemini)
        } else if lower.contains("ollama") {
            Ok(ApiProtocol::Ollama)
        } else {
            Ok(ApiProtocol::OpenAi)
        }
    }
}

impl From<&str> for ApiProtocol {
    fn from(s: &str) -> Self {
        <Self as std::str::FromStr>::from_str(s).unwrap_or(ApiProtocol::OpenAi)
    }
}

impl ApiProtocol {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        <Self as std::str::FromStr>::from_str(s).unwrap_or(ApiProtocol::OpenAi)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub protocol: ApiProtocol,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvidersRegistry {
    pub active_provider_id: String,
    pub providers: Vec<ProviderConfig>,
}

impl Default for ProvidersRegistry {
    fn default() -> Self {
        let env_key = std::env::var("AI_API_KEY").unwrap_or_default();
        let env_base = std::env::var("AI_BASE_URL")
            .unwrap_or_else(|_| "https://openagentic.id/api/v1".to_string());
        let env_model = std::env::var("AI_MODEL").unwrap_or_else(|_| "glm-5.3-flash".to_string());

        let openagentic_key = if !env_key.is_empty() && env_key != "your_api_key_here" {
            env_key.clone()
        } else {
            String::new()
        };

        Self {
            active_provider_id: "openagentic".to_string(),
            providers: vec![
                ProviderConfig {
                    id: "openagentic".to_string(),
                    name: "OpenAgentic (Zhipu GLM)".to_string(),
                    protocol: ApiProtocol::OpenAi,
                    base_url: if env_base.contains("openagentic") {
                        env_base
                    } else {
                        "https://openagentic.id/api/v1".to_string()
                    },
                    api_key: openagentic_key,
                    default_model: if env_model.contains("glm") {
                        env_model
                    } else {
                        "glm-5.3-flash".to_string()
                    },
                    context_window: Some(1_000_000),
                    max_output_tokens: Some(128_000),
                },
                ProviderConfig {
                    id: "openai".to_string(),
                    name: "OpenAI Official".to_string(),
                    protocol: ApiProtocol::OpenAi,
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: String::new(),
                    default_model: "gpt-4o-mini".to_string(),
                    context_window: Some(128_000),
                    max_output_tokens: Some(16_384),
                },
                ProviderConfig {
                    id: "anthropic".to_string(),
                    name: "Anthropic Claude".to_string(),
                    protocol: ApiProtocol::Anthropic,
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    api_key: String::new(),
                    default_model: "claude-3-5-sonnet-20241022".to_string(),
                    context_window: Some(200_000),
                    max_output_tokens: Some(8_192),
                },
                ProviderConfig {
                    id: "deepseek".to_string(),
                    name: "DeepSeek Official".to_string(),
                    protocol: ApiProtocol::OpenAi,
                    base_url: "https://api.deepseek.com/v1".to_string(),
                    api_key: String::new(),
                    default_model: "deepseek-chat".to_string(),
                    context_window: Some(64_000),
                    max_output_tokens: Some(8_192),
                },
                ProviderConfig {
                    id: "groq".to_string(),
                    name: "Groq Cloud".to_string(),
                    protocol: ApiProtocol::OpenAi,
                    base_url: "https://api.groq.com/openai/v1".to_string(),
                    api_key: String::new(),
                    default_model: "llama-3.3-70b-versatile".to_string(),
                    context_window: Some(128_000),
                    max_output_tokens: Some(8_192),
                },
                ProviderConfig {
                    id: "gemini".to_string(),
                    name: "Google Gemini (AI Studio)".to_string(),
                    protocol: ApiProtocol::Gemini,
                    base_url: "https://generativelanguage.googleapis.com".to_string(),
                    api_key: std::env::var("GEMINI_API_KEY").unwrap_or_default(),
                    default_model: "gemini-2.5-flash".to_string(),
                    context_window: Some(1_000_000),
                    max_output_tokens: Some(8_192),
                },
                ProviderConfig {
                    id: "ollama".to_string(),
                    name: "Ollama Local".to_string(),
                    protocol: ApiProtocol::Ollama,
                    base_url: "http://localhost:11434".to_string(),
                    api_key: "ollama".to_string(),
                    default_model: "qwen2.5-coder:7b".to_string(),
                    context_window: Some(32_768),
                    max_output_tokens: Some(4_096),
                },
                ProviderConfig {
                    id: "localai".to_string(),
                    name: "LocalAI / vLLM".to_string(),
                    protocol: ApiProtocol::OpenAi,
                    base_url: "http://localhost:8080/v1".to_string(),
                    api_key: "none".to_string(),
                    default_model: "gpt-4".to_string(),
                    context_window: Some(32_768),
                    max_output_tokens: Some(4_096),
                },
            ],
        }
    }
}

pub fn get_providers_path() -> PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join("ctrl-cli-test-providers.json")
    }
    #[cfg(not(test))]
    {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".ctrl-cli").join("providers.json")
    }
}

pub fn load_providers_registry() -> ProvidersRegistry {
    let path = get_providers_path();

    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(mut reg) = serde_json::from_str::<ProvidersRegistry>(&data) {
                // Sync env vars to matching provider if its api_key is empty
                let env_key = std::env::var("AI_API_KEY").unwrap_or_default();
                let env_base = std::env::var("AI_BASE_URL").unwrap_or_default();
                let gemini_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                let mut changed = false;
                if !gemini_key.is_empty() {
                    for p in reg.providers.iter_mut() {
                        if p.id == "gemini" && p.api_key.is_empty() {
                            p.api_key = gemini_key.clone();
                            changed = true;
                        }
                    }
                }
                if !env_key.is_empty() && env_key != "your_api_key_here" {
                    for p in reg.providers.iter_mut() {
                        let matches = if !env_base.is_empty() {
                            p.base_url.trim_end_matches('/') == env_base.trim_end_matches('/')
                                || (p.id == "openagentic" && env_base.contains("openagentic"))
                        } else {
                            p.id == "openagentic"
                        };
                        if matches && p.api_key.is_empty() {
                            p.api_key = env_key.clone();
                            changed = true;
                        }
                    }
                }
                if changed {
                    save_providers_registry(&reg);
                }
                return reg;
            }
        }
    }

    // Fallback: check workspace local .ctrl/providers.json
    let local_path = PathBuf::from(".ctrl").join("providers.json");
    if local_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&local_path) {
            if let Ok(reg) = serde_json::from_str::<ProvidersRegistry>(&data) {
                return reg;
            }
        }
    }

    let default_reg = ProvidersRegistry::default();
    save_providers_registry(&default_reg);
    default_reg
}

pub fn save_providers_registry(registry: &ProvidersRegistry) {
    #[cfg(test)]
    {
        let _ = registry;
    }
    #[cfg(not(test))]
    {
        let path = get_providers_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(registry) {
            let _ = std::fs::write(&path, &json);
            // Also sync to workspace .ctrl if available
            let local_ctrl = PathBuf::from(".ctrl");
            if local_ctrl.is_dir() {
                let _ = std::fs::write(local_ctrl.join("providers.json"), &json);
            }
        }
    }
}

impl ProvidersRegistry {
    pub fn get_active_provider(&self) -> &ProviderConfig {
        self.providers
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&self.active_provider_id))
            .unwrap_or_else(|| &self.providers[0])
    }

    #[allow(dead_code)]
    pub fn get_active_provider_mut(&mut self) -> Option<&mut ProviderConfig> {
        let id = self.active_provider_id.clone();
        self.providers
            .iter_mut()
            .find(|p| p.id.eq_ignore_ascii_case(&id))
    }

    pub fn switch_active(&mut self, target_id: &str) -> Result<ProviderConfig> {
        let found = self
            .providers
            .iter()
            .find(|p| {
                p.id.eq_ignore_ascii_case(target_id)
                    || p.name.to_lowercase().contains(&target_id.to_lowercase())
            })
            .cloned();

        if let Some(mut p) = found {
            self.active_provider_id = p.id.clone();
            if p.api_key.is_empty() {
                if p.id == "gemini" {
                    let gemini_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                    if !gemini_key.is_empty() {
                        p.api_key = gemini_key.clone();
                        if let Some(pos) = self.providers.iter().position(|pr| pr.id == p.id) {
                            self.providers[pos].api_key = gemini_key;
                        }
                    }
                }
                let env_key = std::env::var("AI_API_KEY").unwrap_or_default();
                let env_base = std::env::var("AI_BASE_URL").unwrap_or_default();
                if !env_key.is_empty() && env_key != "your_api_key_here" {
                    let matches = if !env_base.is_empty() {
                        p.base_url.trim_end_matches('/') == env_base.trim_end_matches('/')
                            || (p.id == "openagentic" && env_base.contains("openagentic"))
                    } else {
                        p.id == "openagentic"
                    };
                    if matches {
                        if let Some(pos) = self.providers.iter().position(|pr| pr.id == p.id) {
                            self.providers[pos].api_key = env_key.clone();
                            p.api_key = env_key;
                        }
                    }
                }
            }
            save_providers_registry(self);
            Ok(p)
        } else {
            bail!("Provider '{}' not found in registry.", target_id)
        }
    }

    pub fn add_or_update(&mut self, provider: ProviderConfig) {
        if let Some(idx) = self
            .providers
            .iter()
            .position(|p| p.id.eq_ignore_ascii_case(&provider.id))
        {
            self.providers[idx] = provider;
        } else {
            self.providers.push(provider);
        }
        save_providers_registry(self);
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        if self.providers.len() <= 1 {
            bail!("Cannot delete the last remaining provider.");
        }
        if self.active_provider_id.eq_ignore_ascii_case(id) {
            bail!("Cannot delete the currently active provider. Switch to another provider first.");
        }

        let orig_len = self.providers.len();
        self.providers.retain(|p| !p.id.eq_ignore_ascii_case(id));
        if self.providers.len() < orig_len {
            save_providers_registry(self);
            Ok(())
        } else {
            bail!("Provider '{}' not found.", id)
        }
    }

    pub fn update_limits(&mut self, id: &str, context_window: u64, max_output: Option<u64>) {
        if let Some(p) = self
            .providers
            .iter_mut()
            .find(|p| p.id.eq_ignore_ascii_case(id))
        {
            p.context_window = Some(context_window);
            p.max_output_tokens = max_output;
            save_providers_registry(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_protocol_from_str_and_display() {
        assert_eq!(ApiProtocol::from_str("openai"), ApiProtocol::OpenAi);
        assert_eq!(ApiProtocol::from_str("anthropic"), ApiProtocol::Anthropic);
        assert_eq!(ApiProtocol::from_str("claude-3-5"), ApiProtocol::Anthropic);
        assert_eq!(ApiProtocol::from_str("gemini"), ApiProtocol::Gemini);
        assert_eq!(ApiProtocol::from_str("google-ai"), ApiProtocol::Gemini);
        assert_eq!(ApiProtocol::from_str("ollama"), ApiProtocol::Ollama);
        assert_eq!(ApiProtocol::from_str("custom-proxy"), ApiProtocol::OpenAi);

        assert_eq!(format!("{}", ApiProtocol::OpenAi), "OpenAI-Compatible");
        assert_eq!(format!("{}", ApiProtocol::Anthropic), "Anthropic Messages API");
        assert_eq!(format!("{}", ApiProtocol::Gemini), "Google Gemini AI Studio");
        assert_eq!(format!("{}", ApiProtocol::Ollama), "Ollama Native API");
    }

    #[test]
    fn test_api_protocol_serde_roundtrip() {
        let protocols = vec![
            (ApiProtocol::OpenAi, "\"openai\""),
            (ApiProtocol::Anthropic, "\"anthropic\""),
            (ApiProtocol::Gemini, "\"gemini\""),
            (ApiProtocol::Ollama, "\"ollama\""),
        ];

        for (proto, expected_json) in protocols {
            let serialized = serde_json::to_string(&proto).expect("serialize protocol");
            assert_eq!(serialized, expected_json);
            let deserialized: ApiProtocol =
                serde_json::from_str(&serialized).expect("deserialize protocol");
            assert_eq!(deserialized, proto);
        }
    }

    #[test]
    fn test_default_registry_contains_gemini_and_ollama() {
        let reg = ProvidersRegistry::default();
        let gemini = reg
            .providers
            .iter()
            .find(|p| p.id == "gemini")
            .expect("gemini provider present in default registry");
        assert_eq!(gemini.protocol, ApiProtocol::Gemini);
        assert_eq!(gemini.base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(gemini.default_model, "gemini-2.5-flash");
        assert_eq!(gemini.context_window, Some(1_000_000));
        assert_eq!(gemini.max_output_tokens, Some(8_192));

        let ollama = reg
            .providers
            .iter()
            .find(|p| p.id == "ollama")
            .expect("ollama provider present in default registry");
        assert_eq!(ollama.protocol, ApiProtocol::Ollama);
        assert_eq!(ollama.base_url, "http://localhost:11434");
        assert_eq!(ollama.default_model, "qwen2.5-coder:7b");
    }

    #[test]
    fn test_switch_active_to_gemini_and_ollama() {
        let mut reg = ProvidersRegistry::default();
        let switched = reg.switch_active("gemini").expect("switch to gemini");
        assert_eq!(switched.id, "gemini");
        assert_eq!(reg.active_provider_id, "gemini");
        assert_eq!(reg.get_active_provider().protocol, ApiProtocol::Gemini);

        let switched_ollama = reg.switch_active("ollama").expect("switch to ollama");
        assert_eq!(switched_ollama.id, "ollama");
        assert_eq!(reg.active_provider_id, "ollama");
        assert_eq!(reg.get_active_provider().protocol, ApiProtocol::Ollama);
    }
}

