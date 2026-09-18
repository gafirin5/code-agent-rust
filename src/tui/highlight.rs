//! Lightweight Zero-Bloat Syntax Highlighting for Terminal and TUI.
//!
//! Provides lexical ANSI syntax coloring for Markdown code blocks (Rust, Python,
//! JavaScript, Shell, JSON) without introducing heavy parsing dependencies.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Highlights markdown fenced code blocks with ANSI color codes.
///
/// Text outside code blocks is preserved as-is. Code inside blocks is highlighted
/// based on the language identifier.
pub fn highlight_markdown_code_blocks_ansi(content: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    let mut lang = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                lang = line.trim_start_matches("```").trim().to_lowercase();
                result.push_str(line);
                result.push('\n');
            } else {
                in_code_block = false;
                result.push_str(line);
                result.push('\n');
            }
        } else if in_code_block {
            let highlighted = highlight_code_tokens_ansi(line, &lang);
            result.push_str(&highlighted);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Highlights a single line or snippet of code using token-based lexical rules.
pub fn highlight_code_tokens_ansi(line: &str, lang: &str) -> String {
    let lang_norm = lang.trim().to_lowercase();
    if lang_norm == "diff" || lang_norm == "patch" {
        if line.starts_with('+') && !line.starts_with("+++") {
            return format!("\x1b[32m{}\x1b[0m", line);
        } else if line.starts_with('-') && !line.starts_with("---") {
            return format!("\x1b[31m{}\x1b[0m", line);
        } else if line.starts_with("@@") {
            return format!("\x1b[36m\x1b[1m{}\x1b[0m", line);
        } else if line.starts_with("diff ") || line.starts_with("index ") || line.starts_with("---") || line.starts_with("+++") {
            return format!("\x1b[35m\x1b[1m{}\x1b[0m", line);
        }
        return line.to_string();
    }
    let keywords = match lang_norm.as_str() {
        "rust" | "rs" => vec![
            "fn", "let", "mut", "pub", "struct", "enum", "match", "impl", "use",
            "mod", "return", "async", "await", "trait", "type", "where", "const",
            "static", "if", "else", "loop", "while", "for", "in", "break", "continue",
        ],
        "python" | "py" => vec![
            "def", "class", "import", "from", "return", "if", "elif", "else",
            "for", "while", "try", "except", "with", "as", "lambda", "pass",
            "raise", "finally", "yield", "async", "await",
        ],
        "javascript" | "js" | "typescript" | "ts" => vec![
            "const", "let", "var", "function", "return", "import", "export",
            "class", "async", "await", "if", "else", "for", "while", "from",
            "new", "try", "catch", "throw", "switch", "case", "default",
        ],
        "shell" | "bash" | "sh" | "zsh" => vec![
            "echo", "if", "then", "else", "elif", "fi", "for", "in", "do",
            "done", "exit", "case", "esac", "set", "export", "local", "source",
        ],
        "json" => vec!["true", "false", "null"],
        _ => return line.to_string(),
    };

    // Check for comments
    let trimmed = line.trim_start();
    if (lang_norm == "rust" || lang_norm == "rs" || lang_norm == "javascript" || lang_norm == "js" || lang_norm == "typescript" || lang_norm == "ts")
        && trimmed.starts_with("//")
    {
        let leading_spaces = &line[..line.len() - trimmed.len()];
        return format!("{}\x1b[90m{}\x1b[0m", leading_spaces, trimmed);
    }
    if (lang_norm == "python" || lang_norm == "py" || lang_norm == "shell" || lang_norm == "bash" || lang_norm == "sh")
        && trimmed.starts_with('#')
    {
        let leading_spaces = &line[..line.len() - trimmed.len()];
        return format!("{}\x1b[90m{}\x1b[0m", leading_spaces, trimmed);
    }

    let mut words = Vec::new();
    for token in line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_') {
        let (lead, trail) = if let Some(idx) = token.find(|c: char| !c.is_alphanumeric() && c != '_') {
            (&token[..idx], &token[idx..])
        } else {
            (token, "")
        };

        if keywords.contains(&lead) {
            words.push(format!("\x1b[35m{}\x1b[0m{}", lead, trail));
        } else if lead.starts_with('"') || token.starts_with('"') || token.starts_with('\'') {
            words.push(format!("\x1b[32m{}\x1b[0m", token));
        } else if !lead.is_empty() && lead.chars().all(|c| c.is_ascii_digit()) {
            words.push(format!("\x1b[36m{}\x1b[0m{}", lead, trail));
        } else {
            words.push(token.to_string());
        }
    }

    words.concat()
}

