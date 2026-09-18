use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserProfile {
    pub name: String,
    pub tech_stack: Vec<String>,
    pub response_language: String,
    pub coding_style: String,
    #[serde(default = "default_true")]
    pub show_token_usage: bool,
    #[serde(default)]
    pub default_ui: Option<String>,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            name: "galangfjr".to_string(),
            tech_stack: vec![
                "Python".to_string(),
                "TypeScript".to_string(),
                "Rust".to_string(),
                "Zig".to_string(),
            ],
            response_language: "Bahasa Indonesia".to_string(),
            coding_style: "Tulis kode yang bersih (clean code), modern, idiomatik, efisien, dan minim dependensi tidak perlu. Berikan penjelasan ringkas dan solutif.".to_string(),
            show_token_usage: true,
            default_ui: None,
        }
    }
}

pub fn load_user_profile() -> UserProfile {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let profile_dir = home.join(".ctrl-cli");
    let profile_path = profile_dir.join("profile.json");

    if profile_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&profile_path) {
            if let Ok(prof) = serde_json::from_str::<UserProfile>(&data) {
                return prof;
            }
        }
    }

    let default_profile = UserProfile::default();
    if std::fs::create_dir_all(&profile_dir).is_ok() {
        if let Ok(json_str) = serde_json::to_string_pretty(&default_profile) {
            let _ = std::fs::write(&profile_path, json_str);
        }
    }
    default_profile
}

pub fn save_user_profile(profile: &UserProfile) {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let profile_dir = home.join(".ctrl-cli");
    let profile_path = profile_dir.join("profile.json");

    if std::fs::create_dir_all(&profile_dir).is_ok() {
        if let Ok(json_str) = serde_json::to_string_pretty(profile) {
            let _ = std::fs::write(&profile_path, json_str);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportedLanguage {
    English,
    Indonesian,
    Chinese,
}

impl std::str::FromStr for SupportedLanguage {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        let trimmed = lower.trim();
        if trimmed == "id"
            || trimmed == "in"
            || trimmed == "ina"
            || trimmed == "ind"
            || lower.contains("indo")
            || lower.contains("bahasa")
        {
            Ok(SupportedLanguage::Indonesian)
        } else if trimmed == "zh"
            || trimmed == "cn"
            || trimmed == "zho"
            || lower.contains("chin")
            || lower.contains("中文")
            || lower.contains("mandarin")
        {
            Ok(SupportedLanguage::Chinese)
        } else {
            Ok(SupportedLanguage::English)
        }
    }
}

impl SupportedLanguage {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        <Self as std::str::FromStr>::from_str(s).unwrap_or(SupportedLanguage::English)
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SupportedLanguage::English => "English",
            SupportedLanguage::Indonesian => "Bahasa Indonesia",
            SupportedLanguage::Chinese => "中文 (Chinese)",
        }
    }

    pub fn directive(&self) -> &'static str {
        match self {
            SupportedLanguage::English => "\
[Active Communication Language: English]
- You MUST communicate, respond, reason, and explain exclusively in English.
- Code comments, documentation, and technical explanations must be in clear English.",
            SupportedLanguage::Indonesian => "\
[Active Communication Language: Bahasa Indonesia]
- Anda HARUS berkomunikasi, merespon, bernalar, dan memberikan penjelasan secara konsisten dalam Bahasa Indonesia.
- Istilah teknis pemrograman standar (seperti fungsi, method, keyword) boleh dipertahankan bila relevan.",
            SupportedLanguage::Chinese => "\
[Active Communication Language: 中文 (Chinese)]
- 你必须全程使用中文（简体中文）进行思考、推理、回复与解释。
- 保证代码注释、架构说明以及与用户的沟通清晰、专业且通俗易懂。",
        }
    }
}
