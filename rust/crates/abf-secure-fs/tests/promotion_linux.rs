//! M2A.2a readable promotion against the real kernel, on synthetic fixtures.
//!
//! Fixtures are created **only** beneath the directory named by
//! `ABF_SECURE_FS_FIXTURE_DIR` (no fallback). Each test owns one fresh case
//! directory `<fixture>/abf15-<test>-<pid>` holding an authorized `root/` and
//! a sibling `outside/`, and removes it when done. Every byte read comes from
//! a synthetic marker written by the test itself.
//!
//! Evidence classes, as labeled in each test: **native** (real kernel,
//! real procfs, real files), **scheduled** (native, with a deterministic
//! harness hook between protocol steps), and **mocked** (a harness override
//! replaces a kernel result; never presented as kernel evidence).

#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]

use abf_secure_fs::{
    AccessError, BackendCapabilities, ObjectKind, PinnedRegularFile, RelativePath, SecureRoot,
};
use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const FIXTURE_ENV: &str = "ABF_SECURE_FS_FIXTURE_DIR";
const CHILD_ENV: &str = "ABF15_PROMOTION_CHILD";
const DEADLINE: Duration = Duration::from_secs(10);

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
        let dir = base.join(format!("abf15-{name}-{}", std::process::id()));
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

    /// Writes a synthetic marker file and returns its inode.
    fn file(&self, relative: &str, marker: &str) -> u64 {
        let path = self.path(relative);
        fs::write(&path, marker_bytes(marker)).unwrap();
        ino(&path)
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

fn marker_bytes(marker: &str) -> Vec<u8> {
    format!("abf15-synthetic {marker}\n").into_bytes()
}

fn ino(path: &Path) -> u64 {
    fs::symlink_metadata(path).unwrap().ino()
}

fn rel(text: &str) -> RelativePath {
    RelativePath::new(text).unwrap()
}

fn mkfifo(path: &Path) {
    let status = Command::new("/usr/bin/mkfifo")
        .arg("-m")
        .arg("600")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "mkfifo failed");
}

fn pin(case: &Case, root: &SecureRoot, relative: &str) -> PinnedRegularFile {
    let pinned = root.pin_regular(&rel(relative)).unwrap();
    let meta = fs::symlink_metadata(case.path(&format!("root/{relative}"))).unwrap();
    assert_eq!(pinned.evidence().object_id().inode(), meta.ino());
    pinned
}

/// Runs `f` on a fresh thread with a deadline, so a blocking open (FIFO)
/// would fail the test instead of hanging it. The promotion inside `f` then
/// runs on a non-main thread, whose `thread-self` is `<pid>/task/<tid>` with
/// `tid != pid`.
fn within_deadline<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(DEADLINE)
        .expect("operation did not finish within the deadline (blocking open?)")
}

/// Diagnostics must never contain synthetic names, markers or paths.
fn assert_private(text: &str, canaries: &[&str]) {
    for canary in canaries {
        assert!(
            !text.contains(canary),
            "diagnostic leaked a synthetic canary: {text}"
        );
    }
    assert!(!text.contains("/proc"), "diagnostic leaked a procfs path");
    assert!(
        !text.contains("abf11-m2a1-fixtures"),
        "diagnostic leaked a path"
    );
}

// ---------------------------------------------------------------------------
// Native (default build, real kernel)
// ---------------------------------------------------------------------------

#[test]
fn capability_is_described_honestly() {
    let caps = BackendCapabilities::compiled();
    assert!(caps.readable_promotion);
    assert!(caps.metadata_pinning);
    assert!(!caps.directory_enumeration && !caps.environment_attested);
}

#[test]
fn promotion_keeps_the_pinned_identity() {
    let case = Case::new("normal");
    let original = case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let pinned = pin(&case, &root, "f.abf15-fixture");
    let pin_evidence = *pinned.evidence();

    let readable = pinned.promote().unwrap();
    let evidence = *readable.evidence();
    assert_eq!(evidence.object_id(), pin_evidence.object_id());
    assert_eq!(evidence.object_id().inode(), original);
    assert_eq!(evidence.link_count(), 1);
    assert_eq!(evidence.size(), marker_bytes("original").len() as u64);
    assert_eq!(readable.reinspect().unwrap(), evidence);

    let debug = format!("{readable:?}");
    assert!(debug.starts_with("ReadableRegularFile"));
    assert_private(&debug, &["f.abf15-fixture", "original", "abf15-synthetic"]);
}

#[test]
fn promotion_works_from_another_thread_than_the_pin() {
    let case = Case::new("other_thread");
    let original = case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let pinned = pin(&case, &root, "f.abf15-fixture");
    let evidence = within_deadline(move || pinned.promote().map(|r| *r.evidence()));
    assert_eq!(evidence.unwrap().object_id().inode(), original);
}

/// Rename while pinned, including out of the authorized root: the pin is an
/// object capability; promotion is not a membership check.
#[test]
fn promotion_after_rename_and_move_outside_keeps_the_original() {
    let case = Case::new("renamed");
    let original = case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let pinned = pin(&case, &root, "f.abf15-fixture");
    fs::rename(
        case.path("root/f.abf15-fixture"),
        case.path("outside/moved.abf15-fixture"),
    )
    .unwrap();
    let readable = pinned.promote().unwrap();
    assert_eq!(readable.evidence().object_id().inode(), original);
    assert_eq!(ino(&case.path("outside/moved.abf15-fixture")), original);
}

/// The original is renamed away and its old name is replaced by another
/// regular file, a symlink, a FIFO or a socket. The replacement is never
/// opened: no FIFO blocking, no socket connection.
#[test]
fn replacement_at_the_old_name_is_never_opened() {
    type Setup = fn(&Case);
    let replacements: [(&str, Setup); 4] = [
        ("regular", |c| {
            c.file("root/f.abf15-fixture", "replacement");
        }),
        ("symlink", |c| {
            c.file("outside/target.abf15-fixture", "symlink-target");
            symlink(
                c.path("outside/target.abf15-fixture"),
                c.path("root/f.abf15-fixture"),
            )
            .unwrap();
        }),
        ("fifo", |c| mkfifo(&c.path("root/f.abf15-fixture"))),
        ("socket", |_| {}),
    ];
    for (name, setup) in replacements {
        let case = Case::new(&format!("replace_{name}"));
        let original = case.file("root/f.abf15-fixture", "original");
        let root = case.authorize();
        let pinned = pin(&case, &root, "f.abf15-fixture");
        fs::rename(
            case.path("root/f.abf15-fixture"),
            case.path("outside/original.abf15-fixture"),
        )
        .unwrap();
        setup(&case);
        let listener = (name == "socket")
            .then(|| UnixListener::bind(case.path("root/f.abf15-fixture")).unwrap());
        let replacement = ino(&case.path("root/f.abf15-fixture"));
        assert_ne!(replacement, original);

        let evidence = within_deadline(move || pinned.promote().map(|r| *r.evidence()));
        assert_eq!(evidence.unwrap().object_id().inode(), original, "{name}");
        if let Some(listener) = listener {
            listener.set_nonblocking(true).unwrap();
            assert!(listener.accept().is_err(), "nothing connected");
        }
    }
}

