use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub prompt_template: String,
    pub path: PathBuf,
}

/// Parses YAML frontmatter if present between `---` markers at the start of `content`.
/// Returns `Some((SkillMetadata, remaining_body))` if frontmatter is found,
/// or `None` if no frontmatter is found.
pub fn parse_skill_frontmatter(content: &str) -> Option<(SkillMetadata, String)> {
    let trimmed = content.trim_start();
    let rest = trimmed.strip_prefix("---")?;

    // Find closing `---`
    let (yaml_block, body) = if let Some(idx) = rest.find("\r\n---") {
        let yaml = &rest[..idx];
        let after = &rest[idx + 5..];
        let body = after.trim_start_matches(['\r', '\n']);
        (yaml, body)
    } else {
        let idx = rest.find("\n---")?;
        let yaml = &rest[..idx];
        let after = &rest[idx + 4..];
        let body = after.trim_start_matches(['\r', '\n']);
        (yaml, body)
    };

    let mut name = String::new();
    let mut description = String::new();
    let mut tools = Vec::new();
    let mut in_tools_list = false;

    for raw_line in yaml_block.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if in_tools_list {
            if line.starts_with('-') {
                let tool_item = line.trim_start_matches('-').trim();
                let clean = tool_item.trim_matches(&['\'', '"', '`'][..]);
                if !clean.is_empty() {
                    tools.push(clean.to_string());
                }
                continue;
            } else if line.contains(':') {
                in_tools_list = false;
            }
        }

        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let val = val.trim();
            match key.as_str() {
                "name" => {
                    name = val.trim_matches(&['\'', '"', '`'][..]).to_string();
                }
                "description" => {
                    description = val.trim_matches(&['\'', '"', '`'][..]).to_string();
                }
                "tools" => {
                    if val.starts_with('[') && val.ends_with(']') {
                        let inner = &val[1..val.len() - 1];
                        for item in inner.split(',') {
                            let clean = item.trim().trim_matches(&['\'', '"', '`'][..]);
                            if !clean.is_empty() {
                                tools.push(clean.to_string());
                            }
                        }
                    } else if val.is_empty() {
                        in_tools_list = true;
                    }
                }
                _ => {}
            }
        }
    }

    let meta = SkillMetadata {
        name,
        description,
        tools,
        prompt_template: body.to_string(),
        path: PathBuf::new(),
    };

    Some((meta, body.to_string()))
}

/// Parses a skill file from the given path, extracting frontmatter or using fallback metadata.
pub fn parse_skill_file(path: &Path) -> Result<SkillMetadata> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read skill file at '{}'", path.display()))?;

    let default_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());

    let default_name = if default_name.eq_ignore_ascii_case("skill") {
        path.parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(default_name)
    } else {
        default_name
            .strip_suffix("_persona")
            .unwrap_or(&default_name)
            .to_string()
    };

    if let Some((mut meta, _body)) = parse_skill_frontmatter(&content) {
        if meta.name.is_empty() {
            meta.name = default_name;
        }
        meta.path = path.to_path_buf();
        Ok(meta)
    } else {
        // Plain markdown file (e.g. prompts/*_persona.md)
        let first_heading = content
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().trim_start_matches('#').trim().to_string())
            .unwrap_or_default();
        let description = if !first_heading.is_empty() {
            first_heading
        } else {
            format!("Skill from {}", path.display())
        };

        Ok(SkillMetadata {
            name: default_name,
            description,
            tools: Vec::new(),
            prompt_template: content,
            path: path.to_path_buf(),
        })
    }
}

/// Resolves the effective workspace root, checking for skills/ or prompts/ directories,
/// falling back to parent directory if executing inside ctrl-cli/ subdirectory.
pub fn resolve_effective_root(workspace_root: &Path) -> PathBuf {
    if workspace_root.join("skills").exists()
        || workspace_root.join(".ctrl").join("skills").exists()
        || workspace_root.join("prompts").exists()
    {
        return workspace_root.to_path_buf();
    }
    let parent = workspace_root.join("..");
    if parent.join("skills").exists()
        || parent.join(".ctrl").join("skills").exists()
        || parent.join("prompts").exists()
    {
        return parent;
    }
    workspace_root.to_path_buf()
}

