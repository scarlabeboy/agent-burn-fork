//! ABF28 research bounded read against the real kernel, on synthetic
//! fixtures only. Compiled **only** in the deliberate two-key research build
//! (feature `fault-injection` + `--cfg abf_secure_fs_harness`) on the
//! supported target; every other build compiles this file to nothing.
//!
//! Fixtures are created **only** beneath the directory named by
//! `ABF_SECURE_FS_FIXTURE_DIR` (no fallback). Each test owns one fresh case
//! directory `<fixture>/abf28-<test>-<pid>` holding an authorized `root/`
//! and a sibling `outside/`, and removes it when done. Every byte read was
//! written by the test itself (deterministic synthetic patterns).
//!
//! Evidence classes, labeled per test: **native** (real kernel, real files,
//! real `pread64`/`fstat`), **scheduled** (native, with a deterministic
//! harness hook between transaction steps), **clamped** (genuine kernel
//! reads with a harness-reduced count) and **mocked** (a harness override
//! replaces a kernel result; never presented as kernel evidence).

#![cfg(all(
    feature = "fault-injection",
    abf_secure_fs_harness,
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]

use abf_secure_fs::fault_injection::{
    self as fi, PreadInjection, ReadEvent, StatInjection, StatOverride, errno,
};
use abf_secure_fs::research_read::{
    Coherence, MAX_CAPTURE_BYTES, MAX_FSTAT_ATTEMPTS, ObservationEnd, PrefixObservation,
    ReadBudget, ReadBudgetKind, ResearchReadError,
};
use abf_secure_fs::{
    AccessError, Cancellation, ObjectKind, ReadableRegularFile, RelativePath, SecureRoot,
};
use std::cell::{Cell, RefCell};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{FileExt, MetadataExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const FIXTURE_ENV: &str = "ABF_SECURE_FS_FIXTURE_DIR";
const CHILD_ENV: &str = "ABF28_READ_CHILD";
const FILE: &str = "f.abf28-fixture";
/// Supervisor deadline of every race scenario (a timeout is a failure).
const RACE_DEADLINE: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

struct Case {
    dir: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let base = std::env::var_os(FIXTURE_ENV).unwrap_or_else(|| {
            panic!("{FIXTURE_ENV} must name the authorized fixture directory (no fallback)")
        });
        let base = PathBuf::from(base);
        assert!(base.is_absolute(), "{FIXTURE_ENV} must be absolute");
        let meta = fs::symlink_metadata(&base).expect("fixture directory must exist");
        assert!(meta.is_dir(), "fixture directory must be a real directory");
        let dir = base.join(format!("abf28-{name}-{}", std::process::id()));
        fs::create_dir(&dir).expect("case directory must be fresh");
        fs::create_dir(dir.join("root")).unwrap();
        fs::create_dir(dir.join("outside")).unwrap();
        Self { dir }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn authorize(&self) -> SecureRoot {
        let text = self.path("root").to_str().unwrap().to_owned();
        match SecureRoot::authorize(&text) {
            Ok(root) => root,
            Err(
                error @ (AccessError::SecurePrimitiveUnavailable
                | AccessError::UnsupportedFilesystem),
            ) => panic!("ENVIRONMENT UNSUPPORTED ({error:?}): this is not a pass"),
            Err(error) => panic!("fixture root authorization failed: {error:?}"),
        }
    }

    /// Writes `bytes` to `root/<FILE>` (fresh) and returns them.
    fn write(&self, bytes: Vec<u8>) -> Vec<u8> {
        let path = self.path(&format!("root/{FILE}"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        bytes
    }

    /// Writes a synthetic file of `len` bytes, authorizes the root, pins and
    /// promotes it.
    fn promoted(&self, len: usize) -> (Vec<u8>, SecureRoot, ReadableRegularFile) {
        let bytes = self.write(pattern(len, 0));
        let root = self.authorize();
        let readable = root
            .pin_regular(&RelativePath::new(FILE).unwrap())
            .unwrap()
            .promote()
            .unwrap();
        (bytes, root, readable)
    }

    fn file(&self) -> PathBuf {
        self.path(&format!("root/{FILE}"))
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // std's remove_dir_all does not follow symlinks. Everything removed
        // here was created by this test inside its own case directory.
        if let Err(error) = fs::remove_dir_all(&self.dir) {
            eprintln!("cleanup failed for a case directory: {error}");
        }
    }
}

/// Deterministic synthetic bytes: a marker line, then a position-dependent
/// pattern (so misplaced or repeated windows are detected).
fn pattern(len: usize, generation: u8) -> Vec<u8> {
    let marker = format!("abf28-synthetic generation {generation}\n").into_bytes();
    (0..len)
        .map(|i| {
            marker
                .get(i)
                .copied()
                .unwrap_or_else(|| ((i * 31 + usize::from(generation) * 7) % 251) as u8)
        })
        .collect()
}

fn read(
    readable: ReadableRegularFile,
    budget: ReadBudget,
    cancel: &Cancellation,
) -> Result<PrefixObservation, ResearchReadError> {
    let permit = fi::mint_research_read_permit().expect("no other permit outstanding");
    readable.research_read_prefix(permit, budget, cancel)
}

fn read_max(readable: ReadableRegularFile) -> Result<PrefixObservation, ResearchReadError> {
    read(readable, ReadBudget::maximum(), &Cancellation::new())
}

/// Resets thread-local instrumentation; on drop clears the hooks and checks
/// that every descriptor wrapped on this thread was closed and every stage
/// disposed. Declare it **first** so it drops after all capabilities.
struct LeakCheck;

impl LeakCheck {
    fn start() -> Self {
        fi::reset();
        Self
    }
}

impl Drop for LeakCheck {
    fn drop(&mut self) {
        fi::clear_read_event_hook();
        if !std::thread::panicking() {
            let counters = fi::fd_counters();
            assert_eq!(
                counters.opened, counters.closed,
                "descriptor leak: {counters:?}"
            );
            let ledger = fi::read_stage_ledger();
            assert_eq!(
                ledger.allocations, ledger.disposals,
                "stage leak: {ledger:?}"
            );
        }
    }
}

/// Runs `action` once, the first time `event` is reached.
fn on(event: ReadEvent, action: impl FnOnce() + 'static) {
    let mut action = Some(action);
    fi::set_read_event_hook(move |seen| {
        if seen == event
            && let Some(action) = action.take()
        {
            action();
        }
    });
}

fn expected_preads(len: usize) -> usize {
    if len >= MAX_CAPTURE_BYTES {
        MAX_CAPTURE_BYTES / 4096
    } else {
        len.div_ceil(4096) + 1
    }
}

/// Every refusal: no observation, both held descriptors closed (only the
/// caller's live `SecureRoot`, two descriptors, remains), stage disposed and
/// zero-filled, permit released.
fn assert_refused_cleanly(
    result: Result<PrefixObservation, ResearchReadError>,
) -> ResearchReadError {
    let error = match result {
        Ok(observation) => panic!("expected a refusal, got {observation:?}"),
        Err(error) => error,
    };
    let counters = fi::fd_counters();
    assert_eq!(
        counters.opened - counters.closed,
        2,
        "{error:?}: only the root remains: {counters:?}"
    );
    let ledger = fi::read_stage_ledger();
    assert_eq!(ledger.allocations, ledger.disposals, "{error:?}");
    if ledger.disposals > 0 {
        assert_eq!(ledger.last_disposal_zeroed, Some(true), "{error:?}");
    }
    let again = fi::mint_research_read_permit();
    assert!(again.is_some(), "the consumed permit was released");
    error
}

// ---------------------------------------------------------------------------
// Native: stable files
// ---------------------------------------------------------------------------

/// Native: stable empty, short, page-sized, multi-page and capture-sized
/// files end exactly as specified, with the exact syscall counts; the
/// identity comes from kernel samples.
#[test]
fn stable_lengths_end_and_count_exactly() {
    let _leaks = LeakCheck::start();
    for len in [
        0, 1, 100, 4095, 4096, 4097, 10_000, 65_535, 65_536, 65_537, 70_000, 200_000,
    ] {
        fi::reset();
        let case = Case::new(&format!("stable{len}"));
        let (bytes, root, readable) = case.promoted(len);
        let pinned_id = readable.evidence().object_id();
        let meta = fs::symlink_metadata(case.file()).unwrap();
        let observation = read_max(readable).unwrap();
        let preads = expected_preads(len);
        assert_eq!(
            observation.bytes(),
            &bytes[..len.min(MAX_CAPTURE_BYTES)],
            "{len}"
        );
        assert_eq!(observation.len(), len.min(MAX_CAPTURE_BYTES));
        assert_eq!(
            observation.end(),
            if len >= MAX_CAPTURE_BYTES {
                ObservationEnd::CaptureLimit
            } else {
                ObservationEnd::ObservedEof
            },
            "{len}"
        );
        assert_eq!(observation.coherence(), Coherence::MutableObservation);
        let counters = observation.counters();
        assert_eq!(counters.pread_attempts(), preads, "{len}");
        assert_eq!(counters.fstat_attempts(), 2 + 2 * preads + 2, "{len}");
        assert_eq!(counters.captured_bytes(), observation.len());
        assert_eq!(counters.eintr_retries(), 0);
        assert_eq!(fi::pread_attempts(), preads as u64, "{len}");
        assert_eq!(fi::fstat_attempts(), (2 + 2 * preads + 2) as u64, "{len}");
        assert!(counters.fstat_attempts() <= MAX_FSTAT_ATTEMPTS);
        assert_eq!(observation.object_id(), pinned_id);
        assert_eq!(observation.object_id().inode(), meta.ino());
        assert_eq!(observation.object_id().device(), meta.dev());
        drop(observation);
        drop(root);
        eprintln!("ABF28-NATIVE stable len={len} preads={preads}");
    }
}

/// Native: a reduced capture budget equal to the true length is a capture
/// limit (no lookahead); one more byte of capture observes the real EOF.
#[test]
fn reduced_capture_distinguishes_capture_limit_from_eof() {
    let _leaks = LeakCheck::start();
    for (capture, end, preads) in [
        (100, ObservationEnd::CaptureLimit, 1),
        (101, ObservationEnd::ObservedEof, 2),
        (40, ObservationEnd::CaptureLimit, 1),
    ] {
        let case = Case::new(&format!("capture{capture}"));
        let (bytes, _root, readable) = case.promoted(100);
        let budget = ReadBudget::new(4096, capture, 262_144, 64, 2, 8).unwrap();
        let observation = read(readable, budget, &Cancellation::new()).unwrap();
        assert_eq!(observation.end(), end, "{capture}");
        assert_eq!(observation.bytes(), &bytes[..capture.min(100)]);
        assert_eq!(observation.counters().pread_attempts(), preads);
    }
}

/// Native: reduced attempt and requested-byte budgets stop with an explicit
/// incomplete end and the validated prefix only.
#[test]
fn budget_stops_are_explicitly_incomplete() {
    let _leaks = LeakCheck::start();
    let case = Case::new("budgets");
    let (bytes, root, readable) = case.promoted(30_000);
    let budget = ReadBudget::new(4096, 65_536, 262_144, 3, 2, 8).unwrap();
    let observation = read(readable, budget, &Cancellation::new()).unwrap();
    assert_eq!(
        observation.end(),
        ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)
    );
    assert_eq!(observation.bytes(), &bytes[..3 * 4096]);
    drop(observation);

    let readable = root
        .pin_regular(&RelativePath::new(FILE).unwrap())
        .unwrap()
        .promote()
        .unwrap();
    let budget = ReadBudget::new(1000, 65_536, 2_500, 64, 2, 8).unwrap();
    let observation = read(readable, budget, &Cancellation::new()).unwrap();
    assert_eq!(
        observation.end(),
        ObservationEnd::BudgetStop(ReadBudgetKind::RequestedBytes)
    );
    assert_eq!(observation.bytes(), &bytes[..2_500]);
    assert_eq!(observation.counters().pread_attempts(), 3);
}

/// Clamped: genuine kernel reads with harness-reduced counts are progress,
/// never EOF; the prefix is reassembled exactly.
#[test]
fn short_reads_progress_until_a_real_zero() {
    let _leaks = LeakCheck::start();
    let case = Case::new("short");
    let (bytes, _root, readable) = case.promoted(10_000);
    fi::inject_pread(&[
        Some(PreadInjection::Clamp(1)),
        Some(PreadInjection::Clamp(1000)),
        Some(PreadInjection::Clamp(7)),
        Some(PreadInjection::Clamp(4095)),
    ]);
    let observation = read_max(readable).unwrap();
    assert_eq!(observation.bytes(), &bytes[..]);
    assert_eq!(observation.end(), ObservationEnd::ObservedEof);
    // 1 + 1000 + 7 + 4095 = 5103; then 4096, then 801, then EOF.
    assert_eq!(observation.counters().pread_attempts(), 7);
    assert_eq!(fi::remaining_pread_injections(), 0);
}

// ---------------------------------------------------------------------------
// Native: descriptors, identity and held-object authority
// ---------------------------------------------------------------------------

const O_ACCMODE: i32 = 0o3;
const O_NONBLOCK: i32 = 0o4000;
const FD_CLOEXEC: i32 = 1;

/// Native: the transaction reads through the read-only, close-on-exec
/// promoted descriptor, ignores (and never needs) its file offset, opens
/// nothing and closes exactly the two held descriptors before release.
#[test]
fn descriptors_are_read_only_cloexec_and_only_the_two_held() {
    let _leaks = LeakCheck::start();
    let case = Case::new("flags");
    let (bytes, _root, readable) = case.promoted(5_000);
    let status = readable.harness_descriptor_status();
    assert_eq!(status.readable_status_flags & O_ACCMODE, 0, "O_RDONLY");
    assert_ne!(status.readable_status_flags & O_NONBLOCK, 0);
    assert_ne!(status.readable_fd_flags & FD_CLOEXEC, 0);
    assert_ne!(status.original_fd_flags & FD_CLOEXEC, 0);
    assert_eq!(readable.harness_seek(4_321), 4_321);

    let before = fi::fd_counters();
    let open_at_release = Rc::new(Cell::new(None));
    let seen = Rc::clone(&open_at_release);
    on(ReadEvent::BeforeFinalCancel, move || {
        let now = fi::fd_counters();
        seen.set(Some(now.opened - now.closed));
    });
    let observation = read_max(readable).unwrap();
    let after = fi::fd_counters();
    assert_eq!(after.opened, before.opened, "no descriptor opened");
    assert_eq!(after.closed, before.closed + 2, "exactly the two held ones");
    assert_eq!(
        open_at_release.get(),
        Some(before.opened - before.closed),
        "both still held at the last poll"
    );
    assert_eq!(observation.bytes(), &bytes[..], "pread from offset 0");
}

/// Native: the same allocation the kernel wrote into is the one released
/// (no copy); dropping the result zero-fills and frees it.
#[test]
fn release_moves_the_same_allocation_and_disposes_it() {
    let _leaks = LeakCheck::start();
    let case = Case::new("transfer");
    let (_bytes, _root, readable) = case.promoted(9_000);
    let observation = read_max(readable).unwrap();
    let ledger = fi::read_stage_ledger();
    assert_eq!(ledger.allocations, 1);
    assert_eq!(ledger.disposals, 0, "alive in the result");
    assert_eq!(observation.bytes().as_ptr() as usize, ledger.last_address);
    drop(observation);
    let ledger = fi::read_stage_ledger();
    assert_eq!(ledger.disposals, 1);
    assert_eq!(ledger.last_disposal_zeroed, Some(true));
}

/// Native (A23-2, research policy A): dropping the root, then moving the
/// held object out of the root (or deleting the root directory itself)
/// before the transaction does not stop it; the bytes are the held
/// object's.
#[test]
fn root_drop_and_move_out_keep_held_object_authority() {
    let _leaks = LeakCheck::start();
    let case = Case::new("moveout");
    let (bytes, root, readable) = case.promoted(700);
    drop(root);
    fs::rename(case.file(), case.path(&format!("outside/{FILE}"))).unwrap();
    fs::remove_dir(case.path("root")).unwrap();
    let observation = read_max(readable).unwrap();
    assert_eq!(observation.bytes(), &bytes[..]);
    assert_eq!(observation.end(), ObservationEnd::ObservedEof);
}

/// Native: replacing the old name by a decoy regular file, a symlink to a
/// decoy or a FIFO never supplies bytes: the held object is read.
#[test]
fn path_replacement_decoys_never_supply_bytes() {
    let _leaks = LeakCheck::start();
    for decoy in ["regular", "symlink", "fifo"] {
        let case = Case::new(&format!("decoy-{decoy}"));
        let (bytes, _root, readable) = case.promoted(300);
        fs::rename(case.file(), case.path("root/moved.abf28-fixture")).unwrap();
        let decoy_bytes = pattern(300, 9);
        fs::write(case.path("outside/decoy.abf28-fixture"), &decoy_bytes).unwrap();
        match decoy {
            "regular" => fs::write(case.file(), &decoy_bytes).unwrap(),
            "symlink" => symlink(case.path("outside/decoy.abf28-fixture"), case.file()).unwrap(),
            _ => {
                let status = Command::new("/usr/bin/mkfifo")
                    .arg("-m")
                    .arg("600")
                    .arg(case.file())
                    .status()
                    .unwrap();
                assert!(status.success(), "mkfifo failed");
            }
        }
        let observation = read_max(readable).unwrap();
        assert_eq!(observation.bytes(), &bytes[..], "{decoy}");
        assert_ne!(observation.bytes(), &decoy_bytes[..]);
    }
}

/// Native: a second link or an unlink observed at entry refuses before any
/// allocation or read.
#[test]
fn link_changes_before_entry_are_refused() {
    let _leaks = LeakCheck::start();
    let case = Case::new("entrylinks");
    let (_bytes, root, readable) = case.promoted(100);
    fs::hard_link(case.file(), case.path("outside/alias.abf28-fixture")).unwrap();
    let error = assert_refused_cleanly(read_max(readable));
    assert_eq!(
        error,
        ResearchReadError::Metadata(AccessError::MultipleLinks)
    );
    assert_eq!((fi::pread_attempts(), fi::fstat_attempts()), (0, 2));
    assert_eq!(fi::read_stage_ledger().allocations, 0);
    fs::remove_file(case.path("outside/alias.abf28-fixture")).unwrap();

    let readable = root
        .pin_regular(&RelativePath::new(FILE).unwrap())
        .unwrap()
        .promote()
        .unwrap();
    fs::remove_file(case.file()).unwrap();
    let error = assert_refused_cleanly(read_max(readable));
    assert_eq!(
        error,
        ResearchReadError::Metadata(AccessError::ObjectChanged)
    );
}

// ---------------------------------------------------------------------------
// Scheduled: deterministic changes during the observation
// ---------------------------------------------------------------------------

/// Scheduled: growth, truncation, a move out of the root (ctime), a second
/// link and an unlink between attempt 1 and its post samples are each
/// refused; nothing is released; exactly one read and both post samples
/// happened.
#[test]
fn changes_after_an_attempt_refuse_the_whole_stage() {
    type Change = fn(&Case);
    let changes: [(&str, Change, ResearchReadError); 5] = [
        (
            "growth",
            |case| {
                let mut file = OpenOptions::new().append(true).open(case.file()).unwrap();
                file.write_all(b"appended").unwrap();
            },
            ResearchReadError::ContentChanged,
        ),
        (
            "truncation",
            |case| {
                let file = OpenOptions::new().write(true).open(case.file()).unwrap();
                file.set_len(5_000).unwrap();
            },
            ResearchReadError::ContentChanged,
        ),
        (
            "move-out",
            |case| fs::rename(case.file(), case.path(&format!("outside/{FILE}"))).unwrap(),
            ResearchReadError::ContentChanged,
        ),
        (
            "second-link",
            |case| fs::hard_link(case.file(), case.path("outside/alias.abf28-fixture")).unwrap(),
            ResearchReadError::Metadata(AccessError::MultipleLinks),
        ),
        (
            "unlink",
            |case| fs::remove_file(case.file()).unwrap(),
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        ),
    ];
    for (name, change, expected) in changes {
        let _leaks = LeakCheck::start();
        let case = Rc::new(Case::new(&format!("change-{name}")));
        let (_bytes, _root, readable) = case.promoted(20_000);
        let hook_case = Rc::clone(&case);
        on(ReadEvent::AfterAttempt(1), move || change(&hook_case));
        let error = assert_refused_cleanly(read_max(readable));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            (1, 4),
            "{name}"
        );
        eprintln!("ABF28-SCHEDULED change {name}: {error:?}");
    }
}

/// Scheduled examination: a same-size in-place rewrite of a not-yet-read
/// region (a would-be mixed-version read). Refusal is expected when the
/// timestamps change between samples; if the timestamps did not change
/// (coarse granularity), the release is still only a mutable observation.
/// The outcome is recorded either way; an undetected change is a documented
/// limitation, not a guarantee.
#[test]
fn same_size_rewrite_is_examined() {
    let _leaks = LeakCheck::start();
    for delay_ms in [0_u64, 20] {
        let case = Rc::new(Case::new(&format!("samesize{delay_ms}")));
        let (bytes, _root, readable) = case.promoted(20_000);
        let hook_case = Rc::clone(&case);
        on(ReadEvent::AfterAttempt(1), move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            let file = OpenOptions::new()
                .write(true)
                .open(hook_case.file())
                .unwrap();
            file.write_all_at(&pattern(4096, 5), 8192).unwrap();
        });
        match read_max(readable) {
            Err(error) => {
                assert_eq!(error, ResearchReadError::ContentChanged);
                eprintln!("ABF28-SAMESIZE delay={delay_ms}ms outcome=ContentChanged");
            }
            Ok(observation) => {
                assert_eq!(observation.coherence(), Coherence::MutableObservation);
                let mixed = observation.bytes() != &bytes[..];
                eprintln!(
                    "ABF28-SAMESIZE delay={delay_ms}ms outcome=Released mixed={mixed} (undetected change)"
                );
            }
        }
    }
}