/// Last unlink before promotion: S0 observes nlink 0 and refuses before any
/// procfs work.
#[test]
fn unlinked_original_is_refused_at_s0() {
    let case = Case::new("unlinked");
    case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let pinned = pin(&case, &root, "f.abf15-fixture");
    fs::remove_file(case.path("root/f.abf15-fixture")).unwrap();
    assert_eq!(pinned.promote().unwrap_err(), AccessError::ObjectChanged);
}

#[test]
fn second_link_is_refused_at_s0_and_accepted_again_when_removed() {
    let case = Case::new("link_s0");
    let original = case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();

    let pinned = pin(&case, &root, "f.abf15-fixture");
    fs::hard_link(
        case.path("root/f.abf15-fixture"),
        case.path("outside/alias.abf15-fixture"),
    )
    .unwrap();
    assert_eq!(pinned.promote().unwrap_err(), AccessError::MultipleLinks);

    // Sampled policy, not history: one link again, a new pin promotes.
    fs::remove_file(case.path("outside/alias.abf15-fixture")).unwrap();
    let pinned = pin(&case, &root, "f.abf15-fixture");
    let readable = pinned.promote().unwrap();
    assert_eq!(readable.evidence().object_id().inode(), original);
}

#[test]
fn reinspection_observes_later_link_changes_on_both_handles() {
    let case = Case::new("reinspect");
    case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let readable = pin(&case, &root, "f.abf15-fixture").promote().unwrap();
    fs::hard_link(
        case.path("root/f.abf15-fixture"),
        case.path("outside/alias.abf15-fixture"),
    )
    .unwrap();
    assert_eq!(
        readable.reinspect().unwrap_err(),
        AccessError::MultipleLinks
    );
    fs::remove_file(case.path("outside/alias.abf15-fixture")).unwrap();
    assert_eq!(readable.reinspect().unwrap().link_count(), 1);
    fs::remove_file(case.path("root/f.abf15-fixture")).unwrap();
    assert_eq!(
        readable.reinspect().unwrap_err(),
        AccessError::ObjectChanged
    );
}

/// Two distinct synthetic objects pinned at the same time: each promotion
/// reaches only its own pin's object (the generated leaf is the pin's own
/// descriptor number).
#[test]
fn each_promotion_uses_only_its_own_descriptor() {
    let case = Case::new("own_descriptor");
    let a = case.file("root/a.abf15-fixture", "object-a");
    let b = case.file("root/b.abf15-fixture", "object-b");
    let root = case.authorize();
    let pin_a = pin(&case, &root, "a.abf15-fixture");
    let pin_b = pin(&case, &root, "b.abf15-fixture");
    let readable_a = pin_a.promote().unwrap();
    assert_eq!(readable_a.evidence().object_id().inode(), a);
    let readable_b = pin_b.promote().unwrap();
    assert_eq!(readable_b.evidence().object_id().inode(), b);
}

#[test]
fn separate_pins_of_one_object_promote_independently() {
    let case = Case::new("two_pins");
    let original = case.file("root/f.abf15-fixture", "original");
    let root = case.authorize();
    let a = pin(&case, &root, "f.abf15-fixture").promote().unwrap();
    let b = pin(&case, &root, "f.abf15-fixture").promote().unwrap();
    assert_eq!(a.evidence().object_id(), b.evidence().object_id());
    assert_eq!(a.evidence().object_id().inode(), original);
    drop(a);
    assert_eq!(b.reinspect().unwrap().object_id().inode(), original);
}

#[test]
fn diagnostics_never_contain_names_or_contents() {
    let canaries = [
        "secret-name.abf15-fixture",
        "secret-content",
        "abf15-synthetic",
    ];
    let case = Case::new("diagnostics");
    case.file("root/secret-name.abf15-fixture", "secret-content");
    let root = case.authorize();

    let pinned = pin(&case, &root, "secret-name.abf15-fixture");
    let readable = pinned.promote().unwrap();
    assert_private(
        &format!("{readable:?} {:?}", readable.evidence()),
        &canaries,
    );

    fs::remove_file(case.path("root/secret-name.abf15-fixture")).unwrap();
    let error = readable.reinspect().unwrap_err();
    assert_private(&format!("{error} {error:?}"), &canaries);

    case.file("root/secret-name.abf15-fixture", "secret-content");
    let pinned = pin(&case, &root, "secret-name.abf15-fixture");
    fs::remove_file(case.path("root/secret-name.abf15-fixture")).unwrap();
    let error = pinned.promote().unwrap_err();
    assert_private(&format!("{error} {error:?}"), &canaries);

    // Every refusal code prints a fixed string.
    for error in [
        AccessError::PromotionWouldBlock,
        AccessError::SecurePrimitiveUnavailable,
        AccessError::ObjectChanged,
        AccessError::MultipleLinks,
        AccessError::NotRegularFile(ObjectKind::Fifo),
        AccessError::RetryExhausted,
        AccessError::UnexpectedOs(24),
    ] {
        assert_private(&format!("{error} {error:?}"), &canaries);
    }
    assert_eq!(
        AccessError::PromotionWouldBlock.to_string(),
        "promotion would block"
    );
}

// ---------------------------------------------------------------------------
// Isolated-process descriptor closure (native; extra harness scenarios)
// ---------------------------------------------------------------------------

/// Re-runs the named test of this binary in a child process with
/// `CHILD_ENV=mode`, single-threaded, and returns its combined output after
/// checking that exactly that one test ran and passed.
fn run_child(test_name: &str, mode: &str) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, mode)
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

mod closure {
    use super::*;
    use std::fs::File;
    use std::os::fd::AsRawFd;

    const PROBES: usize = 8;

    /// Descriptor numbers the kernel hands out next: Linux allocates the
    /// lowest free number, so in a single-threaded child with no other
    /// descriptor activity, any descriptor left open in the probed range
    /// changes this result. No directory census is used.
    pub(super) fn probe(file: &Path) -> Vec<i32> {
        let files: Vec<File> = (0..PROBES).map(|_| File::open(file).unwrap()).collect();
        files.iter().map(|f| f.as_raw_fd()).collect()
    }

    pub(super) fn baseline(file: &Path) -> Vec<i32> {
        let numbers = probe(file);
        let first = numbers[0];
        assert_eq!(
            numbers,
            (first..first + PROBES as i32).collect::<Vec<_>>(),
            "probe range must start contiguous"
        );
        numbers
    }

