use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use crate::tools::result_store::ResultStore;

fn resolve_path(path_str: &str) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(p)
    }
}

pub fn read_file(path_str: &str, start_line: Option<usize>, end_line: Option<usize>) -> Result<String> {
    let path = resolve_path(path_str);
    if !path.exists() {
        anyhow::bail!("File not found: '{}'", path_str);
    }
    if path.is_dir() {
        anyhow::bail!("Path is a directory, not a file: '{}'", path_str);
    }

    let bytes = std::fs::read(&path)
        .with_context(|| format!("Failed to read file: '{}'", path.display()))?;

    // Check if binary (presence of null bytes in first 1024 bytes)
    let check_len = bytes.len().min(1024);
    if bytes[..check_len].contains(&0) {
        return Ok(format!(
            "[Binary File: {} ({} bytes)] Binary content cannot be displayed.",
            path_str,
            bytes.len()
        ));
    }

    let content = String::from_utf8_lossy(&bytes);
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();

    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(total_lines).min(total_lines);

    if start > total_lines && total_lines > 0 {
        anyhow::bail!(
            "start_line ({}) exceeds total lines ({}) in file '{}'",
            start,
            total_lines,
            path_str
        );
    }

    let slice_start = start - 1;
    let slice_end = end.max(slice_start);

    let mut output = String::new();
    output.push_str(&format!(
        "--- File: {} (Lines {}-{} of {}) ---\n",
        path_str, start, slice_end, total_lines
    ));

    for (idx, line) in all_lines[slice_start..slice_end].iter().enumerate() {
        let line_num = slice_start + idx + 1;
        output.push_str(&format!("{:4}: {}\n", line_num, line));
    }

    Ok(ResultStore::process_output(output, 250, 15000))
}

pub fn write_file(path_str: &str, content: &str, overwrite: Option<bool>) -> Result<String> {
    let path = resolve_path(path_str);
    let should_overwrite = overwrite.unwrap_or(true);

    if path.exists() && !should_overwrite {
        anyhow::bail!("File already exists and overwrite is set to false: '{}'", path_str);
    }

    // Save snapshot checkpoint before modifying
    let _ = crate::agent::checkpoint::CheckpointManager::record_checkpoint(path_str, "write_file");

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directories for: '{}'", path.display()))?;
        }
    }

    std::fs::write(&path, content.as_bytes())
        .with_context(|| format!("Failed to write file: '{}'", path.display()))?;

    let line_count = content.lines().count();
    let byte_count = content.len();

    let mut message = format!(
        "Successfully wrote to '{}' ({} lines, {} bytes).",
        path_str, line_count, byte_count
    );

    // Self-Healing Code Loop: Check syntax/compiler diagnostics
    if let Some(diag) = crate::tools::self_heal::check_file_diagnostics(path_str) {
        message.push_str("\n\n⚠️ [Compiler Diagnostics Detected - Self-Healing Loop]:\n");
        message.push_str(&diag);
        message.push_str("\nPlease review and correct the compiler diagnostics in the next turn.");
    }

    Ok(message)
}

pub fn edit_file(
    path_str: &str,
    target_content: &str,
    replacement_content: &str,
    allow_multiple: Option<bool>,
) -> Result<String> {
    let path = resolve_path(path_str);
    if !path.exists() {
        anyhow::bail!("File not found: '{}'", path_str);
    }

    // Save snapshot checkpoint before modifying
    let _ = crate::agent::checkpoint::CheckpointManager::record_checkpoint(path_str, "edit_file");

    let existing = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read file: '{}'", path.display()))?;

    // Normalize Windows CRLF to LF for reliable matching
    let normalized_existing = existing.replace("\r\n", "\n");
    let normalized_target = target_content.replace("\r\n", "\n");
    let normalized_replacement = replacement_content.replace("\r\n", "\n");

    let count = normalized_existing.matches(&normalized_target).count();
    if count == 0 {
        anyhow::bail!(
            "Target content was not found in '{}'. Ensure exact whitespace and context match.",
            path_str
        );
    }

    let multi = allow_multiple.unwrap_or(false);
    if count > 1 && !multi {
        anyhow::bail!(
            "Found {} occurrences of target content in '{}'. Provide more surrounding lines to make the target content unique, or set allow_multiple=true.",
            count,
            path_str
        );
    }

    let updated = if multi {
        normalized_existing.replace(&normalized_target, &normalized_replacement)
    } else {
        // Replace only the first occurrence
        if let Some(pos) = normalized_existing.find(&normalized_target) {
            let mut res = String::with_capacity(normalized_existing.len() + normalized_replacement.len());
            res.push_str(&normalized_existing[..pos]);
            res.push_str(&normalized_replacement);
            res.push_str(&normalized_existing[pos + normalized_target.len()..]);
            res
        } else {
            normalized_existing
        }
    };

    // Preserve CRLF if original had CRLF
    let final_content = if existing.contains("\r\n") {
        updated.replace("\n", "\r\n")
    } else {
        updated
    };

    std::fs::write(&path, final_content.as_bytes())
        .with_context(|| format!("Failed to write edited content to: '{}'", path.display()))?;

    let replaced_str = if count == 1 {
        "1 occurrence".to_string()
    } else {
        format!("{} occurrences", count)
    };

    let mut message = format!(
        "Successfully edited '{}' (replaced {}).",
        path_str, replaced_str
    );

    // Self-Healing Code Loop: Check syntax/compiler diagnostics
    if let Some(diag) = crate::tools::self_heal::check_file_diagnostics(path_str) {
        message.push_str("\n\n⚠️ [Compiler Diagnostics Detected - Self-Healing Loop]:\n");
        message.push_str(&diag);
        message.push_str("\nPlease review and correct the compiler diagnostics in the next turn.");
    }

    Ok(message)
}
