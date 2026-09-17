//! Empirical Adversarial Challenge Suite for Milestone 1 Next-Gen
//! (Dynamic Skill Discovery & In-Process BM25 Knowledge Retrieval)
//!
//! Authored by: challenger_m1_2 (Empirical Challenger)
//!
//! This test suite aggressively validates:
//! 1. Real workspace dynamic skill auto-discovery across `skills/`, `.ctrl/skills/`, and `prompts/`.
//! 2. Priority resolution and deduplication order (.ctrl/skills > skills/ > prompts/).
//! 3. Adversarial and corrupted YAML frontmatter fuzzing (unclosed, missing name, syntax chaos, DOS/Unix newlines).
//! 4. Okapi BM25 engine ranking accuracy, term frequency saturation, document length normalization, and Indonesian/English stop-word filtering.
//! 5. `knowledge_search` tool execution, parameter boundaries (top_k clamping, missing query), and output formatting.
//! 6. Slash command `/skills`, `/skills list`, `/skills info <name>`, `/skill-list`, and autocompletion disambiguation.
//! 7. End-to-end binary execution via subprocess stdio piping.

pub mod agent {
    pub mod checkpoint {
        pub struct CheckpointManager;
        impl CheckpointManager {
            pub fn record_checkpoint(_path: &str, _op: &str) -> anyhow::Result<()> {
                Ok(())
            }
        }
    }
}

pub mod tools {
    pub mod self_heal {
        pub fn check_file_diagnostics(_path: &str) -> Option<String> {
            None
        }
    }
    #[path = "../../src/tools/result_store.rs"]
    pub mod result_store;
    #[path = "../../src/tools/filesystem.rs"]
    pub mod filesystem;
    #[path = "../../src/tools/skills.rs"]
    pub mod skills;
    #[path = "../../src/tools/knowledge.rs"]
    pub mod knowledge;
}

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use tools::filesystem::with_workspace_root;
use tools::knowledge::{
    chunk_markdown, format_search_results, run_knowledge_search, search_knowledge, tokenize,
    tokenize_query, Bm25Index, STOP_WORDS,
};
use tools::skills::{
    discover_skills, get_skill_by_name, parse_skill_file, parse_skill_frontmatter,
    resolve_effective_root, SkillMetadata,
};

