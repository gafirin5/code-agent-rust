use crate::tools::result_store::ResultStore;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

thread_local! {
    static CUSTOM_WORKSPACE_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Executes a closure with a scoped thread-local workspace root override.
/// Primary mechanism for unit & hermetic tests to safely operate in isolated temporary directories.
pub fn with_workspace_root<F, R>(root: PathBuf, f: F) -> R
where
    F: FnOnce() -> R,
{
    CUSTOM_WORKSPACE_ROOT.with(|c| {
        *c.borrow_mut() = Some(root);
    });
    let result = f();
    CUSTOM_WORKSPACE_ROOT.with(|c| {
        *c.borrow_mut() = None;
    });
    result
}

/// Retrieves the active workspace root path, checking thread-local override, environment variable,
/// or falling back to current working directory.
pub fn get_workspace_root() -> PathBuf {
    if let Some(custom) = CUSTOM_WORKSPACE_ROOT.with(|c| c.borrow().clone()) {
        return custom;
    }
    if let Ok(env_root) = std::env::var("CTRL_WORKSPACE_ROOT") {
        if !env_root.trim().is_empty() {
            return PathBuf::from(env_root.trim());
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Clean canonicalizer with Windows extended-length UNC prefix (`\\?\`) stripping.
pub fn normalize_canonical_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Helper to check whether `path` is equal to `root` or a descendant of `root`.
fn is_within_root(path: &Path, root: &Path) -> bool {
    let p_s = path.to_string_lossy();
    let r_s = root.to_string_lossy();

    #[cfg(windows)]
    {
        let p_lower = p_s.to_lowercase().replace('/', "\\");
        let r_lower = r_s.to_lowercase().replace('/', "\\");
        if p_lower.starts_with(&r_lower) {
            let rem = &p_lower[r_lower.len()..];
            rem.is_empty() || rem.starts_with('\\')
        } else {
            false
        }
    }

    #[cfg(not(windows))]
    {
        if p_s.starts_with(&r_s) {
            let rem = &p_s[r_s.len()..];
            rem.is_empty() || rem.starts_with('/')
        } else {
            false
        }
    }
}

/// Resolves and enforces that `path_str` resides securely within the workspace root sandbox.
///
/// Security protections:
/// - Rejects directory traversal sequences (`..`, `../../secret.txt`, `subdir/../../outside.txt`).
/// - Rejects absolute paths targeting sensitive files or locations outside the workspace root.
/// - Strips Windows extended-length UNC prefixes (`\\?\`).
/// - Handles Windows 8.3 short paths and case-insensitivity seamlessly.
/// - Follows existing symlinks for existing files, or verifies nearest existing parent for new files.
pub fn resolve_sandboxed_path(path_str: &str) -> Result<PathBuf> {
    let raw_root = normalize_canonical_path(&get_workspace_root());
    let canonical_root = normalize_canonical_path(
        &raw_root.canonicalize().unwrap_or_else(|_| raw_root.clone()),
    );

    let p = Path::new(path_str);
    let target = if p.is_absolute() {
        p.to_path_buf()
    } else {
        canonical_root.join(p)
    };

    let target = normalize_canonical_path(&target);

    // 1. Lexical traversal check
    let mut normalized = PathBuf::new();
    for comp in target.components() {
        match comp {
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    anyhow::bail!(
                        "Access denied: path '{}' is outside the workspace sandbox root '{}'",
                        path_str,
                        canonical_root.display()
                    );
                }
            }
            std::path::Component::Normal(c) => normalized.push(c),
            std::path::Component::RootDir => normalized.push(std::path::Component::RootDir),
            std::path::Component::Prefix(prefix) => {
                normalized.push(std::path::Component::Prefix(prefix))
            }
            std::path::Component::CurDir => {}
        }
    }

    // 2. Physical canonicalization check
    // If the path exists on disk, canonicalize it and verify against canonical_root
    if normalized.exists() {
        let canon = normalize_canonical_path(
            &normalized
                .canonicalize()
                .unwrap_or_else(|_| normalized.clone()),
        );
        if is_within_root(&canon, &canonical_root) || is_within_root(&normalized, &raw_root) {
            return Ok(canon);
        }
        anyhow::bail!(
            "Access denied: path '{}' is outside the workspace sandbox root '{}'",
            path_str,
            canonical_root.display()
        );
    }

    // If file does not exist yet (e.g. for write_file), find nearest existing parent directory
    let mut curr = normalized.as_path();
    let mut relative_suffix = Vec::new();

    while let Some(parent) = curr.parent() {
        if let Some(file_name) = curr.file_name() {
            relative_suffix.push(file_name);
        }
        if parent.exists() {
            let canon_parent = normalize_canonical_path(
                &parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()),
            );
            if is_within_root(&canon_parent, &canonical_root) || is_within_root(parent, &raw_root) {
                // Rebuild canonical path
                let mut full_canon = canon_parent;
                for part in relative_suffix.into_iter().rev() {
                    full_canon.push(part);
                }
                return Ok(full_canon);
            } else {
                anyhow::bail!(
                    "Access denied: path '{}' is outside the workspace sandbox root '{}'",
                    path_str,
                    canonical_root.display()
                );
            }
        }
        curr = parent;
    }

    // If no parent exists, check lexical normalized against canonical_root and raw_root
    if is_within_root(&normalized, &canonical_root) || is_within_root(&normalized, &raw_root) {
        Ok(normalized)
    } else {
        anyhow::bail!(
            "Access denied: path '{}' is outside the workspace sandbox root '{}'",
            path_str,
            canonical_root.display()
        );
    }
}

pub fn read_file(
    path_str: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String> {
    let path = resolve_sandboxed_path(path_str)?;
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
    let path = resolve_sandboxed_path(path_str)?;
    let should_overwrite = overwrite.unwrap_or(true);

    if path.exists() && !should_overwrite {
        anyhow::bail!(
            "File already exists and overwrite is set to false: '{}'",
            path_str
        );
    }

    // Save snapshot checkpoint before modifying
    let _ = crate::agent::checkpoint::CheckpointManager::record_checkpoint(path_str, "write_file");

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create parent directories for: '{}'",
                    path.display()
                )
            })?;
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
    let path = resolve_sandboxed_path(path_str)?;
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
            let mut res =
                String::with_capacity(normalized_existing.len() + normalized_replacement.len());
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
