use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, SystemTime},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/abf129-unit");
        fs::create_dir_all(&parent).unwrap();
        let path = parent.join(NEXT.fetch_add(1, Ordering::SeqCst).to_string());
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn resolved(&self, text: &str) -> ResolvedFile {
        let path = self.0.join("fiction.abf-fixture");
        fs::write(&path, text).unwrap();
        let metadata = fs::symlink_metadata(&path).unwrap();
        ResolvedFile {
            path,
            relative: "fiction.abf-fixture".into(),
            object: file_object(&metadata),
            len: metadata.len(),
            modified: metadata.modified().unwrap(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn growth_between_resolution_and_open_is_rejected() {
    let fixture = Fixture::new();
    let resolved = fixture.resolved(&format!("{FIXTURE_HEADER}\ninput_tokens=1\n"));
    fs::write(
        &resolved.path,
        format!("{FIXTURE_HEADER}\ninput_tokens=123\n"),
    )
    .unwrap();
    assert_eq!(
        read_bounded(&resolved, 1024),
        Err(FileRejection::ChangedDuringRead)
    );
}

#[test]
fn small_limit_does_not_allow_a_header_sized_read() {
    let fixture = Fixture::new();
    let resolved = fixture.resolved("a");
    assert_eq!(read_bounded(&resolved, 1), Err(FileRejection::TooLarge));
}

#[test]
fn vanished_after_resolution_is_a_coded_failure() {
    let fixture = Fixture::new();
    let resolved = fixture.resolved(&format!("{FIXTURE_HEADER}\n"));
    fs::remove_file(&resolved.path).unwrap();
    assert_eq!(
        read_bounded(&resolved, 1024),
        Err(FileRejection::Io(std::io::ErrorKind::NotFound))
    );
}

#[test]
fn same_size_rewrite_with_changed_mtime_is_rejected() {
    let fixture = Fixture::new();
    let resolved = fixture.resolved(&format!("{FIXTURE_HEADER}\ninput_tokens=1\n"));
    fs::write(
        &resolved.path,
        format!("{FIXTURE_HEADER}\ninput_tokens=2\n"),
    )
    .unwrap();
    File::options()
        .write(true)
        .open(&resolved.path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        read_bounded(&resolved, 1024),
        Err(FileRejection::ChangedDuringRead)
    );
}
