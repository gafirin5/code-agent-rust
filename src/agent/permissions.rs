use anyhow::Result;
use inquire::Select;
use crate::tools::is_mutating_tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionMode {
    Ask,
    AutoApprove,
    ReadOnly,
}

pub struct PermissionGate {
    pub mode: PermissionMode,
}

impl Default for PermissionGate {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Ask,
        }
    }
}

impl PermissionGate {
    pub fn check_and_authorize(&mut self, tool_name: &str, arguments_json: &str) -> Result<bool> {
        // Safe tools are always authorized
        if !is_mutating_tool(tool_name) {
            return Ok(true);
        }

        match self.mode {
            PermissionMode::AutoApprove => Ok(true),
            PermissionMode::ReadOnly => {
                println!("\n🚫 Action blocked: Tool '{}' is mutating and mode is set to ReadOnly.", tool_name);
                Ok(false)
            }
            PermissionMode::Ask => {
                println!("\n⚠️  [Permission Request]");
                println!("  • Tool      : {}", tool_name);
                println!("  • Arguments : {}", arguments_json);

                let choices = vec![
                    "✅ Yes, allow this execution",
                    "🚀 Always allow for this entire session",
                    "❌ No, reject this action",
                ];

                match Select::new("Do you want to authorize this tool call?", choices).prompt() {
                    Ok(choice) if choice.starts_with("✅") => Ok(true),
                    Ok(choice) if choice.starts_with("🚀") => {
                        self.mode = PermissionMode::AutoApprove;
                        println!("✔ Mode updated to: AutoApprove for this session.");
                        Ok(true)
                    }
                    _ => {
                        println!("Action rejected by user.");
                        Ok(false)
                    }
                }
            }
        }
    }
}
