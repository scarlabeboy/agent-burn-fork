#![deny(unsafe_code)]
#[cfg(test)]
#[path = "../src-tauri/src/claude_copy_read.rs"]
mod claude_copy_read;
#[cfg(test)]
#[path = "../src-tauri/src/fictional_file_payload.rs"]
mod fictional_file_payload;
#[cfg(test)]
#[path = "../src-tauri/src/fictional_file_read.rs"]
mod fictional_file_read;

#[cfg(test)]
mod tests {
    use super::claude_copy_read::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Tree(PathBuf);
    impl Tree {
        fn new() -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!(
                    "abf137-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
            fs::create_dir_all(&root).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn selected_copy_has_truthful_sanitized_provenance_and_one_attempt() {
        let tree = Tree::new();
        fs::create_dir(tree.0.join("selected")).unwrap();
        fs::write(
            tree.0.join("selected/copy.jsonl"),
            include_bytes!("../fixtures/abf136-fictional.jsonl"),
        )
        .unwrap();
        let reader = CopyReader::synthetic_test_root(Some(tree.0.clone()));
        let result = reader.run("selected/copy.jsonl").unwrap();
        assert_eq!(
            result,
            include_str!("../fixtures/abf137-expected-payload.json").trim()
        );
        assert!(result.contains("\"provenance\":\"configured-windows-copy\""));
        assert!(result.contains("\"originalSource\":\"not-established\""));
        assert!(result.contains("\"amount\":\"17\""));
        for private in [
            "fictional-fixture",
            "environmentKind",
            "hostMatch",
            "ABF136-CONTENT",
            "abf136",
            "copy.jsonl",
            "selected",
            "abf137-",
        ] {
            assert!(!result.contains(private), "{private}");
        }
        assert_eq!(
            reader.run("selected/copy.jsonl"),
            Err("COPY_ATTEMPT_CONSUMED")
        );
    }
    #[test]
    fn realistic_copy_over_old_byte_line_and_record_caps_recognizes_usage() {
        let tree = Tree::new();
        let events = include_str!("../fixtures/abf139-session-events.jsonl");
        let usage = include_str!("../fixtures/abf139-assistant-usage.jsonl");
        // 130 events plus 4 claims; a 16 KiB user line exceeds the old 8 KiB cap.
        let content = format!(
            "{}{{\"type\":\"user\",\"message\":{{\"content\":\"{}\"}}}}\n{}",
            events.repeat(10),
            "x".repeat(16 * 1024),
            usage
        );
        let content = format!("{}{}", " ".repeat(70 * 1024) + "\n", content);
        fs::write(tree.0.join("copy.jsonl"), content).unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("copy.jsonl")
            .unwrap();
        assert!(result.contains("\"read\":1"));
        assert!(result.contains("\"observations\":135"));
        assert!(result.contains("\"withValidClaim\":4"));
        assert!(result.contains("\"amount\":\"130\""));
        assert!(result.contains("\"oversized\":1"));
        assert!(!result.contains("ABF139-FICTIONAL"));
    }

    #[test]
    fn copy_record_budget_discards_file_without_publishing_partial_claims() {
        let tree = Tree::new();
        let usage = include_str!("../fixtures/abf139-assistant-usage.jsonl")
            .lines()
            .next()
            .unwrap();
        fs::write(
            tree.0.join("copy.jsonl"),
            format!("{usage}\n{}", "{}\n".repeat(1024)),
        )
        .unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("copy.jsonl")
            .unwrap();
        assert!(result.contains("\"outputLimit\":1"));
        assert!(result.contains("\"observations\":0"));
        assert!(result.contains("no-observations"));
        assert!(!result.contains("\"amount\":"));
    }

    #[test]
    fn realistic_no_usage_and_mixed_payloads_match_frontend_fixtures() {
        let tree = Tree::new();
        let events = include_str!("../fixtures/abf139-session-events.jsonl");
        let usage = include_str!("../fixtures/abf139-assistant-usage.jsonl");
        for (content, expected) in [
            (
                events.to_owned(),
                include_str!("../fixtures/abf139-no-usage-payload.json"),
            ),
            (
                format!("{events}{usage}"),
                include_str!("../fixtures/abf139-mixed-payload.json"),
            ),
        ] {
            fs::write(tree.0.join("copy.jsonl"), content).unwrap();
            let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
                .run("copy.jsonl")
                .unwrap();
            assert_eq!(result, expected.trim());
        }
    }

    #[test]
    fn exact_file_budget_is_read_but_not_an_unbounded_buffer() {
        let tree = Tree::new();
        let content = format!("{}\n", " ".repeat(1024 * 1024 - 1));
        fs::write(tree.0.join("copy.jsonl"), content).unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("copy.jsonl")
            .unwrap();
        assert!(result.contains("\"read\":1"));
        assert!(result.contains("\"oversized\":1"));
        assert!(result.contains("no-observations"));
    }

    #[test]
    fn line_at_budget_retains_claims_and_next_byte_makes_it_oversized() {
        let tree = Tree::new();
        let usage = include_str!("../fixtures/abf139-assistant-usage.jsonl")
            .lines()
            .next()
            .unwrap();
        let line = format!("{}{}", usage, " ".repeat(64 * 1024 - usage.len()));
        fs::write(tree.0.join("copy.jsonl"), format!("{line}\n{line} \n")).unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("copy.jsonl")
            .unwrap();
        assert!(result.contains("\"observations\":1"));
        assert!(result.contains("\"amount\":\"120\""));
        assert!(result.contains("\"oversized\":1"));
    }

    #[test]
    fn exact_record_budget_retains_final_assistant_claim() {
        let tree = Tree::new();
        let usage = include_str!("../fixtures/abf139-assistant-usage.jsonl")
            .lines()
            .next()
            .unwrap();
        fs::write(
            tree.0.join("copy.jsonl"),
            format!("{}{usage}\n", "{}\n".repeat(1023)),
        )
        .unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("copy.jsonl")
            .unwrap();
        assert!(result.contains("\"observations\":1024"));
        assert!(result.contains("\"withValidClaim\":1"));
        assert!(result.contains("\"amount\":\"120\""));
    }

    #[test]
    fn disabled_and_invalid_root_are_coded_and_one_shot() {
        let reader = CopyReader::synthetic_test_root(None);
        assert_eq!(reader.run("copy.jsonl"), Err("COPY_NOT_CONFIGURED"));
        assert_eq!(reader.run("copy.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
        let tree = Tree::new();
        assert_eq!(
            CopyReader::synthetic_test_root(Some(tree.0.join("missing"))).run("copy.jsonl"),
            Err("COPY_ROOT_REFUSED")
        );
    }
    #[test]
    fn rejects_windows_unsupported_relative_syntax_before_reading() {
        for path in [
            "",
            "../copy.jsonl",
            "a/../copy.jsonl",
            "/copy.jsonl",
            "C:/copy.jsonl",
            "C:copy.jsonl",
            "copy.jsonl:secret",
            "a\\copy.jsonl",
            "//server/copy.jsonl",
            "a//copy.jsonl",
            "./copy.jsonl",
            "NUL.jsonl",
            "COM1/copy.jsonl",
            "a./copy.jsonl",
            "a /copy.jsonl",
            "*.jsonl",
            "copy.txt",
            "a/b/c/d/e.jsonl",
        ] {
            assert_eq!(admit_relative(path), Err("COPY_PATH_REFUSED"), "{path}");
        }
        assert_eq!(admit_relative("chosen/copy.jsonl"), Ok(()));
    }
    #[test]
    fn missing_and_oversized_files_retain_unavailable_coverage() {
        let tree = Tree::new();
        let missing = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("missing.jsonl")
            .unwrap();
        assert!(missing.contains("\"path\":1"));
        assert!(missing.contains("no-observations"));
        assert!(!missing.contains("\"amount\":\"0\""));
        fs::write(tree.0.join("large.jsonl"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
        let large = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("large.jsonl")
            .unwrap();
        assert!(large.contains("\"tooLarge\":1"));
        assert!(large.contains("\"completeness\":\"incomplete\""));
    }
    #[cfg(unix)]
    #[test]
    fn refuses_redirected_root_ancestor_and_selected_file() {
        let tree = Tree::new();
        fs::create_dir(tree.0.join("real")).unwrap();
        std::os::unix::fs::symlink(tree.0.join("real"), tree.0.join("alias")).unwrap();
        assert_eq!(
            CopyReader::synthetic_test_root(Some(tree.0.join("alias"))).run("copy.jsonl"),
            Err("COPY_ROOT_REFUSED")
        );
        fs::write(tree.0.join("real/copy.jsonl"), "{}\n").unwrap();
        let result = CopyReader::synthetic_test_root(Some(tree.0.clone()))
            .run("alias/copy.jsonl")
            .unwrap();
        assert!(result.contains("\"path\":1"));
    }
    #[test]
    fn volume_support_is_conservative() {
        for kind in [0, 1, 2, 4, 5, 6, 99] {
            assert!(!fixed_local_volume(kind));
        }
        assert!(fixed_local_volume(3));
    }
    #[test]
    fn concurrent_copy_requests_have_one_winner() {
        let tree = Tree::new();
        fs::write(tree.0.join("copy.jsonl"), "{}\n").unwrap();
        let reader = std::sync::Arc::new(CopyReader::synthetic_test_root(Some(tree.0.clone())));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let reader = reader.clone();
                std::thread::spawn(move || reader.run("copy.jsonl").is_ok())
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn path_refusal_consumes_the_native_attempt() {
        let tree = Tree::new();
        let reader = CopyReader::synthetic_test_root(Some(tree.0.clone()));
        assert_eq!(reader.run("../copy.jsonl"), Err("COPY_PATH_REFUSED"));
        assert_eq!(reader.run("copy.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
    }

    #[test]
    fn explicit_network_device_and_unsupported_roots_are_refused_lexically() {
        for root in [
            r"\\server\share\copies",
            r"\\wsl$\Ubuntu\copies",
            r"\\?\C:\copies",
            "C:/",
            "C:copies",
            "C:/a/../copies",
            "C:/NUL",
            "C:/copies.",
            "C:/copies:stream",
        ] {
            assert_eq!(
                CopyReader::new(Some(PathBuf::from(root))).run("copy.jsonl"),
                Err("COPY_ROOT_REFUSED")
            );
        }
    }
}
