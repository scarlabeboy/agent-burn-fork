//! Native Linux pinning tests against the real kernel, on synthetic fixtures.
//!
//! Fixtures are created **only** beneath the directory named by
//! `ABF_SECURE_FS_FIXTURE_DIR`. It must be set explicitly; there is no
//! fallback to a temp directory. Each test owns one fresh case directory
//! `<fixture>/<test>-<pid>` with an authorized `root/` and a sibling
//! `outside/` holding canary objects, and removes it when done.
//!
//! Every result is classified as the original pinned object, an outside
//! canary (always a failure), another object, or a coded refusal. An
//! environment that cannot admit the fixture root fails loudly as
//! "ENVIRONMENT UNSUPPORTED"; it never passes silently.

#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]

use abf_secure_fs::{AccessError, ObjectKind, PinnedRegularFile, RelativePath, SecureRoot};
use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const FIXTURE_ENV: &str = "ABF_SECURE_FS_FIXTURE_DIR";
const FOREIGN_MOUNT_ENV: &str = "ABF_SECURE_FS_FOREIGN_MOUNT_DIR";
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
        let dir = base.join(format!("{name}-{}", std::process::id()));
        fs::create_dir(&dir).expect("case directory must be fresh");
        fs::create_dir(dir.join("root")).unwrap();
        fs::create_dir(dir.join("outside")).unwrap();
        Self { dir }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn root_text(&self) -> String {
        self.path("root").to_str().unwrap().to_owned()
    }

    /// Authorizes `root/`. An unsupported environment is a loud failure.
    fn authorize(&self) -> SecureRoot {
        match SecureRoot::authorize(&self.root_text()) {
            Ok(root) => root,
            Err(
                error @ (AccessError::SecurePrimitiveUnavailable
                | AccessError::UnsupportedFilesystem),
            ) => panic!("ENVIRONMENT UNSUPPORTED ({error:?}): this is not a pass"),
            Err(error) => panic!("fixture root authorization failed: {error:?}"),
        }
    }

    /// Creates a regular file with a synthetic marker and returns its inode.
    fn file(&self, relative: &str, marker: &str) -> u64 {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, format!("abf11-synthetic {marker}\n")).unwrap();
        ino(&path)
    }

    fn dir(&self, relative: &str) {
        fs::create_dir_all(self.path(relative)).unwrap();
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

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Original,
    OutsideCanary,
    OtherObject,
    Refused(AccessError),
}

fn classify(
    result: &Result<PinnedRegularFile, AccessError>,
    original: u64,
    canary: u64,
) -> Outcome {
    match result {
        Ok(pinned) => {
            let inode = pinned.evidence().object_id().inode();
            if inode == canary {
                Outcome::OutsideCanary
            } else if inode == original {
                Outcome::Original
            } else {
                Outcome::OtherObject
            }
        }
        Err(error) => Outcome::Refused(*error),
    }
}

/// Runs `f` on a fresh thread with a deadline, so a blocking open (FIFO)
/// would fail the test instead of hanging it.
fn within_deadline<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(DEADLINE)
        .expect("operation did not finish within the deadline (blocking open?)")
}

// ---------------------------------------------------------------------------
// 1. Valid ordinary pin
// ---------------------------------------------------------------------------

#[test]
fn valid_pin_reports_handle_derived_identity() {
    let case = Case::new("valid_pin");
    let original = case.file("root/a/b/f.abf-fixture", "original");
    let root = case.authorize();

    let root_meta = fs::symlink_metadata(case.path("root")).unwrap();
    assert_eq!(root.object_id().inode(), root_meta.ino());
    assert_eq!(root.object_id().device(), root_meta.dev());
    root.check_binding().unwrap();

    let pinned = root.pin_regular(&rel("a/b/f.abf-fixture")).unwrap();
    let meta = fs::symlink_metadata(case.path("root/a/b/f.abf-fixture")).unwrap();
    let evidence = pinned.evidence();
    assert_eq!(evidence.object_id().inode(), original);
    assert_eq!(evidence.object_id().device(), meta.dev());
    assert_ne!(evidence.object_id().device(), 0);
    assert_eq!(evidence.link_count(), 1);
    assert_eq!(evidence.size(), meta.len());
    assert_eq!(pinned.reinspect().unwrap(), *evidence);

    // Pinning the same path again yields the same object.
    let again = root.pin_regular(&rel("a/b/f.abf-fixture")).unwrap();
    assert_eq!(again.evidence().object_id(), evidence.object_id());

    // Debug never prints paths.
    let debug = format!("{root:?} {pinned:?}");
    assert!(!debug.contains("abf-fixture") && !debug.contains("/root"));
}