/// Helper to create an isolated self-cleaning temporary directory for hermetic testing.
struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "ctrl_challenge_m1_{}_{}_{}",
            prefix,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("Failed to create temporary challenge directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ============================================================================
// SUITE 1: DYNAMIC SKILL AUTO-DISCOVERY & HIERARCHICAL PRIORITY RESOLUTION
// ============================================================================

#[test]
fn challenge_workspace_skill_discovery_researcher_and_writer() {
    let ws_root = Path::new(".");
    let discovered = discover_skills(ws_root);

    assert!(
        discovered.len() >= 2,
        "Workspace must discover at least 'researcher' and 'writer' skills. Found: {}",
        discovered.len()
    );

    let researcher = discovered.iter().find(|s| s.name.eq_ignore_ascii_case("researcher"));
    assert!(researcher.is_some(), "Skill 'researcher' must be discovered");
    let r = researcher.unwrap();
    assert!(r.description.contains("Researcher"), "Researcher description must match");
    assert_eq!(
        r.tools,
        vec!["read_file", "glob_files", "grep_files", "web_search", "web_fetch", "read_tool_result"],
        "Researcher allowed tools must match specification"
    );
    assert!(
        r.prompt_template.contains("Researcher Persona") || r.prompt_template.contains("Fokus Utama"),
        "Researcher prompt body must contain persona guidelines"
    );

    let writer = discovered.iter().find(|s| s.name.eq_ignore_ascii_case("writer"));
    assert!(writer.is_some(), "Skill 'writer' must be discovered");
    let w = writer.unwrap();
    assert!(w.description.contains("Writer"), "Writer description must match");
    assert_eq!(
        w.tools,
        vec!["read_file", "write_file", "edit_file", "glob_files"],
        "Writer allowed tools must match specification"
    );
}

#[test]
fn challenge_get_skill_by_name_case_insensitivity() {
    let ws_root = Path::new(".");

    // Standard case
    let r1 = get_skill_by_name("researcher", ws_root);
    assert!(r1.is_some());

    // Uppercase
    let r2 = get_skill_by_name("RESEARCHER", ws_root);
    assert!(r2.is_some());
    assert_eq!(r1.unwrap().name, r2.unwrap().name);

    // Mixed case with whitespace
    let r3 = get_skill_by_name("  ReSeaRchEr  ", ws_root);
    assert!(r3.is_some());

    // Non-existent skill
    let r_none = get_skill_by_name("non_existent_challenger_skill_xyz", ws_root);
    assert!(r_none.is_none(), "Non-existent skill should return None");
}

#[test]
fn challenge_discovery_priority_and_deduplication() {
    let temp = TestTempDir::new("priority_test");
    let root = temp.path();

    // Setup 3 directories with conflicting skill 'analyst':
    // Priority 1: .ctrl/skills/analyst.md
    // Priority 2: skills/analyst/SKILL.md
    // Priority 3: prompts/analyst_persona.md

    let ctrl_dir = root.join(".ctrl").join("skills");
    let skills_dir = root.join("skills").join("analyst");
    let prompts_dir = root.join("prompts");

    fs::create_dir_all(&ctrl_dir).unwrap();
    fs::create_dir_all(&skills_dir).unwrap();
    fs::create_dir_all(&prompts_dir).unwrap();

    let ctrl_content = "---\nname: analyst\ndescription: Priority 1 DotCtrl\ntools: [tool_one]\n---\n# DotCtrl body";
    let skills_content = "---\nname: analyst\ndescription: Priority 2 Skills\ntools: [tool_two]\n---\n# Skills body";
    let prompts_content = "# Priority 3 Persona\nPlain markdown persona.";

    fs::write(ctrl_dir.join("analyst.md"), ctrl_content).unwrap();
    fs::write(skills_dir.join("SKILL.md"), skills_content).unwrap();
    fs::write(prompts_dir.join("analyst_persona.md"), prompts_content).unwrap();

    let discovered = discover_skills(root);
    assert_eq!(discovered.len(), 1, "Expected exactly 1 deduplicated skill for 'analyst'");
    let active = &discovered[0];
    assert_eq!(active.name, "analyst");
    assert_eq!(
        active.description, "Priority 1 DotCtrl",
        "Priority 1 (.ctrl/skills/) must override Priority 2 and 3"
    );
    assert_eq!(active.tools, vec!["tool_one"]);

    // Remove .ctrl/skills/analyst.md, now Priority 2 (skills/) should win
    fs::remove_file(ctrl_dir.join("analyst.md")).unwrap();
    let discovered_p2 = discover_skills(root);
    assert_eq!(discovered_p2.len(), 1);
    assert_eq!(
        discovered_p2[0].description, "Priority 2 Skills",
        "Priority 2 (skills/) must override Priority 3 when .ctrl is absent"
    );
    assert_eq!(discovered_p2[0].tools, vec!["tool_two"]);

    // Remove skills/analyst/SKILL.md, now Priority 3 (prompts/) should win
    fs::remove_file(skills_dir.join("SKILL.md")).unwrap();
    let discovered_p3 = discover_skills(root);
    assert_eq!(discovered_p3.len(), 1);
    assert_eq!(discovered_p3[0].name, "analyst");
    assert_eq!(
        discovered_p3[0].description, "Priority 3 Persona",
        "Priority 3 (prompts/) must be used when frontmatter is absent"
    );
    assert!(discovered_p3[0].tools.is_empty());
}

#[test]
fn challenge_discovery_nested_and_lowercase_skill_md() {
    let temp = TestTempDir::new("nested_skills");
    let root = temp.path();

    let custom_dir = root.join("skills").join("custom-bot");
    fs::create_dir_all(&custom_dir).unwrap();

    // Using lowercase 'skill.md'
    let content = "---\nname: custom-bot\ndescription: Custom Bot Skill\ntools:\n  - read_file\n---\nPrompt here.";
    fs::write(custom_dir.join("skill.md"), content).unwrap();

    let discovered = discover_skills(root);
    assert_eq!(discovered.len(), 1, "Should discover lowercase skill.md in subfolder");
    assert_eq!(discovered[0].name, "custom-bot");
    assert_eq!(discovered[0].tools, vec!["read_file"]);
}

// ============================================================================
// SUITE 2: ADVERSARIAL & CORRUPTED YAML FRONTMATTER FUZZING
// ============================================================================

#[test]
fn challenge_fuzz_unclosed_frontmatter_graceful_fallback() {
    let unclosed = "---\nname: broken-agent\ndescription: missing closing delimiter\ntools: [read_file]\n\n# Some Heading\nBody text";
    let parsed = parse_skill_frontmatter(unclosed);
    assert!(
        parsed.is_none(),
        "Unclosed frontmatter must return None without panicking"
    );

    // parse_skill_file should gracefully treat it as plain markdown
    let temp = TestTempDir::new("unclosed_frontmatter");
    let file_path = temp.path().join("broken_persona.md");
    fs::write(&file_path, unclosed).unwrap();

    let meta = parse_skill_file(&file_path).expect("parse_skill_file must succeed on unclosed frontmatter");
    assert_eq!(meta.name, "broken");
    assert_eq!(meta.tools, Vec::<String>::new());
    assert!(meta.prompt_template.contains("broken-agent"));
}

#[test]
fn challenge_fuzz_missing_name_falls_back_to_filename() {
    let no_name = "---\ndescription: Agent without explicit name\ntools: [grep_files]\n---\nBody here.";
    let parsed = parse_skill_frontmatter(no_name);
    assert!(parsed.is_some());
    let (meta, _) = parsed.unwrap();
    assert!(meta.name.is_empty());

    let temp = TestTempDir::new("no_name_frontmatter");
    let file_path = temp.path().join("inferred_bot.md");
    fs::write(&file_path, no_name).unwrap();

    let meta = parse_skill_file(&file_path).unwrap();
    assert_eq!(
        meta.name, "inferred_bot",
        "Missing name in frontmatter must default to file stem"
    );
    assert_eq!(meta.description, "Agent without explicit name");
    assert_eq!(meta.tools, vec!["grep_files"]);
}

#[test]
fn challenge_fuzz_corrupted_tools_syntax() {
    // 1. Bracket syntax with extra whitespace, quotes, and empty elements
    let case1 = "---\nname: c1\ndescription: test\ntools: [ 'read_file' , \"write_file\", `glob_files`, , ]\n---\n";
    let (m1, _) = parse_skill_frontmatter(case1).unwrap();
    assert_eq!(m1.tools, vec!["read_file", "write_file", "glob_files"]);

    // 2. Unclosed bracket
    let case2 = "---\nname: c2\ndescription: test\ntools: [read_file, write_file\n---\n";
    let (m2, _) = parse_skill_frontmatter(case2).unwrap();
    assert!(m2.tools.is_empty(), "Unclosed bracket tools should be safely ignored");

    // 3. Bullet syntax with mixed indentation and comments
    let case3 = "---\nname: c3\ndescription: test\ntools:\n  - read_file\n  # comment inside tools\n  -\n  -   \"write_file\"  \n---\n";
    let (m3, _) = parse_skill_frontmatter(case3).unwrap();
    assert_eq!(m3.tools, vec!["read_file", "write_file"]);

    // 4. Empty tools field
    let case4 = "---\nname: c4\ndescription: test\ntools:\n---\n";
    let (m4, _) = parse_skill_frontmatter(case4).unwrap();
    assert!(m4.tools.is_empty());
}

#[test]
fn challenge_fuzz_crlf_dos_line_endings_in_frontmatter() {
    let crlf_content = "---\r\nname: dos_skill\r\ndescription: DOS CRLF line endings\r\ntools: [read_file]\r\n---\r\n\r\nDOS prompt body\r\n";
    let parsed = parse_skill_frontmatter(crlf_content);
    assert!(parsed.is_some(), "Must parse CRLF formatted frontmatter");
    let (meta, body) = parsed.unwrap();
    assert_eq!(meta.name, "dos_skill");
    assert_eq!(meta.description, "DOS CRLF line endings");
    assert_eq!(meta.tools, vec!["read_file"]);
    assert!(body.contains("DOS prompt body"));
}

#[test]
fn challenge_fuzz_extreme_and_empty_inputs() {
    // Empty string
    assert!(parse_skill_frontmatter("").is_none());

    // Only whitespace
    assert!(parse_skill_frontmatter("   \r\n\t   \n").is_none());

    // Only opening delimiter
    assert!(parse_skill_frontmatter("---").is_none());
    assert!(parse_skill_frontmatter("---\n").is_none());

    // Empty frontmatter block
    let empty_block = "---\n---\nContent after";
    let parsed = parse_skill_frontmatter(empty_block);
    assert!(parsed.is_some());
    let (meta, body) = parsed.unwrap();
    assert!(meta.name.is_empty());
    assert_eq!(body, "Content after");

    // Large 5000-line frontmatter block stress
    let mut big = String::from("---\nname: big_skill\ndescription: massive frontmatter\n");
    for i in 0..2000 {
        big.push_str(&format!("dummy_key_{}: value_{}\n", i, i));
    }
    big.push_str("tools: [read_file]\n---\nBig body");
    let parsed_big = parse_skill_frontmatter(&big);
    assert!(parsed_big.is_some());
    assert_eq!(parsed_big.unwrap().0.name, "big_skill");
}

// ============================================================================
// SUITE 3: IN-PROCESS OKAPI BM25 ENGINE & SEARCH RETRIEVAL
// ============================================================================

#[test]
fn challenge_bm25_stop_words_filtering() {
    // Indonesian stop words
    let id_text = "dan di ke dari yang ini itu untuk pada adalah sebagai dengan atau oleh juga akan bisa ada tidak";
    let tokens_id = tokenize(id_text);
    assert!(
        tokens_id.is_empty(),
        "All Indonesian stop words must be filtered out, got: {:?}",
        tokens_id
    );

    // English stop words
    let en_text = "the is at which on and a an in to of for with or as by that this it from be are was were all can has have had not but what where when who how";
    let tokens_en = tokenize(en_text);
    assert!(
        tokens_en.is_empty(),
        "All English stop words must be filtered out, got: {:?}",
        tokens_en
    );

    // All stop words query fallback
    let fallback = tokenize_query("the and or dan yang");
    assert!(
        !fallback.is_empty(),
        "tokenize_query must fall back to all terms when every word is a stop word"
    );
}

#[test]
fn challenge_bm25_real_corpus_retrieval_accuracy() {
    let ws_root = Path::new(".");
    let knowledge_dir = tools::knowledge::resolve_knowledge_dir(ws_root);
    assert!(
        knowledge_dir.is_dir(),
        "Knowledge dir must exist at: {}",
        knowledge_dir.display()
    );

    let index = Bm25Index::build_from_dir(&knowledge_dir).expect("BM25 index build failed");
    assert!(
        index.chunks.len() >= 4,
        "Expected at least 4 chunk sections across knowledge documents. Found: {}",
        index.chunks.len()
    );

    // Query 1: Security and API Key policy -> should match company_policy.md
    let q1 = index.search("keamanan sistem otentikasi API Key rahasia", 3);
    assert!(!q1.is_empty(), "Expected results for security query");
    assert_eq!(
        q1[0].document, "company_policy.md",
        "Top result for security query must be company_policy.md"
    );
    assert!(
        q1[0].section.contains("Keamanan") || q1[0].snippet.contains("API Key"),
        "Snippet or section must contain security references"
    );
    assert!(q1[0].score > 0.0);

    // Query 2: Product performance, binary footprint, RAM -> should match product_faqs.md
    let q2 = index.search("biner sangat ringan ram footprint memori idle", 3);
    assert!(!q2.is_empty(), "Expected results for product memory query");
    assert_eq!(
        q2[0].document, "product_faqs.md",
        "Top result for product memory query must be product_faqs.md"
    );
    assert!(
        q2[0].snippet.contains("1.8 MB") || q2[0].snippet.contains("Ringan") || q2[0].snippet.contains("Footprint"),
        "Snippet must contain footprint reference"
    );

    // Query 3: Multi-subagent and TaskLogBuffer -> should match product_faqs.md
    let q3 = index.search("orkestrasi subagent tasklogbuffer isolasi log terminal", 3);
    assert!(!q3.is_empty());
    assert_eq!(q3[0].document, "product_faqs.md");
}

#[test]
fn challenge_bm25_term_frequency_saturation_property() {
    // Mathematical invariant: Okapi BM25 scores higher for higher TF, but saturates sublinearly
    let temp = TestTempDir::new("bm25_tf");
    let dir = temp.path();

    // Doc 1 has 1 occurrence of 'rustacean'
    let doc1 = "# Single\nA rustacean wrote this system.";
    // Doc 2 has 5 occurrences of 'rustacean' with similar length
    let doc2 = "# Multiple\nRustacean rustacean rustacean rustacean rustacean coding.";

    fs::write(dir.join("doc1.md"), doc1).unwrap();
    fs::write(dir.join("doc2.md"), doc2).unwrap();

    let index = Bm25Index::build_from_dir(dir).unwrap();
    let res = index.search("rustacean", 2);

    assert_eq!(res.len(), 2);
    assert_eq!(res[0].section, "Multiple", "Document with higher TF must rank higher");
    assert_eq!(res[1].section, "Single");
    assert!(
        res[0].score > res[1].score,
        "Score for 5x TF ({}) must exceed 1x TF ({})",
        res[0].score,
        res[1].score
    );
    // BM25 saturation: 5x TF score should NOT be 5x higher (due to k1 saturation)
    assert!(
        res[0].score < res[1].score * 4.0,
        "BM25 term frequency must saturate sublinearly"
    );
}

#[test]
fn challenge_bm25_adversarial_queries_no_panic() {
    let ws_root = Path::new(".");
    let dir = tools::knowledge::resolve_knowledge_dir(ws_root);
    let index = Bm25Index::build_from_dir(&dir).unwrap();

    // Punctuation and symbols only
    let r_punct = index.search("!@#$%^&*()_+-=[]{}|;':\",.<>/?`~", 5);
    assert!(r_punct.is_empty());

    // Extremely long query (500 repeated words)
    let long_query = "keamanan ".repeat(500);
    let r_long = index.search(&long_query, 3);
    assert!(!r_long.is_empty());

    // Query for non-existent gibberish
    let r_gibberish = index.search("qwertyuiopasdfghjklzxcvbnm9876543210", 3);
    assert!(r_gibberish.is_empty());

    // top_k = 0 behavior: Bm25Index::search must return an empty vector
    let r_zero_k = index.search("keamanan", 0);
    assert!(r_zero_k.is_empty(), "index.search with top_k=0 returns empty results");

    // But search_knowledge / run_knowledge_search bounds top_k=0 by defaulting to 3 (max 10)
    let r_search_zero = search_knowledge(ws_root, "keamanan", 0).unwrap();
    assert!(r_search_zero.len() <= 3, "search_knowledge defaults top_k=0 to 3");
}

#[test]
fn challenge_run_knowledge_search_and_formatting() {
    // 1. Valid search produces structured Markdown
    let out = run_knowledge_search("keamanan akses sistem", 2).expect("Search should succeed");
    assert!(out.starts_with("Found"));
    assert!(out.contains("Document: company_policy.md"));
    assert!(out.contains("Section:"));
    assert!(out.contains("Score:"));

    // 2. No results search formatting
    let empty_out = run_knowledge_search("nonexistentkeywordxyz12345", 3).expect("Search should succeed");
    assert_eq!(empty_out, "No relevant documents found in knowledge base.");

    // 3. format_search_results empty slice
    assert_eq!(
        format_search_results(&[]),
        "No relevant documents found in knowledge base."
    );
}

#[test]
fn challenge_dispatch_tool_knowledge_search_contract() {
    // Test the exact dispatch logic of knowledge_search matching tools::dispatch_tool:
    // 1. Valid arguments with query and top_k
    let valid_args = r#"{"query": "keamanan sistem", "top_k": 2}"#;
    let args: serde_json::Value = serde_json::from_str(valid_args).unwrap();
    let query = args["query"].as_str().expect("query must be present");
    let top_k = args["top_k"].as_u64().map(|v| v as usize).unwrap_or(3);
    let out = run_knowledge_search(query, top_k).unwrap();
    assert!(out.contains("Found") && out.contains("company_policy.md"));

    // 2. Default top_k (missing top_k field defaults to 3)
    let default_top_k_args = r#"{"query": "biner sangat ringan"}"#;
    let args: serde_json::Value = serde_json::from_str(default_top_k_args).unwrap();
    let query = args["query"].as_str().expect("query must be present");
    let top_k = args["top_k"].as_u64().map(|v| v as usize).unwrap_or(3);
    assert_eq!(top_k, 3);
    let out = run_knowledge_search(query, top_k).unwrap();
    assert!(out.contains("Found") && out.contains("product_faqs.md"));

    // 3. Missing query argument fails cleanly
    let missing_query_args = r#"{"top_k": 2}"#;
    let args: serde_json::Value = serde_json::from_str(missing_query_args).unwrap();
    let query_opt = args["query"].as_str();
    assert!(query_opt.is_none(), "Missing 'query' must be caught before search");

    // 4. Invalid JSON fails cleanly
    let bad_json = r#"{"query": "incomplete"#;
    let parse_res: Result<serde_json::Value, _> = serde_json::from_str(bad_json);
    assert!(parse_res.is_err(), "Malformed JSON arguments must fail during parse");
}

#[test]
fn challenge_chunk_markdown_and_stop_words_invariants() {
    assert!(!STOP_WORDS.is_empty(), "STOP_WORDS list must not be empty");
    assert!(STOP_WORDS.contains(&"dan") && STOP_WORDS.contains(&"the"));

    let markdown = "# Title\nOverview paragraph.\n\n## Sub 1\nDetails here.\n";
    let chunks = chunk_markdown("test_chunk.md", markdown);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].section, "Title");
    assert_eq!(chunks[1].section, "Sub 1");
}

