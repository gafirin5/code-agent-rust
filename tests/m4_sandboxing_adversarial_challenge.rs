//! Empirical Adversarial Challenge Suite for Milestone 4: Filesystem Path Sandboxing
//!
//! Authored by: challenger_m4_2
//!
//! This test suite aggressively stress-tests the path sandboxing implementation in
//! `ctrl-cli/src/tools/filesystem.rs` and `ctrl-cli/src/tools/search.rs` against
//! adversarial traversal attacks:
//!
//! 1. Relative parent directory escapes (`../`, `../../`, `subdir/../../secret.txt`, `foo/bar/../../../outside.txt`).
//! 2. Absolute path escapes (`C:\Windows\System32`, `C:\`, `/etc/shadow`, `/etc/passwd`, `\Windows\System32`).
//! 3. Windows quirks & normalization (8.3 short names, mixed slashes, redundant slashes, UNC `\\?\` prefixes, case insensitivity).
//! 4. Prefix collision attacks (`workspace_secret` vs `workspace`).
//! 5. Canary integrity verification: verifying that outside canary files are never read, leaked, overwritten, or modified.
//! 6. Search tools sandboxing: verifying `glob_files` and `grep_files` reject escapes via `base_dir`.

pub mod agent {
    #[path = "../../src/agent/checkpoint.rs"]
    pub mod checkpoint;
}

pub mod tools {
    #[path = "../../src/tools/result_store.rs"]
    pub mod result_store;
    #[path = "../../src/tools/self_heal.rs"]
    pub mod self_heal;
    #[path = "../../src/tools/filesystem.rs"]
    pub mod filesystem;
    #[path = "../../src/tools/search.rs"]
    pub mod search;
}

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tools::filesystem::{
    edit_file, normalize_canonical_path, read_file, resolve_sandboxed_path, with_workspace_root,
    write_file,
};
use tools::search::{glob_files, grep_files};

/// Helper creating an isolated test sandbox with an outside sibling directory containing a canary file.
struct SandboxFixture {
    parent_dir: PathBuf,
    workspace_dir: PathBuf,
    _outside_dir: PathBuf,
    canary_file: PathBuf,
    canary_content: String,
}

impl SandboxFixture {
    fn new(name: &str) -> Self {
        let unique = format!(
            "m4_sandbox_{}_{}_{}",
            name,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        );
        let parent_dir = std::env::temp_dir().join(unique);
        let workspace_dir = parent_dir.join("workspace");
        let outside_dir = parent_dir.join("outside_secret");

        fs::create_dir_all(&workspace_dir).expect("Failed to create workspace");
        fs::create_dir_all(&outside_dir).expect("Failed to create outside dir");

        let canary_file = outside_dir.join("canary.txt");
        let canary_content = "TOP_SECRET_CANARY_DO_NOT_LEAK_12345".to_string();
        fs::write(&canary_file, &canary_content).expect("Failed to write canary");

        Self {
            parent_dir,
            workspace_dir,
            _outside_dir: outside_dir,
            canary_file,
            canary_content,
        }
    }

    fn ws(&self) -> PathBuf {
        self.workspace_dir.clone()
    }

    fn verify_canary_untouched(&self) {
        let current = fs::read_to_string(&self.canary_file).expect("Canary file must still exist");
        assert_eq!(
            current, self.canary_content,
            "CRITICAL SECURITY VIOLATION: Canary file was modified by traversal attack!"
        );
    }
}

impl Drop for SandboxFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.parent_dir);
    }
}