/// Native: an ordinary writer that holds the file open for writing without
/// writing does not disturb the observation; its later writes are not part
/// of the released bytes.
#[test]
fn idle_writer_coexists() {
    let _leaks = LeakCheck::start();
    let case = Case::new("writer");
    let (bytes, _root, readable) = case.promoted(12_000);
    let writer = OpenOptions::new().write(true).open(case.file()).unwrap();
    let observation = read_max(readable).unwrap();
    writer.write_all_at(b"later", 0).unwrap();
    assert_eq!(observation.bytes(), &bytes[..]);
    assert_eq!(observation.end(), ObservationEnd::ObservedEof);
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

/// Native: cancellation already requested on entry: no sample, no stage, no
/// read; both descriptors and the permit are still consumed.
#[test]
fn entry_cancellation_samples_allocates_and_reads_nothing() {
    let _leaks = LeakCheck::start();
    let case = Case::new("cancel-entry");
    let (_bytes, _root, readable) = case.promoted(100);
    let cancel = Cancellation::new();
    cancel.request();
    let error = assert_refused_cleanly(read(readable, ReadBudget::maximum(), &cancel));
    assert_eq!(error, ResearchReadError::Cancelled);
    assert_eq!((fi::pread_attempts(), fi::fstat_attempts()), (0, 0));
    assert_eq!(fi::read_stage_ledger().allocations, 0);
}

/// Scheduled: cancellation after the stage allocation, during an `EINTR`
/// retry, after a successful attempt and right before release: never a
/// partial prefix.
#[test]
fn later_cancellation_never_releases_bytes() {
    /// (name, hook event, injection of attempt 1, expected (pread, fstat)).
    type CancelCase = (&'static str, ReadEvent, Option<PreadInjection>, (u64, u64));
    let cases: [CancelCase; 4] = [
        ("after-allocation", ReadEvent::StageAllocated, None, (0, 2)),
        (
            "during-retry",
            ReadEvent::AfterAttempt(1),
            Some(PreadInjection::Errno(errno::EINTR)),
            (1, 4),
        ),
        ("after-progress", ReadEvent::AfterAttempt(2), None, (2, 6)),
        // 10_000 bytes: 3 progress reads + EOF, then the final samples.
        (
            "before-release",
            ReadEvent::BeforeFinalCancel,
            None,
            (4, 12),
        ),
    ];
    for (name, event, injection, counts) in cases {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("cancel-{name}"));
        let (_bytes, _root, readable) = case.promoted(10_000);
        fi::inject_pread(&[injection]);
        let cancel = Arc::new(Cancellation::new());
        let hook_cancel = Arc::clone(&cancel);
        on(event, move || hook_cancel.request());
        let error = assert_refused_cleanly(read(readable, ReadBudget::maximum(), &cancel));
        assert_eq!(error, ResearchReadError::Cancelled, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            counts,
            "{name}"
        );
        assert_eq!(fi::read_stage_ledger().allocations, 1, "{name}");
    }
}

// ---------------------------------------------------------------------------
// Mocked: injected kernel results
// ---------------------------------------------------------------------------

/// Mocked `EINTR`: two consecutive grants then progress is fine, a third
/// consecutive is fatal; eight total grants are fine, a ninth is fatal.
/// Every attempt is charged and followed by both samples.
#[test]
fn eintr_limits_and_exhaustion() {
    let eintr = Some(PreadInjection::Errno(errno::EINTR));
    let step = Some(PreadInjection::Clamp(1));
    let mut eight = Vec::new();
    for _ in 0..4 {
        eight.extend([eintr, eintr, step]);
    }
    let mut nine = eight.clone();
    nine.push(eintr);
    /// (name, pread plan, granted retries or `None` for exhaustion).
    type EintrScript = (&'static str, Vec<Option<PreadInjection>>, Option<usize>);
    let scripts: [EintrScript; 4] = [
        ("two-consecutive", vec![eintr, eintr], Some(2)),
        ("three-consecutive", vec![eintr, eintr, eintr], None),
        ("eight-total", eight, Some(8)),
        ("nine-total", nine, None),
    ];
    for (name, script, granted) in scripts {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("eintr-{name}"));
        let (bytes, _root, readable) = case.promoted(100);
        let attempts = script.len();
        fi::inject_pread(&script);
        match (read_max(readable), granted) {
            (Ok(observation), Some(granted)) => {
                assert_eq!(observation.bytes(), &bytes[..], "{name}");
                assert_eq!(observation.counters().eintr_retries(), granted, "{name}");
            }
            (Err(error), None) => {
                assert_eq!(error, ResearchReadError::RetryExhausted, "{name}");
                assert_eq!(fi::pread_attempts(), attempts as u64, "{name}");
                assert_eq!(fi::fstat_attempts(), 2 + 2 * attempts as u64, "{name}");
                assert_eq!(fi::read_stage_ledger().last_disposal_zeroed, Some(true));
            }
            (other, _) => panic!("{name}: unexpected {other:?}"),
        }
    }
}

/// Mocked: `EAGAIN` is terminal (one attempt, no retry); impossible counts
/// are kernel-contract violations; a fatal errno wins over failing post
/// samples. Both post samples always run.
#[test]
fn injected_read_failures_follow_the_precedence() {
    let fail = Some(StatInjection::Errno(errno::EACCES));
    let scripts: [(
        &str,
        PreadInjection,
        [Option<StatInjection>; 4],
        ResearchReadError,
    ); 6] = [
        (
            "eagain",
            PreadInjection::Errno(errno::EAGAIN),
            [None; 4],
            ResearchReadError::WouldBlock,
        ),
        (
            "over-count",
            PreadInjection::Return(4097),
            [None, None, fail, fail],
            ResearchReadError::KernelContractViolation,
        ),
        (
            "negative-count",
            PreadInjection::Return(-2),
            [None; 4],
            ResearchReadError::KernelContractViolation,
        ),
        (
            "minus-one-without-errno",
            PreadInjection::Errno(0),
            [None; 4],
            ResearchReadError::KernelContractViolation,
        ),
        (
            "eio-with-failing-samples",
            PreadInjection::Errno(errno::EIO),
            [None, None, fail, Some(StatInjection::Errno(errno::ENOENT))],
            ResearchReadError::ReadFailed(AccessError::UnexpectedOs(errno::EIO)),
        ),
        (
            "eacces",
            PreadInjection::Errno(errno::EACCES),
            [None; 4],
            ResearchReadError::ReadFailed(AccessError::Inaccessible),
        ),
    ];
    for (name, injection, stats, expected) in scripts {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("inject-{name}"));
        let (_bytes, _root, readable) = case.promoted(100);
        fi::inject_pread(&[Some(injection)]);
        fi::inject_fstat(&stats);
        let error = assert_refused_cleanly(read_max(readable));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            (1, 4),
            "{name}"
        );
        assert_eq!(
            fi::remaining_fstat_injections(),
            0,
            "{name}: both post samples ran"
        );
    }
}