    /// Numbers inside `[first baseline, last probe]` that are held while a
    /// capability is alive.
    pub(super) fn held(file: &Path, baseline: &[i32]) -> Vec<i32> {
        let probes = probe(file);
        let last = *probes.last().unwrap();
        (baseline[0]..=last)
            .filter(|n| !probes.contains(n))
            .collect()
    }

    /// Native scenarios, run in a child process.
    fn native_scenarios(case: &Case, root: &SecureRoot, probe_file: &Path, base: &[i32]) {
        // Success: exactly two descriptors (original + readable) held while
        // alive; none after drop.
        let readable = pin(case, root, "f.abf15-fixture").promote().unwrap();
        assert_eq!(held(probe_file, base).len(), 2, "success retains two");
        drop(readable);
        assert_eq!(probe(probe_file), base, "after success drop");

        // S0 refusal (second link): the consumed pin is closed.
        let pinned = pin(case, root, "f.abf15-fixture");
        assert_eq!(held(probe_file, base).len(), 1, "a pin holds one");
        fs::hard_link(
            case.path("root/f.abf15-fixture"),
            case.path("outside/alias.abf15-fixture"),
        )
        .unwrap();
        assert_eq!(pinned.promote().unwrap_err(), AccessError::MultipleLinks);
        fs::remove_file(case.path("outside/alias.abf15-fixture")).unwrap();
        assert_eq!(probe(probe_file), base, "after S0 refusal");
        eprintln!("ABF15-CLOSURE native scenarios: success, S0 refusal");
    }

    #[test]
    fn descriptors_close_exactly_in_an_isolated_process() {
        if std::env::var_os(CHILD_ENV).is_none() {
            let out = run_child(
                "closure::descriptors_close_exactly_in_an_isolated_process",
                "closure",
            );
            assert!(out.contains("ABF15-CLOSURE native scenarios"));
            #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
            assert!(out.contains("ABF15-CLOSURE harness scenarios"));
            return;
        }
        let case = Case::new("closure");
        case.file("root/f.abf15-fixture", "original");
        case.file("outside/probe.abf15-fixture", "probe");
        let probe_file = case.path("outside/probe.abf15-fixture");
        let root = case.authorize();
        let base = baseline(&probe_file);

        native_scenarios(&case, &root, &probe_file, &base);
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        super::scheduled::closure_harness_scenarios(&case, &root, &probe_file, &base);
        assert_eq!(probe(&probe_file), base, "final");
    }
}