#[test]
fn test_challenge_relative_parent_escapes() {
    let fix = SandboxFixture::new("rel_traversal");

    with_workspace_root(fix.ws(), || {
        let attacks = vec![
            "../outside_secret/canary.txt",
            "..\\outside_secret\\canary.txt",
            "../../outside_secret/canary.txt",
            "../../../outside_secret/canary.txt",
            "subdir/../../outside_secret/canary.txt",
            "foo/bar/../../../outside_secret/canary.txt",
            "foo/bar/baz/../../../../outside_secret/canary.txt",
            "./../outside_secret/canary.txt",
            ".\\..\\outside_secret\\canary.txt",
            "subdir/./../../outside_secret/canary.txt",
            "subdir/.././../outside_secret/canary.txt",
            "..",
            "../",
            "..\\",
            "subdir/../..",
            "subdir/../../",
        ];

        for attack in attacks {
            // 1. resolve_sandboxed_path must reject
            let res = resolve_sandboxed_path(attack);
            assert!(
                res.is_err(),
                "Attack path '{}' must be rejected by resolve_sandboxed_path",
                attack
            );
            let err_msg = res.unwrap_err().to_string();
            assert!(
                err_msg.contains("Access denied"),
                "Error for '{}' must contain 'Access denied', got: {}",
                attack,
                err_msg
            );

            // 2. read_file must reject
            let read_res = read_file(attack, None, None);
            assert!(
                read_res.is_err(),
                "read_file('{}') must fail with Access denied",
                attack
            );
            let read_err = read_res.unwrap_err().to_string();
            assert!(
                read_err.contains("Access denied"),
                "read_file('{}') error: {}",
                attack,
                read_err
            );
            assert!(
                !read_err.contains(&fix.canary_content),
                "Error message must not leak canary content!"
            );

            // 3. write_file must reject without modifying
            let write_res = write_file(attack, "MALICIOUS_OVERWRITE", Some(true));
            assert!(
                write_res.is_err(),
                "write_file('{}') must fail with Access denied",
                attack
            );
            assert!(
                write_res.unwrap_err().to_string().contains("Access denied"),
                "write_file('{}') must return Access denied",
                attack
            );

            // 4. edit_file must reject without modifying
            let edit_res = edit_file(attack, "TOP_SECRET", "HACKED", Some(false));
            assert!(
                edit_res.is_err(),
                "edit_file('{}') must fail with Access denied",
                attack
            );
            assert!(
                edit_res.unwrap_err().to_string().contains("Access denied"),
                "edit_file('{}') must return Access denied",
                attack
            );
        }
    });

    fix.verify_canary_untouched();
}

#[test]
fn test_challenge_absolute_path_escapes() {
    let fix = SandboxFixture::new("abs_escapes");

    with_workspace_root(fix.ws(), || {
        let canary_abs = fix.canary_file.to_str().unwrap();

        #[cfg(windows)]
        let abs_attacks = vec![
            canary_abs.to_string(),
            r"C:\Windows\System32\cmd.exe".to_string(),
            r"C:\Windows\win.ini".to_string(),
            r"C:\Program Files".to_string(),
            r"C:\".to_string(),
            r"D:\nonexistent_secret.txt".to_string(),
            r"Z:\evil_drive.txt".to_string(),
            "/etc/shadow".to_string(),
            "/etc/passwd".to_string(),
            r"\Windows\System32\cmd.exe".to_string(),
            r"\outside_secret\canary.txt".to_string(),
        ];

        #[cfg(not(windows))]
        let abs_attacks = vec![
            canary_abs.to_string(),
            "/etc/shadow".to_string(),
            "/etc/passwd".to_string(),
            "/bin/sh".to_string(),
            "/tmp".to_string(),
        ];

        for attack in &abs_attacks {
            let res = resolve_sandboxed_path(attack);
            assert!(
                res.is_err(),
                "Absolute attack '{}' must be rejected by resolve_sandboxed_path",
                attack
            );
            let err_msg = res.unwrap_err().to_string();
            assert!(
                err_msg.contains("Access denied"),
                "Error for '{}' must contain 'Access denied', got: {}",
                attack,
                err_msg
            );

            let read_res = read_file(attack, None, None);
            assert!(
                read_res.is_err(),
                "read_file('{}') must fail with Access denied",
                attack
            );
            assert!(
                read_res.unwrap_err().to_string().contains("Access denied"),
                "read_file('{}') must return Access denied",
                attack
            );

            let write_res = write_file(attack, "MALICIOUS", Some(true));
            assert!(
                write_res.is_err(),
                "write_file('{}') must fail with Access denied",
                attack
            );
            assert!(
                write_res.unwrap_err().to_string().contains("Access denied"),
                "write_file('{}') must return Access denied",
                attack
            );
        }
    });

    fix.verify_canary_untouched();
}

