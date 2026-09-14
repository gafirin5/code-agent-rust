use crate::tools::is_mutating_tool;
use anyhow::Result;
use inquire::Select;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionMode {
    Ask,
    AutoApprove,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionResponse {
    AllowOnce,
    AlwaysAllow,
    Deny,
}

#[derive(Debug)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub arguments_json: String,
    pub response_tx: std::sync::mpsc::Sender<PermissionResponse>,
}

pub struct PermissionGate {
    pub mode: PermissionMode,
    pub tui_requester: Option<std::sync::mpsc::Sender<PermissionRequest>>,
}

impl Default for PermissionGate {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Ask,
            tui_requester: None,
        }
    }
}

impl PermissionGate {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            tui_requester: None,
        }
    }

    pub fn with_tui_requester(
        mut self,
        requester: std::sync::mpsc::Sender<PermissionRequest>,
    ) -> Self {
        self.tui_requester = Some(requester);
        self
    }

    pub fn check_and_authorize(&mut self, tool_name: &str, arguments_json: &str) -> Result<bool> {
        // Safe tools are always authorized
        if !is_mutating_tool(tool_name) {
            return Ok(true);
        }

        match self.mode {
            PermissionMode::AutoApprove => Ok(true),
            PermissionMode::ReadOnly => {
                println!(
                    "\n🚫 Action blocked: Tool '{}' is mutating and mode is set to ReadOnly.",
                    tool_name
                );
                Ok(false)
            }
            PermissionMode::Ask => {
                if let Some(requester) = &self.tui_requester {
                    let (resp_tx, resp_rx) = std::sync::mpsc::channel();
                    let req = PermissionRequest {
                        tool_name: tool_name.to_string(),
                        arguments_json: arguments_json.to_string(),
                        response_tx: resp_tx,
                    };
                    if requester.send(req).is_ok() {
                        match resp_rx.recv() {
                            Ok(PermissionResponse::AllowOnce) => Ok(true),
                            Ok(PermissionResponse::AlwaysAllow) => {
                                self.mode = PermissionMode::AutoApprove;
                                Ok(true)
                            }
                            Ok(PermissionResponse::Deny) | Err(_) => Ok(false),
                        }
                    } else {
                        Ok(false)
                    }
                } else {
                    println!("\n⚠️  [Permission Request]");
                    println!("  • Tool      : {}", tool_name);
                    println!("  • Arguments : {}", arguments_json);

                    let choices = vec![
                        "✅ Yes, allow this execution",
                        "🚀 Always allow for this entire session",
                        "❌ No, reject this action",
                    ];

                    match Select::new("Do you want to authorize this tool call?", choices).prompt()
                    {
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
}