/// Mocked: when the first post sample fails, the second is still taken; the
/// original owner's refusal is reported first.
#[test]
fn both_post_samples_run_when_the_first_fails() {
    for (stats, expected) in [
        (
            [None, None, Some(StatInjection::Errno(errno::EACCES)), None],
            ResearchReadError::Metadata(AccessError::Inaccessible),
        ),
        (
            [
                None,
                None,
                Some(StatInjection::Errno(errno::EACCES)),
                Some(StatInjection::Errno(errno::ENOENT)),
            ],
            ResearchReadError::Metadata(AccessError::Inaccessible),
        ),
        (
            [None, None, None, Some(StatInjection::Errno(errno::ENOENT))],
            ResearchReadError::Metadata(AccessError::NotFound),
        ),
    ] {
        let _leaks = LeakCheck::start();
        let case = Case::new("postboth");
        let (_bytes, _root, readable) = case.promoted(100);
        fi::inject_fstat(&stats);
        let error = assert_refused_cleanly(read_max(readable));
        assert_eq!(error, expected);
        assert_eq!((fi::pread_attempts(), fi::fstat_attempts()), (1, 4));
        assert_eq!(fi::remaining_fstat_injections(), 0);
    }
}

/// Mocked (ABF30, A29-1): a kernel-contract violation detected in **either**
/// post sample outranks a fatal read, terminal `EAGAIN`, `EINTR` and an
/// ordinary refusal of the other owner; a fatal read still outranks ordinary
/// post refusals. Every case makes exactly one read and both post samples,
/// consumes both injections and releases nothing.
#[test]
fn post_contract_outranks_every_other_attempt_outcome() {
    use ResearchReadError::{KernelContractViolation as K, Metadata, ReadFailed, WouldBlock};
    let contract = Some(StatInjection::Errno(0));
    let fail = |code| Some(StatInjection::Errno(code));
    let eio = Some(PreadInjection::Errno(errno::EIO));
    let eagain = Some(PreadInjection::Errno(errno::EAGAIN));
    type Combination = (
        &'static str,
        usize,
        Option<PreadInjection>,
        [Option<StatInjection>; 2],
        ResearchReadError,
    );
    let cases: [Combination; 15] = [
        ("eio-second-contract", 100, eio, [None, contract], K),
        ("eio-first-contract", 100, eio, [contract, None], K),
        ("eagain-first-contract", 100, eagain, [contract, None], K),
        ("eagain-second-contract", 100, eagain, [None, contract], K),
        (
            "eacces-second-contract",
            100,
            None,
            [fail(errno::EACCES), contract],
            K,
        ),
        (
            "first-contract-enoent",
            100,
            None,
            [contract, fail(errno::ENOENT)],
            K,
        ),
        ("both-contract", 100, None, [contract, contract], K),
        (
            "eintr-second-contract",
            100,
            Some(PreadInjection::Errno(errno::EINTR)),
            [fail(errno::EACCES), contract],
            K,
        ),
        (
            "negative-ordinary",
            100,
            Some(PreadInjection::Return(-2)),
            [fail(errno::EACCES), fail(errno::ENOENT)],
            K,
        ),
        (
            "overcount-ordinary",
            100,
            Some(PreadInjection::Return(4097)),
            [fail(errno::EACCES), fail(errno::ENOENT)],
            K,
        ),
        // A genuine zero-byte read of an empty file.
        (
            "eof-second-contract",
            0,
            None,
            [fail(errno::EACCES), contract],
            K,
        ),
        (
            "eio-ordinary-stays-fatal",
            100,
            eio,
            [fail(errno::EACCES), fail(errno::ENOENT)],
            ReadFailed(AccessError::UnexpectedOs(errno::EIO)),
        ),
        (
            "eio-detector-change-stays-fatal",
            100,
            eio,
            [
                Some(StatInjection::Override(StatOverride {
                    size: Some(101),
                    ..StatOverride::default()
                })),
                None,
            ],
            ReadFailed(AccessError::UnexpectedOs(errno::EIO)),
        ),
        (
            "eagain-ordinary-stays-terminal",
            100,
            eagain,
            [None, fail(errno::ENOENT)],
            WouldBlock,
        ),
        (
            "ordinary-original-first",
            100,
            None,
            [fail(errno::EACCES), fail(errno::ENOENT)],
            Metadata(AccessError::Inaccessible),
        ),
    ];
    for (name, len, read, [original, readable], expected) in cases {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("prec-{name}"));
        let (_bytes, _root, readable_file) = case.promoted(len);
        fi::inject_pread(&[read]);
        fi::inject_fstat(&[None, None, original, readable]);
        let error = assert_refused_cleanly(read_max(readable_file));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            (1, 4),
            "{name}: one read, both post samples"
        );
        assert_eq!(fi::remaining_pread_injections(), 0, "{name}");
        assert_eq!(fi::remaining_fstat_injections(), 0, "{name}");
        assert_eq!(fi::read_stage_ledger().allocations, 1, "{name}");
        eprintln!("ABF30-PRECEDENCE {name}: {error:?}");
    }
}

