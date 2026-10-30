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

#[test]
fn folder_selection_is_pending_until_analysis_and_shares_attempt() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    fs::write(tree.0.join("b.jsonl"), "{}\n").unwrap();
    fs::create_dir(tree.0.join("ignored")).unwrap();
    fs::write(tree.0.join("ignored/hidden.jsonl"), "PRIVATE").unwrap();
    let reader = tree.reader();
    assert_eq!(reader.select_folder_with(|| Ok(None)), Ok(None));
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(2))
    );
    assert_eq!(reader.run("a.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
    let payload = reader.analyze_folder().unwrap();
    assert!(payload.contains("\"read\":2"));
    assert!(payload.contains("selected-windows-folder"));
    assert!(!payload.contains("a.jsonl"));
    assert!(!payload.contains("PRIVATE"));
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn folder_worst_case_4096_conflicts_are_bounded_and_private() {
    let tree = Tree::new();
    let base = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    let base = base
        .replace("PRIVATE-UUID", &"u".repeat(256))
        .replace("PRIVATE-SESSION", &"s".repeat(256))
        .replace("PRIVATE-REQUEST", &"r".repeat(256))
        .replace("PRIVATE-MESSAGE", &"m".repeat(256))
        .replace("fictional-model", &"f".repeat(256));
    for file in 0..16 {
        let records: String = (0..256)
            .map(|line| {
                let stamp = format!("{:0256}", file * 256 + line);
                format!("{}\n", base.replace("fictional-time", &stamp))
            })
            .collect();
        assert!(records.len() <= 1024 * 1024);
        fs::write(tree.0.join(format!("{file:02}.jsonl")), records).unwrap();
    }
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(16))
    );
    let start = std::time::Instant::now();
    let result = reader.analyze_folder().unwrap();
    eprintln!(
        "ABF148B 4096 maximum-identity metadata conflicts: {:?}",
        start.elapsed()
    );
    assert!(result.contains("\"conflictingEvidence\":4096"));
    assert!(result.len() <= 16384);
    assert!(!result.contains("PRIVATE"));
}