// ---------------------------------------------------------------------------
// 3. Directory, symlink and hardlink rejection
// ---------------------------------------------------------------------------

#[test]
fn directory_as_file_is_refused() {
    let case = Case::new("dir_as_file");
    case.dir("root/a/sub");
    let root = case.authorize();
    assert_eq!(
        root.pin_regular(&rel("a")).unwrap_err(),
        AccessError::NotRegularFile(ObjectKind::Directory)
    );
    assert_eq!(
        root.pin_regular(&rel("a/sub")).unwrap_err(),
        AccessError::NotRegularFile(ObjectKind::Directory)
    );
}

/// O_PATH|O_NOFOLLOW returns a handle to a final symlink even with
/// RESOLVE_NO_SYMLINKS. The handle type check must refuse it, whatever the
/// target is.
#[test]
fn final_symlinks_are_refused_by_handle_type() {
    let case = Case::new("final_symlink");
    let canary = case.file("outside/canary.abf-fixture", "outside-canary");
    let inside = case.file("root/inside.abf-fixture", "inside");
    symlink(
        case.path("outside/canary.abf-fixture"),
        case.path("root/abs"),
    )
    .unwrap();
    symlink("../outside/canary.abf-fixture", case.path("root/rel_out")).unwrap();
    symlink("inside.abf-fixture", case.path("root/rel_in")).unwrap();
    symlink("missing", case.path("root/dangling")).unwrap();
    let root = case.authorize();
    for name in ["abs", "rel_out", "rel_in", "dangling"] {
        let result = root.pin_regular(&rel(name));
        assert_eq!(
            classify(&result, inside, canary),
            Outcome::Refused(AccessError::NotRegularFile(ObjectKind::Symlink)),
            "{name}"
        );
    }
}

/// Intermediate symlinks are refused by RESOLVE_NO_SYMLINKS, including one
/// that stays inside the root (which BENEATH alone would allow).
#[test]
fn intermediate_symlinks_are_refused() {
    let case = Case::new("intermediate_symlink");
    let canary = case.file("outside/d/f.abf-fixture", "outside-canary");
    let original = case.file("root/a/f.abf-fixture", "original");
    symlink(case.path("outside/d"), case.path("root/abs")).unwrap();
    symlink("../outside/d", case.path("root/rel_out")).unwrap();
    symlink("a", case.path("root/rel_in")).unwrap();
    let root = case.authorize();
    for name in ["abs", "rel_out", "rel_in"] {
        let result = root.pin_regular(&rel(&format!("{name}/f.abf-fixture")));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::ConfinementRefused),
            "{name}"
        );
    }
}

#[test]
fn hardlinks_are_refused_inside_and_across_the_boundary() {
    let case = Case::new("hardlinks");
    let canary = case.file("outside/canary.abf-fixture", "outside-canary");
    let original = case.file("root/one.abf-fixture", "inside");
    fs::hard_link(
        case.path("root/one.abf-fixture"),
        case.path("root/two.abf-fixture"),
    )
    .unwrap();
    fs::hard_link(
        case.path("outside/canary.abf-fixture"),
        case.path("root/alias.abf-fixture"),
    )
    .unwrap();
    let root = case.authorize();
    for name in ["one.abf-fixture", "two.abf-fixture", "alias.abf-fixture"] {
        let result = root.pin_regular(&rel(name));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::MultipleLinks),
            "{name}"
        );
    }
    // Back to one link: accepted again (sampled policy, not history).
    fs::remove_file(case.path("root/two.abf-fixture")).unwrap();
    let result = root.pin_regular(&rel("one.abf-fixture"));
    assert_eq!(classify(&result, original, canary), Outcome::Original);
}

// ---------------------------------------------------------------------------
// 4. FIFO / socket rejection without I/O open
// ---------------------------------------------------------------------------

#[test]
fn fifo_is_refused_without_blocking() {
    let case = Case::new("fifo");
    mkfifo(&case.path("root/pipe.abf-fixture"));
    let root = case.authorize();
    // No reader or writer exists for the FIFO. An ordinary open would block.
    let result = within_deadline(move || root.pin_regular(&rel("pipe.abf-fixture")).map(|_| ()));
    assert_eq!(
        result.unwrap_err(),
        AccessError::NotRegularFile(ObjectKind::Fifo)
    );
}