/// Returns a human-friendly language label with an icon for code block headers.
pub fn get_language_label(lang: &str) -> &'static str {
    match lang.trim().to_lowercase().as_str() {
        "rust" | "rs" => "🦀 Rust",
        "python" | "py" => "🐍 Python",
        "javascript" | "js" => "📜 JavaScript",
        "typescript" | "ts" => "📘 TypeScript",
        "shell" | "bash" | "sh" | "zsh" => "🐚 Shell",
        "json" => "📋 JSON",
        "toml" => "⚙ TOML",
        "markdown" | "md" => "📝 Markdown",
        "diff" | "patch" => "🔍 Git Diff",
        "yaml" | "yml" => "📄 YAML",
        "sql" => "🗄 SQL",
        "html" => "🌐 HTML",
        "css" => "🎨 CSS",
        _ => "💻 Code",
    }
}

/// Highlights a single line of code into styled Ratatui Spans without intermediate ANSI escapes.
pub fn highlight_code_line_spans<'a>(line: &'a str, lang: &str) -> Vec<Span<'a>> {
    let lang_norm = lang.trim().to_lowercase();
    if lang_norm == "diff" || lang_norm == "patch" {
        if line.starts_with('+') && !line.starts_with("+++") {
            return vec![Span::styled(
                line,
                Style::default().fg(Color::Rgb(163, 190, 140)), // green
            )];
        } else if line.starts_with('-') && !line.starts_with("---") {
            return vec![Span::styled(
                line,
                Style::default().fg(Color::Rgb(191, 97, 106)), // red
            )];
        } else if line.starts_with("@@") {
            return vec![Span::styled(
                line,
                Style::default()
                    .fg(Color::Rgb(136, 192, 208)) // cyan
                    .add_modifier(Modifier::BOLD),
            )];
        } else if line.starts_with("diff ") || line.starts_with("index ") || line.starts_with("---") || line.starts_with("+++") {
            return vec![Span::styled(
                line,
                Style::default()
                    .fg(Color::Rgb(180, 142, 173)) // purple
                    .add_modifier(Modifier::BOLD),
            )];
        } else {
            return vec![Span::raw(line)];
        }
    }

    let keywords: &[&str] = match lang_norm.as_str() {
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "struct", "enum", "match", "impl", "use",
            "mod", "return", "async", "await", "trait", "type", "where", "const",
            "static", "if", "else", "loop", "while", "for", "in", "break", "continue",
            "Self", "self", "super", "crate", "ref", "move", "unsafe",
        ],
        "python" | "py" => &[
            "def", "class", "import", "from", "return", "if", "elif", "else",
            "for", "while", "try", "except", "with", "as", "lambda", "pass",
            "raise", "finally", "yield", "async", "await", "True", "False", "None",
        ],
        "javascript" | "js" | "typescript" | "ts" => &[
            "const", "let", "var", "function", "return", "import", "export",
            "class", "async", "await", "if", "else", "for", "while", "from",
            "new", "try", "catch", "throw", "switch", "case", "default",
            "true", "false", "null", "undefined",
        ],
        "shell" | "bash" | "sh" | "zsh" => &[
            "echo", "if", "then", "else", "elif", "fi", "for", "in", "do",
            "done", "exit", "case", "esac", "set", "export", "local", "source",
        ],
        "json" => &["true", "false", "null"],
        _ => return vec![Span::raw(line)],
    };

    let trimmed = line.trim_start();
    let leading_len = line.len() - trimmed.len();
    let leading_spaces = &line[..leading_len];

    // Comments check
    let is_c_style_comment = (lang_norm == "rust" || lang_norm == "rs" || lang_norm == "javascript" || lang_norm == "js" || lang_norm == "typescript" || lang_norm == "ts") && trimmed.starts_with("//");
    let is_hash_comment = (lang_norm == "python" || lang_norm == "py" || lang_norm == "shell" || lang_norm == "bash" || lang_norm == "sh") && trimmed.starts_with('#');

    if is_c_style_comment || is_hash_comment {
        let mut spans = Vec::new();
        if !leading_spaces.is_empty() {
            spans.push(Span::raw(leading_spaces));
        }
        spans.push(Span::styled(
            trimmed,
            Style::default()
                .fg(Color::Rgb(108, 118, 137))
                .add_modifier(Modifier::ITALIC),
        ));
        return spans;
    }

    let mut spans = Vec::new();
    if !leading_spaces.is_empty() {
        spans.push(Span::raw(leading_spaces));
    }

    for token in trimmed.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_') {
        let (lead, trail) = if let Some(idx) = token.find(|c: char| !c.is_alphanumeric() && c != '_') {
            (&token[..idx], &token[idx..])
        } else {
            (token, "")
        };

        if keywords.contains(&lead) {
            spans.push(Span::styled(
                lead,
                Style::default()
                    .fg(Color::Rgb(180, 142, 173))
                    .add_modifier(Modifier::BOLD),
            ));
            if !trail.is_empty() {
                spans.push(Span::raw(trail));
            }
        } else if lead.starts_with('"') || token.starts_with('"') || token.starts_with('\'') {
            spans.push(Span::styled(
                token,
                Style::default().fg(Color::Rgb(163, 190, 140)),
            ));
        } else if !lead.is_empty() && lead.chars().all(|c| c.is_ascii_digit()) {
            spans.push(Span::styled(
                lead,
                Style::default().fg(Color::Rgb(143, 188, 187)),
            ));
            if !trail.is_empty() {
                spans.push(Span::raw(trail));
            }
        } else if !lead.is_empty() && lead.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            spans.push(Span::styled(
                lead,
                Style::default()
                    .fg(Color::Rgb(235, 203, 139))
                    .add_modifier(Modifier::BOLD),
            ));
            if !trail.is_empty() {
                spans.push(Span::raw(trail));
            }
        } else {
            spans.push(Span::raw(token));
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_rust_code_block() {
        let md = "```rust\npub fn hello() -> &'static str {\n    \"world\"\n}\n```";
        let highlighted = highlight_markdown_code_blocks_ansi(md);
        assert!(highlighted.contains("\x1b[35mpub\x1b[0m"));
        assert!(highlighted.contains("\x1b[35mfn\x1b[0m"));
        assert!(highlighted.contains("\x1b[32m"));
    }

    #[test]
    fn test_highlight_python_code_block() {
        let md = "```python\ndef calculate(x):\n    return x * 2\n```";
        let highlighted = highlight_markdown_code_blocks_ansi(md);
        assert!(highlighted.contains("\x1b[35mdef\x1b[0m"));
        assert!(highlighted.contains("\x1b[35mreturn\x1b[0m"));
    }

    #[test]
    fn test_highlight_unknown_language_unmodified() {
        let md = "```unknown\nsome raw code\n```";
        let highlighted = highlight_markdown_code_blocks_ansi(md);
        assert!(highlighted.contains("some raw code"));
    }

    #[test]
    fn test_highlight_code_line_spans() {
        let spans = highlight_code_line_spans("pub fn main() {", "rust");
        assert!(!spans.is_empty());
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "pub fn main() {");

        let py_spans = highlight_code_line_spans("# a comment", "python");
        assert_eq!(py_spans.len(), 1);

        let labels = get_language_label("rs");
        assert!(labels.contains("Rust"));
        let diff_labels = get_language_label("diff");
        assert!(diff_labels.contains("Diff"));
    }

    #[test]
    fn test_highlight_diff_code_block() {
        let md = "```diff\n+added line\n-removed line\n@@ -1,3 +1,4 @@\n```";
        let highlighted = highlight_markdown_code_blocks_ansi(md);
        assert!(highlighted.contains("\x1b[32m+added line\x1b[0m"));
        assert!(highlighted.contains("\x1b[31m-removed line\x1b[0m"));
        assert!(highlighted.contains("\x1b[36m\x1b[1m@@ -1,3 +1,4 @@\x1b[0m"));
    }

    #[test]
    fn test_highlight_diff_spans() {
        let add_span = highlight_code_line_spans("+let x = 10;", "diff");
        assert_eq!(add_span[0].content, "+let x = 10;");

        let del_span = highlight_code_line_spans("-let x = 5;", "diff");
        assert_eq!(del_span[0].content, "-let x = 5;");

        let hunk_span = highlight_code_line_spans("@@ -10,4 +10,5 @@", "diff");
        assert_eq!(hunk_span[0].content, "@@ -10,4 +10,5 @@");
    }
}
