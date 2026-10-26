//! Deterministic change-during-read cases: resolve a file, change it, then read with the stale
//! resolution. Fictional data under the crate-local `target/` directory only.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, SystemTime},
};

use super::*;
use crate::identity::{EnvironmentIdentity, OpaqueId, SourceInstance, WslDistroName};
use crate::scope::{AuthorizedRoot, HostPolicy};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("abf134d-unit")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const LINE: &str = "{\"message\":{\"usage\":{\"input_tokens\":1}}}\n";

#[test]
fn repeated_interruptions_stop_with_a_coded_io_failure() {
    struct Interrupted(usize);
    impl Read for Interrupted {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            self.0 += 1;
            assert!(self.0 <= 9, "unbounded interrupted-read retries");
            Err(io::ErrorKind::Interrupted.into())
        }
    }
    let result = scan_stream(
        Interrupted(0),
        1,
        &ClaudeReadLimits::default(),
        LongLimits::MAX,
        1,
    );
    assert_eq!(
        result.unwrap_err(),
        ClaudeFileRejection::Io(io::ErrorKind::Interrupted)
    );
}

#[test]
fn a_transient_interruption_is_retried() {
    struct Once(bool);
    impl Read for Once {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.0 {
                self.0 = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            buffer[0] = b'\n';
            Ok(1)
        }
    }
    // The stream deliberately grows after one byte: the retry must reach growth detection.
    assert_eq!(
        scan_stream(
            Once(false),
            1,
            &ClaudeReadLimits::default(),
            LongLimits::MAX,
            1
        )
        .unwrap_err(),
        ClaudeFileRejection::ChangedDuringRead
    );
}

fn scope(dir: &Dir) -> CollectionScope {
    let id = |v: &str| OpaqueId::new(v).unwrap();
    CollectionScope::new(
        EnvironmentIdentity::wsl(id("env"), WslDistroName::new("Fictional").unwrap()),
        SourceInstance::new(id("src"), Harness::ClaudeCode),
        AuthorizedRoot::new(&dir.0).unwrap(),
        HostPolicy::RequireNative,
    )
    .unwrap()
}

fn read(scope: &CollectionScope, resolved: &ResolvedFile) -> Result<Scanned, ClaudeFileRejection> {
    read_lines(resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16)
        .inspect(|scanned| assert!(scope.limits().max_file_bytes > 0 && !scanned.lines.is_empty()))
}

#[test]
fn unchanged_file_reads() {
    let dir = Dir::new();
    fs::write(dir.0.join("a.jsonl"), LINE).unwrap();
    let scope = scope(&dir);
    let resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    assert_eq!(read(&scope, &resolved).unwrap().counts.observations, 1);
}

#[test]
fn growth_after_resolution_is_changed_during_read() {
    let dir = Dir::new();
    let path = dir.0.join("a.jsonl");
    fs::write(&path, LINE).unwrap();
    let scope = scope(&dir);
    let resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    fs::write(&path, format!("{LINE}{LINE}")).unwrap();
    assert_eq!(
        read_lines(&resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16).unwrap_err(),
        ClaudeFileRejection::ChangedDuringRead
    );
}

#[test]
fn shrinkage_after_resolution_is_changed_during_read() {
    let dir = Dir::new();
    let path = dir.0.join("a.jsonl");
    fs::write(&path, format!("{LINE}{LINE}")).unwrap();
    let scope = scope(&dir);
    let resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    fs::write(&path, LINE).unwrap();
    assert_eq!(
        read_lines(&resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16).unwrap_err(),
        ClaudeFileRejection::ChangedDuringRead
    );
}

#[test]
fn same_length_rewrite_with_new_modification_time_is_changed_during_read() {
    let dir = Dir::new();
    let path = dir.0.join("a.jsonl");
    fs::write(&path, LINE).unwrap();
    let scope = scope(&dir);
    let resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(3600))
        .unwrap();
    assert_eq!(
        read_lines(&resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16).unwrap_err(),
        ClaudeFileRejection::ChangedDuringRead
    );
}

#[test]
fn replaced_file_object_is_changed_during_read() {
    let dir = Dir::new();
    let path = dir.0.join("a.jsonl");
    fs::write(&path, LINE).unwrap();
    let scope = scope(&dir);
    let resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    let other = dir.0.join("b.jsonl");
    fs::write(&other, LINE).unwrap();
    // Same length, but a different file object and modification time at the old path.
    fs::rename(&other, &path).unwrap();
    let result = read_lines(&resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16);
    assert_eq!(result.unwrap_err(), ClaudeFileRejection::ChangedDuringRead);
}

#[test]
fn growth_is_detected_before_reading_beyond_the_recorded_length() {
    let dir = Dir::new();
    let path = dir.0.join("a.jsonl");
    fs::write(&path, LINE).unwrap();
    let scope = scope(&dir);
    let mut resolved = scope
        .resolve_regular_file_with("a.jsonl", CLAUDE_EXTENSION)
        .unwrap();
    // Pretend the file was shorter when resolved: the metadata check at open catches it.
    resolved.len -= 1;
    assert_eq!(
        read_lines(&resolved, &ClaudeReadLimits::default(), LongLimits::MAX, 16).unwrap_err(),
        ClaudeFileRejection::ChangedDuringRead
    );
}