/// Mocked (ABF30): the entry pair, the post pair after positive progress
/// and the final pair after staged bytes (also at an attempt-budget, a
/// capture and an end-of-file end) rank a contract violation of either
/// owner before any ordinary refusal; ordinary pairs stay original-first.
#[test]
fn entry_progress_and_final_pairs_rank_contract_first() {
    use ResearchReadError::{KernelContractViolation as K, Metadata};
    let contract = Some(StatInjection::Errno(0));
    let fail = |code| Some(StatInjection::Errno(code));
    let nlink = Some(StatInjection::Override(StatOverride {
        nlink: Some(2),
        ..StatOverride::default()
    }));
    let max = ReadBudget::maximum();
    let one_attempt = ReadBudget::new(4096, 65_536, 262_144, 1, 2, 8).unwrap();
    let capture = ReadBudget::new(4096, 100, 262_144, 64, 2, 8).unwrap();
    /// (name, file length, budget, `fstat` attempts before the pair, the
    /// pair, expected, expected (pread, fstat), stage allocated).
    type PairCase = (
        &'static str,
        usize,
        ReadBudget,
        usize,
        [Option<StatInjection>; 2],
        ResearchReadError,
        (u64, u64),
        u64,
    );
    let cases: [PairCase; 13] = [
        (
            "entry-first-contract",
            100,
            max,
            0,
            [contract, fail(errno::EACCES)],
            K,
            (0, 2),
            0,
        ),
        (
            "entry-second-contract",
            100,
            max,
            0,
            [fail(errno::EACCES), contract],
            K,
            (0, 2),
            0,
        ),
        (
            "entry-nlink-second-contract",
            100,
            max,
            0,
            [nlink, contract],
            K,
            (0, 2),
            0,
        ),
        (
            "entry-ordinary",
            100,
            max,
            0,
            [fail(errno::EACCES), fail(errno::ENOENT)],
            Metadata(AccessError::Inaccessible),
            (0, 2),
            0,
        ),
        // 20_000 bytes: the post pair of attempt 2 is fstat 5 and 6.
        (
            "progress-ordinary",
            20_000,
            max,
            4,
            [fail(errno::EACCES), fail(errno::ENOENT)],
            Metadata(AccessError::Inaccessible),
            (2, 6),
            1,
        ),
        (
            "progress-second-contract",
            20_000,
            max,
            4,
            [fail(errno::EACCES), contract],
            K,
            (2, 6),
            1,
        ),
        // 20_000 bytes: 5 progress reads + EOF; the final pair is fstat 15
        // and 16, after 20_000 staged bytes.
        (
            "final-first-contract",
            20_000,
            max,
            14,
            [contract, None],
            K,
            (6, 16),
            1,
        ),
        (
            "final-eio-second-contract",
            20_000,
            max,
            14,
            [fail(errno::EIO), contract],
            K,
            (6, 16),
            1,
        ),
        (
            "final-nlink-second-contract",
            20_000,
            max,
            14,
            [nlink, contract],
            K,
            (6, 16),
            1,
        ),
        (
            "final-ordinary",
            20_000,
            max,
            14,
            [fail(errno::EIO), fail(errno::ENOENT)],
            Metadata(AccessError::UnexpectedOs(errno::EIO)),
            (6, 16),
            1,
        ),
        // Budget adjacency: one funded attempt, then the final pair.
        (
            "budget-end-second-contract",
            100,
            one_attempt,
            4,
            [fail(errno::EACCES), contract],
            K,
            (1, 6),
            1,
        ),
        // Capture adjacency: 100 of 100 bytes, no lookahead, final pair.
        (
            "capture-end-first-contract",
            100,
            capture,
            4,
            [contract, None],
            K,
            (1, 6),
            1,
        ),
        // EOF adjacency: the post pair of the genuine zero read (attempt 2).
        (
            "eof-second-contract",
            100,
            max,
            4,
            [fail(errno::EACCES), contract],
            K,
            (2, 6),
            1,
        ),
    ];
    for (name, len, budget, before, pair, expected, counts, allocations) in cases {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("pair-{name}"));
        let (_bytes, _root, readable) = case.promoted(len);
        let mut plan = vec![None; before];
        plan.extend(pair);
        fi::inject_fstat(&plan);
        let error = assert_refused_cleanly(read(readable, budget, &Cancellation::new()));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            counts,
            "{name}"
        );
        assert_eq!(fi::remaining_fstat_injections(), 0, "{name}");
        assert_eq!(fi::read_stage_ledger().allocations, allocations, "{name}");
        eprintln!("ABF30-PAIR {name}: {error:?}");
    }
}

