#[cfg(test)]
mod tests {
    use crate::tools::filesystem::{edit_file, read_file, write_file};
    use crate::tools::result_store::ResultStore;
    use crate::tools::search::{glob_files, grep_files};

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
}