#[test]
fn test_challenge_windows_quirks_mixed_slashes_and_unc() {
    let fix = SandboxFixture::new("win_quirks");

    with_workspace_root(fix.ws(), || {
        let canary_abs = fix.canary_file.to_str().unwrap();

        // 1. Mixed slashes traversal
        let mixed_attacks = vec![
            "subdir/..\\..//outside_secret/canary.txt",
            "subdir\\../..\\outside_secret/canary.txt",
            "foo/bar\\..\\..//../outside_secret/canary.txt",
            "subdir////..//..////outside_secret//canary.txt",
            "subdir\\\\..\\\\..\\\\outside_secret\\\\canary.txt",
        ];

        for attack in mixed_attacks {
            let res = resolve_sandboxed_path(attack);
            assert!(
                res.is_err(),
                "Mixed slash attack '{}' must be rejected",
                attack
            );
            assert!(
                res.unwrap_err().to_string().contains("Access denied"),
                "Mixed slash attack '{}' must return Access denied",
                attack
            );
        }

        // 2. UNC Extended prefix escapes
        #[cfg(windows)]
        {
            let unc_canary = format!(r"\\?\{}", canary_abs);
            let unc_attacks = vec![
                unc_canary,
                r"\\?\C:\Windows\System32\cmd.exe".to_string(),
                r"\\?\C:\Program Files".to_string(),
                r"\\localhost\c$\Windows\System32".to_string(),
                r"\\127.0.0.1\c$\secret.txt".to_string(),
            ];

            for attack in &unc_attacks {
                let res = resolve_sandboxed_path(attack);
                assert!(res.is_err(), "UNC attack '{}' must be rejected", attack);
                assert!(
                    res.unwrap_err().to_string().contains("Access denied"),
                    "UNC attack '{}' must return Access denied",
                    attack
                );
            }

            // 3. 8.3 Short name escapes
            let short_attacks = vec![
                r"C:\PROGRA~1\anything.txt",
                r"C:\PROGRA~2\anything.txt",
                r"C:\GHOSTT~1\anything.txt",
                r"..\..\PROGRA~1\test.txt",
            ];

            for attack in short_attacks {
                let res = resolve_sandboxed_path(attack);
                assert!(
                    res.is_err(),
                    "8.3 Short name attack '{}' must be rejected",
                    attack
                );
                assert!(
                    res.unwrap_err().to_string().contains("Access denied"),
                    "8.3 Short name attack '{}' must return Access denied",
                    attack
                );
            }
        }
    });

    fix.verify_canary_untouched();
}

#[test]
fn test_challenge_prefix_collision_and_case_insensitivity() {
    let fix = SandboxFixture::new("prefix_collision");

    // Create a colliding sibling folder: e.g. "workspace_private" adjacent to "workspace"
    let colliding_dir = fix.parent_dir.join("workspace_private");
    fs::create_dir_all(&colliding_dir).expect("Failed to create colliding dir");
    let secret_in_collision = colliding_dir.join("confidential.txt");
    fs::write(&secret_in_collision, "CONFIDENTIAL_PRIVATE_DATA").expect("write private");

    with_workspace_root(fix.ws(), || {
        // 1. Prefix collision attack: trying to access workspace_private
        let collision_abs = secret_in_collision.to_str().unwrap();
        let collision_rel = "../workspace_private/confidential.txt";

        let err_abs = resolve_sandboxed_path(collision_abs);
        assert!(
            err_abs.is_err(),
            "Absolute prefix collision '{}' must be rejected",
            collision_abs
        );
        assert!(
            err_abs.unwrap_err().to_string().contains("Access denied"),
            "Prefix collision must return Access denied"
        );

        let err_rel = resolve_sandboxed_path(collision_rel);
        assert!(
            err_rel.is_err(),
            "Relative prefix collision '{}' must be rejected",
            collision_rel
        );
        assert!(
            err_rel.unwrap_err().to_string().contains("Access denied"),
            "Prefix collision must return Access denied"
        );

        // 2. Case variation for legitimate paths inside workspace (Windows case insensitivity)
        // Write a legitimate file inside workspace
        let write_res = write_file("case_test.txt", "CASE_SENSITIVE_CONTENT", Some(true));
        assert!(write_res.is_ok(), "Legitimate write must succeed");

        // Read using exact case
        let read1 = read_file("case_test.txt", None, None);
        assert!(read1.is_ok());

        // Read using uppercase / lowercase
        let read2 = read_file("CASE_TEST.TXT", None, None);
        assert!(read2.is_ok());

        // Read using mixed case with relative prefix
        let read3 = read_file("./SubDir/../Case_Test.txt", None, None);
        assert!(read3.is_ok());

        // 3. Extended UNC path for legitimate inside file
        #[cfg(windows)]
        {
            let ws_abs = fix.ws().canonicalize().unwrap();
            let norm_ws = normalize_canonical_path(&ws_abs);
            let inside_file = norm_ws.join("case_test.txt");
            let unc_inside = format!(r"\\?\{}", inside_file.display());

            let unc_res = resolve_sandboxed_path(&unc_inside);
            assert!(
                unc_res.is_ok(),
                "Legitimate file referenced via UNC prefix \\?\\ must resolve successfully: {:?}",
                unc_res
            );
        }
    });

    let _ = fs::remove_dir_all(colliding_dir);
}