/// Scheduled (ABF30): cancellation requested after positive progress loses
/// to a contract violation and to an ordinary refusal of the same post
/// pair, and otherwise refuses; never a partial prefix.
#[test]
fn cancellation_after_progress_ranks_below_post_refusals() {
    let contract = Some(StatInjection::Errno(0));
    for (name, readable, expected) in [
        (
            "contract",
            contract,
            ResearchReadError::KernelContractViolation,
        ),
        (
            "ordinary",
            Some(StatInjection::Errno(errno::ENOENT)),
            ResearchReadError::Metadata(AccessError::NotFound),
        ),
        ("clean", None, ResearchReadError::Cancelled),
    ] {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("cancelprec-{name}"));
        let (_bytes, _root, readable_file) = case.promoted(20_000);
        fi::inject_fstat(&[None, None, None, None, None, readable]);
        let cancel = Arc::new(Cancellation::new());
        let hook_cancel = Arc::clone(&cancel);
        on(ReadEvent::AfterAttempt(2), move || hook_cancel.request());
        let error = assert_refused_cleanly(read(readable_file, ReadBudget::maximum(), &cancel));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            (2, 6),
            "{name}"
        );
    }
}

/// Clamped (ABF30): 64 genuine one-byte kernel reads reach exactly the
/// unchanged hard ceilings (64 attempts, 262144 requested bytes, 132
/// `fstat`) and release a budget-stopped prefix; counted and charged
/// totals agree.
#[test]
fn worst_case_native_counts_reach_the_unchanged_ceilings() {
    let _leaks = LeakCheck::start();
    let case = Case::new("ceilings");
    let (bytes, _root, readable) = case.promoted(100_000);
    fi::inject_pread(&[Some(PreadInjection::Clamp(1)); 64]);
    let observation = read_max(readable).unwrap();
    assert_eq!(
        observation.end(),
        ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)
    );
    assert_eq!(observation.bytes(), &bytes[..64]);
    let counters = observation.counters();
    assert_eq!(counters.pread_attempts(), 64);
    assert_eq!(counters.requested_bytes(), 262_144);
    assert_eq!(counters.fstat_attempts(), MAX_FSTAT_ATTEMPTS);
    assert_eq!(counters.captured_bytes(), 64);
    assert_eq!(fi::pread_attempts(), 64);
    assert_eq!(fi::fstat_attempts(), MAX_FSTAT_ATTEMPTS as u64);
    assert_eq!(fi::read_stage_ledger().allocations, 1);
}

