use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use crate::tools::result_store::ResultStore;

fn should_skip_dir(dir_name: &str) -> bool {
    matches!(
        dir_name,
        ".git" | ".cargo" | "node_modules" | "target" | ".zig-cache" | "zig-out" | ".vscode" | ".idea"
    )
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>, max_files: usize) {
    if files.len() >= max_files {
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !should_skip_dir(name) {
                        collect_files_recursive(&path, files, max_files);
                    }
                }
            } else if path.is_file() {
                files.push(path);
                if files.len() >= max_files {
                    return;
                }
            }
        }
    }
}

fn matches_glob_simple(filename: &str, pattern: &str) -> bool {
    if pattern == "*" || pattern == "*.*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        filename.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        filename.starts_with(prefix)
    } else if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            filename.starts_with(parts[0]) && filename.ends_with(parts[1])
        } else {
            filename.contains(pattern.trim_matches('*'))
        }
    } else {
        filename == pattern
    }
}

pub fn glob_files(
    pattern: &str,
    path_str: Option<&str>,
    mode: Option<&str>,
) -> Result<String> {
    let base_dir = path_str.unwrap_or(".");
    let root = Path::new(base_dir);
    if !root.exists() {
        anyhow::bail!("Base directory not found: '{}'", base_dir);
    }

    let mut files = Vec::new();
    collect_files_recursive(root, &mut files, 5000);

    let clean_pattern = pattern.replace('\\', "/");
    let is_path_pattern = clean_pattern.contains('/');

    let mut matched = Vec::new();
    for file in files {
        let rel_path = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let is_match = if is_path_pattern {
            matches_glob_simple(&rel_path, &clean_pattern)
        } else {
            matches_glob_simple(filename, &clean_pattern)
        };

        if is_match {
            matched.push(rel_path);
        }
    }

    let is_count = mode.map(|m| m.eq_ignore_ascii_case("count")).unwrap_or(false);
    if is_count {
        return Ok(format!(
            "Total matched files for pattern '{}': {}",
            pattern,
            matched.len()
        ));
    }

    if matched.is_empty() {
        return Ok(format!("No files found matching pattern '{}' in '{}'", pattern, base_dir));
    }

    let total = matched.len();
    let display_limit = 100;
    let mut out = format!("Matched {} file(s) for pattern '{}':\n", total, pattern);
    for path in matched.iter().take(display_limit) {
        out.push_str(&format!("  {}\n", path));
    }

    if total > display_limit {
        out.push_str(&format!("  ... and {} more files (use more specific pattern)\n", total - display_limit));
    }

    Ok(out)
}

pub fn grep_files(
    query: &str,
    path_str: Option<&str>,
    include: Option<&str>,
    case_insensitive: Option<bool>,
    head_limit: Option<usize>,
    offset: Option<usize>,
    context_lines: Option<usize>,
) -> Result<String> {
    let base_dir = path_str.unwrap_or(".");
    let root = Path::new(base_dir);
    if !root.exists() {
        anyhow::bail!("Directory not found: '{}'", base_dir);
    }

    let mut files = Vec::new();
    collect_files_recursive(root, &mut files, 3000);

    let ci = case_insensitive.unwrap_or(false);
    let target_query = if ci { query.to_lowercase() } else { query.to_string() };
    let limit = head_limit.unwrap_or(50).max(1);
    let skip_count = offset.unwrap_or(1).saturating_sub(1);
    let ctx = context_lines.unwrap_or(0);

    struct MatchItem {
        file: String,
        line_num: usize,
        line_text: String,
        context_before: Vec<(usize, String)>,
        context_after: Vec<(usize, String)>,
    }

    let mut all_matches = Vec::new();

    for file_path in files {
        let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(pat) = include {
            if !matches_glob_simple(filename, pat) {
                continue;
            }
        }

        let Ok(bytes) = fs::read(&file_path) else {
            continue;
        };

        // Skip binary files
        let check_len = bytes.len().min(1024);
        if bytes[..check_len].contains(&0) {
            continue;
        }

        let content = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = content.lines().collect();
        let rel_file = file_path.strip_prefix(root).unwrap_or(&file_path).to_string_lossy().replace('\\', "/");

        for (idx, line) in lines.iter().enumerate() {
            let line_to_check = if ci { line.to_lowercase() } else { line.to_string() };
            if line_to_check.contains(&target_query) {
                let mut before = Vec::new();
                if ctx > 0 {
                    let start_ctx = idx.saturating_sub(ctx);
                    for c_idx in start_ctx..idx {
                        before.push((c_idx + 1, lines[c_idx].to_string()));
                    }
                }

                let mut after = Vec::new();
                if ctx > 0 {
                    let end_ctx = (idx + 1 + ctx).min(lines.len());
                    for c_idx in (idx + 1)..end_ctx {
                        after.push((c_idx + 1, lines[c_idx].to_string()));
                    }
                }

                all_matches.push(MatchItem {
                    file: rel_file.clone(),
                    line_num: idx + 1,
                    line_text: line.to_string(),
                    context_before: before,
                    context_after: after,
                });
            }
        }
    }

    let total_matches = all_matches.len();
    if total_matches == 0 {
        return Ok(format!("No matches found for query '{}' in '{}'", query, base_dir));
    }

    let sliced = all_matches.into_iter().skip(skip_count).take(limit).collect::<Vec<_>>();
    let mut out = format!(
        "Found {} match(es) for query '{}' (showing {}-{}):\n\n",
        total_matches,
        query,
        skip_count + 1,
        (skip_count + sliced.len()).min(total_matches)
    );

    for m in sliced {
        out.push_str(&format!("{}:{}:\n", m.file, m.line_num));
        for (b_num, b_line) in m.context_before {
            out.push_str(&format!("  {:4}- {}\n", b_num, b_line));
        }
        out.push_str(&format!("  {:4}: {}\n", m.line_num, m.line_text));
        for (a_num, a_line) in m.context_after {
            out.push_str(&format!("  {:4}+ {}\n", a_num, a_line));
        }
        out.push('\n');
    }

    Ok(ResultStore::process_output(out, 200, 15000))
}
