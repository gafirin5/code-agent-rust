use anyhow::Result;
use std::sync::Arc;
use crate::agent::orchestrator::run_agent_loop;
use crate::agent::permissions::{PermissionGate, PermissionMode};
use crate::agent::tasks::{CancellationToken, OutputSink, TaskLogBuffer, TaskManager};

/// Builds the system prompt for a subagent given a task and skill description.
fn build_subagent_system_prompt(task: &str, skill_desc: &str) -> String {
    format!(
        "You are an autonomous subagent worker delegated by the primary agent to execute this specific task:\n\
         Task: {}\n\
         Assigned Role/Skill: {}\n\n\
         Guidelines:\n\
         - Use tools (read_file, glob_files, grep_files, web_fetch, web_search, code_check) to research or execute the task.\n\
         - Execute the assigned task directly; do not delegate to further subagents.\n\
         - Keep your diffs or actions focused.\n\
         - When your investigation or work is complete, provide a structured final answer summarizing your findings, results, and recommendations.",
        task, skill_desc
    )
}

/// Executes an isolated subagent delegation task (synchronous / blocking).
pub fn run_subagent(
    task: &str,
    skill: Option<&str>,
    model_override: Option<&str>,
    max_turns: Option<usize>,
    api_key: &str,
    base_url: &str,
    parent_model: &str,
) -> Result<String> {
    if api_key.trim().is_empty() || api_key == "your_api_key_here" {
        anyhow::bail!(
            "AI_API_KEY is not set or configured. Subagent delegation requires an active AI provider API key."
        );
    }

    let model = model_override.unwrap_or(parent_model);
    let turns = max_turns.unwrap_or(8).clamp(1, 20);

    let skill_desc = skill.unwrap_or("general technical investigator");
    let system_prompt = build_subagent_system_prompt(task, skill_desc);

    let mut sub_conversation = Vec::new();
    let mut permission_gate = PermissionGate {
        mode: PermissionMode::AutoApprove,
    };

    println!("\n🤖 \x1B[1m[Subagent Delegated]\x1B[0m Starting isolated task: \"{}\" (Model: {})", task, model);

    let protocol = crate::agent::provider::ApiProtocol::from_str(base_url);
    let result = run_agent_loop(
        task,
        &mut sub_conversation,
        model,
        api_key,
        base_url,
        protocol,
        &mut permission_gate,
        &system_prompt,
        turns,
        false, // Subagent can run without streaming to avoid terminal collisions
        None,
        None,
        None,
    )?;

    println!("✔ \x1B[32m[Subagent Complete]\x1B[0m Finished in {} turns.", sub_conversation.len() / 2);

    Ok(format!(
        "### Subagent Task Result:\n{}\n\n(Delegated subagent executed {} tool calls)",
        result.final_content, result.tools_executed
    ))
}

/// Spawns an isolated subagent as a background task via TaskManager.
/// Returns the assigned task ID immediately without blocking.
pub fn run_subagent_background(
    task: &str,
    skill: Option<&str>,
    model_override: Option<&str>,
    max_turns: Option<usize>,
    api_key: &str,
    base_url: &str,
    parent_model: &str,
) -> Result<String> {
    if api_key.trim().is_empty() || api_key == "your_api_key_here" {
        anyhow::bail!(
            "AI_API_KEY is not set or configured. Subagent delegation requires an active AI provider API key."
        );
    }

    let model = model_override.unwrap_or(parent_model).to_string();
    let turns = max_turns.unwrap_or(8).clamp(1, 20);
    let skill_desc = skill.unwrap_or("general technical investigator").to_string();
    let system_prompt = build_subagent_system_prompt(task, &skill_desc);

    // Clone owned values for the closure (moved into worker thread)
    let task_owned = task.to_string();
    let api_key_owned = api_key.to_string();
    let base_url_owned = base_url.to_string();
    let model_owned = model.clone();

    let task_name = format!("subagent:{}", skill_desc);
    let task_desc = if task.len() > 80 {
        format!("{}...", &task[..77])
    } else {
        task.to_string()
    };

    let tm = TaskManager::global();
    let (task_id, _token, _logs) = tm.spawn_task_with_sink(
        task_name,
        task_desc,
        move |cancel_token: CancellationToken, logs: Arc<TaskLogBuffer>| {
            // Check cancellation before starting
            cancel_token.check().map_err(|e| anyhow::anyhow!("{}", e))?;

            // Instantiate isolated buffered sink
            let sink = OutputSink::Buffered(logs);

            sink.emit(&format!(
                "🤖 [Subagent Delegated] Starting isolated task: \"{}\" (Model: {})",
                task_owned, model_owned
            ));

            let mut sub_conversation = Vec::new();
            let mut permission_gate = PermissionGate {
                mode: PermissionMode::AutoApprove,
            };

            let protocol = crate::agent::provider::ApiProtocol::from_str(&base_url_owned);

            // Run agent loop with output redirected silently to sink
            let result = run_agent_loop(
                &task_owned,
                &mut sub_conversation,
                &model_owned,
                &api_key_owned,
                &base_url_owned,
                protocol,
                &mut permission_gate,
                &system_prompt,
                turns,
                false, // Disable streaming in background mode
                None,
                Some(&sink),
                Some(&cancel_token),
            )?;

            sink.emit(&format!(
                "✔ [Subagent Complete] Finished in {} turns with {} tool executions.",
                sub_conversation.len() / 2,
                result.tools_executed
            ));

            // Check cancellation after completion
            cancel_token.check().map_err(|e| anyhow::anyhow!("{}", e))?;

            Ok(format!(
                "### Subagent Task Result:\n{}\n\n(Delegated subagent executed {} tool calls)",
                result.final_content, result.tools_executed
            ))
        },
    ).map_err(|e| anyhow::anyhow!("Failed to spawn background subagent: {}", e))?;

    Ok(format!(
        "Background subagent launched successfully.\n\
         Task ID: {}\n\
         Model: {}\n\
         Skill: {}\n\n\
         Use /tasks logs {} to inspect background logs.\n\
         Use manage_task(action=\"status\", task_id=\"{}\") to check progress.\n\
         Use manage_task(action=\"await\", task_id=\"{}\") to wait for results.",
        task_id, model, skill_desc, task_id, task_id, task_id
    ))
}