/// Mocked entry samples: `fstat` errno, identity mismatch, wrong type, extra
/// link, owners disagreeing and impossible timestamps refuse before any
/// stage or read; both entry samples are still taken.
#[test]
fn entry_sample_failures_and_mismatches() {
    let over = |o: StatOverride| Some(StatInjection::Override(o));
    let cases: [(&str, [Option<StatInjection>; 2], ResearchReadError); 8] = [
        (
            "errno-original",
            [Some(StatInjection::Errno(errno::EIO)), None],
            ResearchReadError::Metadata(AccessError::UnexpectedOs(errno::EIO)),
        ),
        (
            "errno-zero",
            [None, Some(StatInjection::Errno(0))],
            ResearchReadError::KernelContractViolation,
        ),
        (
            "identity",
            [
                None,
                over(StatOverride {
                    inode: Some(1),
                    ..StatOverride::default()
                }),
            ],
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        ),
        (
            "zero-identity",
            [
                over(StatOverride {
                    device: Some(0),
                    ..StatOverride::default()
                }),
                None,
            ],
            ResearchReadError::Metadata(AccessError::IdentityUnavailable),
        ),
        (
            "directory",
            [
                over(StatOverride {
                    mode: Some(0o040_700),
                    ..StatOverride::default()
                }),
                None,
            ],
            ResearchReadError::Metadata(AccessError::NotRegularFile(ObjectKind::Directory)),
        ),
        (
            "nlink",
            [
                None,
                over(StatOverride {
                    nlink: Some(2),
                    ..StatOverride::default()
                }),
            ],
            ResearchReadError::Metadata(AccessError::MultipleLinks),
        ),
        (
            "owners-disagree",
            [
                None,
                over(StatOverride {
                    size: Some(101),
                    ..StatOverride::default()
                }),
            ],
            ResearchReadError::ContentChanged,
        ),
        (
            "timestamp",
            [
                over(StatOverride {
                    ctime: Some((1, 1_000_000_000)),
                    ..StatOverride::default()
                }),
                None,
            ],
            ResearchReadError::InvalidMetadata,
        ),
    ];
    for (name, stats, expected) in cases {
        let _leaks = LeakCheck::start();
        let case = Case::new(&format!("entry-{name}"));
        let (_bytes, _root, readable) = case.promoted(100);
        fi::inject_fstat(&stats);
        let error = assert_refused_cleanly(read_max(readable));
        assert_eq!(error, expected, "{name}");
        assert_eq!(
            (fi::pread_attempts(), fi::fstat_attempts()),
            (0, 2),
            "{name}"
        );
        assert_eq!(fi::read_stage_ledger().allocations, 0, "{name}");
    }
}