#[test]
fn socket_is_refused_without_connecting() {
    let case = Case::new("socket");
    let listener = UnixListener::bind(case.path("root/sock.abf-fixture")).unwrap();
    let root = case.authorize();
    let result = within_deadline(move || root.pin_regular(&rel("sock.abf-fixture")).map(|_| ()));
    assert_eq!(
        result.unwrap_err(),
        AccessError::NotRegularFile(ObjectKind::Socket)
    );
    // Nothing connected to the socket.
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}

#[test]
fn missing_and_non_directory_components_are_coded() {
    let case = Case::new("missing");
    case.file("root/f.abf-fixture", "inside");
    let root = case.authorize();
    assert_eq!(
        root.pin_regular(&rel("absent.abf-fixture")).unwrap_err(),
        AccessError::NotFound
    );
    assert_eq!(
        root.pin_regular(&rel("f.abf-fixture/x")).unwrap_err(),
        AccessError::NotADirectory
    );
}

// ---------------------------------------------------------------------------
// Root admission (native)
// ---------------------------------------------------------------------------

#[test]
fn root_admission_refusals() {
    let case = Case::new("root_admission");
    case.dir("outside/real");
    case.file("outside/plain.abf-fixture", "x");
    symlink(case.path("outside/real"), case.path("root_link")).unwrap();
    symlink(case.path("outside"), case.path("ancestor_link")).unwrap();
    let text = |relative: &str| case.path(relative).to_str().unwrap().to_owned();

    // Final component is a symlink (even to a real directory).
    let error = SecureRoot::authorize(&text("root_link")).unwrap_err();
    assert!(
        matches!(
            error,
            AccessError::NotADirectory | AccessError::ConfinementRefused
        ),
        "{error:?}"
    );
    // Symlinked ancestor.
    assert_eq!(
        SecureRoot::authorize(&text("ancestor_link/real")).unwrap_err(),
        AccessError::ConfinementRefused
    );
    // Regular file.
    assert_eq!(
        SecureRoot::authorize(&text("outside/plain.abf-fixture")).unwrap_err(),
        AccessError::NotADirectory
    );
    // Missing.
    assert_eq!(
        SecureRoot::authorize(&text("absent")).unwrap_err(),
        AccessError::NotFound
    );
}

/// A directory on a different mount (tmpfs) must be refused by
/// RESOLVE_NO_XDEV during resolution (EXDEV), before filesystem admission.
/// The probe directory is created by this gate; nothing is written inside it.
#[test]
fn root_on_another_mount_is_refused_by_no_xdev() {
    let probe = std::env::var(FOREIGN_MOUNT_ENV).unwrap_or_else(|_| {
        panic!("{FOREIGN_MOUNT_ENV} must name an existing directory on a different mount")
    });
    assert_eq!(
        SecureRoot::authorize(&probe).unwrap_err(),
        AccessError::ConfinementRefused
    );
}

// ---------------------------------------------------------------------------
// 5. Root replaced before pin (static)
// ---------------------------------------------------------------------------

#[test]
fn root_replaced_by_directory_is_detected_and_never_adopted() {
    let case = Case::new("root_replaced_dir");
    let original = case.file("root/f.abf-fixture", "original");
    let root = case.authorize();
    let authorized = root.object_id();

    fs::rename(case.path("root"), case.path("root.old")).unwrap();
    let replacement = case.file("root/f.abf-fixture", "replacement");
    assert_ne!(replacement, original);

    assert_eq!(
        root.check_binding().unwrap_err(),
        AccessError::RootBindingChanged
    );
    let result = root.pin_regular(&rel("f.abf-fixture"));
    assert_eq!(
        classify(&result, original, replacement),
        Outcome::Refused(AccessError::RootBindingChanged)
    );
    assert_eq!(root.object_id(), authorized, "held root never swapped");

    // Restoring the original object restores the binding (object semantics).
    fs::rename(case.path("root"), case.path("replacement")).unwrap();
    fs::rename(case.path("root.old"), case.path("root")).unwrap();
    root.check_binding().unwrap();
    let result = root.pin_regular(&rel("f.abf-fixture"));
    assert_eq!(classify(&result, original, replacement), Outcome::Original);
}

