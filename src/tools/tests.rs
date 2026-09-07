#[cfg(test)]
mod tests {
    use crate::agent::checkpoint::{format_color_diff, generate_unified_diff, CheckpointManager};
    use crate::agent::compaction::estimate_tokens;
    use crate::tools::filesystem::{edit_file, read_file, write_file};
    use crate::tools::mcp::McpConfigFile;
    use crate::tools::result_store::ResultStore;
    use crate::tools::search::{glob_files, grep_files};
    use crate::tools::self_heal::check_file_diagnostics;
    use crate::tools::web::html_to_markdown;
    use crate::tools::get_available_tools;
    use crate::types::{ChatMessage, MessageRole};

    #[test]
    fn test_filesystem_lifecycle() {
        let temp_dir = std::env::temp_dir().join("ctrl_cli_test_fs");
        let test_file = temp_dir.join("sample.txt");
        let test_path = test_file.to_str().unwrap();

        // 1. Write file
        let initial_content = "Line 1\nLine 2: Target to replace\nLine 3\nLine 4\nLine 5";
        let write_res = write_file(test_path, initial_content, Some(true));
        assert!(write_res.is_ok(), "Write file should succeed");

        // 2. Read file with range
        let read_res = read_file(test_path, Some(2), Some(4)).unwrap();
        assert!(read_res.contains("Target to replace"));
        assert!(read_res.contains("   2:"));

        // 3. Edit file (chunk replace)
        let edit_res = edit_file(
            test_path,
            "Line 2: Target to replace",
            "Line 2: Replaced Content Successfully",
            Some(false),
        );
        assert!(edit_res.is_ok(), "Edit file should succeed");

        // 4. Verify edited content
        let verify_read = read_file(test_path, None, None).unwrap();
        assert!(verify_read.contains("Replaced Content Successfully"));
        assert!(!verify_read.contains("Target to replace"));

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_edit_file_target_not_found() {
        let temp_dir = std::env::temp_dir().join("ctrl_cli_test_edit_err");
        let test_file = temp_dir.join("sample2.txt");
        let test_path = test_file.to_str().unwrap();

        let _ = write_file(test_path, "Hello World\nRust Agent", Some(true));
        let err = edit_file(test_path, "Nonexistent text", "Something", Some(false));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Target content was not found"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_result_store_pagination() {
        let mut long_text = String::new();
        for i in 1..=50 {
            long_text.push_str(&format!("This is line number {}\n", i));
        }

        // Process with low limit (10 lines)
        let processed = ResultStore::process_output(long_text, 10, 500);
        assert!(processed.contains("[OUTPUT TRUNCATED]"));
        assert!(processed.contains("read_tool_result"));
        assert!(processed.contains("result_id='tr_"));

        // Paginate lines 11 to 20
        let paged = ResultStore::read_result("tr_1", 11, 10);
        assert!(paged.is_ok());
        let page_text = paged.unwrap();
        assert!(page_text.contains("This is line number 11"));
        assert!(page_text.contains("This is line number 20"));
    }

    #[test]
    fn test_glob_and_grep() {
        let glob_res = glob_files("*.toml", Some("."), Some("list")).unwrap();
        assert!(glob_res.contains("Cargo.toml"));

        let grep_res = grep_files("ctrl-cli", Some("."), Some("Cargo.toml"), Some(false), Some(10), Some(1), Some(1)).unwrap();
        assert!(grep_res.contains("Cargo.toml"));
    }

    #[test]
    fn test_checkpoint_and_undo() {
        let temp_dir = std::env::temp_dir().join("ctrl_cli_test_checkpoint");
        let test_file = temp_dir.join("checkpoint_target.txt");
        let test_path = test_file.to_str().unwrap();
        let snap_dir = temp_dir.join("snapshots");

        let _ = std::fs::create_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&snap_dir);
        std::fs::write(&test_file, b"Original Content V1").unwrap();

        CheckpointManager::with_snapshot_dir(snap_dir, || {
            // Record checkpoint before modification
            let cp_id = CheckpointManager::record_checkpoint(test_path, "edit_file").unwrap();
            assert!(cp_id > 0);

            // Mutate file
            std::fs::write(&test_file, b"Mutated Content V2").unwrap();
            assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "Mutated Content V2");

            // Undo
            let undo_res = CheckpointManager::undo_last().unwrap();
            assert!(undo_res.contains("Restored previous state"));
            assert_eq!(std::fs::read_to_string(&test_file).unwrap(), "Original Content V1");
        });

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_unified_diff() {
        let old = "fn main() {\n    println!(\"hello\");\n}";
        let new = "fn main() {\n    println!(\"hello world!\");\n}";

        let diff = generate_unified_diff("main.rs", old, new);
        assert!(diff.contains("-    println!(\"hello\");"));
        assert!(diff.contains("+    println!(\"hello world!\");"));

        let colorized = format_color_diff(&diff);
        assert!(colorized.contains("\x1B[31m-"));
        assert!(colorized.contains("\x1B[32m+"));

        // Verify insertion does not desynchronize subsequent matching lines (LCS verification)
        let old2 = "Line 1\nLine 2\nLine 3";
        let new2 = "Line 0\nLine 1\nLine 2\nLine 3";
        let diff2 = generate_unified_diff("test.txt", old2, new2);
        assert!(diff2.contains("+Line 0"));
        assert!(diff2.contains(" Line 1"));
        assert!(diff2.contains(" Line 2"));
        assert!(diff2.contains(" Line 3"));
        assert!(!diff2.contains("-Line 1"));
    }

