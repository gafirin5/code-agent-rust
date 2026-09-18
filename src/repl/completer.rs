use inquire::autocompletion::{Autocomplete, Replacement};
use inquire::error::CustomUserError;

use crate::repl::commands::COMMAND_SPECS;
use crate::skill::get_available_skills;

#[derive(Clone, Default)]
pub struct SlashCompleter;

impl Autocomplete for SlashCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut suggestions = Vec::new();
        let trimmed = input.trim();

        if trimmed == "?"
            || trimmed == "/?"
            || trimmed.starts_with("? ")
            || trimmed.starts_with("/? ")
        {
            suggestions.push("/help".to_string());
            suggestions.push("? /compact".to_string());
            suggestions.push("? /provider".to_string());
            suggestions.push("? /lang".to_string());
            return Ok(suggestions);
        }

        if let Some(prefix) = input.strip_prefix("/model ") {
            let models = [
                "glm-5.3-flash",
                "glm-4-plus",
                "deepseek-chat",
                "deepseek-reasoner",
                "claude-3-5-sonnet-20241022",
                "claude-3-7-sonnet",
                "llama-3.3-70b-versatile",
                "qwen-2.5-coder-32b",
                "gpt-4o-mini",
                "gpt-4o",
            ];
            for m in models {
                if m.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/model {}", m));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/skill ") {
            let mut skill_ids: Vec<String> = get_available_skills()
                .into_iter()
                .map(|s| s.id.into_owned())
                .collect();
            let root = crate::tools::filesystem::get_workspace_root();
            let discovered = crate::tools::skills::discover_skills(&root);
            for s in discovered {
                if !skill_ids.iter().any(|id| id.eq_ignore_ascii_case(&s.name)) {
                    skill_ids.push(s.name);
                }
            }
            skill_ids.push("reset".to_string());
            for id in skill_ids {
                if id.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/skill {}", id));
                }
            }
        } else if input.starts_with("/lang ") || input.starts_with("/language ") {
            let prefix = input
                .strip_prefix("/lang ")
                .or_else(|| input.strip_prefix("/language "))
                .unwrap_or_default();
            let langs = ["en (English)", "id (Bahasa Indonesia)", "zh (中文)"];
            for l in langs {
                if l.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    let code = l.split_whitespace().next().unwrap_or("en");
                    suggestions.push(format!("/lang {}", code));
                }
            }
        } else if input.starts_with("/provider ") || input.starts_with("/providers ") {
            let prefix = input
                .strip_prefix("/provider ")
                .or_else(|| input.strip_prefix("/providers "))
                .unwrap_or_default();
            let subs = ["list", "switch", "add", "delete", "probe"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/provider {}", s));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/tokens ") {
            let subs = ["toggle", "on", "off"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/tokens {}", s));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/tasks ") {
            let subs = ["list", "view", "cancel", "wait", "logs", "clear"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/tasks {}", s));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/stats ") {
            let subs = ["table", "json", "--json", "reset"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/stats {}", s));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/metrics ") {
            let subs = ["table", "json", "--json", "reset"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/metrics {}", s));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/skills info ") {
            let root = crate::tools::filesystem::get_workspace_root();
            let discovered = crate::tools::skills::discover_skills(&root);
            for s in discovered {
                if s.name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/skills info {}", s.name));
                }
            }
        } else if let Some(prefix) = input.strip_prefix("/skills ") {
            let subs = ["list", "info"];
            for s in subs {
                if s.starts_with(&prefix.to_lowercase()) {
                    suggestions.push(format!("/skills {}", s));
                }
            }
        } else if input.starts_with('/') {
            let lower_input = input.to_lowercase();
            let subcommands = [
                (
                    "/skills list",
                    "Tampilkan daftar semua skill yang ditemukan di workspace",
                ),
                (
                    "/skills info",
                    "Tampilkan detail dan prompt template skill tertentu",
                ),
                (
                    "/provider list",
                    "Tampilkan tabel semua provider terkonfigurasi",
                ),
                ("/provider switch", "Beralih ke provider tertentu"),
                (
                    "/provider add",
                    "Tambah konfigurasi provider / custom endpoint baru",
                ),
                ("/provider delete", "Hapus konfigurasi provider"),
                (
                    "/provider probe",
                    "Uji konektivitas & context limits provider",
                ),
                (
                    "/tokens toggle",
                    "Aktifkan / nonaktifkan badge token otomatis",
                ),
                ("/stream toggle", "Toggle on/off streaming SSE"),
                ("/stats table", "Tampilkan tabel metrik resource sistem"),
                ("/stats json", "Output metrik proses format JSON"),
                ("/lang en", "Switch response language to English"),
                ("/lang id", "Ganti bahasa respon ke Bahasa Indonesia"),
                ("/lang zh", "切换回复语言为中文 (Chinese)"),
            ];
            for (sc, desc) in subcommands {
                if sc.starts_with(&lower_input) {
                    suggestions.push(format!("{:<17} — {}", sc, desc));
                }
            }
            for spec in COMMAND_SPECS {
                let matches_primary = spec.primary.starts_with(&lower_input);
                let matches_alias = spec
                    .aliases
                    .iter()
                    .any(|&a| a.starts_with('/') && a.starts_with(&lower_input));
                if matches_primary || matches_alias {
                    let formatted = format!("{:<17} — {}", spec.primary, spec.description);
                    if !suggestions.iter().any(|s| s.starts_with(spec.primary)) {
                        suggestions.push(formatted);
                    }
                }
            }
        }
        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        if let Some(h) = highlighted_suggestion {
            let clean = if let Some((cmd, _)) = h.split_once(" — ") {
                cmd.trim().to_string()
            } else if let Some((cmd, _)) = h.split_once(" - ") {
                cmd.trim().to_string()
            } else {
                h.trim().to_string()
            };
            return Ok(Replacement::Some(clean));
        }

        if let Ok(suggestions) = self.get_suggestions(input) {
            let clean_suggestions: Vec<String> = suggestions
                .iter()
                .map(|s| {
                    if let Some((cmd, _)) = s.split_once(" — ") {
                        cmd.trim().to_string()
                    } else if let Some((cmd, _)) = s.split_once(" - ") {
                        cmd.trim().to_string()
                    } else {
                        s.trim().to_string()
                    }
                })
                .collect();

            if clean_suggestions.len() == 1 {
                return Ok(Replacement::Some(clean_suggestions[0].clone()));
            } else if let Some(first) = clean_suggestions.first() {
                let common = clean_suggestions.iter().fold(first.clone(), |acc, item| {
                    acc.chars()
                        .zip(item.chars())
                        .take_while(|(a, b)| a == b)
                        .map(|(a, _)| a)
                        .collect()
                });
                if common.len() > input.len() {
                    return Ok(Replacement::Some(common));
                }
            }
        }
        Ok(Replacement::None)
    }
}