/// Mocked: a refused stage allocation happens after the entry samples and
/// reads nothing.
#[test]
fn allocation_refusal_reads_nothing() {
    let _leaks = LeakCheck::start();
    let case = Case::new("alloc");
    let (_bytes, _root, readable) = case.promoted(100);
    fi::refuse_next_read_stage();
    let error = assert_refused_cleanly(read_max(readable));
    assert_eq!(error, ResearchReadError::AllocationRefused);
    assert_eq!((fi::pread_attempts(), fi::fstat_attempts()), (0, 2));
    assert_eq!(fi::read_stage_ledger().allocations, 0);
}

/// Mocked: after several successful native reads, a failing or changed
/// final sample refuses release; the whole stage is disposed.
#[test]
fn failed_release_after_successful_reads_returns_no_bytes() {
    // 20_000 bytes: 5 progress reads + EOF = 6 attempts; the final samples
    // are fstat attempts 15 and 16.
    let finals: [(
        Option<StatInjection>,
        Option<StatInjection>,
        ResearchReadError,
    ); 3] = [
        (
            Some(StatInjection::Errno(errno::EIO)),
            None,
            ResearchReadError::Metadata(AccessError::UnexpectedOs(errno::EIO)),
        ),
        (
            None,
            Some(StatInjection::Override(StatOverride {
                mtime: Some((7, 7)),
                ..StatOverride::default()
            })),
            ResearchReadError::ContentChanged,
        ),
        (
            Some(StatInjection::Override(StatOverride {
                nlink: Some(0),
                ..StatOverride::default()
            })),
            None,
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        ),
    ];
    for (original, readable_final, expected) in finals {
        let _leaks = LeakCheck::start();
        let case = Case::new("finalfail");
        let (_bytes, _root, readable) = case.promoted(20_000);
        let mut plan = vec![None; 14];
        plan.extend([original, readable_final]);
        fi::inject_fstat(&plan);
        let error = assert_refused_cleanly(read_max(readable));
        assert_eq!(error, expected);
        assert_eq!((fi::pread_attempts(), fi::fstat_attempts()), (6, 16));
    }
}

// ---------------------------------------------------------------------------
// Ownership, permit and privacy
// ---------------------------------------------------------------------------

/// Scheduled panic inside the transaction (after attempt 1): unwinding
/// closes both descriptors, disposes the zero-filled stage and releases the
/// permit; no result is fabricated.
#[test]
fn panic_inside_the_transaction_cleans_up() {
    for event in [
        ReadEvent::EntrySampled,
        ReadEvent::StageAllocated,
        ReadEvent::AfterAttempt(1),
        ReadEvent::BeforeFinalCancel,
        ReadEvent::OwnersClosed,
    ] {
        fi::reset();
        {
            let case = Case::new("panic");
            let (_bytes, _root, readable) = case.promoted(9_000);
            fi::set_read_event_hook(move |seen| {
                if seen == event {
                    panic!("ABF28 scheduled panic");
                }
            });
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_max(readable)));
            assert!(result.is_err(), "{event:?}");
            fi::clear_read_event_hook();
        }
        let counters = fi::fd_counters();
        assert_eq!(counters.opened, counters.closed, "{event:?}: {counters:?}");
        let ledger = fi::read_stage_ledger();
        assert_eq!(ledger.allocations, ledger.disposals, "{event:?}");
        if ledger.disposals > 0 {
            assert_eq!(ledger.last_disposal_zeroed, Some(true), "{event:?}");
        }
        assert!(fi::mint_research_read_permit().is_some(), "{event:?}");
    }
    fi::reset();
}

/// Native: one outstanding permit per thread; a transaction consumes it in
/// success and refusal; another thread mints its own.
#[test]
fn permit_is_single_outstanding_and_consumed() {
    let _leaks = LeakCheck::start();
    let first = fi::mint_research_read_permit().unwrap();
    assert!(fi::mint_research_read_permit().is_none(), "no queue");
    assert!(
        std::thread::spawn(|| fi::mint_research_read_permit().is_some())
            .join()
            .unwrap(),
        "per-thread"
    );
    drop(first);

    let case = Case::new("permit");
    let (_bytes, root, readable) = case.promoted(50);
    let permit = fi::mint_research_read_permit().unwrap();
    let observation = readable
        .research_read_prefix(permit, ReadBudget::maximum(), &Cancellation::new())
        .unwrap();
    assert!(
        fi::mint_research_read_permit().is_some(),
        "consumed by success"
    );
    drop(observation);

    let readable = root
        .pin_regular(&RelativePath::new(FILE).unwrap())
        .unwrap()
        .promote()
        .unwrap();
    let permit = fi::mint_research_read_permit().unwrap();
    let cancel = Cancellation::new();
    cancel.request();
    assert!(
        readable
            .research_read_prefix(permit, ReadBudget::maximum(), &cancel)
            .is_err()
    );
    assert!(
        fi::mint_research_read_permit().is_some(),
        "consumed by refusal"
    );
}