// ---------------------------------------------------------------------------
// Scheduled and mocked (test harness: feature `fault-injection` +
// `--cfg abf_secure_fs_harness`)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
mod scheduled {
    use super::*;
    use abf_secure_fs::ReadableRegularFile;
    use abf_secure_fs::fault_injection::{
        self as fi, MAX_SYNTHETIC_READ, SchedulePoint, SelfLinkInjection, SyntheticReadError, errno,
    };
    use std::cell::Cell;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, ChildStdin, ChildStdout, Stdio};
    use std::rc::Rc;

    // Independent expected values from the installed headers
    // (asm-generic/fcntl.h, linux/openat2.h, asm-generic/errno*.h).
    const O_ACCMODE: i32 = 0o3;
    const O_RDONLY: u64 = 0;
    const O_CREAT: i32 = 0o100;
    const O_TRUNC: i32 = 0o1000;
    const O_APPEND: i32 = 0o2000;
    const O_NONBLOCK: u64 = 0o4000;
    const O_LARGEFILE: i32 = 0o100_000;
    const O_DIRECTORY: u64 = 0o200_000;
    const O_NOFOLLOW: u64 = 0o400_000;
    const O_CLOEXEC: u64 = 0o2_000_000;
    const O_PATH: u64 = 0o10_000_000;
    const FD_CLOEXEC: i32 = 1;
    const DIR_FLAGS: u64 = O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
    const LINK_FLAGS: u64 = O_PATH | O_NOFOLLOW | O_CLOEXEC;
    const READ_FLAGS: u64 = O_RDONLY | O_CLOEXEC | O_NONBLOCK;
    // RESOLVE_NO_XDEV 1, NO_MAGICLINKS 2, NO_SYMLINKS 4, BENEATH 8.
    const ANCHOR_MASK: u64 = 0x07;
    const PROC_MOUNT_MASK: u64 = 0x0E;
    const FULL_MASK: u64 = 0x0F;

    /// The exact `(flags, mode, resolve)` of the five promotion opens:
    /// `/`, `proc`, `thread-self`, `<pid>/task/<tid>/fd`, `<fd>` leaf.
    const PROMOTION_CALLS: [(u64, u64, u64); 5] = [
        (DIR_FLAGS, 0, ANCHOR_MASK),
        (DIR_FLAGS, 0, PROC_MOUNT_MASK),
        (LINK_FLAGS, 0, FULL_MASK),
        (DIR_FLAGS, 0, FULL_MASK),
        (READ_FLAGS, 0, 0),
    ];

    /// Resets thread-local instrumentation; on drop clears the hook and
    /// checks that every descriptor wrapped on this thread was closed.
    /// Declare it **first** so it drops after all capabilities.
    struct LeakCheck;

    impl LeakCheck {
        fn start() -> Self {
            fi::reset();
            Self
        }
    }

    impl Drop for LeakCheck {
        fn drop(&mut self) {
            fi::clear_schedule_hook();
            if !std::thread::panicking() {
                let counters = fi::fd_counters();
                assert_eq!(
                    counters.opened, counters.closed,
                    "descriptor leak: {counters:?}"
                );
            }
        }
    }

    type Action = Box<dyn FnOnce()>;

    /// Runs each action once, the first time its point is reached.
    fn schedule(actions: Vec<(SchedulePoint, Action)>) {
        let mut actions: Vec<(SchedulePoint, Option<Action>)> =
            actions.into_iter().map(|(p, a)| (p, Some(a))).collect();
        fi::set_schedule_hook(move |point| {
            for (at, action) in &mut actions {
                if *at == point
                    && let Some(action) = action.take()
                {
                    action();
                    return;
                }
            }
        });
    }

    fn at(point: SchedulePoint, action: impl FnOnce() + 'static) -> (SchedulePoint, Action) {
        (point, Box::new(action))
    }

    fn paths(case: &Case) -> impl Fn(&str) -> PathBuf + Clone + 'static {
        let base = case.dir.clone();
        move |relative: &str| base.join(relative)
    }

    /// Records that a schedule point was reached.
    fn reached(point: SchedulePoint) -> (Rc<Cell<bool>>, (SchedulePoint, Action)) {
        let flag = Rc::new(Cell::new(false));
        let set = Rc::clone(&flag);
        (flag, at(point, move || set.set(true)))
    }

    fn read_all(readable: &ReadableRegularFile) -> Vec<u8> {
        readable.harness_read_at(0, MAX_SYNTHETIC_READ).unwrap()
    }

    fn calls(log: &[fi::OpenAttempt]) -> Vec<(u64, u64, u64)> {
        log.iter().map(|a| (a.flags, a.mode, a.resolve)).collect()
    }

    /// Authorizes, pins `root/f.abf15-fixture` and clears the open log, so
    /// the log and the injection plans then cover the promotion only.
    fn pinned_case(name: &str) -> (Case, SecureRoot, PinnedRegularFile, u64) {
        let case = Case::new(name);
        let original = case.file("root/f.abf15-fixture", "original");
        let root = case.authorize();
        let pinned = pin(&case, &root, "f.abf15-fixture");
        fi::take_openat2_log();
        (case, root, pinned, original)
    }

    // -- native bytes, flags and offsets ---------------------------------------

    /// Native: bounded read returns exactly the original synthetic marker;
    /// short and past-EOF reads are short; range refusals read nothing.
    #[test]
    fn promoted_handle_reads_the_original_marker_only() {
        let _leaks = LeakCheck::start();
        let (_case, _root, pinned, _) = pinned_case("bytes");
        let readable = pinned.promote().unwrap();
        let expected = marker_bytes("original");
        assert_eq!(read_all(&readable), expected);
        assert_eq!(readable.harness_read_at(0, 5).unwrap(), &expected[..5]);
        assert_eq!(readable.harness_read_at(4, 3).unwrap(), &expected[4..7]);
        assert_eq!(
            readable.harness_read_at(expected.len() as u64, 16).unwrap(),
            b""
        );
        assert_eq!(fi::read_attempts(), 4);
        for (offset, limit) in [
            (0, 0),
            (0, MAX_SYNTHETIC_READ + 1),
            (u64::MAX, 1),
            (i64::MAX as u64, 1),
        ] {
            assert_eq!(
                readable.harness_read_at(offset, limit).unwrap_err(),
                SyntheticReadError::Range,
                "{offset} {limit}"
            );
        }
        assert_eq!(fi::read_attempts(), 4, "range refusals never read");
    }

    /// Native: rename the original away, replace the old name (regular,
    /// symlink, FIFO, socket) right before the terminal open. The bytes are
    /// the original marker; the replacement is never read or activated.
    #[test]
    fn replacement_before_the_open_never_supplies_bytes() {
        for kind in ["regular", "symlink", "fifo", "socket"] {
            let bytes = within_deadline(move || {
                let _leaks = LeakCheck::start();
                let (case, _root, pinned, _) = pinned_case(&format!("sched_replace_{kind}"));
                case.file("outside/canary.abf15-fixture", "outside-canary");
                let p = paths(&case);
                let listener: Rc<Cell<Option<UnixListener>>> = Rc::new(Cell::new(None));
                let keep = Rc::clone(&listener);
                schedule(vec![at(SchedulePoint::PromoteBeforeOpen, move || {
                    let old = p("root/f.abf15-fixture");
                    fs::rename(&old, p("outside/original.abf15-fixture")).unwrap();
                    match kind {
                        "regular" => fs::write(&old, marker_bytes("replacement")).unwrap(),
                        "symlink" => symlink(p("outside/canary.abf15-fixture"), &old).unwrap(),
                        "fifo" => mkfifo(&old),
                        _ => keep.set(Some(UnixListener::bind(&old).unwrap())),
                    }
                })]);
                let readable = pinned.promote().unwrap();
                let bytes = read_all(&readable);
                if let Some(listener) = listener.take() {
                    listener.set_nonblocking(true).unwrap();
                    assert!(listener.accept().is_err(), "nothing connected");
                }
                bytes
            });
            assert_eq!(bytes, marker_bytes("original"), "{kind}");
        }
    }

    /// Native flag log: the exact `open_how` of every promotion call, and
    /// the unchanged source-path masks of the pin before it.
    #[test]
    fn every_promotion_call_carries_exactly_its_reviewed_flags() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_flags");
        case.file("root/f.abf15-fixture", "original");
        let root = case.authorize();
        assert_eq!(
            calls(&fi::take_openat2_log()),
            vec![(DIR_FLAGS, 0, ANCHOR_MASK), (DIR_FLAGS, 0, FULL_MASK)]
        );
        let pinned = root.pin_regular(&rel("f.abf15-fixture")).unwrap();
        assert_eq!(
            calls(&fi::take_openat2_log()),
            vec![
                (DIR_FLAGS, 0, FULL_MASK),
                (LINK_FLAGS, 0, FULL_MASK),
                (LINK_FLAGS, 0, FULL_MASK),
                (DIR_FLAGS, 0, FULL_MASK),
            ],
            "source-path masks unchanged"
        );
        let readable = pinned.promote().unwrap();
        let log = fi::take_openat2_log();
        assert_eq!(calls(&log), PROMOTION_CALLS.to_vec());
        assert!(log.iter().all(|a| a.injected_errno.is_none()));
        assert_eq!(fi::self_link_reads(), 1);
        // Reinspection and reads never open anything.
        readable.reinspect().unwrap();
        read_all(&readable);
        assert!(fi::take_openat2_log().is_empty());
    }

    /// Native fcntl: the new descriptor is O_RDONLY|O_NONBLOCK with
    /// FD_CLOEXEC and no O_PATH/write/append/create/truncate bit; the
    /// retained original is still O_PATH with FD_CLOEXEC.
    #[test]
    fn kernel_reports_the_expected_descriptor_flags() {
        let _leaks = LeakCheck::start();
        let (_case, _root, pinned, _) = pinned_case("sched_fcntl");
        let readable = pinned.promote().unwrap();
        let status = readable.harness_descriptor_status();
        eprintln!(
            "ABF15-FCNTL readable F_GETFL={:#o} F_GETFD={} original F_GETFL={:#o} F_GETFD={}",
            status.readable_status_flags,
            status.readable_fd_flags,
            status.original_status_flags,
            status.original_fd_flags
        );
        let readable_flags = status.readable_status_flags;
        assert_eq!(readable_flags & O_ACCMODE, O_RDONLY as i32);
        assert_eq!(
            readable_flags & !(O_LARGEFILE | O_CLOEXEC as i32),
            O_NONBLOCK as i32,
            "only O_NONBLOCK (plus kernel O_LARGEFILE)"
        );
        assert_eq!(
            readable_flags & (O_PATH as i32 | O_APPEND | O_CREAT | O_TRUNC),
            0
        );
        assert_eq!(status.readable_fd_flags & FD_CLOEXEC, FD_CLOEXEC);
        assert_ne!(status.original_status_flags & O_PATH as i32, 0);
        assert_eq!(status.original_status_flags & O_NONBLOCK as i32, 0);
        assert_eq!(status.original_fd_flags & FD_CLOEXEC, FD_CLOEXEC);
    }

    /// Native: two promotions of separately pinned handles of one object are
    /// distinct open descriptions (seeking one does not move the other), and
    /// the bounded positioned read does not move the offset.
    #[test]
    fn separate_promotions_have_independent_offsets() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_offsets");
        case.file("root/f.abf15-fixture", "original");
        let root = case.authorize();
        let a = pin(&case, &root, "f.abf15-fixture").promote().unwrap();
        let b = pin(&case, &root, "f.abf15-fixture").promote().unwrap();
        assert_eq!((a.harness_offset(), b.harness_offset()), (0, 0));
        assert_eq!(a.harness_seek(7), 7);
        assert_eq!((a.harness_offset(), b.harness_offset()), (7, 0));
        assert_eq!(read_all(&a), marker_bytes("original"));
        assert_eq!(a.harness_offset(), 7, "positioned read keeps the offset");
        assert_eq!(b.harness_seek(3), 3);
        assert_eq!((a.harness_offset(), b.harness_offset()), (7, 3));
    }

    // -- scheduled link-count samples -------------------------------------------

    fn link_alias(p: &impl Fn(&str) -> PathBuf) {
        fs::hard_link(p("root/f.abf15-fixture"), p("outside/alias.abf15-fixture")).unwrap();
    }

    fn unlink_alias(p: &impl Fn(&str) -> PathBuf) {
        fs::remove_file(p("outside/alias.abf15-fixture")).unwrap();
    }

    /// Scheduled: a second link added after S0 (before or right after the
    /// open) is refused by S1, before any read; one added after S1 is
    /// refused by S2.
    #[test]
    fn second_link_is_refused_by_the_first_later_sample() {
        let cases = [
            (SchedulePoint::PromoteAfterS0, SchedulePoint::PromoteAfterS1),
            (
                SchedulePoint::PromoteBeforeOpen,
                SchedulePoint::PromoteAfterS1,
            ),
            (
                SchedulePoint::PromoteAfterOpen,
                SchedulePoint::PromoteAfterS1,
            ),
            (SchedulePoint::PromoteAfterS1, SchedulePoint::PromoteAfterS2),
        ];
        for (link_at, unreached) in cases {
            let _leaks = LeakCheck::start();
            let (case, _root, pinned, _) = pinned_case("sched_link_later");
            let p = paths(&case);
            let (after, mark) = reached(unreached);
            schedule(vec![at(link_at, move || link_alias(&p)), mark]);
            assert_eq!(
                pinned.promote().unwrap_err(),
                AccessError::MultipleLinks,
                "{link_at:?}"
            );
            assert!(
                !after.get(),
                "{link_at:?}: refusal must precede {unreached:?}"
            );
        }
    }

    /// Scheduled (ABF11C F04 lesson): a link present only at S1 and removed
    /// at the next point is still refused at S1; the removal is never
    /// reached.
    #[test]
    fn link_seen_only_by_s1_is_refused() {
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, original) = pinned_case("sched_link_s1_only");
        let p1 = paths(&case);
        let p2 = paths(&case);
        let removal = Rc::new(Cell::new(false));
        let removed = Rc::clone(&removal);
        schedule(vec![
            at(SchedulePoint::PromoteAfterOpen, move || link_alias(&p1)),
            at(SchedulePoint::PromoteAfterS1, move || {
                removed.set(true);
                unlink_alias(&p2);
            }),
        ]);
        assert_eq!(pinned.promote().unwrap_err(), AccessError::MultipleLinks);
        assert!(!removal.get());
        assert_eq!(ino(&case.path("outside/alias.abf15-fixture")), original);
    }

    /// Limitation, demonstrated: a link added and removed entirely between
    /// two samples, or added after S2, is not observed by promotion.
    #[test]
    fn transient_or_late_links_are_not_observed_by_promotion() {
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, original) = pinned_case("sched_link_transient");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PromoteAfterS0, move || {
            link_alias(&p);
            unlink_alias(&p);
        })]);
        let readable = pinned.promote().unwrap();
        assert_eq!(readable.evidence().object_id().inode(), original);
        drop(readable);

        let (case, _root, pinned, _) = pinned_case("sched_link_after_s2");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PromoteAfterS2, move || {
            link_alias(&p)
        })]);
        let readable = pinned.promote().unwrap();
        // Disclosed race: the later reinspection and the read refuse.
        assert_eq!(
            readable.reinspect().unwrap_err(),
            AccessError::MultipleLinks
        );
        assert_eq!(
            readable.harness_read_at(0, 16).unwrap_err(),
            SyntheticReadError::Access(AccessError::MultipleLinks)
        );
    }

    /// Scheduled unlink: right before the open, the readable open succeeds on
    /// the deleted original inode and S1 observes nlink 0; after S1, S2
    /// refuses; after S2 the race is disclosed and later checks refuse.
    #[test]
    fn last_unlink_during_promotion() {
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, _) = pinned_case("sched_unlink_open");
        let p = paths(&case);
        let (opened, mark) = reached(SchedulePoint::PromoteAfterOpen);
        schedule(vec![
            at(SchedulePoint::PromoteBeforeOpen, move || {
                fs::remove_file(p("root/f.abf15-fixture")).unwrap();
            }),
            mark,
        ]);
        assert_eq!(pinned.promote().unwrap_err(), AccessError::ObjectChanged);
        assert!(
            opened.get(),
            "the internal open of the deleted inode succeeded"
        );

        let (case, _root, pinned, _) = pinned_case("sched_unlink_s1");
        let p = paths(&case);
        let (after, mark) = reached(SchedulePoint::PromoteAfterS2);
        schedule(vec![
            at(SchedulePoint::PromoteAfterS1, move || {
                fs::remove_file(p("root/f.abf15-fixture")).unwrap();
            }),
            mark,
        ]);
        assert_eq!(pinned.promote().unwrap_err(), AccessError::ObjectChanged);
        assert!(!after.get());

        let (case, _root, pinned, _) = pinned_case("sched_unlink_s2");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PromoteAfterS2, move || {
            fs::remove_file(p("root/f.abf15-fixture")).unwrap();
        })]);
        let readable = pinned.promote().unwrap();
        assert_eq!(
            readable.reinspect().unwrap_err(),
            AccessError::ObjectChanged
        );
        assert_eq!(
            readable.harness_read_at(0, 16).unwrap_err(),
            SyntheticReadError::Access(AccessError::ObjectChanged)
        );
    }

    /// Native S0 refusals happen before any procfs open.
    #[test]
    fn s0_refusals_open_nothing() {
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, _) = pinned_case("sched_s0_nothing");
        fs::hard_link(
            case.path("root/f.abf15-fixture"),
            case.path("outside/alias.abf15-fixture"),
        )
        .unwrap();
        assert_eq!(pinned.promote().unwrap_err(), AccessError::MultipleLinks);
        assert!(fi::take_openat2_log().is_empty());
        assert_eq!(fi::self_link_reads(), 0);
    }

    // -- the synthetic read helper ----------------------------------------------

    /// Scheduled: a link added between the read and its post-check withholds
    /// the staged bytes; a pre-check refusal never reads.
    #[test]
    fn read_helper_returns_bytes_only_after_its_post_check() {
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, _) = pinned_case("sched_read_post");
        let readable = pinned.promote().unwrap();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::ReadAfterRead, move || {
            link_alias(&p)
        })]);
        assert_eq!(
            readable.harness_read_at(0, 64).unwrap_err(),
            SyntheticReadError::Access(AccessError::MultipleLinks)
        );
        assert_eq!(fi::read_attempts(), 1, "the read happened; bytes withheld");

        // Pre-check refusal (link still present): no read attempt.
        assert_eq!(
            readable.harness_read_at(0, 64).unwrap_err(),
            SyntheticReadError::Access(AccessError::MultipleLinks)
        );
        assert_eq!(fi::read_attempts(), 1);
        unlink_alias(&paths(&case));
        assert_eq!(read_all(&readable), marker_bytes("original"));
    }

    /// Mocked read errors: EINTR is retried at most twice; others are coded.
    #[test]
    fn read_helper_retries_are_bounded() {
        let _leaks = LeakCheck::start();
        let (_case, _root, pinned, _) = pinned_case("inj_read");
        let readable = pinned.promote().unwrap();
        fi::inject_read(&[Some(errno::EINTR), Some(errno::EINTR)]);
        assert_eq!(read_all(&readable), marker_bytes("original"));
        assert_eq!(fi::read_attempts(), 3);

        // Attempt counts below are deltas: a reset here would also reset the
        // descriptor counters of the live capability.
        fi::inject_read(&[
            Some(errno::EINTR),
            Some(errno::EINTR),
            Some(errno::EINTR),
            Some(errno::EIO),
        ]);
        assert_eq!(
            readable.harness_read_at(0, 8).unwrap_err(),
            SyntheticReadError::Access(AccessError::RetryExhausted)
        );
        assert_eq!(fi::read_attempts(), 6, "fourth entry unconsumed");

        fi::inject_read(&[Some(errno::EIO)]);
        assert_eq!(
            readable.harness_read_at(0, 8).unwrap_err(),
            SyntheticReadError::Access(AccessError::UnexpectedOs(errno::EIO))
        );
        assert_eq!(fi::read_attempts(), 7);
    }

    // -- mocked identity and procfs failures ------------------------------------

    /// Mocked identity: inspection order of a promotion is [S0, S1, S2].
    #[test]
    fn injected_identity_mismatch_or_zero_refuses() {
        for step in 0..3 {
            for (forged, expected) in [
                (None, AccessError::ObjectChanged),
                (Some((0, 0)), AccessError::IdentityUnavailable),
            ] {
                let _leaks = LeakCheck::start();
                let (case, _root, pinned, original) = pinned_case("inj_identity");
                let dev = fs::symlink_metadata(case.path("root/f.abf15-fixture"))
                    .unwrap()
                    .dev();
                let identity = forged.unwrap_or((dev, original + 1));
                let mut plan = vec![None; step];
                plan.push(Some(identity));
                fi::inject_identity(&plan);
                assert_eq!(
                    pinned.promote().unwrap_err(),
                    expected,
                    "step {step} {identity:?}"
                );
                let opened_leaf = calls(&fi::take_openat2_log()).len() == 5;
                assert_eq!(opened_leaf, step > 0, "S0 refusal opens nothing");
            }
        }
    }

    /// Mocked procfs magic: a non-proc filesystem is refused after the
    /// `proc` open, with no further open.
    #[test]
    fn non_procfs_magic_is_refused() {
        for magic in [0xEF53_i64, 0x0102_1994, 0] {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_proc_magic");
            fi::inject_filesystem_magic(Some(magic));
            assert_eq!(
                pinned.promote().unwrap_err(),
                AccessError::SecurePrimitiveUnavailable,
                "{magic:#x}"
            );
            assert_eq!(
                calls(&fi::take_openat2_log()),
                PROMOTION_CALLS[..2].to_vec()
            );
            assert_eq!(fi::self_link_reads(), 0);
        }
    }

    /// Mocked errno at each promotion open: coded, no extra attempt, no
    /// fallback, identical flags, every acquired descriptor closed.
    #[test]
    fn injected_open_failures_at_each_step_fail_closed() {
        let codes = [
            (errno::ENOSYS, [AccessError::SecurePrimitiveUnavailable; 5]),
            (errno::EPERM, [AccessError::SecurePrimitiveUnavailable; 5]),
            (
                errno::ENOENT,
                [
                    AccessError::NotFound,
                    AccessError::SecurePrimitiveUnavailable,
                    AccessError::SecurePrimitiveUnavailable,
                    AccessError::SecurePrimitiveUnavailable,
                    AccessError::SecurePrimitiveUnavailable,
                ],
            ),
            (errno::EACCES, [AccessError::Inaccessible; 5]),
            (errno::ELOOP, [AccessError::ConfinementRefused; 5]),
            (errno::EMFILE, [AccessError::UnexpectedOs(errno::EMFILE); 5]),
        ];
        for (code, expected) in codes {
            for step in 0..5 {
                let _leaks = LeakCheck::start();
                let (_case, _root, pinned, _) = pinned_case("inj_open_step");
                let mut plan = vec![None; step];
                plan.push(Some(code));
                fi::inject_openat2(&plan);
                assert_eq!(
                    pinned.promote().unwrap_err(),
                    expected[step],
                    "errno {code} step {step}"
                );
                let log = fi::take_openat2_log();
                assert_eq!(calls(&log), PROMOTION_CALLS[..=step].to_vec());
                assert_eq!(fi::remaining_injections(), 0);
            }
        }
    }

    /// Mocked retries: proc opens retry EAGAIN/EINTR at most twice; the
    /// terminal open retries only EINTR, and EAGAIN/EWOULDBLOCK is terminal
    /// after exactly one attempt. A sentinel entry proves the bound.
    #[test]
    fn retries_are_bounded_and_terminal_eagain_is_never_retried() {
        let sentinel = Some(errno::EINVAL);
        // Terminal EAGAIN: one attempt, sentinel unconsumed.
        {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_leaf_eagain");
            fi::inject_openat2(&[None, None, None, None, Some(errno::EWOULDBLOCK), sentinel]);
            assert_eq!(
                pinned.promote().unwrap_err(),
                AccessError::PromotionWouldBlock
            );
            assert_eq!(fi::remaining_injections(), 1);
            assert_eq!(calls(&fi::take_openat2_log()), PROMOTION_CALLS.to_vec());
        }
        // Terminal EINTR: three identical attempts, then exhaustion.
        {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_leaf_eintr");
            let eintr = Some(errno::EINTR);
            fi::inject_openat2(&[None, None, None, None, eintr, eintr, eintr, sentinel]);
            assert_eq!(pinned.promote().unwrap_err(), AccessError::RetryExhausted);
            assert_eq!(fi::remaining_injections(), 1);
            let log = calls(&fi::take_openat2_log());
            assert_eq!(log.len(), 7);
            assert!(log[4..].iter().all(|c| *c == PROMOTION_CALLS[4]));
        }
        // Terminal EINTR within the bound recovers.
        {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_leaf_recover");
            let eintr = Some(errno::EINTR);
            fi::inject_openat2(&[None, None, None, None, eintr, eintr]);
            let readable = pinned.promote().unwrap();
            assert_eq!(read_all(&readable), marker_bytes("original"));
        }
        // Each proc open: three transient attempts, then exhaustion.
        for step in 0..4 {
            for transient in [errno::EAGAIN, errno::EINTR] {
                let _leaks = LeakCheck::start();
                let (_case, _root, pinned, _) = pinned_case("inj_proc_retry");
                let mut plan = vec![None; step];
                plan.extend([Some(transient); 3]);
                plan.push(sentinel);
                fi::inject_openat2(&plan);
                assert_eq!(
                    pinned.promote().unwrap_err(),
                    AccessError::RetryExhausted,
                    "step {step} errno {transient}"
                );
                assert_eq!(fi::remaining_injections(), 1);
                let log = calls(&fi::take_openat2_log());
                assert_eq!(log.len(), step + 3);
                assert!(log[step..].iter().all(|c| *c == PROMOTION_CALLS[step]));
            }
        }
    }

    /// Mocked self link: errno, malformed, empty and possibly truncated
    /// payloads fail closed before the task fd directory is opened; EINTR is
    /// retried at most twice.
    #[test]
    fn injected_self_link_failures_fail_closed() {
        use SelfLinkInjection::{Errno, Payload};
        // Malformed payloads are built around this process's own PID (whose
        // main-thread task shares this descriptor table), so even a
        // defective lenient parser could only reach this process's own
        // entries, never another process's.
        let pid = std::process::id();
        let spelled = |text: String| Payload(text.into_bytes());
        let full = {
            let mut bytes = format!("{pid}/task/{pid}").into_bytes();
            bytes.resize(32, b'1');
            bytes
        };
        let spu = AccessError::SecurePrimitiveUnavailable;
        let cases: Vec<(SelfLinkInjection, AccessError)> = vec![
            (Errno(errno::ENOSYS), spu),
            (Errno(errno::ENOENT), spu),
            (Errno(errno::EACCES), AccessError::Inaccessible),
            (
                Errno(errno::EAGAIN),
                AccessError::UnexpectedOs(errno::EAGAIN),
            ),
            (Payload(Vec::new()), spu),
            (Payload(b"self".to_vec()), spu),
            (spelled(format!("0{pid}/task/{pid}")), spu),
            (spelled(format!("{pid}/task/0{pid}")), spu),
            (spelled(format!("{pid}/task/{pid}/fd")), spu),
            (spelled(format!("{pid}/task/{pid}\n")), spu),
            (spelled(format!("+{pid}/task/{pid}")), spu),
            (spelled(format!("../{pid}/task/{pid}")), spu),
            (spelled(format!("{pid}/task/0")), spu),
            (spelled(format!("2147483648/task/{pid}")), spu),
            (Payload(full.clone()), spu),
            (Payload(full.repeat(2)), spu),
        ];
        for (injection, expected) in cases {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_self_link");
            fi::inject_self_link(&[Some(injection.clone())]);
            assert_eq!(pinned.promote().unwrap_err(), expected, "{injection:?}");
            assert_eq!(
                calls(&fi::take_openat2_log()),
                PROMOTION_CALLS[..3].to_vec()
            );
            assert_eq!(fi::self_link_reads(), 1);
        }

        // EINTR bound: three attempts; a planned fourth (EACCES) would turn
        // the result into Inaccessible if it were consumed.
        let eintr = Some(Errno(errno::EINTR));
        {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_self_link_eintr");
            fi::inject_self_link(&[
                eintr.clone(),
                eintr.clone(),
                eintr.clone(),
                Some(Errno(errno::EACCES)),
            ]);
            assert_eq!(pinned.promote().unwrap_err(), AccessError::RetryExhausted);
            assert_eq!(fi::self_link_reads(), 3);
        }
        {
            let _leaks = LeakCheck::start();
            let (_case, _root, pinned, _) = pinned_case("inj_self_link_recover");
            fi::inject_self_link(&[eintr.clone(), eintr]);
            assert!(pinned.promote().is_ok());
            assert_eq!(fi::self_link_reads(), 3);
        }
    }

    // -- unwinding ---------------------------------------------------------------

    /// Scheduled panic after the readable open: unwinding closes everything
    /// (counter evidence here; real descriptor numbers in the isolated
    /// closure test).
    #[test]
    fn panic_during_promotion_closes_every_descriptor() {
        for point in [
            SchedulePoint::PromoteAfterS0,
            SchedulePoint::PromoteBeforeOpen,
            SchedulePoint::PromoteAfterOpen,
            SchedulePoint::PromoteAfterS1,
            SchedulePoint::PromoteAfterS2,
        ] {
            fi::reset();
            {
                let (_case, _root, pinned, _) = pinned_case("sched_panic");
                fi::set_schedule_hook(move |p| {
                    if p == point {
                        panic!("ABF15 scheduled panic");
                    }
                });
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pinned.promote()));
                assert!(result.is_err(), "{point:?}");
                fi::clear_schedule_hook();
            }
            let counters = fi::fd_counters();
            assert_eq!(counters.opened, counters.closed, "{point:?}: {counters:?}");
        }
        fi::reset();
    }

    /// Harness scenarios of the isolated closure test (child process).
    pub(super) fn closure_harness_scenarios(
        case: &Case,
        root: &SecureRoot,
        probe_file: &Path,
        base: &[i32],
    ) {
        use super::closure::{held, probe};
        fi::reset();

        // S1 refusal after the readable open.
        let pinned = pin(case, root, "f.abf15-fixture");
        let p = paths(case);
        schedule(vec![at(SchedulePoint::PromoteAfterOpen, move || {
            link_alias(&p)
        })]);
        assert_eq!(pinned.promote().unwrap_err(), AccessError::MultipleLinks);
        fi::clear_schedule_hook();
        unlink_alias(&paths(case));
        assert_eq!(probe(probe_file), base, "after S1 refusal");

        // Panic at every promotion point.
        for point in [
            SchedulePoint::PromoteAfterS0,
            SchedulePoint::PromoteBeforeOpen,
            SchedulePoint::PromoteAfterOpen,
            SchedulePoint::PromoteAfterS1,
            SchedulePoint::PromoteAfterS2,
        ] {
            let pinned = pin(case, root, "f.abf15-fixture");
            fi::set_schedule_hook(move |p| {
                if p == point {
                    panic!("ABF15 scheduled panic");
                }
            });
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pinned.promote()));
            assert!(result.is_err());
            fi::clear_schedule_hook();
            assert_eq!(probe(probe_file), base, "after panic at {point:?}");
        }

        // Injected failure at each of the five opens.
        for step in 0..5 {
            let pinned = pin(case, root, "f.abf15-fixture");
            let mut plan = vec![None; step];
            plan.push(Some(errno::EACCES));
            fi::inject_openat2(&plan);
            assert_eq!(pinned.promote().unwrap_err(), AccessError::Inaccessible);
            assert_eq!(probe(probe_file), base, "after injected failure {step}");
        }

        // Read post-check refusal keeps exactly two until drop.
        let readable = pin(case, root, "f.abf15-fixture").promote().unwrap();
        let p = paths(case);
        schedule(vec![at(SchedulePoint::ReadAfterRead, move || {
            link_alias(&p)
        })]);
        assert!(readable.harness_read_at(0, 16).is_err());
        fi::clear_schedule_hook();
        unlink_alias(&paths(case));
        assert_eq!(held(probe_file, base).len(), 2);
        drop(readable);
        assert_eq!(probe(probe_file), base, "after read refusal");
        fi::reset();
        eprintln!("ABF15-CLOSURE harness scenarios: S1 refusal, 5 panics, 5 open failures, read");
    }

    // -- separate-process ordinary writer -----------------------------------------

    struct Writer {
        child: Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    }

    impl Writer {
        fn spawn() -> Self {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "scheduled::separate_process_unlocked_writer_coexists",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD_ENV, "writer")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdin = child.stdin.take().unwrap();
            let stdout = BufReader::new(child.stdout.take().unwrap());
            let mut writer = Self {
                child,
                stdin,
                stdout,
            };
            assert_eq!(writer.reply(), "ready");
            writer
        }

        fn reply(&mut self) -> String {
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(self.stdout.read_line(&mut line).unwrap(), 0, "writer died");
                if let Some(start) = line.find("<<W ") {
                    let rest = &line[start + 4..];
                    return rest[..rest.find(">>").unwrap()].to_owned();
                }
            }
        }

        fn ask(&mut self, command: &str) -> String {
            writeln!(self.stdin, "{command}").unwrap();
            self.stdin.flush().unwrap();
            self.reply()
        }

        fn finish(mut self) {
            assert_eq!(self.ask("exit"), "bye");
            let status = self.child.wait().unwrap();
            assert!(status.success(), "writer process failed");
        }
    }

    /// Child side: an ordinary O_WRONLY|O_APPEND writer, no locks, driven
    /// line by line over stdin. Every command must succeed.
    fn writer_child() {
        use std::io::{Seek, stdin};
        let mut file: Option<fs::File> = None;
        let say = |text: String| println!("<<W {text}>>");
        say("ready".to_owned());
        for line in stdin().lock().lines() {
            let line = line.unwrap();
            let mut parts = line.splitn(3, ' ');
            let reply = match (parts.next().unwrap(), parts.next(), parts.next()) {
                ("open", Some(path), None) => {
                    file = Some(fs::OpenOptions::new().append(true).open(path).unwrap());
                    "ok".to_owned()
                }
                ("append", Some(text), None) => {
                    file.as_mut().unwrap().write_all(text.as_bytes()).unwrap();
                    "ok".to_owned()
                }
                ("offset", None, None) => {
                    format!("{}", file.as_mut().unwrap().stream_position().unwrap())
                }
                ("try-lock", None, None) => {
                    let f = file.as_ref().unwrap();
                    f.try_lock().unwrap();
                    f.unlock().unwrap();
                    "ok".to_owned()
                }
                ("truncate", None, None) => {
                    file.as_ref().unwrap().set_len(0).unwrap();
                    "ok".to_owned()
                }
                ("rename", Some(from), Some(to)) => {
                    fs::rename(from, to).unwrap();
                    "ok".to_owned()
                }
                ("unlink", Some(path), None) => {
                    fs::remove_file(path).unwrap();
                    "ok".to_owned()
                }
                ("exit", None, None) => {
                    say("bye".to_owned());
                    return;
                }
                _ => panic!("unknown writer command"),
            };
            say(reply);
        }
    }

    /// Native, separate processes: an ordinary unlocked writer keeps
    /// appending, truncating, renaming and unlinking the promoted object.
    /// The collector holds no lock (the writer can take and release an
    /// exclusive `flock`), does not move the writer's offset, reads the
    /// original object's current bytes at controlled barriers, and refuses
    /// once the policy fails. This is coexistence evidence for ordinary
    /// unlocked writers only (no lease or POSIX-lock holder is tested).
    #[test]
    fn separate_process_unlocked_writer_coexists() {
        if std::env::var_os(CHILD_ENV).is_some_and(|mode| mode == "writer") {
            writer_child();
            return;
        }
        let _leaks = LeakCheck::start();
        let (case, _root, pinned, original) = pinned_case("writer");
        let file = case.path("root/f.abf15-fixture");
        let moved = case.path("outside/renamed.abf15-fixture");
        let mut writer = Writer::spawn();
        assert_eq!(writer.ask(&format!("open {}", file.display())), "ok");

        let readable = pinned.promote().unwrap();
        assert_eq!(writer.ask("try-lock"), "ok", "collector holds no flock");
        assert_eq!(read_all(&readable), marker_bytes("original"));

        assert_eq!(writer.ask("append w1;"), "ok");
        let mut expected = marker_bytes("original");
        expected.extend_from_slice(b"w1;");
        assert_eq!(read_all(&readable), expected);
        assert_eq!(writer.ask("offset"), expected.len().to_string());

        // Moving the readable offset never moves the writer's.
        readable.harness_seek(3);
        assert_eq!(writer.ask("append w2;"), "ok");
        expected.extend_from_slice(b"w2;");
        assert_eq!(writer.ask("offset"), expected.len().to_string());
        assert_eq!(readable.harness_offset(), 3);
        assert_eq!(read_all(&readable), expected);

        // Truncation and rewrite: same object, new bytes (no snapshot).
        assert_eq!(writer.ask("truncate"), "ok");
        assert_eq!(read_all(&readable), b"");
        assert_eq!(writer.ask("append w3;"), "ok");
        assert_eq!(read_all(&readable), b"w3;");

        // Rename by the writer: still the same single-link object.
        assert_eq!(
            writer.ask(&format!("rename {} {}", file.display(), moved.display())),
            "ok"
        );
        assert_eq!(readable.reinspect().unwrap().object_id().inode(), original);
        assert_eq!(read_all(&readable), b"w3;");

        // Unlink by the writer: the policy refuses; no bytes.
        assert_eq!(writer.ask(&format!("unlink {}", moved.display())), "ok");
        assert_eq!(
            readable.harness_read_at(0, 16).unwrap_err(),
            SyntheticReadError::Access(AccessError::ObjectChanged)
        );
        writer.finish();
    }
}