#[test]
fn folder_refusals_clear_and_changed_file_never_rearm_or_publish_partial_results() {
    let empty = Tree::new();
    assert_eq!(
        empty
            .reader()
            .select_folder_with(|| Ok(Some(empty.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    reader.clear_folder();
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
    assert_eq!(
        reader.select_with(|| panic!("must not select")),
        Err("COPY_ATTEMPT_CONSUMED")
    );
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    fs::write(tree.0.join("a.jsonl"), "changed").unwrap();
    assert_eq!(reader.analyze_folder(), Err("COPY_READ_FAILED"));
    assert_eq!(reader.run("a.jsonl"), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn folder_limits_duplicates_and_rejected_files_fail_closed() {
    let tree = Tree::new();
    for index in 0..17 {
        fs::write(tree.0.join(format!("{index}.jsonl")), "{}\n").unwrap();
    }
    assert_eq!(
        tree.reader()
            .select_folder_with(|| Ok(Some(tree.0.clone()))),
        Err("COPY_FOLDER_LIMIT")
    );
    let large = Tree::new();
    fs::write(large.0.join("a.jsonl"), vec![b' '; 3 * 1024 * 1024 + 1]).unwrap();
    assert_eq!(
        large
            .reader()
            .select_folder_with(|| Ok(Some(large.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
    let aggregate = Tree::new();
    for index in 0..9 {
        fs::write(
            aggregate.0.join(format!("{index}.jsonl")),
            vec![b' '; 3 * 1024 * 1024],
        )
        .unwrap();
    }
    assert_eq!(
        aggregate
            .reader()
            .select_folder_with(|| Ok(Some(aggregate.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
    let rejected = Tree::new();
    fs::create_dir(rejected.0.join("directory.jsonl")).unwrap();
    assert_eq!(
        rejected
            .reader()
            .select_folder_with(|| Ok(Some(rejected.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
}

#[cfg(unix)]
#[test]
fn folder_redirects_and_case_ambiguities_are_refused() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    fs::write(tree.0.join("A.jsonl"), "{}\n").unwrap();
    assert_eq!(
        tree.reader()
            .select_folder_with(|| Ok(Some(tree.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
    let linked = Tree::new();
    std::os::unix::fs::symlink(tree.0.join("a.jsonl"), linked.0.join("link.jsonl")).unwrap();
    assert_eq!(
        linked
            .reader()
            .select_folder_with(|| Ok(Some(linked.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
    std::os::unix::fs::symlink(&tree.0, linked.0.join("alias")).unwrap();
    assert_eq!(
        linked
            .reader()
            .select_folder_with(|| Ok(Some(linked.0.join("alias")))),
        Err("COPY_ROOT_REFUSED")
    );
}

#[test]
fn folder_selection_blocks_overlapping_actions_and_clear_drops_late_pending_selection() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| {
            assert_eq!(reader.analyze_folder(), Err("COPY_BUSY"));
            assert_eq!(reader.run("a.jsonl"), Err("COPY_BUSY"));
            reader.clear_folder();
            Ok(Some(tree.0.clone()))
        })
        .unwrap();
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn analysis_without_pending_selection_consumes_attempt() {
    let reader = CopyReader::synthetic_test_root(None);
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
    assert_eq!(
        reader.select_with(|| panic!("failed analysis must consume")),
        Err("COPY_ATTEMPT_CONSUMED")
    );
}

#[test]
fn folder_cross_file_replay_matches_aggregate_frontend_golden() {
    let tree = Tree::new();
    let record = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    fs::write(tree.0.join("b.jsonl"), format!("{record}\n")).unwrap();
    fs::write(tree.0.join("a.jsonl"), format!("{record}\n")).unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert_eq!(
        reader.analyze_folder().unwrap(),
        include_str!("../../fixtures/abf148b-folder-payload.json").trim()
    );
}

#[test]
fn folder_enumeration_budget_is_inclusive_and_unrelated_contents_are_ignored() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    for index in 0..1023 {
        fs::write(tree.0.join(format!("unrelated-{index}.txt")), "PRIVATE").unwrap();
    }
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(1))
    );
    assert!(!reader.analyze_folder().unwrap().contains("PRIVATE"));
    fs::write(tree.0.join("one-more.txt"), "PRIVATE").unwrap();
    assert_eq!(
        tree.reader()
            .select_folder_with(|| Ok(Some(tree.0.clone()))),
        Err("COPY_FOLDER_LIMIT")
    );
}

#[test]
fn folder_exact_aggregate_byte_limit_retains_explicit_pending_tail_caveats() {
    let tree = Tree::new();
    for index in 0..8 {
        fs::File::create(tree.0.join(format!("{index}.jsonl")))
            .unwrap()
            .set_len(3 * 1024 * 1024)
            .unwrap();
    }
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(8))
    );
    let result = reader.analyze_folder().unwrap();
    assert!(result.contains("\"read\":8"));
    assert!(result.contains("\"pendingTailBytes\":25165824"));
    assert!(result.contains("\"pendingTails\":true"));
    // One extra byte in a ninth file crosses the total budget while every file
    // remains individually admissible. Admission occurs before content reads.
    fs::File::create(tree.0.join("extra.jsonl"))
        .unwrap()
        .set_len(1)
        .unwrap();
    assert_eq!(
        tree.reader()
            .select_folder_with(|| Ok(Some(tree.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
}

#[test]
fn clear_before_queued_folder_choose_cannot_leave_pending_authority() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    let reader = tree.reader();
    reader.clear_folder();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert!(reader.pending.lock().unwrap().is_none());
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn ignored_folder_entries_are_counted_as_uncovered_without_reading_them() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    fs::create_dir(tree.0.join("ignored")).unwrap();
    fs::write(tree.0.join("ignored/hidden.jsonl"), "PRIVATE").unwrap();
    fs::write(tree.0.join("ignored.JSONL"), "PRIVATE").unwrap();
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(1))
    );
    let result = reader.analyze_folder().unwrap();
    assert!(result.contains("\"read\":1"));
    assert!(result.contains("\"skipped\":{\"total\":2,\"mayHideData\":2,\"outOfScope\":0}"));
    assert!(result.contains("\"completeness\":\"incomplete\""));
    assert!(result.contains("\"skippedInScope\":true"));
    assert!(!result.contains("PRIVATE"));
    assert!(!result.contains("ignored"));
}

#[test]
fn folder_three_mib_file_boundary_is_inclusive_and_legacy_limit_is_unchanged() {
    let tree = Tree::new();
    let file = fs::File::create(tree.0.join("a.jsonl")).unwrap();
    file.set_len(3 * 1024 * 1024).unwrap();
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(1))
    );
    assert!(reader.analyze_folder().unwrap().contains("\"read\":1"));
    assert!(
        tree.reader()
            .run("a.jsonl")
            .unwrap()
            .contains("\"tooLarge\":1")
    );
    file.set_len(3 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        tree.reader()
            .select_folder_with(|| Ok(Some(tree.0.clone()))),
        Err("COPY_FOLDER_REFUSED")
    );
}

#[test]
fn folder_dialog_policy_disables_new_folder_button() {
    assert_ne!(FOLDER_DIALOG_FLAGS & 0x200, 0); // BIF_NONEWFOLDERBUTTON
    assert_ne!(FOLDER_DIALOG_FLAGS & 0x40, 0); // BIF_NEWDIALOGSTYLE
    assert_ne!(FOLDER_DIALOG_FLAGS & 0x1, 0); // BIF_RETURNONLYFSDIRS
}

#[test]
fn genuine_folder_cancellation_after_clear_releases_only_the_unselected_attempt() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    let reader = tree.reader();
    reader.clear_folder();
    assert_eq!(reader.select_folder_with(|| Ok(None)), Ok(None));
    assert!(!reader.attempted.load(Ordering::SeqCst));
    assert!(!reader.discarded.load(Ordering::SeqCst));
    reader
        .select_folder_with(|| {
            reader.clear_folder();
            Ok(None)
        })
        .unwrap();
    assert!(!reader.attempted.load(Ordering::SeqCst));
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert!(reader.analyze_folder().is_ok());
    reader.clear_folder();
    assert_eq!(
        reader.select_folder_with(|| panic!("clear never rearms selected attempts")),
        Err("COPY_ATTEMPT_CONSUMED")
    );
}

#[test]
fn folder_analysis_rejects_discard_signal_before_pending_cleanup() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n").unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    // Model Clear's flag publication before its pending-mutex cleanup gets scheduled.
    reader.discarded.store(true, Ordering::SeqCst);
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
    reader.clear_folder();
    assert!(reader.pending.lock().unwrap().is_none());
}

#[test]
fn cleared_queued_folder_failure_remains_consumed() {
    let reader = CopyReader::synthetic_test_root(None);
    reader.clear_folder();
    assert_eq!(
        reader.select_folder_with(|| Err("COPY_DIALOG_FAILED")),
        Err("COPY_DIALOG_FAILED")
    );
    assert!(reader.attempted.load(Ordering::SeqCst));
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
    assert_eq!(
        reader.select_folder_with(|| panic!("failure remains consumed")),
        Err("COPY_ATTEMPT_CONSUMED")
    );
}

#[test]
fn folder_line_limit_accepts_256_kib_with_lf_or_crlf_across_io_chunks() {
    let record = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    for (ending, cr_bytes) in [("\n", 0), ("\r\n", 1)] {
        let tree = Tree::new();
        // CR counts toward the limit; LF does not. LF starts the fifth I/O chunk.
        let line = format!(
            "{record}{}{ending}",
            " ".repeat(262144 - cr_bytes - record.len())
        );
        assert_eq!(line.len(), 262145);
        fs::write(tree.0.join("a.jsonl"), line).unwrap();
        let reader = tree.reader();
        assert_eq!(
            reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
            Ok(Some(1))
        );
        let result = reader.analyze_folder().unwrap();
        assert!(result.contains("\"observations\":1"));
        assert!(result.contains("\"oversized\":0"));
        assert!(result.contains("\"amount\":\"10\""));
        assert!(result.contains("\"provenance\":\"selected-windows-folder\""));
        assert!(result.len() <= 16384);
    }
}

#[test]
fn folder_line_limit_rejects_256_kib_plus_one_and_resumes_after_lf_or_crlf() {
    let record = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    for (ending, cr_bytes) in [("\n", 0), ("\r\n", 1)] {
        let tree = Tree::new();
        let line = format!(
            "{record}{}{ending}",
            " ".repeat(262145 - cr_bytes - record.len())
        );
        assert_eq!(line.len(), 262146);
        fs::write(tree.0.join("a.jsonl"), format!("{line}{record}\n")).unwrap();
        let reader = tree.reader();
        reader
            .select_folder_with(|| Ok(Some(tree.0.clone())))
            .unwrap();
        let result = reader.analyze_folder().unwrap();
        assert!(result.contains("\"observations\":1"));
        assert!(result.contains("\"oversized\":1"));
        assert!(result.contains("\"unparsedLines\":true"));
        assert!(result.contains("\"amount\":\"10\""));
    }
}

#[test]
fn folder_crlf_split_at_64_kib_boundary_preserves_claims_and_cross_file_replay() {
    let tree = Tree::new();
    let record = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    // CR is the final byte of the first chunk, LF is the first of the next.
    fs::write(
        tree.0.join("a.jsonl"),
        format!("{record}{}\r\n", " ".repeat(65535 - record.len())),
    )
    .unwrap();
    fs::write(
        tree.0.join("b.jsonl"),
        format!("{record}{}\n", " ".repeat(199922 - record.len())),
    )
    .unwrap();
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(2))
    );
    let result = reader.analyze_folder().unwrap();
    assert!(result.contains("\"observations\":2"));
    assert!(result.contains("\"oversized\":0"));
    assert!(result.contains("\"amount\":\"20\""));
    assert!(result.contains("\"representatives\":1,\"exactReplayCopies\":1"));
}

#[test]
fn folder_large_lines_and_rejections_share_the_4096_retained_outcome_budget() {
    let tree = Tree::new();
    let record = include_str!("../../fixtures/abf141-replay.jsonl")
        .lines()
        .next()
        .unwrap();
    let line = format!("{record}{}", " ".repeat(262144 - record.len()));
    fs::write(
        tree.0.join("a.jsonl"),
        format!("{}[\n", "{}\n".repeat(4093)),
    )
    .unwrap();
    fs::write(tree.0.join("b.jsonl"), format!("{line}\n{line} \n")).unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    let result = reader.analyze_folder().unwrap();
    assert!(result.contains("\"observations\":4094"));
    assert!(result.contains("\"rejected\":{\"total\":1"));
    assert!(result.contains("\"oversized\":1"));
    assert!(result.contains("\"unparsedLines\":true"));
    assert!(result.contains("\"amount\":\"10\""));
    assert!(result.len() <= 16384);

    fs::write(tree.0.join("b.jsonl"), format!("{line}\n{line} \n{{}}\n")).unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert_eq!(reader.analyze_folder(), Err("COPY_READ_FAILED"));
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn folder_record_budget_is_shared_and_partial_file_failure_rejects_everything() {
    let tree = Tree::new();
    fs::write(tree.0.join("a.jsonl"), "{}\n".repeat(2048)).unwrap();
    fs::write(tree.0.join("b.jsonl"), "{}\n".repeat(2048)).unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert!(
        reader
            .analyze_folder()
            .unwrap()
            .contains("\"observations\":4096")
    );
    fs::write(tree.0.join("b.jsonl"), "{}\n".repeat(2049)).unwrap();
    let reader = tree.reader();
    reader
        .select_folder_with(|| Ok(Some(tree.0.clone())))
        .unwrap();
    assert_eq!(reader.analyze_folder(), Err("COPY_READ_FAILED"));
    assert_eq!(reader.analyze_folder(), Err("COPY_ATTEMPT_CONSUMED"));
}

#[test]
fn folder_sixteen_files_are_admitted_without_independent_summaries() {
    let tree = Tree::new();
    for index in 0..16 {
        fs::write(tree.0.join(format!("{index}.jsonl")), "{}\n").unwrap();
    }
    let reader = tree.reader();
    assert_eq!(
        reader.select_folder_with(|| Ok(Some(tree.0.clone()))),
        Ok(Some(16))
    );
    let result = reader.analyze_folder().unwrap();
    assert!(result.contains("\"read\":16"));
    assert!(result.contains("\"observations\":16"));
    assert_eq!(result.matches("\"replay\":").count(), 1);
}
