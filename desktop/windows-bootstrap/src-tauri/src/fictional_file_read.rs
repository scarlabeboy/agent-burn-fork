//! ABF136: fixed fictional one-shot read only. No production collector authority.
use abf_collection_boundary::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::fictional_file_payload as payload;

const DIRECTORY: &str = "abf136-fictional-claude-once";
const FILE: &str = "fictional.jsonl";
const FIXTURE: &[u8] = include_bytes!("../../fixtures/abf136-fictional.jsonl");
const UNAVAILABLE: &str = "Fictional file diagnostic unavailable; no result. Use a fresh isolated runtime root for another attempt.";

pub struct Diagnostic {
    root: PathBuf,
    attempted: AtomicBool,
}
impl Diagnostic {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            attempted: AtomicBool::new(false),
        }
    }

    pub fn run(&self) -> Result<String, &'static str> {
        // Failure also consumes the attempt. UI clearing must never rearm this guard.
        if self.attempted.swap(true, Ordering::SeqCst) {
            return Err(UNAVAILABLE);
        }
        self.read_fixture().map_err(|_| UNAVAILABLE)
    }

    fn read_fixture(&self) -> Result<String, ()> {
        validate_root(&self.root)?;
        let child = self.root.join(DIRECTORY);
        // create_dir is exclusive, including existing directories and dangling links.
        fs::create_dir(&child).map_err(|_| ())?;
        validate_root(&child)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(child.join(FILE))
            .map_err(|_| ())?;
        file.write_all(FIXTURE).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        validate_root(&self.root)?;
        validate_root(&child)?;
        summarize_file(&child, read_limits())
    }
}

// Private, fixed filename only. Caller paths/limits never cross the IPC boundary.
fn summarize_file(child: &Path, limits: ClaudeReadLimits) -> Result<String, ()> {
    let scope = scope(child)?;
    let run = ScanRun::new(id("abf136-run"), CaptureTime::NotRecorded);
    let batch = collect_claude_jsonl_file(&scope, &run, FILE, limits).map_err(|_| ())?;
    if batch.host_match != HostMatch::Simulated {
        return Err(());
    }
    Ok(payload::fictional_payload_json(&batch.summary()))
}

fn read_limits() -> ClaudeReadLimits {
    ClaudeReadLimits {
        max_files: 1,
        max_total_bytes: 8192,
        max_line_bytes: 2048,
        max_records: 16,
    }
}

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).expect("fixed fictional id")
}

fn scope(child: &Path) -> Result<CollectionScope, ()> {
    // Windows runtime simulates WSL; the Linux-only helper harness simulates Windows.
    #[cfg(windows)]
    let environment = EnvironmentIdentity::wsl(
        id("abf136-env"),
        WslDistroName::new("Fictional-Demo").unwrap(),
    );
    #[cfg(not(windows))]
    let environment = EnvironmentIdentity::windows(id("abf136-env"));
    CollectionScope::new(
        environment,
        SourceInstance::new(id("abf136-source"), Harness::ClaudeCode),
        AuthorizedRoot::new(child).map_err(|_| ())?,
        HostPolicy::AllowSimulated,
    )
    .map_err(|_| ())?
    .with_limits(CollectionLimits {
        max_file_bytes: 8192,
        max_entries: 1,
        max_depth: 1,
    })
    .map_err(|_| ())
}

