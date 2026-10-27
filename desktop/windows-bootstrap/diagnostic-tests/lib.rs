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
        fs::write(tree.0.join("large.jsonl"), vec![b'x'; 65537]).unwrap();
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