#[test]
fn root_replaced_by_symlink_is_detected_even_to_the_original_object() {
    let case = Case::new("root_replaced_symlink");
    let original = case.file("root/f.abf-fixture", "original");
    let canary = case.file("outside/f.abf-fixture", "outside-canary");
    let root = case.authorize();

    // Symlink to the ORIGINAL object: still a binding change (no symlinks).
    fs::rename(case.path("root"), case.path("root.old")).unwrap();
    symlink(case.path("root.old"), case.path("root")).unwrap();
    assert_eq!(
        root.check_binding().unwrap_err(),
        AccessError::RootBindingChanged
    );
    let result = root.pin_regular(&rel("f.abf-fixture"));
    assert_eq!(
        classify(&result, original, canary),
        Outcome::Refused(AccessError::RootBindingChanged)
    );

    // Symlink to an outside directory.
    fs::remove_file(case.path("root")).unwrap();
    symlink(case.path("outside"), case.path("root")).unwrap();
    let result = root.pin_regular(&rel("f.abf-fixture"));
    assert_eq!(
        classify(&result, original, canary),
        Outcome::Refused(AccessError::RootBindingChanged)
    );
}

#[test]
fn root_removed_or_moved_is_a_binding_change() {
    let case = Case::new("root_moved");
    let original = case.file("root/f.abf-fixture", "original");
    let root = case.authorize();
    // The held object moves outside: its files are no longer pinnable through
    // this capability because the binding check fails.
    fs::rename(case.path("root"), case.path("outside/root-moved")).unwrap();
    let result = root.pin_regular(&rel("f.abf-fixture"));
    assert_eq!(
        classify(&result, original, 0),
        Outcome::Refused(AccessError::RootBindingChanged)
    );
}

// ---------------------------------------------------------------------------
// 7. Replacement after a successful pin
// ---------------------------------------------------------------------------

#[test]
fn replacement_after_pin_cannot_change_pinned_evidence() {
    let case = Case::new("after_pin");
    let original = case.file("root/a/f.abf-fixture", "original");
    let canary = case.file("outside/d/f.abf-fixture", "outside-canary");
    let root = case.authorize();
    let pinned = root.pin_regular(&rel("a/f.abf-fixture")).unwrap();
    let before = *pinned.evidence();

    // Final file replaced by another regular file.
    fs::rename(
        case.path("root/a/f.abf-fixture"),
        case.path("outside/moved.abf-fixture"),
    )
    .unwrap();
    case.file("root/a/f.abf-fixture", "replacement");
    // Intermediate replaced by a symlink to an outside directory.
    fs::rename(case.path("root/a"), case.path("outside/a-moved")).unwrap();
    symlink(case.path("outside/d"), case.path("root/a")).unwrap();

    assert_eq!(*pinned.evidence(), before);
    let now = pinned.reinspect().unwrap();
    assert_eq!(now.object_id(), before.object_id());
    assert_eq!(now.object_id().inode(), original);
    assert_ne!(now.object_id().inode(), canary);
    // The held object is the one now at outside/moved: same object, moved.
    assert_eq!(ino(&case.path("outside/moved.abf-fixture")), original);

    // A new pin through the replaced path is refused, never the canary.
    let result = root.pin_regular(&rel("a/f.abf-fixture"));
    assert_eq!(
        classify(&result, original, canary),
        Outcome::Refused(AccessError::ConfinementRefused)
    );
}

#[test]
fn link_count_change_after_pin_is_observed_by_reinspection() {
    let case = Case::new("link_after_pin");
    case.file("root/f.abf-fixture", "original");
    let root = case.authorize();
    let pinned = root.pin_regular(&rel("f.abf-fixture")).unwrap();
    fs::hard_link(case.path("root/f.abf-fixture"), case.path("outside/alias")).unwrap();
    assert_eq!(pinned.reinspect().unwrap_err(), AccessError::MultipleLinks);
    fs::remove_file(case.path("outside/alias")).unwrap();
    assert_eq!(pinned.reinspect().unwrap().link_count(), 1);
    // Unlinked while held: zero links is not a valid single-link object.
    fs::remove_file(case.path("root/f.abf-fixture")).unwrap();
    assert_eq!(pinned.reinspect().unwrap_err(), AccessError::ObjectChanged);
}

// ---------------------------------------------------------------------------
// 8. Rename schedules do not turn stale anchors into outside authorization
// ---------------------------------------------------------------------------