    #[test]
    fn test_self_heal_diagnostics() {
        let temp_dir = std::env::temp_dir().join("ctrl_cli_test_diag");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Valid JSON
        let valid_json = temp_dir.join("valid.json");
        std::fs::write(&valid_json, b"{\"key\": \"value\"}").unwrap();
        assert!(check_file_diagnostics(valid_json.to_str().unwrap()).is_none());

        // Invalid JSON
        let invalid_json = temp_dir.join("invalid.json");
        std::fs::write(&invalid_json, b"{\"key\": invalid_value}").unwrap();
        let diag = check_file_diagnostics(invalid_json.to_str().unwrap());
        assert!(diag.is_some());
        assert!(diag.unwrap().contains("Invalid JSON syntax"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_html_to_markdown() {
        let sample_html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <style>body { color: red; }</style>
                <script>alert('bad');</script>
            </head>
            <body>
                <h1>Documentation Title</h1>
                <p>This is a paragraph with a <a href="https://example.com">link here</a> &amp; some text.</p>
                <ul>
                    <li>First point</li>
                    <li>Second point with <code>inline_code</code></li>
                </ul>
                <pre><code>let x = 42;</code></pre>
            </body>
            </html>
        "#;

        let markdown = html_to_markdown(sample_html);
        assert!(markdown.contains("# Documentation Title"));
        assert!(!markdown.contains("alert('bad')"));
        assert!(!markdown.contains("body { color: red; }"));
        assert!(markdown.contains("link here"));
        assert!(markdown.contains("[link here](https://example.com)"));
        assert!(markdown.contains("First point"));
        assert!(markdown.contains("`inline_code`"));
        assert!(markdown.contains("&"));
    }

    #[test]
    fn test_context_compaction_boundary_safety() {
        use crate::types::{FunctionCall, ToolCall};

        // Construct conversation where naive len - 4 would cut on a Tool message (index 7)
        let mut conversation = vec![
            ChatMessage::system("System instructions"),
            ChatMessage::user("Turn 1 user request"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"a.txt\"}".to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            ChatMessage::tool_result("call_1", "read_file", "Content of a.txt"),
            ChatMessage::assistant(Some("Finished turn 1".to_string()), None),
            ChatMessage::user("Turn 2 user request"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_2".to_string(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: "write_file".to_string(),
                        arguments: "{\"path\":\"b.txt\",\"content\":\"hi\"}".to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            ChatMessage::tool_result("call_2", "write_file", "Wrote b.txt"),
            ChatMessage {
                role: MessageRole::Assistant,
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_3".to_string(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: "shell".to_string(),
                        arguments: "{\"command\":\"cargo check\"}".to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
            ChatMessage::tool_result("call_3", "shell", "ok"),
            ChatMessage::assistant(Some("All done".to_string()), None),
        ];

        let before_len = conversation.len();
        assert_eq!(before_len, 11);

        // Compact context using force_compact_context
        let res = crate::agent::compaction::force_compact_context(&mut conversation, "dummy-model", "fake-key", "http://127.0.0.1", crate::agent::provider::ApiProtocol::OpenAi);
        assert!(res.is_ok());

        // Verify boundary safety:
        // 1. First message must remain System
        assert_eq!(conversation[0].role, MessageRole::System);
        // 2. Second message must be the summary
        assert_eq!(conversation[1].role, MessageRole::User);
        assert!(conversation[1].content.as_ref().unwrap().contains("Context Compaction"));
        // 3. Third message must NOT be a Tool result (no orphan Tool messages allowed!)
        assert_ne!(conversation[2].role, MessageRole::Tool);
        // 4. Verify no tool message in the entire conversation is preceded by a non-assistant message
        for (i, msg) in conversation.iter().enumerate() {
            if msg.role == MessageRole::Tool {
                assert!(i > 0);
                assert_eq!(conversation[i - 1].role, MessageRole::Assistant);
                assert!(conversation[i - 1].tool_calls.is_some());
            }
        }
    }

    #[test]
    fn test_context_compaction_estimation() {
        let messages = vec![
            ChatMessage::system("You are a helpful coding assistant."),
            ChatMessage::user("Please build a microservice in Rust with Actix Web."),
            ChatMessage {
                role: MessageRole::Assistant,
                content: Some("I will help you create the Cargo.toml and src/main.rs.".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
            },
        ];

        let tokens = estimate_tokens(&messages);
        assert!(tokens > 10);
        assert!(tokens < 200);
    }

    #[test]
    fn test_mcp_config_structure() {
        let config_json = r#"{
            "mcpServers": {
                "sqlite": {
                    "command": "uvx",
                    "args": ["mcp-server-sqlite", "--db-path", "test.db"],
                    "env": { "DEBUG": "1" }
                }
            }
        }"#;

        let parsed: Result<McpConfigFile, _> = serde_json::from_str(config_json);
        assert!(parsed.is_ok());
        let cfg = parsed.unwrap();
        assert!(cfg.mcp_servers.contains_key("sqlite"));
        let sqlite = &cfg.mcp_servers["sqlite"];
        assert_eq!(sqlite.command, "uvx");
        assert_eq!(sqlite.args, vec!["mcp-server-sqlite", "--db-path", "test.db"]);
        assert_eq!(sqlite.env.get("DEBUG").unwrap(), "1");
    }

    #[test]
    fn test_tools_suite_includes_all_seven_features() {
        let tools = get_available_tools();
        let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

        // Check the 14 core tools
        assert!(tool_names.contains(&"read_file".to_string()));
        assert!(tool_names.contains(&"write_file".to_string()));
        assert!(tool_names.contains(&"edit_file".to_string()));
        assert!(tool_names.contains(&"glob_files".to_string()));
        assert!(tool_names.contains(&"grep_files".to_string()));
        assert!(tool_names.contains(&"shell".to_string()));
        assert!(tool_names.contains(&"read_tool_result".to_string()));
        assert!(tool_names.contains(&"ask_user_question".to_string()));
        assert!(tool_names.contains(&"skill".to_string()));
        assert!(tool_names.contains(&"manage_memory".to_string()));
        assert!(tool_names.contains(&"code_check".to_string()));
        assert!(tool_names.contains(&"web_fetch".to_string()));
        assert!(tool_names.contains(&"web_search".to_string()));
        assert!(tool_names.contains(&"subagent".to_string()));
        assert!(tool_names.contains(&"manage_task".to_string()));

        // Verify subagent tool has background parameter
        let subagent_tool = tools.iter().find(|t| t.function.name == "subagent").unwrap();
        let params = &subagent_tool.function.parameters;
        assert!(params["properties"]["background"].is_object(),
            "subagent tool should have 'background' parameter");
    }

    #[test]
    fn test_prefix_autocorrect_and_resolution() {
        use crate::resolve_slash_command;

        // 1. ? and /? resolve to /help
        let (q_res, _) = resolve_slash_command("?");
        assert_eq!(q_res, "/help");
        let (slash_q_res, _) = resolve_slash_command("/?");
        assert_eq!(slash_q_res, "/help");

        // 2. ? with command query e.g. "? /comp" or "? compact"
        let (q_comp, _) = resolve_slash_command("? /comp");
        assert_eq!(q_comp, "/help /compact");
        let (q_prov, _) = resolve_slash_command("? provider");
        assert_eq!(q_prov, "/help /provider");

        // 3. /comp -> /compact
        let (comp_res, note1) = resolve_slash_command("/comp");
        assert_eq!(comp_res, "/compact");
        assert!(note1.is_some() && note1.unwrap().contains("/compact"));

        // 4. /und -> /undo
        let (und_res, note2) = resolve_slash_command("/und");
        assert_eq!(und_res, "/undo");
        assert!(note2.is_some() && note2.unwrap().contains("/undo"));

        // 5. /dif -> /diff
        let (dif_res, note3) = resolve_slash_command("/dif");
        assert_eq!(dif_res, "/diff");
        assert!(note3.is_some() && note3.unwrap().contains("/diff"));

        // 6. /per -> /permissions
        let (per_res, note4) = resolve_slash_command("/per");
        assert_eq!(per_res, "/permissions");
        assert!(note4.is_some() && note4.unwrap().contains("/permissions"));

        // 7. /prov list -> /provider list, /provid -> /provider
        let (prov_res, _) = resolve_slash_command("/prov list");
        assert_eq!(prov_res, "/provider list");
        let (provid_res, _) = resolve_slash_command("/provid");
        assert_eq!(provid_res, "/provider");
        let (provide_res, _) = resolve_slash_command("/provide");
        assert_eq!(provide_res, "/provider");

        // 8. /prob -> /probe
        let (prob_res, _) = resolve_slash_command("/prob");
        assert_eq!(prob_res, "/probe");

        // 9. /mod -> /model (doesn't conflict with /models alias)
        let (mod_res, _) = resolve_slash_command("/mod");
        assert_eq!(mod_res, "/model");

        // 10. /ski -> /skill (doesn't conflict with /skills alias)
        let (ski_res, _) = resolve_slash_command("/ski");
        assert_eq!(ski_res, "/skill");

        // 11. /de -> /dev (doesn't conflict with /developer alias)
        let (de_res, _) = resolve_slash_command("/de");
        assert_eq!(de_res, "/dev");

        // 12. /checkp -> /checkpoints & /check exact match
        let (checkp_res, _) = resolve_slash_command("/checkp");
        assert_eq!(checkp_res, "/checkpoints");
        let (check_res, note_chk) = resolve_slash_command("/check");
        assert_eq!(check_res, "/check");
        assert!(note_chk.is_none());

        // 13. /lan id, /la zh, /l en -> /lang
        let (lan_res, _) = resolve_slash_command("/lan id");
        assert_eq!(lan_res, "/lang id");
        let (la_res, _) = resolve_slash_command("/la zh");
        assert_eq!(la_res, "/lang zh");
        let (l_res, _) = resolve_slash_command("/l en");
        assert_eq!(l_res, "/lang en");

        // 14. Ambiguous command prefixes
        let (_, amb_note1) = resolve_slash_command("/che");
        assert!(amb_note1.is_some() && amb_note1.unwrap().contains("Ambiguous"));
        let (_, amb_note2) = resolve_slash_command("/to");
        assert!(amb_note2.is_some() && amb_note2.unwrap().contains("Ambiguous"));

        // 15. Non-slash prompt untouched
        let (plain_res, note_none) = resolve_slash_command("create a new rust function");
        assert_eq!(plain_res, "create a new rust function");
        assert!(note_none.is_none());
    }

    #[test]
    fn test_slash_completer_suggestions_and_tab_complete() {
        use inquire::autocompletion::{Autocomplete, Replacement};
        use crate::SlashCompleter;

        let mut completer = SlashCompleter;

        // Suggestions for /comp (now includes description beside /)
        let suggs = completer.get_suggestions("/comp").unwrap();
        assert!(suggs.iter().any(|s| s.starts_with("/compact")));

        // Tab completion on /comp completes to /compact
        let comp_repl = completer.get_completion("/comp", None).unwrap();
        match comp_repl {
            Replacement::Some(s) => assert_eq!(s, "/compact"),
            _ => panic!("Expected replacement for /comp"),
        }

        // Suggestions for /lang
        let lang_suggs = completer.get_suggestions("/lang ").unwrap();
        assert!(lang_suggs.iter().any(|s| s.contains("en")));
        assert!(lang_suggs.iter().any(|s| s.contains("id")));
        assert!(lang_suggs.iter().any(|s| s.contains("zh")));

        // Suggestions for /provider
        let prov_suggs = completer.get_suggestions("/provider ").unwrap();
        assert!(prov_suggs.contains(&"/provider list".to_string()));
        assert!(prov_suggs.contains(&"/provider switch".to_string()));
        assert!(prov_suggs.contains(&"/provider add".to_string()));

        // Suggestions for ?
        let q_suggs = completer.get_suggestions("?").unwrap();
        assert!(q_suggs.contains(&"/help".to_string()));
    }

    #[test]
    fn test_trilingual_support_and_directives() {
        use crate::{build_system_prompt, SupportedLanguage, UserProfile};

        // 1. Language parsing including abbreviations
        assert_eq!(SupportedLanguage::from_str("en"), SupportedLanguage::English);
        assert_eq!(SupportedLanguage::from_str("english"), SupportedLanguage::English);
        assert_eq!(SupportedLanguage::from_str("id"), SupportedLanguage::Indonesian);
        assert_eq!(SupportedLanguage::from_str("in"), SupportedLanguage::Indonesian);
        assert_eq!(SupportedLanguage::from_str("ina"), SupportedLanguage::Indonesian);
        assert_eq!(SupportedLanguage::from_str("Bahasa Indonesia"), SupportedLanguage::Indonesian);
        assert_eq!(SupportedLanguage::from_str("zh"), SupportedLanguage::Chinese);
        assert_eq!(SupportedLanguage::from_str("cn"), SupportedLanguage::Chinese);
        assert_eq!(SupportedLanguage::from_str("中文"), SupportedLanguage::Chinese);
        assert_eq!(SupportedLanguage::from_str("chinese"), SupportedLanguage::Chinese);

        // 2. Directives content
        assert!(SupportedLanguage::English.directive().contains("English"));
        assert!(SupportedLanguage::Indonesian.directive().contains("Bahasa Indonesia"));
        assert!(SupportedLanguage::Chinese.directive().contains("中文"));

        // 3. System prompt inclusion
        let mut profile = UserProfile::default();
        profile.response_language = "English".to_string();
        let prompt_en = build_system_prompt(None, &profile);
        assert!(prompt_en.contains("[Active Communication Language: English]"));

        profile.response_language = "Bahasa Indonesia".to_string();
        let prompt_id = build_system_prompt(None, &profile);
        assert!(prompt_id.contains("[Active Communication Language: Bahasa Indonesia]"));

        profile.response_language = "中文".to_string();
        let prompt_zh = build_system_prompt(None, &profile);
        assert!(prompt_zh.contains("[Active Communication Language: 中文 (Chinese)]"));
    }

    #[test]
    fn test_provider_registry_lifecycle() {
        use crate::agent::provider::{ApiProtocol, ProviderConfig, ProvidersRegistry};

        let mut reg = ProvidersRegistry::default();
        assert_eq!(reg.active_provider_id, "openagentic");
        assert!(reg.providers.len() >= 6);

        // Check protocols
        let anthropic_prov = reg.providers.iter().find(|p| p.id == "anthropic").unwrap();
        assert_eq!(anthropic_prov.protocol, ApiProtocol::Anthropic);

        let openai_prov = reg.providers.iter().find(|p| p.id == "openai").unwrap();
        assert_eq!(openai_prov.protocol, ApiProtocol::OpenAi);

        // Test switch
        let switched = reg.switch_active("deepseek");
        assert!(switched.is_ok());
        assert_eq!(reg.active_provider_id, "deepseek");

        // Test add custom provider
        let custom = ProviderConfig {
            id: "my-test-vllm".to_string(),
            name: "My Local vLLM".to_string(),
            protocol: ApiProtocol::OpenAi,
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: "test-key".to_string(),
            default_model: "qwen-coder".to_string(),
            context_window: Some(65_536),
            max_output_tokens: Some(4_096),
        };
        reg.add_or_update(custom);
        assert!(reg.providers.iter().any(|p| p.id == "my-test-vllm"));

        // Test update limits
        reg.update_limits("my-test-vllm", 131_072, Some(8_192));
        let updated = reg.providers.iter().find(|p| p.id == "my-test-vllm").unwrap();
        assert_eq!(updated.context_window, Some(131_072));
        assert_eq!(updated.max_output_tokens, Some(8_192));

        // Test cannot remove active
        assert!(reg.remove("deepseek").is_err());

        // Test remove custom
        assert!(reg.remove("my-test-vllm").is_ok());
        assert!(!reg.providers.iter().any(|p| p.id == "my-test-vllm"));
    }

    #[test]
    fn test_model_capability_resolution() {
        use crate::agent::probe::resolve_model_limits;
        use crate::agent::provider::{ApiProtocol, ProviderConfig};
        use crate::get_model_context_info;

        // GLM-5.3-Flash: 1M context, 128k output
        let (glm_ctx, glm_out, _) = resolve_model_limits("glm-5.3-flash");
        assert_eq!(glm_ctx, 1_000_000);
        assert_eq!(glm_out, Some(128_000));

        // Claude 3.5 Sonnet: 200k context, 8k output
        let (claude_ctx, claude_out, _) = resolve_model_limits("claude-3-5-sonnet-20241022");
        assert_eq!(claude_ctx, 200_000);
        assert_eq!(claude_out, Some(8_192));

        // Claude 3 Haiku & Opus: 200k context, 4k output limit (HTTP 400 guard)
        let (haiku_ctx, haiku_out, _) = resolve_model_limits("claude-3-haiku-20240307");
        assert_eq!(haiku_ctx, 200_000);
        assert_eq!(haiku_out, Some(4_096));
        let (opus_ctx, opus_out, _) = resolve_model_limits("claude-3-opus-20240229");
        assert_eq!(opus_ctx, 200_000);
        assert_eq!(opus_out, Some(4_096));

        // DeepSeek Chat: 64k context, 8k output
        let (ds_ctx, ds_out, _) = resolve_model_limits("deepseek-chat");
        assert_eq!(ds_ctx, 64_000);
        assert_eq!(ds_out, Some(8_192));

        // Provider override in get_model_context_info
        let custom_prov = ProviderConfig {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            protocol: ApiProtocol::OpenAi,
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: "".to_string(),
            default_model: "test-model".to_string(),
            context_window: Some(500_000),
            max_output_tokens: Some(32_000),
        };
        let info = get_model_context_info("test-model", Some(&custom_prov));
        assert_eq!(info.context_window, 500_000);
        assert_eq!(info.max_output, Some(32_000));
    }

    #[test]
    fn test_anthropic_payload_and_tool_conversion() {
        use crate::agent::orchestrator::{build_anthropic_messages, build_anthropic_tools};
        use crate::types::{ChatMessage, FunctionCall, ToolCall};
        use crate::tools::get_available_tools;

        // 1. Tool conversion
        let tools = get_available_tools();
        let anthropic_tools = build_anthropic_tools(&tools);
        assert!(!anthropic_tools.is_empty());
        let first_tool = &anthropic_tools[0];
        assert!(first_tool.get("name").is_some());
        assert!(first_tool.get("description").is_some());
        assert!(first_tool.get("input_schema").is_some());

        // 2. Message conversion
        let mut conv = Vec::new();
        conv.push(ChatMessage::system("You are a helpful assistant."));
        conv.push(ChatMessage::user("Please read main.rs"));

        let assistant_with_tools = ChatMessage {
            role: MessageRole::Assistant,
            content: Some("I will read the file for you.".to_string()),
            tool_calls: Some(vec![ToolCall {
                id: "call_123".to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: "read_file".to_string(),
                    arguments: r#"{"path": "src/main.rs"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };
        conv.push(assistant_with_tools);
        conv.push(ChatMessage::tool_result("call_123", "read_file", "fn main() {}"));

        let anthropic_msgs = build_anthropic_messages(&conv);
        // System message filtered from messages
        assert_eq!(anthropic_msgs.len(), 3);
        assert_eq!(anthropic_msgs[0]["role"], "user");
        assert_eq!(anthropic_msgs[1]["role"], "assistant");
        assert_eq!(anthropic_msgs[2]["role"], "user");

        // Tool use block present in assistant content
        let asst_blocks = anthropic_msgs[1]["content"].as_array().unwrap();
        assert!(asst_blocks.iter().any(|b| b["type"] == "tool_use" && b["id"] == "call_123"));

        // Tool result block present in user content
        let tool_res_blocks = anthropic_msgs[2]["content"].as_array().unwrap();
        assert!(tool_res_blocks.iter().any(|b| b["type"] == "tool_result" && b["tool_use_id"] == "call_123"));
    }

    #[test]
    fn test_anthropic_compaction_and_heuristic_fallback() {
        use crate::agent::provider::ApiProtocol;
        use crate::types::ChatMessage;

        let mut conv = vec![
            ChatMessage::system("System instructions"),
            ChatMessage::user("User prompt 1"),
            ChatMessage::assistant(Some("Assistant response 1".to_string()), None),
            ChatMessage::user("User prompt 2"),
            ChatMessage::assistant(Some("Assistant response 2".to_string()), None),
            ChatMessage::user("User prompt 3"),
            ChatMessage::assistant(Some("Assistant response 3".to_string()), None),
        ];

        let before_len = conv.len();
        // Force compact with Anthropic protocol against mock address (triggers graceful heuristic fallback)
        let res = crate::agent::compaction::force_compact_context(
            &mut conv,
            "claude-3-5-sonnet-20241022",
            "mock-key",
            "http://127.0.0.1:9",
            ApiProtocol::Anthropic,
        );
        assert!(res.is_ok());
        assert!(conv.len() < before_len);
        assert!(conv[1].content.as_ref().unwrap().contains("Context Compaction"));
    }

    #[test]
    fn test_commands_specs_coverage_and_uniqueness() {
        use crate::COMMAND_SPECS;
        use std::collections::HashSet;

        let mut all_primaries = HashSet::new();
        for spec in COMMAND_SPECS {
            assert!(spec.primary.starts_with('/'));
            assert!(all_primaries.insert(spec.primary), "Duplicate primary command: {}", spec.primary);
            for alias in spec.aliases {
                assert!(!alias.is_empty());
            }
        }
        assert!(all_primaries.contains("/help"));
        assert!(all_primaries.contains("/model"));
        assert!(all_primaries.contains("/provider"));
        assert!(all_primaries.contains("/probe"));
        assert!(all_primaries.contains("/lang"));
        assert!(all_primaries.contains("/compact"));
        assert!(all_primaries.contains("/tasks"));
    }

    #[test]
    fn test_manage_task_dispatch_list_empty() {
        let res = crate::tools::dispatch_tool("manage_task", r#"{"action":"list"}"#).unwrap();
        assert!(res.contains("No background tasks found") || res.contains("Background Tasks"));
    }

    #[test]
    fn test_manage_task_dispatch_lifecycle() {
        use crate::agent::tasks::TaskManager;
        use std::thread;
        use std::time::Duration;

        let tm = TaskManager::global();
        let (id, _token) = tm.spawn_task(
            "test_dispatch".to_string(),
            "testing dispatch_manage_task".to_string(),
            |_| {
                thread::sleep(Duration::from_millis(50));
                Ok("dispatch success".to_string())
            },
        ).unwrap();

        // 1. Status query
        let status_args = format!(r#"{{"action":"status","task_id":"{}"}}"#, id);
        let status_res = crate::tools::dispatch_tool("manage_task", &status_args).unwrap();
        assert!(status_res.contains(&id));
        assert!(status_res.contains("test_dispatch"));

        // 2. Await task
        let await_args = format!(r#"{{"action":"await","task_id":"{}","timeout_secs":5}}"#, id);
        let await_res = crate::tools::dispatch_tool("manage_task", &await_args).unwrap();
        assert!(await_res.contains("finished"));
        assert!(await_res.contains("dispatch success"));

        // 3. Post-completion status shows finished
        let post_status = crate::tools::dispatch_tool("manage_task", &status_args).unwrap();
        assert!(post_status.contains("completed"));
    }

    #[test]
    fn test_manage_task_dispatch_cancel() {
        use crate::agent::tasks::TaskManager;
        use std::thread;
        use std::time::Duration;

        let tm = TaskManager::global();
        let (id, _token) = tm.spawn_task(
            "test_cancel".to_string(),
            "testing cancellation dispatch".to_string(),
            |token| {
                for _ in 0..100 {
                    if token.is_cancelled() {
                        return Err(anyhow::anyhow!("cancelled cleanly"));
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Ok("done".to_string())
            },
        ).unwrap();

        let cancel_args = format!(r#"{{"action":"cancel","task_id":"{}"}}"#, id);
        let cancel_res = crate::tools::dispatch_tool("manage_task", &cancel_args).unwrap();
        assert!(cancel_res.contains("has been cancelled"));

        // Await to ensure it transitioned to terminal
        let await_args = format!(r#"{{"action":"await","task_id":"{}","timeout_secs":5}}"#, id);
        let await_res = crate::tools::dispatch_tool("manage_task", &await_args).unwrap();
        assert!(await_res.contains("cancelled") || await_res.contains("finished"));
    }

    #[test]
    fn test_manage_task_dispatch_status_includes_logs() {
        use crate::agent::tasks::TaskManager;

        let tm = TaskManager::global();
        let (id, _token) = tm.spawn_task(
            "test_status_logs".to_string(),
            "testing logs in status".to_string(),
            |_| {
                Ok("done".to_string())
            },
        ).unwrap();

        // Push some logs to the task's log buffer
        if let Some(task) = tm.get_inner(&id) {
            task.log_buffer.push("[TEST] Initializing runner");
            task.log_buffer.push("[TEST] Processing step 1");
            task.log_buffer.push("[TEST] Step 1 finished cleanly");
        }

        let status_args = format!(r#"{{"action":"status","task_id":"{}"}}"#, id);
        let res = crate::tools::dispatch_tool("manage_task", &status_args).unwrap();

        assert!(res.contains(&id));
        assert!(res.contains("Recent Logs"));
        assert!(res.contains("[TEST] Initializing runner"));
        assert!(res.contains("[TEST] Step 1 finished cleanly"));
    }

    #[test]
    fn test_manage_task_dispatch_logs_action() {
        use crate::agent::tasks::TaskManager;

        let tm = TaskManager::global();
        let (id, _token) = tm.spawn_task(
            "test_logs_action".to_string(),
            "testing logs action".to_string(),
            |_| {
                Ok("done".to_string())
            },
        ).unwrap();

        if let Some(task) = tm.get_inner(&id) {
            for i in 0..25 {
                task.log_buffer.push(format!("[LOG] Entry number {}", i));
            }
        }

        // Test default logs action
        let logs_args = format!(r#"{{"action":"logs","task_id":"{}"}}"#, id);
        let res = crate::tools::dispatch_tool("manage_task", &logs_args).unwrap();
        assert!(res.contains(&format!("Task '{}' Logs", id)));
        assert!(res.contains("showing 25 of 25 lines"));
        assert!(res.contains("[LOG] Entry number 0"));
        assert!(res.contains("[LOG] Entry number 24"));

        // Test with limit parameter
        let limit_args = format!(r#"{{"action":"logs","task_id":"{}","limit":10}}"#, id);
        let limit_res = crate::tools::dispatch_tool("manage_task", &limit_args).unwrap();
        assert!(limit_res.contains("showing 10 of 25 lines"));
        assert!(limit_res.contains("15 earlier lines omitted"));
        assert!(limit_res.contains("[LOG] Entry number 24"));
        assert!(!limit_res.contains("[LOG] Entry number 0"));
    }

    #[test]
    fn test_manage_task_dispatch_logs_empty() {
        use crate::agent::tasks::TaskManager;

        let tm = TaskManager::global();
        let (id, _token) = tm.spawn_task(
            "test_empty_logs".to_string(),
            "testing empty logs".to_string(),
            |_| Ok("ok".to_string()),
        ).unwrap();

        let logs_args = format!(r#"{{"action":"logs","task_id":"{}"}}"#, id);
        let res = crate::tools::dispatch_tool("manage_task", &logs_args).unwrap();
        assert!(res.contains("has no recorded logs"));
    }

    #[test]
    fn test_manage_task_dispatch_logs_not_found() {
        let logs_args = r#"{"action":"logs","task_id":"task-nonexistent-999"}"#;
        let res = crate::tools::dispatch_tool("manage_task", logs_args).unwrap();
        assert!(res.contains("Task 'task-nonexistent-999' not found."));
    }

    #[test]
    fn test_manage_task_dispatch_invalid_action() {
        let invalid_args = r#"{"action":"invalid_xyz"}"#;
        let err = crate::tools::dispatch_tool("manage_task", invalid_args).unwrap_err();
        assert!(err.to_string().contains("Valid: list, status, await, cancel, logs"));
    }
}

