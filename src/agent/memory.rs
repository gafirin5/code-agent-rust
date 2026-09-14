use crate::types::ChatMessage;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub struct MemoryManager;

impl MemoryManager {
    /// Resolves the .ctrl directory inside the current workspace.
    pub fn get_ctrl_dir() -> PathBuf {
        let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        current.join(".ctrl")
    }

    /// Loads long-term memory notes from .ctrl/MEMORY.md if it exists.
    pub fn load_long_term_memory() -> Option<String> {
        let path = Self::get_ctrl_dir().join("MEMORY.md");
        if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .filter(|s| !s.trim().is_empty())
        } else {
            None
        }
    }

    /// Appends or writes content to .ctrl/MEMORY.md.
    pub fn save_long_term_memory(action: &str, content: &str) -> Result<String> {
        let dir = Self::get_ctrl_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join("MEMORY.md");

        match action {
            "append" => {
                let existing = fs::read_to_string(&path).unwrap_or_default();
                let updated = if existing.trim().is_empty() {
                    format!("- {}\n", content.trim())
                } else {
                    format!("{}\n- {}\n", existing.trim_end(), content.trim())
                };
                fs::write(&path, updated.as_bytes())?;
                Ok(format!(
                    "Appended note to workspace memory ({})",
                    path.display()
                ))
            }
            "set" | "write" => {
                fs::write(&path, content.as_bytes())?;
                Ok(format!("Updated workspace memory ({})", path.display()))
            }
            "read" => {
                let existing = fs::read_to_string(&path)
                    .unwrap_or_else(|_| "(No workspace memory recorded yet)".to_string());
                Ok(existing)
            }
            _ => anyhow::bail!(
                "Unknown memory action: '{}'. Use 'read', 'append', or 'set'.",
                action
            ),
        }
    }

    /// Saves session chat history to .ctrl/session.json.
    pub fn save_session_history(messages: &[ChatMessage]) -> Result<()> {
        let dir = Self::get_ctrl_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join("session.json");
        let json = serde_json::to_string_pretty(messages)?;
        fs::write(&path, json.as_bytes())?;
        Ok(())
    }

    /// Loads previous session chat history from .ctrl/session.json.
    pub fn load_session_history() -> Option<Vec<ChatMessage>> {
        let path = Self::get_ctrl_dir().join("session.json");
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(history) = serde_json::from_str::<Vec<ChatMessage>>(&data) {
                    return Some(history);
                }
            }
        }
        None
    }

    /// Clears the saved session history.
    pub fn clear_session_history() -> Result<()> {
        let path = Self::get_ctrl_dir().join("session.json");
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }
}