fn validate_root(path: &Path) -> Result<(), ()> {
    // Reject network/device syntax before filesystem contact. No canonicalize-then-trust.
    check_root_text(path.to_str().ok_or(())?, HostFamily::current()).map_err(|_| ())?;
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| ())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(());
            }
        }
    }
    AuthorizedRoot::new(path).map_err(|_| ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Tree(std::path::PathBuf);
    impl Tree {
        fn new() -> Self {
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!(
                    "abf136-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
            std::fs::create_dir_all(&root).unwrap();
            Self(std::fs::canonicalize(root).unwrap())
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn reads_a_new_fixed_file_once_and_preserves_it() {
        let tree = Tree::new();
        let diagnostic = Diagnostic::new(tree.0.clone());
        let result = diagnostic.run().unwrap();
        let expected = include_str!("../../fixtures/abf136-expected-payload.json").trim();
        // Linux harness simulates Windows; the Windows app simulates Selected WSL.
        #[cfg(not(windows))]
        let expected = expected.replace(
            "\"environmentKind\":\"wsl\"",
            "\"environmentKind\":\"windows\"",
        );
        assert_eq!(result, expected);
        assert!(result.contains("\"hostMatch\":\"simulated\""));
        assert!(result.contains("\"provenance\":\"fictional-fixture\""));
        assert!(result.contains("\"amount\":\"17\""));
        assert_eq!(
            std::fs::read(tree.0.join(DIRECTORY).join(FILE)).unwrap(),
            FIXTURE
        );
        assert!(diagnostic.run().is_err());
        assert!(!result.contains("jsonl"));
        assert!(!result.contains("ABF136-CONTENT"));
    }
    #[test]
    fn existing_directory_is_never_reused_and_failure_is_one_shot() {
        let tree = Tree::new();
        std::fs::create_dir(tree.0.join(DIRECTORY)).unwrap();
        std::fs::write(tree.0.join(DIRECTORY).join(FILE), "sentinel").unwrap();
        let diagnostic = Diagnostic::new(tree.0.clone());
        assert!(diagnostic.run().is_err());
        assert!(diagnostic.run().is_err());
        assert_eq!(
            std::fs::read_to_string(tree.0.join(DIRECTORY).join(FILE)).unwrap(),
            "sentinel"
        );
    }
    #[test]
    fn missing_root_is_not_created() {
        let tree = Tree::new();
        let root = tree.0.join("missing");
        assert!(Diagnostic::new(root.clone()).run().is_err());
        assert!(!root.exists());
    }
    #[cfg(unix)]
    #[test]
    fn redirected_root_and_existing_child_are_refused() {
        let tree = Tree::new();
        let alias = tree.0.join("alias");
        std::os::unix::fs::symlink(&tree.0, &alias).unwrap();
        assert!(Diagnostic::new(alias).run().is_err());
        std::os::unix::fs::symlink(&tree.0, tree.0.join(DIRECTORY)).unwrap();
        assert!(Diagnostic::new(tree.0.clone()).run().is_err());
        assert!(!tree.0.join(FILE).exists());
    }
    #[test]
    fn rejected_file_keeps_coverage_instead_of_zero_usage() {
        let tree = Tree::new();
        std::fs::write(tree.0.join(FILE), vec![b'x'; 8193]).unwrap();
        let result = summarize_file(&tree.0, read_limits()).unwrap();
        assert!(result.contains("\"completeness\":\"incomplete\""));
        assert!(result.contains("\"tooLarge\":1"));
        assert!(result.contains("\"reason\":\"no-observations\""));
        assert!(!result.contains("\"amount\":\"0\""));
    }
    #[test]
    fn missing_file_reports_rejected_coverage() {
        let tree = Tree::new();
        let result = summarize_file(&tree.0, read_limits()).unwrap();
        assert!(result.contains("\"path\":1"));
        assert!(result.contains("\"rejectedFiles\":true"));
    }

    #[test]
    fn output_limit_is_explicit() {
        let tree = Tree::new();
        std::fs::write(tree.0.join(FILE), FIXTURE).unwrap();
        let result = summarize_file(
            &tree.0,
            ClaudeReadLimits {
                max_records: 1,
                ..read_limits()
            },
        )
        .unwrap();
        assert!(result.contains("\"outputLimit\":1"));
        assert!(result.contains("\"rejectedFiles\":true"));
    }
    #[test]
    fn concurrent_requests_have_only_one_winner() {
        let tree = Tree::new();
        let diagnostic = std::sync::Arc::new(Diagnostic::new(tree.0.clone()));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let diagnostic = diagnostic.clone();
                std::thread::spawn(move || diagnostic.run().is_ok())
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
        // Every request must finish; exactly one may materialize and read.
        assert!(diagnostic.run().is_err());
    }
}
