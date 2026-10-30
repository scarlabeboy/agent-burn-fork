use super::*;
use std::sync::atomic::AtomicUsize;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "abf145-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
        fs::create_dir_all(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    fn reader(&self) -> CopyReader {
        CopyReader::synthetic_test_root(Some(self.0.clone()))
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn native_selection_requires_bounded_ordinary_windows_jsonl_syntax() {
    assert_eq!(admit_selected_text(r"C:\chosen\session.jsonl"), Ok(()));
    assert_eq!(admit_selected_text("c:/chosen/session.jsonl"), Ok(()));
    for path in [
        r"\\server\share\session.jsonl",
        r"\\wsl$\Ubuntu\session.jsonl",
        r"\\wsl.localhost\Ubuntu\session.jsonl",
        r"\\?\C:\chosen\session.jsonl",
        r"\\.\C:\chosen\session.jsonl",
        "C:session.jsonl",
        "/mnt/c/session.jsonl",
        "C:/chosen/../session.jsonl",
        "C:/chosen/./session.jsonl",
        "C:/chosen//session.jsonl",
        "C:/chosen/session.txt",
        "C:/chosen/session.JSONL",
        "C:/chosen/NUL.jsonl",
        "C:/chosen/session.jsonl:stream",
        "C:/chosen/session.jsonl ",
        "C:/chosen./session.jsonl",
        "C:/chosen/?.jsonl",
        "C:/chosen/é.jsonl",
        "C:/session.jsonl",
        "",
    ] {
        assert_eq!(
            admit_selected_text(path),
            Err("COPY_PATH_REFUSED"),
            "{path}"
        );
    }
    assert_eq!(
        admit_selected_text(&format!("C:/chosen/{}.jsonl", "a".repeat(260))),
        Err("COPY_PATH_REFUSED")
    );
}

#[test]
fn selected_drive_mapping_requires_fixed_volume_and_direct_disk_device() {
    assert!(direct_fixed_device(3, r"\Device\HarddiskVolume12"));
    for kind in [0, 1, 2, 4, 5, 6, 99] {
        assert!(!direct_fixed_device(kind, r"\Device\HarddiskVolume12"));
    }
    for device in [
        r"\??\C:\copies",
        r"\Device\Mup\server\share",
        "",
        r"\Device\HarddiskVolume12\alias",
        r"\Device\HarddiskVolume",
    ] {
        assert!(!direct_fixed_device(3, device));
    }
}

#[test]
fn cancelled_dialog_does_not_read_or_consume_and_selected_file_uses_existing_payload() {
    let tree = Tree::new();
    fs::write(
        tree.0.join("selected.jsonl"),
        include_bytes!("../../fixtures/abf143-candidates.jsonl"),
    )
    .unwrap();
    let reader = tree.reader();
    assert_eq!(reader.select_with(|| Ok(None)), Ok(None));
    let payload = reader
        .select_with(|| Ok(Some(tree.0.join("selected.jsonl"))))
        .unwrap()
        .unwrap();
    assert_eq!(
        payload,
        include_str!("../../fixtures/abf143-candidates-payload.json").trim()
    );
    assert!(payload.len() <= 16384);
    assert!(!payload.contains("selected.jsonl"));
    assert!(!payload.contains("PRIVATE"));
    assert_eq!(reader.run("selected.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn legacy_attempt_prevents_dialog_and_dialog_failure_consumes_both_paths() {
    let tree = Tree::new();
    let legacy = tree.reader();
    legacy.run("missing.jsonl").unwrap();
    assert_eq!(
        legacy.select_with(|| panic!("must not open a dialog")),
        Err("COPY_ATTEMPT_CONSUMED")
    );
    let reader = tree.reader();
    assert_eq!(
        reader.select_with(|| Err("COPY_DIALOG_FAILED")),
        Err("COPY_DIALOG_FAILED")
    );
    assert_eq!(reader.run("missing.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn only_native_selected_target_is_used_without_a_configured_root() {
    let tree = Tree::new();
    fs::write(tree.0.join("selected.jsonl"), "{}\n").unwrap();
    let reader = CopyReader::synthetic_test_root(None);
    let result = reader
        .select_with(|| Ok(Some(tree.0.join("selected.jsonl"))))
        .unwrap()
        .unwrap();
    assert!(result.contains("\"read\":1"));
    assert_eq!(
        reader.select_with(|| Ok(Some(tree.0.join("unselected.jsonl")))),
        Err("COPY_ATTEMPT_CONSUMED")
    );
}

#[test]
fn selection_reserves_both_paths_until_cancelled() {
    let tree = Tree::new();
    let reader = tree.reader();
    assert_eq!(
        reader.select_with(|| {
            assert_eq!(reader.run("missing.jsonl"), Err("COPY_BUSY"));
            assert_eq!(
                reader.select_with(|| panic!("duplicate dialog")),
                Err("COPY_BUSY")
            );
            Ok(None)
        }),
        Ok(None)
    );
    assert!(reader.run("missing.jsonl").is_ok());
}

#[test]
fn missing_selected_file_and_invalid_selection_consume_attempt() {
    let tree = Tree::new();
    let reader = tree.reader();
    assert_eq!(
        reader.select_with(|| Ok(Some(tree.0.join("missing.jsonl")))),
        Err("COPY_PATH_REFUSED")
    );
    assert_eq!(reader.run("missing.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
    assert_eq!(
        tree.reader()
            .select_with(|| Ok(Some(tree.0.join("wrong.txt")))),
        Err("COPY_PATH_REFUSED")
    );
}

#[cfg(unix)]
#[test]
fn selected_file_and_ancestor_redirections_are_refused() {
    let tree = Tree::new();
    fs::create_dir(tree.0.join("real")).unwrap();
    fs::write(tree.0.join("real/session.jsonl"), "{}\n").unwrap();
    std::os::unix::fs::symlink(tree.0.join("real"), tree.0.join("alias")).unwrap();
    std::os::unix::fs::symlink(tree.0.join("real/session.jsonl"), tree.0.join("link.jsonl"))
        .unwrap();
    assert_eq!(
        tree.reader()
            .select_with(|| Ok(Some(tree.0.join("alias/session.jsonl")))),
        Err("COPY_ROOT_REFUSED")
    );
    assert_eq!(
        tree.reader()
            .select_with(|| Ok(Some(tree.0.join("link.jsonl")))),
        Err("COPY_PATH_REFUSED")
    );
}

#[test]
fn picker_reads_only_selected_file_even_when_another_root_is_configured() {
    let selected = Tree::new();
    let configured = Tree::new();
    fs::write(
        selected.0.join("session.jsonl"),
        include_bytes!("../../fixtures/abf141-replay.jsonl"),
    )
    .unwrap();
    fs::write(
        configured.0.join("session.jsonl"),
        "PRIVATE INVALID CONTENT\n",
    )
    .unwrap();
    let reader = configured.reader();
    assert_eq!(
        reader
            .select_with(|| Ok(Some(selected.0.join("session.jsonl"))))
            .unwrap()
            .unwrap(),
        include_str!("../../fixtures/abf141-replay-payload.json").trim()
    );
}

#[test]
fn selected_file_preserves_file_line_and_record_caps_without_partial_total() {
    let tree = Tree::new();
    fs::write(tree.0.join("large.jsonl"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    let oversized = tree
        .reader()
        .select_with(|| Ok(Some(tree.0.join("large.jsonl"))))
        .unwrap()
        .unwrap();
    assert!(oversized.contains("\"tooLarge\":1"));
    assert!(!oversized.contains("\"amount\":"));
    fs::write(tree.0.join("records.jsonl"), "{}\n".repeat(1025)).unwrap();
    let records = tree
        .reader()
        .select_with(|| Ok(Some(tree.0.join("records.jsonl"))))
        .unwrap()
        .unwrap();
    assert!(records.contains("\"outputLimit\":1"));
    assert!(!records.contains("\"amount\":"));
    let assistant = include_str!("../../fixtures/abf141-replay.jsonl");
    fs::write(
        tree.0.join("line.jsonl"),
        format!("{}\n{assistant}", "x".repeat(64 * 1024 + 1)),
    )
    .unwrap();
    let line = tree
        .reader()
        .select_with(|| Ok(Some(tree.0.join("line.jsonl"))))
        .unwrap()
        .unwrap();
    assert!(line.contains("\"oversized\":1"));
    assert!(line.contains("\"amount\":\"20\""));
    assert!(line.len() <= 16384);
}

#[test]
fn concurrent_picker_and_legacy_calls_allow_exactly_one_read() {
    let tree = Tree::new();
    fs::write(tree.0.join("session.jsonl"), "{}\n").unwrap();
    let reader = std::sync::Arc::new(tree.reader());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|index| {
            let reader = reader.clone();
            let barrier = barrier.clone();
            let path = tree.0.join("session.jsonl");
            std::thread::spawn(move || {
                barrier.wait();
                if index % 2 == 0 {
                    reader.run("session.jsonl").is_ok()
                } else {
                    reader.select_with(|| Ok(Some(path))).is_ok()
                }
            })
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
fn directory_disguised_as_selected_jsonl_is_refused() {
    let tree = Tree::new();
    fs::create_dir(tree.0.join("directory.jsonl")).unwrap();
    assert_eq!(
        tree.reader()
            .select_with(|| Ok(Some(tree.0.join("directory.jsonl")))),
        Err("COPY_PATH_REFUSED")
    );
}