#[test]
fn test_challenge_search_tools_sandboxing() {
    let fix = SandboxFixture::new("search_sandbox");

    with_workspace_root(fix.ws(), || {
        // Write legitimate internal files
        let _ = write_file("module/sub1.rs", "pub fn alpha() { secret(); }", Some(true));
        let _ = write_file("module/sub2.rs", "pub fn beta() {}", Some(true));

        // 1. glob_files with legitimate internal path
        let glob_ok = glob_files("*.rs", Some("module"), None);
        assert!(glob_ok.is_ok());
        let glob_str = glob_ok.unwrap();
        assert!(glob_str.contains("sub1.rs") && glob_str.contains("sub2.rs"));

        // 2. glob_files with traversal base_dir
        let attacks = vec![
            "../",
            "..\\",
            "../../",
            "../outside_secret",
            "module/../../outside_secret",
        ];

        for attack in &attacks {
            let glob_err = glob_files("*.txt", Some(attack), None);
            assert!(
                glob_err.is_err(),
                "glob_files must reject traversal base_dir '{}'",
                attack
            );
            assert!(
                glob_err.unwrap_err().to_string().contains("Access denied"),
                "glob_files must return Access denied for '{}'",
                attack
            );
        }

        // 3. grep_files with legitimate internal path
        let grep_ok = grep_files("alpha", Some("module"), None, None, None, None, None);
        assert!(grep_ok.is_ok());
        assert!(grep_ok.unwrap().contains("pub fn alpha()"));

        // 4. grep_files with traversal base_dir
        for attack in &attacks {
            let grep_err = grep_files("CANARY", Some(attack), None, None, None, None, None);
            assert!(
                grep_err.is_err(),
                "grep_files must reject traversal base_dir '{}'",
                attack
            );
            assert!(
                grep_err.unwrap_err().to_string().contains("Access denied"),
                "grep_files must return Access denied for '{}'",
                attack
            );
        }
    });

    fix.verify_canary_untouched();
}

#[test]
fn test_challenge_nonexistent_outside_directory_creation() {
    let fix = SandboxFixture::new("nonexistent_parent");

    with_workspace_root(fix.ws(), || {
        // Attempting to write a file into a completely non-existent outside directory
        let malicious_paths = vec![
            "../never_existed_dir/evil.txt",
            "../../completely_fake_dir/payload.exe",
            "subdir/../../../deep_fake/leak.txt",
        ];

        for path in malicious_paths {
            let res = write_file(path, "PAYLOAD", Some(true));
            assert!(
                res.is_err(),
                "Writing to non-existent outside directory '{}' must fail",
                path
            );
            assert!(
                res.unwrap_err().to_string().contains("Access denied"),
                "Error for '{}' must be Access denied",
                path
            );
        }

        // Legitimate nested new directory creation INSIDE workspace must succeed
        let legit_nested = "deep/nested/sub/created_file.txt";
        let legit_res = write_file(legit_nested, "SAFE_CONTENT", Some(true));
        assert!(
            legit_res.is_ok(),
            "Writing nested file inside workspace must succeed: {:?}",
            legit_res
        );

        let legit_read = read_file(legit_nested, None, None);
        assert!(legit_read.is_ok());
        assert!(legit_read.unwrap().contains("SAFE_CONTENT"));
    });
}