/// A component-walking implementation that kept a handle to `a` would now
/// authorize files placed in the moved directory. This implementation always
/// resolves the full path from the held root.
#[test]
fn intermediate_moved_outside_is_not_a_stale_anchor() {
    let case = Case::new("stale_anchor");
    let original = case.file("root/a/f.abf-fixture", "original");
    let root = case.authorize();
    let first = root.pin_regular(&rel("a/f.abf-fixture"));
    assert_eq!(classify(&first, original, 0), Outcome::Original);

    fs::rename(case.path("root/a"), case.path("outside/a")).unwrap();
    let canary = case.file("outside/a/g.abf-fixture", "outside-canary");
    let result = root.pin_regular(&rel("a/g.abf-fixture"));
    assert_eq!(
        classify(&result, original, canary),
        Outcome::Refused(AccessError::NotFound)
    );

    symlink("../outside/a", case.path("root/a")).unwrap();
    let result = root.pin_regular(&rel("a/g.abf-fixture"));
    assert_eq!(
        classify(&result, original, canary),
        Outcome::Refused(AccessError::ConfinementRefused)
    );
}

/// Supplementary concurrent stress (real threads, unchanged release code).
/// A racer moves `root/a` outside, swaps the outside canary in and out of it,
/// and moves it back. The canary is **never** beneath the root at any instant.
/// No pin may ever report it. A finite stress run is evidence, not a proof.
#[test]
fn stress_intermediate_moved_out_and_back() {
    let case = Case::new("stress_move_out");
    let original = case.file("root/a/f.abf-fixture", "original");
    let canary = case.file("outside/canary", "outside-canary");
    let root = case.authorize();
    let (stats, cycles) = race(&case, &root, original, canary, |p| {
        fs::rename(p("root/a"), p("outside/park")).unwrap();
        fs::rename(p("outside/park/f.abf-fixture"), p("outside/hold")).unwrap();
        fs::rename(p("outside/canary"), p("outside/park/f.abf-fixture")).unwrap();
        fs::rename(p("outside/park/f.abf-fixture"), p("outside/canary")).unwrap();
        fs::rename(p("outside/hold"), p("outside/park/f.abf-fixture")).unwrap();
        fs::rename(p("outside/park"), p("root/a")).unwrap();
    });
    eprintln!("stress_move_out: racer cycles={cycles} outcomes={stats:?}");
    assert_eq!(stats.canary, 0, "outside canary was reported as authorized");
    assert_eq!(stats.other, 0);
}

/// Supplementary concurrent stress: `root/a` alternates between the real
/// directory and a symlink to an outside directory holding a canary at the
/// same relative name (the ABF9 attack shape).
#[test]
fn stress_intermediate_symlink_swap() {
    let case = Case::new("stress_symlink");
    let original = case.file("root/a/f.abf-fixture", "original");
    let canary = case.file("outside/d/f.abf-fixture", "outside-canary");
    symlink("../outside/d", case.path("root/a_link")).unwrap();
    let root = case.authorize();
    let (stats, cycles) = race(&case, &root, original, canary, |p| {
        fs::rename(p("root/a"), p("outside/stash")).unwrap();
        fs::rename(p("root/a_link"), p("root/a")).unwrap();
        fs::rename(p("root/a"), p("root/a_link")).unwrap();
        fs::rename(p("outside/stash"), p("root/a")).unwrap();
    });
    eprintln!("stress_symlink: racer cycles={cycles} outcomes={stats:?}");
    assert_eq!(stats.canary, 0, "outside canary was reported as authorized");
    assert_eq!(stats.other, 0);
}

#[derive(Debug, Default)]
struct RaceStats {
    original: u64,
    canary: u64,
    other: u64,
    refused: std::collections::BTreeMap<String, u64>,
}

const RACE_CYCLES: u64 = 3000;

fn race(
    case: &Case,
    root: &SecureRoot,
    original: u64,
    canary: u64,
    cycle: impl Fn(&dyn Fn(&str) -> PathBuf) + Send + 'static,
) -> (RaceStats, u64) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let done = Arc::new(AtomicBool::new(false));
    let racer_done = Arc::clone(&done);
    let base = case.dir.clone();
    let racer = std::thread::spawn(move || {
        let p = |relative: &str| base.join(relative);
        for _ in 0..RACE_CYCLES {
            cycle(&p);
        }
        racer_done.store(true, Ordering::SeqCst);
        RACE_CYCLES
    });
    let path = rel("a/f.abf-fixture");
    let mut stats = RaceStats::default();
    while !done.load(Ordering::SeqCst) {
        match classify(&root.pin_regular(&path), original, canary) {
            Outcome::Original => stats.original += 1,
            Outcome::OutsideCanary => stats.canary += 1,
            Outcome::OtherObject => stats.other += 1,
            Outcome::Refused(error) => *stats.refused.entry(format!("{error:?}")).or_default() += 1,
        }
    }
    let cycles = racer.join().expect("racer thread panicked");
    (stats, cycles)
}