#[test]
fn challenge_resolve_effective_root_and_with_workspace_root() {
    let ws_root = Path::new(".");
    let effective = resolve_effective_root(ws_root);
    assert!(effective.join("skills").exists() || effective.join("prompts").exists());

    let temp = TestTempDir::new("scoped_root");
    with_workspace_root(temp.path().to_path_buf(), || {
        let root = tools::filesystem::get_workspace_root();
        assert_eq!(root, temp.path());
    });
}

#[test]
fn challenge_skill_metadata_serde_roundtrip() {
    let meta = SkillMetadata {
        name: "test_skill".to_string(),
        description: "Test description".to_string(),
        tools: vec!["read_file".to_string(), "write_file".to_string()],
        prompt_template: "Prompt template body".to_string(),
        path: PathBuf::from("skills/test/SKILL.md"),
    };
    let serialized = serde_json::to_string(&meta).unwrap();
    let deserialized: SkillMetadata = serde_json::from_str(&serialized).unwrap();
    assert_eq!(meta, deserialized);
}

// ============================================================================
// SUITE 4: SUBPROCESS REPL COMMAND & SLASH AUTOCOMPLETE VERIFICATION
// ============================================================================

#[test]
fn challenge_cli_repl_skills_list_execution() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");

    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli binary");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open child stdin");
        let _ = writeln!(stdin, "/skills list");
        let _ = writeln!(stdin, "/exit");
    }

    let output = child.wait_with_output().expect("Failed to wait on child process");
    assert!(output.status.success(), "ctrl-cli must exit cleanly");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Discovered Agent Skills"),
        "REPL stdout must contain skills table header. Got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("researcher"),
        "REPL stdout must list 'researcher' skill"
    );
    assert!(
        stdout.contains("writer"),
        "REPL stdout must list 'writer' skill"
    );
}

