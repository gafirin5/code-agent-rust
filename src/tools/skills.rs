use anyhow::Result;
use std::path::PathBuf;

pub fn load_skill_content(name: &str) -> Result<String> {
    let clean_name = name.trim().to_lowercase();

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
}