// ---------------------------------------------------------------------------
// Deterministic schedules around the real kernel calls (test harness:
// feature `fault-injection` + `--cfg abf_secure_fs_harness`)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
mod scheduled {
    use super::*;
    use abf_secure_fs::fault_injection::{self as fi, SchedulePoint};

    /// Resets thread-local instrumentation, and on drop checks that every
    /// descriptor wrapped on this thread was closed. Declare it **first**
    /// so it drops after all capabilities.
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

    // -- flags ---------------------------------------------------------------

    /// Independent expected values from the installed headers:
    /// O_PATH 0o10000000, O_DIRECTORY 0o200000, O_NOFOLLOW 0o400000,
    /// O_CLOEXEC 0o2000000; RESOLVE_NO_XDEV 1, NO_MAGICLINKS 2,
    /// NO_SYMLINKS 4, BENEATH 8.
    #[test]
    fn every_openat2_call_carries_the_mandatory_flags() {
        let _leaks = LeakCheck::start();
        let case = Case::new("flags");
        case.file("root/f.abf-fixture", "original");
        let dir_flags = 0o10_000_000 | 0o200_000 | 0o400_000 | 0o2_000_000;
        let file_flags = 0o10_000_000 | 0o400_000 | 0o2_000_000;
        let full_mask = 0x1 | 0x2 | 0x4 | 0x8;
        {
            let root = case.authorize();
            let log = fi::take_openat2_log();
            assert_eq!(log.len(), 2);
            assert_eq!(
                (log[0].flags, log[0].mode, log[0].resolve),
                (dir_flags, 0, 0x1 | 0x2 | 0x4)
            );
            assert_eq!(
                (log[1].flags, log[1].mode, log[1].resolve),
                (dir_flags, 0, full_mask)
            );

            let _pinned = root.pin_regular(&rel("f.abf-fixture")).unwrap();
            let log = fi::take_openat2_log();
            let observed: Vec<_> = log.iter().map(|a| (a.flags, a.mode, a.resolve)).collect();
            assert_eq!(
                observed,
                vec![
                    (dir_flags, 0, full_mask),  // binding check
                    (file_flags, 0, full_mask), // acquisition
                    (file_flags, 0, full_mask), // verification
                    (dir_flags, 0, full_mask),  // final binding check
                ]
            );
            assert!(log.iter().all(|a| a.injected_errno.is_none()));

            root.check_binding().unwrap();
            let log = fi::take_openat2_log();
            assert_eq!(log.len(), 1);
            assert_eq!((log[0].flags, log[0].resolve), (dir_flags, full_mask));
        }
    }

    // -- 5. root replacement at scheduled points --------------------------------

