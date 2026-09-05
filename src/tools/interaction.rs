use anyhow::Result;
use inquire::{Select, Text};

pub fn ask_user_question(question: &str, options: Option<Vec<String>>) -> Result<String> {
    println!("\n❓ [Agent Asks for Clarification]");
    if let Some(opts) = options {
        if !opts.is_empty() {
            let mut all_opts = opts;
            all_opts.push("Other (type custom response)".to_string());
            match Select::new(question, all_opts).prompt() {
                Ok(choice) => {
                    if choice.starts_with("Other (") {
                        match Text::new("Your custom response:").prompt() {
                            Ok(custom) => Ok(custom),
                            Err(_) => Ok("(user cancelled)".to_string()),
                        }
                    } else {
                        Ok(choice)
                    }
                }
                Err(_) => Ok("(user cancelled prompt)".to_string()),
            }
        } else {
            match Text::new(question).prompt() {
                Ok(ans) => Ok(ans),
                Err(_) => Ok("(user cancelled prompt)".to_string()),
            }
        }
    } else {
        match Text::new(question).prompt() {
            Ok(ans) => Ok(ans),
            Err(_) => Ok("(user cancelled prompt)".to_string()),
        }
    }
}