fn scan_dir_skills(
    dir: &Path,
    discovered: &mut Vec<SkillMetadata>,
    seen_names: &mut HashSet<String>,
) {
    if !dir.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut sorted_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted_entries.sort_by_key(|e| e.path());

    for entry in sorted_entries {
        let path = entry.path();
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "md") {
                if let Ok(meta) = parse_skill_file(&path) {
                    let key = meta.name.to_lowercase();
                    if !seen_names.contains(&key) {
                        seen_names.insert(key);
                        discovered.push(meta);
                    }
                }
            }
        } else if path.is_dir() {
            // Subdirectory (e.g. skills/researcher/SKILL.md)
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                if let Ok(meta) = parse_skill_file(&skill_file) {
                    let key = meta.name.to_lowercase();
                    if !seen_names.contains(&key) {
                        seen_names.insert(key);
                        discovered.push(meta);
                    }
                }
            } else if let Ok(sub_entries) = std::fs::read_dir(&path) {
                let mut sorted_sub: Vec<_> = sub_entries.filter_map(|e| e.ok()).collect();
                sorted_sub.sort_by_key(|e| e.path());
                for sub_e in sorted_sub {
                    let sub_path = sub_e.path();
                    if sub_path.is_file() && sub_path.extension().is_some_and(|ext| ext == "md") {
                        if let Ok(meta) = parse_skill_file(&sub_path) {
                            let key = meta.name.to_lowercase();
                            if !seen_names.contains(&key) {
                                seen_names.insert(key);
                                discovered.push(meta);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Automatically discovers skills from workspace directories:
/// 1. `.ctrl/skills/`
/// 2. `skills/`
/// 3. `prompts/`
///
/// Deduplicates by skill name with priority matching the order above.
pub fn discover_skills(workspace_root: &Path) -> Vec<SkillMetadata> {
    let base = resolve_effective_root(workspace_root);
    let mut discovered: Vec<SkillMetadata> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    // Priority 1: .ctrl/skills/
    let ctrl_skills = base.join(".ctrl").join("skills");
    scan_dir_skills(&ctrl_skills, &mut discovered, &mut seen_names);

    // Priority 2: skills/
    let skills_dir = base.join("skills");
    scan_dir_skills(&skills_dir, &mut discovered, &mut seen_names);

    // Priority 3: prompts/
    let prompts_dir = base.join("prompts");
    scan_dir_skills(&prompts_dir, &mut discovered, &mut seen_names);

    discovered
}

/// Finds a skill by name (case-insensitive) among discovered workspace skills.
pub fn get_skill_by_name(name: &str, workspace_root: &Path) -> Option<SkillMetadata> {
    let clean = name.trim().to_lowercase();
    let skills = discover_skills(workspace_root);
    skills.into_iter().find(|s| s.name.to_lowercase() == clean)
}

pub fn load_skill_content(name: &str) -> Result<String> {
    let clean_name = name.trim().to_lowercase();

    // First attempt discovery via get_skill_by_name
    let root = crate::tools::filesystem::get_workspace_root();
    if let Some(meta) = get_skill_by_name(&clean_name, &root) {
        if let Ok(content) = std::fs::read_to_string(&meta.path) {
            return Ok(format!(
                "--- Loaded Skill '{}' from '{}' ---\n\n{}",
                name,
                meta.path.display(),
                content
            ));
        }
    }

    // Check workspace candidates
    let candidates = [
        PathBuf::from(format!("skills/{}/SKILL.md", clean_name)),
        PathBuf::from(format!("skills/{}.md", clean_name)),
        PathBuf::from(format!("prompts/{}_persona.md", clean_name)),
        PathBuf::from(format!("prompts/{}.md", clean_name)),
        PathBuf::from(format!(".ctrl/skills/{}.md", clean_name)),
        PathBuf::from(format!(".ctrl/skills/{}/SKILL.md", clean_name)),
        // Parent directory fallback when running from inside ctrl-cli/
        PathBuf::from(format!("../skills/{}/SKILL.md", clean_name)),
        PathBuf::from(format!("../skills/{}.md", clean_name)),
        PathBuf::from(format!("../prompts/{}_persona.md", clean_name)),
        PathBuf::from(format!("../prompts/{}.md", clean_name)),
        PathBuf::from(format!("../.ctrl/skills/{}.md", clean_name)),
    ];

    for path in &candidates {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                return Ok(format!(
                    "--- Loaded Skill '{}' from '{}' ---\n\n{}",
                    name,
                    path.display(),
                    content
                ));
            }
        }
    }

    // Check home directory ~/.ctrl-cli/skills/
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);

    if let Some(h) = home {
        let home_skill = h
            .join(".ctrl-cli")
            .join("skills")
            .join(format!("{}.md", clean_name));
        if home_skill.exists() {
            if let Ok(content) = std::fs::read_to_string(&home_skill) {
                return Ok(format!(
                    "--- Loaded Skill '{}' from '{}' ---\n\n{}",
                    name,
                    home_skill.display(),
                    content
                ));
            }
        }
    }

    anyhow::bail!(
        "Skill '{}' not found in workspace (checked: skills/{}/SKILL.md, prompts/{}_persona.md, .ctrl/skills/{}.md).",
        name,
        clean_name,
        clean_name,
        clean_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_skill_researcher() {
        let res = load_skill_content("researcher");
        assert!(res.is_ok(), "Expected to load researcher skill: {:?}", res.err());
        let content = res.unwrap();
        assert!(content.contains("Researcher Persona") || content.contains("Researcher"));
    }

    #[test]
    fn test_load_skill_writer() {
        let res = load_skill_content("writer");
        assert!(res.is_ok(), "Expected to load writer skill: {:?}", res.err());
        let content = res.unwrap();
        assert!(content.contains("Writer Persona") || content.contains("Writer"));
    }

    #[test]
    fn test_load_non_existent_skill() {
        let res = load_skill_content("non_existent_skill_xyz_123");
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_skill_frontmatter_inline_tools() {
        let sample = r#"---
name: sample-analyzer
description: An in-depth code analytics subagent
tools: [read_file, grep_files, glob_files]
---
# Instructions
Analyze the given target repository.
"#;
        let parsed = parse_skill_frontmatter(sample);
        assert!(parsed.is_some());
        let (meta, body) = parsed.unwrap();
        assert_eq!(meta.name, "sample-analyzer");
        assert_eq!(meta.description, "An in-depth code analytics subagent");
        assert_eq!(meta.tools, vec!["read_file", "grep_files", "glob_files"]);
        assert!(body.contains("Analyze the given target repository."));
    }

    #[test]
    fn test_parse_skill_frontmatter_bullet_tools() {
        let sample = r#"---
name: multi-tool-worker
description: Handles multiple tasks
tools:
  - read_file
  - write_file
---
# Worker
Do the work.
"#;
        let parsed = parse_skill_frontmatter(sample);
        assert!(parsed.is_some());
        let (meta, _) = parsed.unwrap();
        assert_eq!(meta.name, "multi-tool-worker");
        assert_eq!(meta.tools, vec!["read_file", "write_file"]);
    }

    #[test]
    fn test_discover_and_get_skill() {
        let root = Path::new(".");
        let skills = discover_skills(root);
        assert!(!skills.is_empty(), "Expected to discover at least researcher and writer");
        
        let researcher = get_skill_by_name("researcher", root);
        assert!(researcher.is_some(), "Expected to find researcher skill");
        let r = researcher.unwrap();
        assert_eq!(r.name, "researcher");
        assert!(!r.tools.is_empty());
        assert!(r.description.contains("Researcher"));

        let writer = get_skill_by_name("writer", root);
        assert!(writer.is_some(), "Expected to find writer skill");
        let w = writer.unwrap();
        assert_eq!(w.name, "writer");
        assert!(!w.tools.is_empty());
    }
}