    #[test]
    fn root_replaced_by_directory_between_checks_and_acquisition() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_root_dir");
        let original = case.file("root/f.abf-fixture", "original");
        let replacement_dir = case.path("replacement");
        fs::create_dir(&replacement_dir).unwrap();
        let replacement = case.file("replacement/f.abf-fixture", "replacement");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinBeforeAcquire, move || {
            fs::rename(p("root"), p("root.old")).unwrap();
            fs::rename(p("replacement"), p("root")).unwrap();
        })]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        // Acquisition went through the held original; the final binding
        // check sees the replacement and refuses.
        assert_eq!(
            classify(&result, original, replacement),
            Outcome::Refused(AccessError::RootBindingChanged)
        );
    }

    #[test]
    fn root_replaced_by_symlink_after_acquisition() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_root_symlink");
        let original = case.file("root/f.abf-fixture", "original");
        let canary = case.file("outside/f.abf-fixture", "outside-canary");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::rename(p("root"), p("root.old")).unwrap();
            symlink(p("outside"), p("root")).unwrap();
        })]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::RootBindingChanged)
        );
    }

    /// U2 limitation, demonstrated: a root rename restored between the
    /// binding checks is not detected. It cannot redirect the held root,
    /// so the result is still the original object.
    #[test]
    fn transient_root_rename_restored_between_checks_keeps_original() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_root_transient");
        let original = case.file("root/f.abf-fixture", "original");
        let canary = case.file("outside/f.abf-fixture", "outside-canary");
        let root = case.authorize();
        let p1 = paths(&case);
        let p2 = paths(&case);
        schedule(vec![
            at(SchedulePoint::PinBeforeAcquire, move || {
                fs::rename(p1("root"), p1("root.away")).unwrap();
                symlink(p1("outside"), p1("root")).unwrap();
            }),
            at(SchedulePoint::PinAfterAcquire, move || {
                fs::remove_file(p2("root")).unwrap();
                fs::rename(p2("root.away"), p2("root")).unwrap();
            }),
        ]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(classify(&result, original, canary), Outcome::Original);
    }

    #[test]
    fn authorize_acquires_what_is_bound_and_refuses_a_symlink_swap() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_authorize");
        case.dir("outside/real");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::AuthorizeAfterAnchor, move || {
            fs::rename(p("root"), p("root.old")).unwrap();
            symlink(p("outside/real"), p("root")).unwrap();
        })]);
        let error = SecureRoot::authorize(&case.root_text()).unwrap_err();
        assert!(
            matches!(
                error,
                AccessError::NotADirectory | AccessError::ConfinementRefused
            ),
            "{error:?}"
        );
    }

    // -- 6. intermediate symlink replacement before acquisition -----------------

    #[test]
    fn intermediate_swapped_to_outside_symlink_before_acquisition() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_intermediate_symlink");
        let original = case.file("root/a/f.abf-fixture", "original");
        let canary = case.file("outside/d/f.abf-fixture", "outside-canary");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinBeforeAcquire, move || {
            fs::rename(p("root/a"), p("outside/a-moved")).unwrap();
            symlink(p("outside/d"), p("root/a")).unwrap();
        })]);
        let result = root.pin_regular(&rel("a/f.abf-fixture"));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::ConfinementRefused)
        );
    }

    /// Object-capability semantics, demonstrated: a directory renamed INTO
    /// the root is inside the root at resolution time. Its file is not an
    /// outside object any more. It is reported as another object, never as
    /// the original.
    #[test]
    fn directory_renamed_into_root_before_acquisition_is_inside() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_moved_in");
        let original = case.file("root/a/f.abf-fixture", "original");
        let moved_in = case.file("outside/d/f.abf-fixture", "moved-in");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinBeforeAcquire, move || {
            fs::rename(p("root/a"), p("outside/a-moved")).unwrap();
            fs::rename(p("outside/d"), p("root/a")).unwrap();
        })]);
        let result = root.pin_regular(&rel("a/f.abf-fixture"));
        assert_eq!(classify(&result, original, 0), Outcome::OtherObject);
        assert_eq!(result.unwrap().evidence().object_id().inode(), moved_in);
    }

    // -- 8. changes between acquisition and verification ------------------------

    #[test]
    fn intermediate_moved_out_after_acquisition_fails_verification() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_moved_out");
        let original = case.file("root/a/f.abf-fixture", "original");
        let canary = case.file("outside/canary", "outside-canary");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::rename(p("root/a"), p("outside/a")).unwrap();
        })]);
        let result = root.pin_regular(&rel("a/f.abf-fixture"));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::ObjectChanged)
        );
    }

    #[test]
    fn intermediate_symlinked_after_acquisition_fails_verification() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_symlink_after");
        let original = case.file("root/a/f.abf-fixture", "original");
        let canary = case.file("outside/d/f.abf-fixture", "outside-canary");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::rename(p("root/a"), p("outside/a-moved")).unwrap();
            symlink(p("outside/d"), p("root/a")).unwrap();
        })]);
        let result = root.pin_regular(&rel("a/f.abf-fixture"));
        assert_eq!(
            classify(&result, original, canary),
            Outcome::Refused(AccessError::ObjectChanged)
        );
    }

    #[test]
    fn final_file_replaced_after_acquisition_fails_verification() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_final_replaced");
        let original = case.file("root/f.abf-fixture", "original");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::rename(p("root/f.abf-fixture"), p("outside/old")).unwrap();
            fs::write(p("root/f.abf-fixture"), "abf11-synthetic replacement\n").unwrap();
        })]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(
            classify(&result, original, 0),
            Outcome::Refused(AccessError::ObjectChanged)
        );
    }

    /// U2 limitation, demonstrated: once verified, the original object can
    /// be moved out of the root before the pin returns. The held object is
    /// still the original verified one, never an outside canary.
    #[test]
    fn move_out_after_verification_keeps_the_verified_original() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_after_verify");
        let original = case.file("root/a/f.abf-fixture", "original");
        let canary = case.file("outside/canary", "outside-canary");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterVerify, move || {
            fs::rename(p("root/a"), p("outside/a")).unwrap();
        })]);
        let result = root.pin_regular(&rel("a/f.abf-fixture"));
        assert_eq!(classify(&result, original, canary), Outcome::Original);
    }

    #[test]
    fn hardlink_added_after_acquisition_is_refused() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_link_added");
        let original = case.file("root/f.abf-fixture", "original");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::hard_link(p("root/f.abf-fixture"), p("outside/alias")).unwrap();
        })]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(
            classify(&result, original, 0),
            Outcome::Refused(AccessError::MultipleLinks)
        );
    }

    /// ABF12 F04 regression: a hard link is added after acquisition, so the
    /// verifier observes the real nlink 2, and the schedule removes it again
    /// after verification, before the final inspection. The verifier sample
    /// is enforced, so the pin is refused **at verification**: the removal
    /// step is never reached. Without the verifier policy this exact
    /// schedule reaches the removal and returns the original object. No
    /// identity or metadata is injected.
    #[test]
    fn hardlink_seen_only_by_the_verifier_is_refused() {
        use std::cell::Cell;
        use std::rc::Rc;

        let _leaks = LeakCheck::start();
        let case = Case::new("sched_link_verifier_only");
        let original = case.file("root/f.abf-fixture", "original");
        let root = case.authorize();
        let p1 = paths(&case);
        let p2 = paths(&case);
        let reached_after_verify = Rc::new(Cell::new(false));
        let reached = Rc::clone(&reached_after_verify);
        schedule(vec![
            at(SchedulePoint::PinAfterAcquire, move || {
                fs::hard_link(p1("root/f.abf-fixture"), p1("outside/alias")).unwrap();
                assert_eq!(
                    fs::symlink_metadata(p1("root/f.abf-fixture"))
                        .unwrap()
                        .nlink(),
                    2
                );
            }),
            at(SchedulePoint::PinAfterVerify, move || {
                reached.set(true);
                fs::remove_file(p2("outside/alias")).unwrap();
            }),
        ]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(
            classify(&result, original, 0),
            Outcome::Refused(AccessError::MultipleLinks)
        );
        assert!(
            !reached_after_verify.get(),
            "the refusal must come from the verifier sample"
        );
        assert_eq!(ino(&case.path("outside/alias")), original);
    }

    /// Limitation, demonstrated: `nlink == 1` is sampled. A hard link that is
    /// added and removed entirely between two samples is not observed, and
    /// the original object is pinned.
    #[test]
    fn transient_hardlink_between_samples_is_not_observed() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_link_transient");
        let original = case.file("root/f.abf-fixture", "original");
        let root = case.authorize();
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::PinAfterAcquire, move || {
            fs::hard_link(p("root/f.abf-fixture"), p("outside/alias")).unwrap();
            fs::remove_file(p("outside/alias")).unwrap();
        })]);
        let result = root.pin_regular(&rel("f.abf-fixture"));
        assert_eq!(classify(&result, original, 0), Outcome::Original);
    }

    // -- final-object substitution before acquisition ---------------------------

    #[test]
    fn final_file_substituted_before_acquisition() {
        type Setup = fn(&dyn Fn(&str) -> PathBuf);
        let substitutions: [(&str, Setup, AccessError); 4] = [
            (
                "fifo",
                |p| mkfifo(&p("root/f.abf-fixture")),
                AccessError::NotRegularFile(ObjectKind::Fifo),
            ),
            (
                "symlink",
                |p| symlink(p("outside/canary"), p("root/f.abf-fixture")).unwrap(),
                AccessError::NotRegularFile(ObjectKind::Symlink),
            ),
            (
                "socket",
                |p| drop(UnixListener::bind(p("root/f.abf-fixture")).unwrap()),
                AccessError::NotRegularFile(ObjectKind::Socket),
            ),
            (
                "outside_hardlink",
                |p| fs::hard_link(p("outside/canary"), p("root/f.abf-fixture")).unwrap(),
                AccessError::MultipleLinks,
            ),
        ];
        for (name, setup, expected) in substitutions {
            let outcome = within_deadline(move || {
                let _leaks = LeakCheck::start();
                let case = Case::new(&format!("sched_substitute_{name}"));
                let original = case.file("root/f.abf-fixture", "original");
                let canary = case.file("outside/canary", "outside-canary");
                let root = case.authorize();
                let p = paths(&case);
                schedule(vec![at(SchedulePoint::PinBeforeAcquire, move || {
                    fs::rename(p("root/f.abf-fixture"), p("outside/original")).unwrap();
                    setup(&p);
                })]);
                let result = root.pin_regular(&rel("f.abf-fixture"));
                classify(&result, original, canary)
            });
            assert_eq!(outcome, Outcome::Refused(expected), "{name}");
        }
    }
}
