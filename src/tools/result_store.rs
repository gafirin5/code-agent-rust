use std::collections::HashMap;
use std::sync::Mutex;

static RESULT_STORE: Mutex<Option<ResultStore>> = Mutex::new(None);

pub struct ResultStore {
    counter: usize,
    entries: HashMap<String, String>,
}

impl ResultStore {
    fn new() -> Self {
        Self {
            counter: 0,
            entries: HashMap::new(),
        }
    }

    pub fn instance<F, R>(f: F) -> R
    where
        F: FnOnce(&mut ResultStore) -> R,
    {
        let mut lock = RESULT_STORE.lock().unwrap();
        if lock.is_none() {
            *lock = Some(ResultStore::new());
        }
        f(lock.as_mut().unwrap())
    }

    /// Stores the output and returns truncated output if it exceeds thresholds.
    pub fn process_output(output: String, max_lines: usize, max_chars: usize) -> String {
        let lines: Vec<&str> = output.lines().collect();
        let total_lines = lines.len();
        let total_chars = output.len();

        if total_lines <= max_lines && total_chars <= max_chars {
            return output;
        }

        Self::instance(|store| {
            store.counter += 1;
            let result_id = format!("tr_{}", store.counter);
            store.entries.insert(result_id.clone(), output.clone());

            let preview_lines = lines.iter().take(max_lines).cloned().collect::<Vec<&str>>().join("\n");
            format!(
                "[OUTPUT TRUNCATED]\nShowing first {} of {} lines ({} total characters).\nUse tool `read_tool_result` with result_id='{}', offset={}, limit={} to read more.\n\n{}",
                max_lines,
                total_lines,
                total_chars,
                result_id,
                max_lines + 1,
                max_lines,
                preview_lines
            )
        })
    }

    /// Paginates through stored output lines (1-indexed offset).
    pub fn read_result(result_id: &str, offset: usize, limit: usize) -> Result<String, String> {
        Self::instance(|store| {
            let content = store
                .entries
                .get(result_id)
                .ok_or_else(|| format!("Result ID '{}' not found or expired.", result_id))?;

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();

            let start = if offset > 0 { offset - 1 } else { 0 };
            if start >= total_lines {
                return Ok(format!(
                    "[EOF] Offset {} is beyond total lines ({} lines).",
                    offset, total_lines
                ));
            }

            let end = (start + limit).min(total_lines);
            let selected_lines = lines[start..end].join("\n");
            let has_more = end < total_lines;

            let header = format!(
                "--- Result ID: {} (Lines {}-{} of {}) ---\n",
                result_id,
                start + 1,
                end,
                total_lines
            );
            let footer = if has_more {
                format!(
                    "\n\n[More lines remaining. Call read_tool_result with offset={}]",
                    end + 1
                )
            } else {
                "\n\n[End of output]".to_string()
            };

            Ok(format!("{}{}{}", header, selected_lines, footer))
        })
    }
}