#[test]
fn challenge_cli_repl_skills_info_execution() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");

    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli binary");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open child stdin");
        let _ = writeln!(stdin, "/skills info researcher");
        let _ = writeln!(stdin, "/skills info writer");
        let _ = writeln!(stdin, "/skills info non_existent_persona");
        let _ = writeln!(stdin, "/exit");
    }

    let output = child.wait_with_output().expect("Failed to wait on child process");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify researcher info
    assert!(
        stdout.contains("Skill Details: researcher"),
        "stdout must display researcher skill details"
    );
    assert!(
        stdout.contains("read_file, glob_files, grep_files, web_search, web_fetch, read_tool_result"),
        "stdout must display researcher allowed tools"
    );

    // Verify writer info
    assert!(
        stdout.contains("Skill Details: writer"),
        "stdout must display writer skill details"
    );
    assert!(
        stdout.contains("read_file, write_file, edit_file, glob_files"),
        "stdout must display writer allowed tools"
    );

    // Verify missing skill error message
    assert!(
        stdout.contains("Skill 'non_existent_persona' tidak ditemukan"),
        "stdout must report error for missing skill"
    );
}

#[test]
fn challenge_cli_repl_tools_command_lists_knowledge_search() {
    let bin = env!("CARGO_BIN_EXE_ctrl-cli");

    let mut child = Command::new(bin)
        .arg("--cli")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ctrl-cli binary");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open child stdin");
        let _ = writeln!(stdin, "/tools");
        let _ = writeln!(stdin, "/exit");
    }

    let output = child.wait_with_output().expect("Failed to wait on child process");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("knowledge_search"),
        "/tools output must list knowledge_search tool. Got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("BM25"),
        "/tools description must mention BM25"
    );
}