/// Native and mocked: no diagnostic contains file bytes, names or paths.
#[test]
fn diagnostics_never_contain_bytes_names_or_paths() {
    let _leaks = LeakCheck::start();
    let case = Case::new("SECRET-CASE");
    let canary = b"SECRET-CONTENT-CANARY ".repeat(40);
    let bytes = case.write(canary.clone());
    let root = case.authorize();
    let readable = root
        .pin_regular(&RelativePath::new(FILE).unwrap())
        .unwrap()
        .promote()
        .unwrap();
    let permit = fi::mint_research_read_permit().unwrap();
    let mut texts = vec![format!("{permit:?} {:?}", ReadBudget::maximum())];
    let observation = readable
        .research_read_prefix(permit, ReadBudget::maximum(), &Cancellation::new())
        .unwrap();
    assert_eq!(observation.bytes(), &bytes[..]);
    texts.push(format!(
        "{observation:?} {:?} {:?} {:?}",
        observation.counters(),
        observation.end(),
        observation.coherence()
    ));
    for error in [
        ResearchReadError::InvalidBudget,
        ResearchReadError::Cancelled,
        ResearchReadError::AllocationRefused,
        ResearchReadError::KernelContractViolation,
        ResearchReadError::ReadFailed(AccessError::UnexpectedOs(5)),
        ResearchReadError::WouldBlock,
        ResearchReadError::RetryExhausted,
        ResearchReadError::Metadata(AccessError::MultipleLinks),
        ResearchReadError::InvalidMetadata,
        ResearchReadError::ContentChanged,
    ] {
        texts.push(format!("{error:?} {error}"));
    }
    let path = case.dir.to_str().unwrap().to_owned();
    for text in texts {
        for needle in ["SECRET", FILE, "abf28-synthetic", path.as_str(), "/proc"] {
            assert!(
                !text.contains(needle),
                "diagnostic leaked {needle:?}: {text}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Supervised finite concurrent writers
// ---------------------------------------------------------------------------

/// Runs `scenario` on a fresh thread under the race deadline. A timeout is a
/// failure, never a pass.
fn supervised<T: Send + 'static>(name: &str, scenario: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(scenario());
    });
    rx.recv_timeout(RACE_DEADLINE)
        .unwrap_or_else(|_| panic!("TIMEOUT in race scenario {name}: this is not a pass"))
}

/// Native, supervised: a separate thread performs a finite number of
/// ordinary unlocked writes (append and truncate back, or same-size
/// rewrites of the tail) while this thread runs a finite number of
/// transactions. Every outcome must be a `ContentChanged` refusal or a
/// release with the never-rewritten head intact. A release whose rewritten
/// region matches no single written version (a mixed-version read that the
/// sampled change detector missed) is **counted and reported**, not hidden:
/// it is the documented limitation of metadata-equality detection.
#[test]
fn finite_concurrent_writers_are_refused_or_observed_consistently() {
    for mode in ["append-truncate", "tail-rewrite"] {
        let outcome = supervised(mode, move || {
            let _leaks = LeakCheck::start();
            let case = Case::new(&format!("race-{mode}"));
            let base = case.write(pattern(6_000, 0));
            let root = case.authorize();
            let stop = Arc::new(AtomicBool::new(false));
            let writer_stop = Arc::clone(&stop);
            let path = case.file();
            let writer = std::thread::spawn(move || {
                let file = OpenOptions::new().write(true).open(&path).unwrap();
                let mut writes = 0_u32;
                while writes < 4_000 && !writer_stop.load(Ordering::Relaxed) {
                    if mode == "append-truncate" {
                        file.write_all_at(&pattern(512, 3), 6_000).unwrap();
                        file.set_len(6_000).unwrap();
                    } else {
                        let generation = (writes % 2) as u8 + 1;
                        file.write_all_at(&pattern(1_000, generation)[..], 5_000)
                            .unwrap();
                    }
                    writes += 1;
                }
                writes
            });
            let (released, refused, mixed) = (
                RefCell::new(0_u32),
                RefCell::new(0_u32),
                RefCell::new(0_u32),
            );
            let versions: Vec<Vec<u8>> = if mode == "append-truncate" {
                let mut grown = base.clone();
                grown.extend(pattern(512, 3));
                vec![base.clone(), grown]
            } else {
                (1..=2)
                    .map(|generation| {
                        let mut version = base.clone();
                        version[5_000..].copy_from_slice(&pattern(1_000, generation));
                        version
                    })
                    .chain([base.clone()])
                    .collect()
            };
            for _ in 0..60 {
                let readable = match root
                    .pin_regular(&RelativePath::new(FILE).unwrap())
                    .and_then(|pinned| pinned.promote())
                {
                    Ok(readable) => readable,
                    Err(error) => panic!("{mode}: pin/promote refused: {error:?}"),
                };
                match read_max(readable) {
                    Ok(observation) => {
                        let got = observation.bytes();
                        assert!(got.len() >= 5_000, "{mode}: {}", got.len());
                        assert_eq!(&got[..5_000], &base[..5_000], "{mode}: stable head");
                        if !versions.iter().any(|version| version.as_slice() == got) {
                            *mixed.borrow_mut() += 1;
                        }
                        *released.borrow_mut() += 1;
                    }
                    Err(ResearchReadError::ContentChanged) => *refused.borrow_mut() += 1,
                    Err(other) => panic!("{mode}: unexpected refusal {other:?}"),
                }
            }
            stop.store(true, Ordering::Relaxed);
            let writes = writer.join().unwrap();
            let (released, refused, mixed) = (
                released.into_inner(),
                refused.into_inner(),
                mixed.into_inner(),
            );
            (writes, released, refused, mixed)
        });
        eprintln!(
            "ABF28-RACE {mode}: writes={} released={} refused(ContentChanged)={} \
             released-matching-no-single-version={}",
            outcome.0, outcome.1, outcome.2, outcome.3
        );
        assert_eq!(outcome.1 + outcome.2, 60);
    }
}

// ---------------------------------------------------------------------------
// Isolated process: real descriptor numbers
// ---------------------------------------------------------------------------

/// Re-runs the named test of this binary in a child process with
/// `CHILD_ENV=1`, single-threaded, and returns its combined output after
/// checking that exactly that one test ran and passed.
fn run_child(test_name: &str) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "child failed:\n{text}");
    assert!(
        text.contains("1 passed") && text.contains("0 failed"),
        "child did not run exactly one passing test:\n{text}"
    );
    text
}

/// Descriptor numbers the kernel hands out next (lowest free number): in a
/// single-threaded child, any descriptor left open changes the result.
fn probe(file: &Path) -> Vec<i32> {
    use std::os::fd::AsRawFd;
    let files: Vec<fs::File> = (0..8).map(|_| fs::File::open(file).unwrap()).collect();
    files.iter().map(|f| f.as_raw_fd()).collect()
}

/// Native, isolated process: after a released observation, after a refusal
/// and after a panic, the real descriptor table is back to its baseline.
#[test]
fn descriptors_close_exactly_in_an_isolated_process() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let out = run_child("descriptors_close_exactly_in_an_isolated_process");
        assert!(out.contains("ABF28-CLOSURE scenarios: release, refusal, panic"));
        return;
    }
    fi::reset();
    let case = Case::new("closure");
    fs::write(case.path("outside/probe.abf28-fixture"), b"probe").unwrap();
    let probe_file = case.path("outside/probe.abf28-fixture");
    let (_bytes, root, readable) = case.promoted(5_000);
    drop(readable);
    let base = probe(&probe_file);

    let promote = || {
        root.pin_regular(&RelativePath::new(FILE).unwrap())
            .unwrap()
            .promote()
            .unwrap()
    };
    let observation = read_max(promote()).unwrap();
    assert_eq!(probe(&probe_file), base, "closed before release");
    drop(observation);

    fi::inject_pread(&[Some(PreadInjection::Errno(errno::EIO))]);
    assert!(read_max(promote()).is_err());
    assert_eq!(probe(&probe_file), base, "after refusal");

    let readable = promote();
    fi::set_read_event_hook(|event| {
        if event == ReadEvent::AfterAttempt(1) {
            panic!("ABF28 closure panic");
        }
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_max(readable)));
    assert!(result.is_err());
    fi::clear_read_event_hook();
    assert_eq!(probe(&probe_file), base, "after panic");
    eprintln!("ABF28-CLOSURE scenarios: release, refusal, panic");
    fi::reset();
}
