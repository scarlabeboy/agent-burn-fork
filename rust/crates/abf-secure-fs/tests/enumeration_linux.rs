//! M2A.2b native directory enumeration tests against the real kernel, on
//! synthetic fixtures only.
//!
//! Fixtures are created **only** beneath `ABF_SECURE_FS_FIXTURE_DIR` (no
//! fallback). Each test owns one fresh case directory
//! `<fixture>/abf18-<test>-<pid>` with an authorized `root/` and a sibling
//! `outside/` holding canary objects, and removes it when done. No file
//! content is read by the crate; names are never exposed, so candidates are
//! identified through their sorted ordinal and the handle-derived inode of a
//! fresh root-relative pin.
//!
//! Evidence classes: the default module runs the real kernel only. The
//! `scheduled` module (two-key harness) adds deterministic schedule hooks
//! around the real calls, the exact `open_how` log, descriptor counters and
//! injected `getdents64`/errno/identity answers; injected results are
//! labelled as such.

#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]

use abf_secure_fs::{
    AccessError, BackendCapabilities, BudgetKind, Cancellation, Coverage, DirectoryListing,
    DirectoryTarget, EnumerationBudget, EnumerationError, ObjectKind, PinnedDirectory,
    RelativePath, SecureRoot, StreamEnd,
};
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const FIXTURE_ENV: &str = "ABF_SECURE_FS_FIXTURE_DIR";
const CHILD_ENV: &str = "ABF18_ENUMERATION_CHILD";
const DEADLINE: Duration = Duration::from_secs(20);

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
        let dir = base.join(format!("abf18-{name}-{}", std::process::id()));
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

    fn file(&self, relative: &str, marker: &str) -> u64 {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, format!("abf18-synthetic {marker}\n")).unwrap();
        ino(&path)
    }

    fn dir(&self, relative: &str) -> u64 {
        fs::create_dir_all(self.path(relative)).unwrap();
        ino(&self.path(relative))
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        // remove_dir_all does not follow symlinks; everything here was
        // created by this test inside its own case directory.
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

fn max() -> EnumerationBudget {
    EnumerationBudget::maximum()
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

fn pin_dir<'r>(root: &'r SecureRoot, relative: &str) -> PinnedDirectory<'r> {
    root.pin_directory(DirectoryTarget::Relative(&rel(relative)))
        .unwrap()
}

fn list<'r>(root: &'r SecureRoot, relative: &str) -> DirectoryListing<'r> {
    pin_dir(root, relative)
        .enumerate(max(), &Cancellation::new())
        .unwrap()
}

/// Inode of a fresh pin of every ordinal, or the refusal.
fn pinned(listing: &DirectoryListing<'_>) -> Vec<Result<u64, EnumerationError>> {
    (0..listing.len())
        .map(|i| {
            listing
                .pin_regular(i)
                .map(|p| p.evidence().object_id().inode())
        })
        .collect()
}

fn within_deadline<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(DEADLINE)
        .expect("operation did not finish within the deadline (blocking open?)")
}

// ---------------------------------------------------------------------------
// Capability, root and relative pins
// ---------------------------------------------------------------------------

#[test]
fn capability_is_described_honestly() {
    let caps = BackendCapabilities::compiled();
    assert!(caps.directory_enumeration);
    assert_eq!(caps.required_resolve_mask, 0x0F);
    assert!(!caps.environment_attested);
}

#[test]
fn root_target_pins_the_held_root_and_lists_it() {
    let case = Case::new("root_target");
    let a = case.file("root/a.abf18", "a");
    let b = case.file("root/b.abf18", "b");
    let root = case.authorize();
    let pinned_root = root.pin_directory(DirectoryTarget::Root).unwrap();
    assert_eq!(pinned_root.object_id(), root.object_id());
    let listing = pinned_root.enumerate(max(), &Cancellation::new()).unwrap();
    assert_eq!(listing.directory_object_id(), root.object_id());
    assert_eq!(listing.len(), 2);
    assert_eq!(listing.status().end(), StreamEnd::Eof);
    assert_eq!(listing.status().coverage(), Coverage::Exhausted);
    let counters = listing.counters();
    assert_eq!(counters.navigation_records(), 2);
    assert_eq!(counters.records_inspected(), 4);
    assert_eq!(counters.rejected_names(), 0);
    // Sorted by name bytes: a, b.
    assert_eq!(pinned(&listing), vec![Ok(a), Ok(b)]);
}

#[test]
fn empty_directory_reaches_real_eof() {
    let case = Case::new("empty");
    let id = case.dir("root/empty");
    let root = case.authorize();
    let pin = pin_dir(&root, "empty");
    assert_eq!(pin.object_id().inode(), id);
    let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
    assert!(listing.is_empty());
    assert_eq!(listing.status().end(), StreamEnd::Eof);
    assert_eq!(listing.status().coverage(), Coverage::Exhausted);
    let counters = listing.counters();
    // One batch with `.` and `..`, then the zero-byte end of stream.
    assert_eq!(counters.read_attempts(), 2);
    assert_eq!(counters.navigation_records(), 2);
    assert_eq!(counters.candidates(), 0);
}

#[test]
fn nested_entries_are_discovered_but_never_visited() {
    let case = Case::new("nested");
    let f = case.file("root/d/f.abf18", "f");
    case.file("root/d/sub/deep.abf18", "deep");
    case.file("root/d/sub/deeper/x.abf18", "x");
    let root = case.authorize();
    let listing = list(&root, "d");
    // `f.abf18` < `sub`.
    assert_eq!(listing.len(), 2);
    assert_eq!(listing.counters().records_inspected(), 4);
    assert_eq!(
        pinned(&listing),
        vec![
            Ok(f),
            Err(EnumerationError::Access(AccessError::NotRegularFile(
                ObjectKind::Directory
            )))
        ]
    );
}

#[test]
fn directory_pin_refusals_are_coded() {
    let case = Case::new("pin_refusals");
    case.file("root/file.abf18", "file");
    case.dir("outside/target");
    symlink(case.path("outside/target"), case.path("root/link")).unwrap();
    case.dir("root/d");
    let root = case.authorize();
    let pin = |text: &str| {
        root.pin_directory(DirectoryTarget::Relative(&rel(text)))
            .map(|p| p.object_id())
    };
    assert_eq!(pin("file.abf18"), Err(AccessError::NotADirectory));
    // Observed natively (ABF18): a final symlink is not followed;
    // `O_PATH|O_NOFOLLOW` yields the link itself and `O_DIRECTORY` refuses
    // it with ENOTDIR. An intermediate symlink is ELOOP (NO_SYMLINKS).
    assert_eq!(pin("link"), Err(AccessError::NotADirectory));
    assert_eq!(pin("link/x"), Err(AccessError::ConfinementRefused));
    assert_eq!(pin("missing"), Err(AccessError::NotFound));
    assert!(pin("d").is_ok());
}

// ---------------------------------------------------------------------------
// Names and special entries
// ---------------------------------------------------------------------------

#[test]
fn special_entries_are_listed_but_never_activated() {
    let case = Case::new("special");
    let canary = case.file("outside/canary.abf18", "outside canary");
    case.dir("root/d");
    symlink(
        case.path("outside/canary.abf18"),
        case.path("root/d/a-link"),
    )
    .unwrap();
    mkfifo(&case.path("root/d/b-pipe"));
    let _socket = UnixListener::bind(case.path("root/d/c-sock")).unwrap();
    let hl = case.file("root/d/d-hardlink", "hardlink");
    fs::hard_link(case.path("root/d/d-hardlink"), case.path("outside/alias")).unwrap();
    let plain = case.file("root/d/e-plain", "plain");
    let root_text = case.root_text();
    let results = within_deadline(move || {
        let root = SecureRoot::authorize(&root_text).unwrap();
        let listing = list(&root, "d");
        assert_eq!(listing.len(), 5);
        assert_eq!(listing.status().coverage(), Coverage::Exhausted);
        pinned(&listing)
    });
    assert_eq!(
        results,
        vec![
            Err(EnumerationError::Access(AccessError::NotRegularFile(
                ObjectKind::Symlink
            ))),
            Err(EnumerationError::Access(AccessError::NotRegularFile(
                ObjectKind::Fifo
            ))),
            Err(EnumerationError::Access(AccessError::NotRegularFile(
                ObjectKind::Socket
            ))),
            Err(EnumerationError::Access(AccessError::MultipleLinks)),
            Ok(plain),
        ]
    );
    assert!(!results.contains(&Ok(canary)));
    assert!(!results.contains(&Ok(hl)));
}

#[test]
fn name_policy_applies_to_real_entries() {
    let case = Case::new("names");
    case.dir("root/d");
    let raw = |name: &[u8]| case.path("root/d").join(OsStr::from_bytes(name));
    fs::write(raw(b"\xff\xfe-not-utf8"), "x").unwrap();
    fs::write(raw(b"colon:name"), "x").unwrap();
    fs::write(raw(b"back\\slash"), "x").unwrap();
    fs::write(raw(b"control\x01name"), "x").unwrap();
    fs::write(raw("next\u{85}line".as_bytes()), "x").unwrap();
    let long = case.file(&format!("root/d/{}", "L".repeat(255)), "long");
    let ok = case.file("root/d/ok", "ok");
    let root = case.authorize();
    let listing = list(&root, "d");
    let counters = listing.counters();
    assert_eq!(counters.rejected_names(), 5);
    assert_eq!(counters.candidates(), 2);
    assert_eq!(counters.navigation_records(), 2);
    assert_eq!(counters.retained_name_bytes(), 255 + 2);
    // Real end of stream after policy rejections: Partial, never Exhausted.
    assert_eq!(listing.status().end(), StreamEnd::Eof);
    assert_eq!(listing.status().coverage(), Coverage::Partial);
    assert_eq!(pinned(&listing), vec![Ok(long), Ok(ok)]);
}

#[test]
fn many_entries_span_several_bounded_reads() {
    let case = Case::new("many");
    case.dir("root/d");
    let mut expected = Vec::new();
    for i in 0..400 {
        expected.push(case.file(&format!("root/d/entry-{i:04}-{}", "p".repeat(40)), "m"));
    }
    let root = case.authorize();
    let listing = list(&root, "d");
    let counters = listing.counters();
    assert_eq!(listing.status().coverage(), Coverage::Exhausted);
    assert_eq!(counters.candidates(), 400);
    assert_eq!(counters.records_inspected(), 402);
    // About 72 bytes per record: several 8192-byte reads plus EOF.
    assert!(counters.read_attempts() >= 4, "{counters:?}");
    assert!(counters.read_attempts() <= 64);
    assert_eq!(
        pinned(&listing),
        expected.into_iter().map(Ok).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Budgets on a real directory
// ---------------------------------------------------------------------------

#[test]
fn budgets_stop_conservatively_on_a_real_directory() {
    let case = Case::new("budgets");
    case.dir("root/d");
    for i in 0..600 {
        case.file(&format!("root/d/n{i:04}"), "b");
    }
    case.dir("root/three");
    for name in ["x", "y", "z"] {
        case.file(&format!("root/three/{name}"), "t");
    }
    let root = case.authorize();
    let run = |relative: &str, budget: EnumerationBudget| {
        pin_dir(&root, relative)
            .enumerate(budget, &Cancellation::new())
            .unwrap()
    };

    let full = run("d", max());
    assert_eq!(full.len(), 512);
    assert_eq!(
        full.status().end(),
        StreamEnd::Budget(BudgetKind::Candidates)
    );
    assert_eq!(full.status().coverage(), Coverage::Partial);

    let one_read = run("d", EnumerationBudget::new(4096, 512, 65_536, 1).unwrap());
    assert_eq!(
        one_read.status().end(),
        StreamEnd::Budget(BudgetKind::ReadAttempts)
    );
    assert_eq!(one_read.counters().read_attempts(), 1);
    assert!(!one_read.is_empty() && one_read.len() < 600);

    let ten = run("d", EnumerationBudget::new(10, 10, 65_536, 64).unwrap());
    assert_eq!(
        ten.status().end(),
        StreamEnd::Budget(BudgetKind::InspectedRecords)
    );
    assert_eq!(ten.counters().records_inspected(), 10);

    let bytes = run("d", EnumerationBudget::new(4096, 512, 100, 64).unwrap());
    assert_eq!(
        bytes.status().end(),
        StreamEnd::Budget(BudgetKind::NameBytes)
    );
    assert!(bytes.counters().retained_name_bytes() <= 100);

    // Exactly N candidates with candidates == N: Partial, and no extra read
    // to discover the end of stream.
    let exact = run(
        "three",
        EnumerationBudget::new(4096, 3, 65_536, 64).unwrap(),
    );
    assert_eq!(exact.len(), 3);
    assert_eq!(
        exact.status().end(),
        StreamEnd::Budget(BudgetKind::Candidates)
    );
    assert_eq!(exact.counters().read_attempts(), 1);
    let roomy = run("three", max());
    assert_eq!(roomy.status().coverage(), Coverage::Exhausted);
    assert_eq!(roomy.counters().read_attempts(), 2);
}

#[test]
fn cancellation_before_the_call_refuses() {
    let case = Case::new("precancel");
    case.file("root/d/f", "f");
    let root = case.authorize();
    let cancel = Cancellation::new();
    cancel.request();
    assert!(cancel.is_requested());
    assert_eq!(
        pin_dir(&root, "d").enumerate(max(), &cancel).unwrap_err(),
        EnumerationError::Cancelled
    );
}

#[test]
fn ordinals_are_bounds_checked() {
    let case = Case::new("ordinals");
    case.file("root/d/f", "f");
    let root = case.authorize();
    let listing = list(&root, "d");
    assert_eq!(listing.len(), 1);
    for ordinal in [1, 2, 512, usize::MAX] {
        assert_eq!(
            listing.pin_regular(ordinal).unwrap_err(),
            EnumerationError::OrdinalOutOfRange
        );
    }
}

// ---------------------------------------------------------------------------
// Binding changes before enumeration
// ---------------------------------------------------------------------------

#[test]
fn persistent_directory_changes_before_enumeration_are_refused() {
    type Mutation = fn(&Case);
    let mutations: [(&str, Mutation); 6] = [
        ("renamed inside", |c| {
            fs::rename(c.path("root/d"), c.path("root/e")).unwrap()
        }),
        ("moved outside", |c| {
            fs::rename(c.path("root/d"), c.path("outside/d")).unwrap()
        }),
        ("replaced by a directory", |c| {
            fs::rename(c.path("root/d"), c.path("outside/d")).unwrap();
            fs::create_dir(c.path("root/d")).unwrap();
        }),
        ("replaced by a symlink", |c| {
            fs::rename(c.path("root/d"), c.path("outside/d")).unwrap();
            symlink(c.path("outside/d"), c.path("root/d")).unwrap();
        }),
        ("intermediate replaced", |c| {
            fs::rename(c.path("root/a"), c.path("outside/a")).unwrap();
            fs::create_dir_all(c.path("root/a/d")).unwrap();
        }),
        ("deleted", |c| {
            fs::remove_file(c.path("root/a/d/f")).unwrap();
            fs::remove_dir(c.path("root/a/d")).unwrap();
        }),
    ];
    for (label, mutate) in mutations {
        let case = Case::new("before_enum");
        case.file("root/a/d/f", "f");
        case.file("root/d/f", "f");
        let root = case.authorize();
        let target = if label.starts_with("intermediate") || label == "deleted" {
            "a/d"
        } else {
            "d"
        };
        let pin = pin_dir(&root, target);
        mutate(&case);
        assert_eq!(
            pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
            EnumerationError::DirectoryBindingChanged,
            "{label}"
        );
    }
}

#[test]
fn root_replacement_refuses_enumeration() {
    let case = Case::new("root_replaced");
    case.file("root/d/f", "f");
    let root = case.authorize();
    let pin = pin_dir(&root, "d");
    let root_pin = root.pin_directory(DirectoryTarget::Root).unwrap();
    fs::rename(case.path("root"), case.path("old-root")).unwrap();
    fs::create_dir_all(case.path("root/d")).unwrap();
    assert_eq!(
        pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
        EnumerationError::Access(AccessError::RootBindingChanged)
    );
    assert_eq!(
        root_pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
        EnumerationError::Access(AccessError::RootBindingChanged)
    );
    fs::remove_dir_all(case.path("root")).unwrap();
    fs::rename(case.path("old-root"), case.path("root")).unwrap();
}

/// Documented limitation: a move restored before the next check is not
/// observed. The listing is of the original held object.
#[test]
fn transient_move_restored_before_enumeration_is_not_observed() {
    let case = Case::new("transient");
    let f = case.file("root/d/f", "f");
    let root = case.authorize();
    let pin = pin_dir(&root, "d");
    let id = pin.object_id();
    fs::rename(case.path("root/d"), case.path("outside/d")).unwrap();
    fs::rename(case.path("outside/d"), case.path("root/d")).unwrap();
    let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
    assert_eq!(listing.directory_object_id(), id);
    assert_eq!(pinned(&listing), vec![Ok(f)]);
}

// ---------------------------------------------------------------------------
// Child authorization after discovery
// ---------------------------------------------------------------------------

/// Stale-directory attack: after the listing, the held directory moves
/// outside (with its file), and a different directory with a same-named file
/// takes its place. A child request must refuse; it must never resolve the
/// name beneath the moved (outside) directory, nor adopt the replacement.
#[test]
fn child_requests_never_use_a_stale_directory() {
    for replace in [false, true] {
        let case = Case::new("stale_dir");
        let original = case.file("root/d/f", "original");
        let root = case.authorize();
        let listing = list(&root, "d");
        fs::rename(case.path("root/d"), case.path("outside/d")).unwrap();
        let replacement = replace.then(|| case.file("root/d/f", "replacement"));
        let result = listing
            .pin_regular(0)
            .map(|p| p.evidence().object_id().inode());
        assert_eq!(
            result,
            Err(EnumerationError::DirectoryBindingChanged),
            "replace={replace}"
        );
        assert_ne!(result, Ok(original));
        if let Some(replacement) = replacement {
            assert_ne!(result, Ok(replacement));
        }
    }
}

#[test]
fn child_renamed_directory_inside_root_is_refused() {
    let case = Case::new("renamed_dir");
    case.file("root/d/f", "f");
    let root = case.authorize();
    let listing = list(&root, "d");
    fs::rename(case.path("root/d"), case.path("root/e")).unwrap();
    assert_eq!(
        listing.pin_regular(0).unwrap_err(),
        EnumerationError::DirectoryBindingChanged
    );
}

/// The discovered name is a request for whatever is **currently** bound:
/// a replacement regular file is pinned as a different object; a symlink or
/// removal is refused by the unchanged file protocol.
#[test]
fn entry_changed_after_discovery_is_judged_as_it_is_now() {
    let case = Case::new("entry_changed");
    let original = case.file("root/d/f", "original");
    case.file("outside/target", "outside target");
    let root = case.authorize();
    let listing = list(&root, "d");

    let replacement = case.file("root/d/f.new", "replacement");
    fs::rename(case.path("root/d/f.new"), case.path("root/d/f")).unwrap();
    let now = listing.pin_regular(0).unwrap();
    assert_eq!(now.evidence().object_id().inode(), replacement);
    assert_ne!(replacement, original);

    fs::remove_file(case.path("root/d/f")).unwrap();
    symlink(case.path("outside/target"), case.path("root/d/f")).unwrap();
    assert_eq!(
        listing.pin_regular(0).unwrap_err(),
        EnumerationError::Access(AccessError::NotRegularFile(ObjectKind::Symlink))
    );

    fs::remove_file(case.path("root/d/f")).unwrap();
    assert_eq!(
        listing.pin_regular(0).unwrap_err(),
        EnumerationError::Access(AccessError::NotFound)
    );
}

/// A regular file moved in under a discovered name is currently inside the
/// root and single-linked: it may be pinned (moved-in object, no origin
/// claim). It is not an outside escape.
#[test]
fn moved_in_child_is_pinned_as_the_current_object() {
    let case = Case::new("moved_in");
    case.file("root/d/g", "original");
    let moved = case.file("outside/incoming", "moved in");
    let root = case.authorize();
    let listing = list(&root, "d");
    fs::rename(case.path("outside/incoming"), case.path("root/d/g")).unwrap();
    assert_eq!(
        listing
            .pin_regular(0)
            .unwrap()
            .evidence()
            .object_id()
            .inode(),
        moved
    );
}

#[test]
fn hardlink_policy_is_unchanged_for_children() {
    let case = Case::new("hardlink");
    case.file("root/d/f", "f");
    fs::hard_link(case.path("root/d/f"), case.path("root/d/g")).unwrap();
    let root = case.authorize();
    let listing = list(&root, "d");
    assert_eq!(listing.len(), 2);
    for ordinal in 0..2 {
        assert_eq!(
            listing.pin_regular(ordinal).unwrap_err(),
            EnumerationError::Access(AccessError::MultipleLinks)
        );
    }
    fs::remove_file(case.path("root/d/g")).unwrap();
    assert!(listing.pin_regular(0).is_ok());
}

#[test]
fn separate_enumerations_are_independent_and_deterministic() {
    let case = Case::new("independent");
    for name in ["c", "a", "b"] {
        case.file(&format!("root/d/{name}"), name);
    }
    let root = case.authorize();
    let first = list(&root, "d");
    let second = list(&root, "d");
    assert_eq!(pinned(&first), pinned(&second));
    assert_eq!(first.status(), second.status());
    assert_eq!(first.directory_object_id(), second.directory_object_id());
}

#[test]
fn diagnostics_never_contain_names_or_paths() {
    let case = Case::new("privacy");
    case.file("root/SECRETDIR/SECRETFILE", "SECRETCONTENT");
    let root = case.authorize();
    let pin = pin_dir(&root, "SECRETDIR");
    let mut text = format!("{pin:?}");
    let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
    text += &format!(
        "{listing:?} {:?} {:?}",
        listing.status(),
        listing.counters()
    );
    let error = listing.pin_regular(9).unwrap_err();
    text += &format!("{error:?} {error}");
    fs::rename(case.path("root/SECRETDIR"), case.path("outside/SECRETDIR")).unwrap();
    let error = listing.pin_regular(0).unwrap_err();
    text += &format!("{error:?} {error}");
    let error = root
        .pin_directory(DirectoryTarget::Relative(&rel("SECRETMISSING")))
        .unwrap_err();
    text += &format!("{error:?} {error}");
    for canary in ["SECRET", "abf18-privacy", "abf11-m2a1-fixtures", "/proc"] {
        assert!(!text.contains(canary), "diagnostic leaked {canary}: {text}");
    }
}

// ---------------------------------------------------------------------------
// Bounded churn (finite stress: supplemental, not a proof)
// ---------------------------------------------------------------------------

#[test]
fn bounded_churn_terminates_within_bounds() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let case = Case::new("churn");
    case.dir("root/d");
    let canary = case.file("outside/canary", "outside canary");
    for i in 0..50 {
        case.file(&format!("root/d/stable-{i:02}"), "stable");
    }
    let root = case.authorize();
    let done = Arc::new(AtomicBool::new(false));
    let churn_done = Arc::clone(&done);
    let base = case.dir.clone();
    let churn = std::thread::spawn(move || {
        for i in 0..1500_u32 {
            let p = |r: &str| base.join(r);
            let name = format!("root/d/churn-{}", i % 40);
            let _ = fs::write(p(&name), "c");
            let _ = fs::rename(p(&name), p(&format!("root/d/moved-{}", i % 30)));
            let _ = fs::remove_file(p(&format!("root/d/moved-{}", (i + 7) % 30)));
            if i % 50 == 0 {
                let _ = symlink(p("outside/canary"), p("root/d/link"));
                let _ = fs::remove_file(p("root/d/link"));
            }
        }
        churn_done.store(true, Ordering::SeqCst);
    });
    let mut listings = 0;
    while !done.load(Ordering::SeqCst) && listings < 400 {
        let listing = list(&root, "d");
        let counters = listing.counters();
        assert!(listing.len() <= 512);
        assert!(counters.read_attempts() <= 64);
        assert!(counters.records_inspected() <= 4096);
        assert!(counters.retained_name_bytes() <= 65_536);
        assert!(!listing.is_empty());
        for i in 0..listing.len().min(5) {
            if let Ok(pin) = listing.pin_regular(i) {
                assert_ne!(pin.evidence().object_id().inode(), canary);
            }
        }
        listings += 1;
    }
    churn.join().unwrap();
    assert!(listings > 0);
    eprintln!("ABF18-CHURN listings completed during churn: {listings}");
}

// ---------------------------------------------------------------------------
// Real descriptor closure, in an isolated child process
// ---------------------------------------------------------------------------

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

    /// Linux allocates the lowest free descriptor number, so in a
    /// single-threaded child any descriptor left open in the probed range
    /// changes this result. No directory census is used.
    pub(super) fn probe(file: &Path) -> Vec<i32> {
        let files: Vec<File> = (0..PROBES).map(|_| File::open(file).unwrap()).collect();
        files.iter().map(|f| f.as_raw_fd()).collect()
    }

    pub(super) fn baseline(file: &Path) -> Vec<i32> {
        let numbers = probe(file);
        let first = numbers[0];
        assert_eq!(numbers, (first..first + PROBES as i32).collect::<Vec<_>>());
        numbers
    }

    pub(super) fn held(file: &Path, baseline: &[i32]) -> usize {
        let probes = probe(file);
        let last = *probes.last().unwrap();
        (baseline[0]..=last).filter(|n| !probes.contains(n)).count()
    }

    fn native_scenarios(case: &Case, root: &SecureRoot, probe_file: &Path, base: &[i32]) {
        // A pin holds one descriptor; a released listing keeps only the pin.
        let pin = pin_dir(root, "d");
        assert_eq!(held(probe_file, base), 1, "pin");
        let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
        assert_eq!(held(probe_file, base), 1, "listing keeps the pin only");
        let child = listing.pin_regular(0).unwrap();
        assert_eq!(held(probe_file, base), 2, "listing + child pin");
        drop(child);
        drop(listing);
        assert_eq!(probe(probe_file), base, "after success");

        // Root target.
        let listing = root
            .pin_directory(DirectoryTarget::Root)
            .unwrap()
            .enumerate(max(), &Cancellation::new())
            .unwrap();
        assert_eq!(held(probe_file, base), 1);
        drop(listing);
        assert_eq!(probe(probe_file), base, "after root target");

        // Partial (budget).
        let partial = pin_dir(root, "d")
            .enumerate(
                EnumerationBudget::new(1, 1, 65_536, 64).unwrap(),
                &Cancellation::new(),
            )
            .unwrap();
        assert_eq!(partial.status().coverage(), Coverage::Partial);
        drop(partial);
        assert_eq!(probe(probe_file), base, "after partial");

        // Pre-cancelled: refused, the consumed pin is closed.
        let cancel = Cancellation::new();
        cancel.request();
        assert!(pin_dir(root, "d").enumerate(max(), &cancel).is_err());
        assert_eq!(probe(probe_file), base, "after cancellation");

        // Binding refusal during enumeration.
        let pin = pin_dir(root, "d");
        fs::rename(case.path("root/d"), case.path("outside/d")).unwrap();
        assert!(pin.enumerate(max(), &Cancellation::new()).is_err());
        fs::rename(case.path("outside/d"), case.path("root/d")).unwrap();
        assert_eq!(probe(probe_file), base, "after binding refusal");

        // Child refusal.
        let listing = list(root, "d");
        fs::hard_link(case.path("root/d/f"), case.path("outside/alias")).unwrap();
        assert!(listing.pin_regular(0).is_err());
        fs::remove_file(case.path("outside/alias")).unwrap();
        drop(listing);
        assert_eq!(probe(probe_file), base, "after child refusal");

        // Directory pin refusal.
        assert!(
            root.pin_directory(DirectoryTarget::Relative(&rel("d/f")))
                .is_err()
        );
        assert_eq!(probe(probe_file), base, "after pin refusal");
        eprintln!("ABF18-CLOSURE native scenarios: success, root, partial, cancel, refusals");
    }

    #[test]
    fn descriptors_close_exactly_in_an_isolated_process() {
        if std::env::var_os(CHILD_ENV).is_none() {
            let out = run_child(
                "closure::descriptors_close_exactly_in_an_isolated_process",
                "closure",
            );
            assert!(out.contains("ABF18-CLOSURE native scenarios"));
            #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
            assert!(out.contains("ABF18-CLOSURE harness scenarios"));
            return;
        }
        let case = Case::new("closure");
        case.file("root/d/f", "f");
        case.file("outside/probe", "probe");
        let probe_file = case.path("outside/probe");
        let root = case.authorize();
        let base = baseline(&probe_file);
        native_scenarios(&case, &root, &probe_file, &base);
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        super::scheduled::closure_harness_scenarios(&case, &root, &probe_file, &base);
        assert_eq!(probe(&probe_file), base, "final");
    }
}

// ---------------------------------------------------------------------------
// Scheduled and injected (two-key harness)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
mod scheduled {
    use super::*;
    use abf_secure_fs::fault_injection::{self as fi, DirentInjection, SchedulePoint, errno};
    use std::cell::Cell;
    use std::rc::Rc;

    // Independent expected values from the installed headers
    // (asm-generic/fcntl.h, linux/openat2.h).
    const O_ACCMODE: i32 = 0o3;
    const O_NONBLOCK: u64 = 0o4000;
    const O_DIRECTORY: u64 = 0o200_000;
    const O_NOFOLLOW: u64 = 0o400_000;
    const O_CLOEXEC: u64 = 0o2_000_000;
    const O_PATH: u64 = 0o10_000_000;
    const FD_CLOEXEC: i32 = 1;
    const DIR_FLAGS: u64 = O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC;
    const FILE_FLAGS: u64 = O_PATH | O_NOFOLLOW | O_CLOEXEC;
    const READ_DIR_FLAGS: u64 = O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK;
    const FULL_MASK: u64 = 0x0F;

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

    /// Runs each action once, at the n-th time (1-based) its point is
    /// reached.
    fn schedule(actions: Vec<(SchedulePoint, usize, Action)>) {
        let mut actions: Vec<(SchedulePoint, usize, Option<Action>)> = actions
            .into_iter()
            .map(|(p, n, a)| (p, n, Some(a)))
            .collect();
        let mut seen: Vec<(SchedulePoint, usize)> = Vec::new();
        fi::set_schedule_hook(move |point| {
            let count = match seen.iter_mut().find(|(p, _)| *p == point) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    seen.push((point, 1));
                    1
                }
            };
            for (at, nth, action) in &mut actions {
                if *at == point
                    && *nth == count
                    && let Some(action) = action.take()
                {
                    action();
                }
            }
        });
    }

    fn at(
        point: SchedulePoint,
        nth: usize,
        action: impl FnOnce() + 'static,
    ) -> (SchedulePoint, usize, Action) {
        (point, nth, Box::new(action))
    }

    fn paths(case: &Case) -> impl Fn(&str) -> PathBuf + Clone + 'static {
        let base = case.dir.clone();
        move |relative: &str| base.join(relative)
    }

    fn calls(log: &[fi::OpenAttempt]) -> Vec<(u64, u64, u64)> {
        log.iter().map(|a| (a.flags, a.mode, a.resolve)).collect()
    }

    /// Independent kernel-layout record builder (8-byte aligned).
    fn record(name: &[u8], ino: u64, kind: u8) -> Vec<u8> {
        let reclen = (19 + name.len() + 1).div_ceil(8) * 8;
        let mut out = Vec::new();
        out.extend_from_slice(&ino.to_ne_bytes());
        out.extend_from_slice(&0x7fff_ffff_i64.to_ne_bytes());
        out.extend_from_slice(&(reclen as u16).to_ne_bytes());
        out.push(kind);
        out.extend_from_slice(name);
        out.resize(reclen, 0);
        out
    }

    // -- exact flags -----------------------------------------------------------

    #[test]
    fn every_directory_open_carries_its_exact_flags_and_full_mask() {
        let _leaks = LeakCheck::start();
        let case = Case::new("flags");
        case.file("root/d/f", "f");
        let root = case.authorize();
        fi::take_openat2_log();

        let pin = pin_dir(&root, "d");
        assert_eq!(
            calls(&fi::take_openat2_log()),
            vec![(DIR_FLAGS, 0, FULL_MASK); 4],
            "binding, acquisition, verification, final binding"
        );
        let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
        let log = calls(&fi::take_openat2_log());
        // start (2), read open (1), post-open (2), 2 attempts x 2, final (2).
        assert_eq!(log.len(), 11, "{log:?}");
        assert_eq!(log[2], (READ_DIR_FLAGS, 0, FULL_MASK));
        assert_eq!(
            log.iter()
                .filter(|c| **c == (READ_DIR_FLAGS, 0, FULL_MASK))
                .count(),
            1
        );
        assert!(
            log.iter()
                .enumerate()
                .all(|(i, c)| i == 2 || *c == (DIR_FLAGS, 0, FULL_MASK))
        );
        assert_eq!(fi::getdents_attempts(), 2);

        // Kernel view of the read handle (sampled right after its open).
        let (status, fd_flags) = fi::last_directory_read_status().unwrap();
        assert_eq!(status & O_ACCMODE, 0, "read-only");
        assert_ne!(status as u64 & O_NONBLOCK, 0, "non-blocking");
        assert_eq!(status as u64 & O_PATH, 0, "not an O_PATH handle");
        assert_eq!(fd_flags & FD_CLOEXEC, FD_CLOEXEC);
        eprintln!("ABF18-FLAGS directory read handle F_GETFL={status:#o} F_GETFD={fd_flags}");

        // Child: wrapper checks around the unchanged four-open file pin.
        let _child = listing.pin_regular(0).unwrap();
        let log = calls(&fi::take_openat2_log());
        assert_eq!(
            log,
            vec![
                (DIR_FLAGS, 0, FULL_MASK),  // root binding
                (DIR_FLAGS, 0, FULL_MASK),  // directory binding
                (DIR_FLAGS, 0, FULL_MASK),  // pin_regular: root binding
                (FILE_FLAGS, 0, FULL_MASK), // acquisition (from the root)
                (FILE_FLAGS, 0, FULL_MASK), // verification (from the root)
                (DIR_FLAGS, 0, FULL_MASK),  // pin_regular: final root binding
                (DIR_FLAGS, 0, FULL_MASK),  // root binding
                (DIR_FLAGS, 0, FULL_MASK),  // directory binding
            ]
        );

        // Root target: binding, `.`, final binding; then enumeration.
        let root_pin = root.pin_directory(DirectoryTarget::Root).unwrap();
        assert_eq!(
            calls(&fi::take_openat2_log()),
            vec![(DIR_FLAGS, 0, FULL_MASK); 3]
        );
        let _root_listing = root_pin.enumerate(max(), &Cancellation::new()).unwrap();
        let log = calls(&fi::take_openat2_log());
        assert_eq!(log.len(), 6, "{log:?}");
        assert!(log.iter().all(|c| c.2 == FULL_MASK && c.1 == 0));
    }

    #[test]
    fn descriptor_peaks_stay_within_the_ceilings() {
        let case = Case::new("peaks");
        case.file("root/d/f", "f");
        let root = case.authorize();
        // The root's two descriptors were opened before this reset.
        fi::reset();
        let listing = list(&root, "d");
        let enumeration_peak = fi::peak_open_descriptors();
        fi::reset_peak_descriptors();
        let child = listing.pin_regular(0).unwrap();
        let child_peak = fi::peak_open_descriptors();
        drop(child);
        drop(listing);
        eprintln!(
            "ABF18-PEAK enumeration {} + root 2; child {} + root 2",
            enumeration_peak, child_peak
        );
        // pin + read + one verifier.
        assert_eq!(enumeration_peak, 3);
        assert!(enumeration_peak + 2 <= 5);
        // listing pin + file + one verifier/binding handle.
        assert_eq!(child_peak, 3);
        assert!(child_peak + 2 <= 6);
        // Everything opened since the reset is closed (the root is not).
        let counters = fi::fd_counters();
        assert_eq!(counters.opened, counters.closed);
        drop(root);
        fi::reset();
    }

    // -- getdents injection (forged directory data) ------------------------------

    fn enumerate_injected(
        name: &str,
        plan: &[Option<DirentInjection>],
    ) -> (Result<(usize, StreamEnd, Coverage), EnumerationError>, u64) {
        fi::reset();
        let (result, attempts) = {
            let case = Case::new(name);
            case.file("root/d/f", "f");
            let root = case.authorize();
            let pin = pin_dir(&root, "d");
            fi::inject_getdents(plan);
            let result = pin
                .enumerate(max(), &Cancellation::new())
                .map(|l| (l.len(), l.status().end(), l.status().coverage()));
            (result, fi::getdents_attempts())
        };
        let counters = fi::fd_counters();
        assert_eq!(counters.opened, counters.closed, "{counters:?}");
        assert_eq!(fi::remaining_getdents_injections(), 0, "{plan:?}");
        (result, attempts)
    }

    #[test]
    fn getdents_errors_are_fatal_never_eof() {
        let _leaks = LeakCheck::start();
        use DirentInjection::{Batch, Errno, Return};
        let valid = [record(b".", 1, 4), record(b"x", 2, 8)].concat();
        let cases: Vec<(Vec<Option<DirentInjection>>, EnumerationError)> = vec![
            (
                vec![Some(Errno(errno::ENOENT))],
                EnumerationError::Access(AccessError::NotFound),
            ),
            (
                vec![Some(Errno(errno::EAGAIN))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EAGAIN)),
            ),
            (
                vec![Some(Errno(errno::EIO))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EIO)),
            ),
            (
                vec![Some(Errno(errno::EINVAL))],
                EnumerationError::Access(AccessError::SecurePrimitiveUnavailable),
            ),
            (
                vec![Some(Errno(errno::EBADF))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EBADF)),
            ),
            (
                vec![Some(Batch(valid.clone())), Some(Errno(errno::ENOENT))],
                EnumerationError::Access(AccessError::NotFound),
            ),
            (
                vec![Some(Return(8193))],
                EnumerationError::MalformedDirectoryData,
            ),
            (
                vec![Some(Return(-1))],
                EnumerationError::MalformedDirectoryData,
            ),
            (
                vec![Some(Errno(errno::EINTR)); 3],
                EnumerationError::Access(AccessError::RetryExhausted),
            ),
        ];
        for (plan, expected) in cases {
            let (result, _) = enumerate_injected("inject_errors", &plan);
            assert_eq!(result.unwrap_err(), expected, "{plan:?}");
        }
        // Two interrupts are retried; the real kernel then answers.
        let (result, attempts) =
            enumerate_injected("inject_eintr", &vec![Some(Errno(errno::EINTR)); 2]);
        assert_eq!(result.unwrap(), (1, StreamEnd::Eof, Coverage::Exhausted));
        assert_eq!(attempts, 4);
    }

    /// F19-1: a fatal `getdents64` attempt (injected answer, real kernel
    /// everywhere else) is still followed by its binding check, visible as a
    /// root and a directory re-resolution in the exact `openat2` log, and the
    /// fatal read error keeps precedence. Opens: start check (2), read open
    /// (1), post-open check (2), then 2 per attempt.
    #[test]
    fn fatal_getdents_attempt_is_followed_by_its_binding_check() {
        use DirentInjection::{Errno, Return};
        let cases: Vec<(&str, Vec<Option<DirentInjection>>, EnumerationError, usize)> = vec![
            (
                "f19_eio",
                vec![Some(Errno(errno::EIO))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EIO)),
                7,
            ),
            (
                "f19_enoent",
                vec![Some(Errno(errno::ENOENT))],
                EnumerationError::Access(AccessError::NotFound),
                7,
            ),
            (
                "f19_eagain",
                vec![Some(Errno(errno::EAGAIN))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EAGAIN)),
                7,
            ),
            (
                "f19_overlong",
                vec![Some(Return(8193))],
                EnumerationError::MalformedDirectoryData,
                7,
            ),
            (
                "f19_second",
                vec![None, Some(Errno(errno::EIO))],
                EnumerationError::Access(AccessError::UnexpectedOs(errno::EIO)),
                9,
            ),
        ];
        for (name, plan, expected, opens) in cases {
            let _leaks = LeakCheck::start();
            let case = Case::new(name);
            case.file("root/SECRETDIR/SECRETFILE", "f");
            let root = case.authorize();
            let pin = pin_dir(&root, "SECRETDIR");
            fi::take_openat2_log();
            fi::inject_getdents(&plan);
            let error = pin.enumerate(max(), &Cancellation::new()).unwrap_err();
            assert_eq!(error, expected, "{name}");
            let log = calls(&fi::take_openat2_log());
            assert_eq!(log.len(), opens, "{name}: {log:?}");
            assert_eq!(
                &log[opens - 2..],
                &[(DIR_FLAGS, 0, FULL_MASK), (DIR_FLAGS, 0, FULL_MASK)],
                "{name}: post-attempt root and directory binding"
            );
            assert_eq!(fi::getdents_attempts(), plan.len() as u64, "no retry");
            assert_eq!(fi::remaining_getdents_injections(), 0);
            let text = format!("{error:?} {error}");
            assert!(!text.contains("SECRET"), "{text}");
        }
    }

    /// F19-1 precedence natively: the directory moves out right after a
    /// fatal attempt. The post-attempt check runs (and observes the move:
    /// its directory re-resolution is attempted), but the fatal read error is
    /// returned, and nothing is released.
    #[test]
    fn fatal_getdents_error_keeps_precedence_over_a_failing_check() {
        let _leaks = LeakCheck::start();
        let (case, _) = standard_case("f19_precedence");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        let p = paths(&case);
        fi::take_openat2_log();
        fi::inject_getdents(&[Some(DirentInjection::Errno(errno::EIO))]);
        schedule(vec![at(
            SchedulePoint::EnumerateAfterAttempt,
            1,
            move || fs::rename(p("root/d"), p("outside/d")).unwrap(),
        )]);
        let result = pin.enumerate(max(), &Cancellation::new());
        fi::clear_schedule_hook();
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::Access(AccessError::UnexpectedOs(errno::EIO))
        );
        let log = calls(&fi::take_openat2_log());
        // start (2), read open (1), post-open (2), post-attempt root (1) and
        // the directory re-resolution that finds nothing (1).
        assert_eq!(log.len(), 7, "{log:?}");
        assert_eq!(fi::getdents_attempts(), 1);
    }

    /// F19-1 cleanup: a panic during the post-attempt check phase of a fatal
    /// attempt closes every descriptor.
    #[test]
    fn panic_after_a_fatal_attempt_closes_every_descriptor() {
        let case = Case::new("f19_panic");
        case.file("root/d/f", "f");
        let root = case.authorize();
        fi::reset();
        {
            let pin = pin_dir(&root, "d");
            fi::inject_getdents(&[Some(DirentInjection::Errno(errno::EIO))]);
            fi::set_schedule_hook(|p| {
                if p == SchedulePoint::EnumerateAfterAttempt {
                    panic!("ABF18B scheduled panic");
                }
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pin.enumerate(max(), &Cancellation::new())
            }));
            fi::clear_schedule_hook();
            assert!(result.is_err());
        }
        let counters = fi::fd_counters();
        assert_eq!(counters.opened, counters.closed, "{counters:?}");
        drop(root);
        fi::reset();
    }

    #[test]
    fn malformed_injected_tail_discards_the_valid_prefix() {
        let _leaks = LeakCheck::start();
        let mut bytes = [record(b"good-a", 1, 8), record(b"good-b", 2, 8)].concat();
        bytes.extend_from_slice(&[0_u8; 19]);
        let (result, _) = enumerate_injected("inject_tail", &[Some(DirentInjection::Batch(bytes))]);
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::MalformedDirectoryData
        );
    }

    /// Forged names, inode numbers and types authorize nothing: each
    /// candidate is judged by a fresh root-relative pin.
    #[test]
    fn forged_entries_never_authorize() {
        let _leaks = LeakCheck::start();
        let case = Case::new("forged");
        let real = case.file("root/d/real", "real");
        let canary = case.file("outside/canary", "outside canary");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        let forged = [
            record(b"canary", canary, 8),
            record(b"ghost", real, 8),
            record(b"real", 999_999, 4),
        ]
        .concat();
        fi::inject_getdents(&[
            Some(DirentInjection::Batch(forged)),
            Some(DirentInjection::Batch(Vec::new())),
        ]);
        let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
        assert_eq!(listing.len(), 3);
        assert_eq!(listing.status().coverage(), Coverage::Exhausted);
        assert_eq!(
            pinned(&listing),
            vec![
                Err(EnumerationError::Access(AccessError::NotFound)),
                Err(EnumerationError::Access(AccessError::NotFound)),
                Ok(real),
            ]
        );
    }

    // -- scheduled binding changes during enumeration ----------------------------

    fn enumerate_with(
        case: &Case,
        actions: Vec<(SchedulePoint, usize, Action)>,
    ) -> Result<Vec<Result<u64, EnumerationError>>, EnumerationError> {
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        schedule(actions);
        let result = pin.enumerate(max(), &Cancellation::new());
        fi::clear_schedule_hook();
        result.map(|listing| pinned(&listing))
    }

    fn standard_case(name: &str) -> (Case, u64) {
        let case = Case::new(name);
        let f = case.file("root/d/f", "original");
        case.file("outside/f-canary", "outside canary");
        (case, f)
    }

    #[test]
    fn directory_moved_out_during_enumeration_discards_everything() {
        for (point, nth) in [
            (SchedulePoint::EnumerateBeforeReadOpen, 1),
            (SchedulePoint::EnumerateAfterReadOpen, 1),
            (SchedulePoint::EnumerateAfterS2, 1),
            (SchedulePoint::EnumerateAfterAttempt, 1),
            (SchedulePoint::EnumerateAfterAttempt, 2),
            (SchedulePoint::EnumerateBeforeFinalCheck, 1),
        ] {
            let _leaks = LeakCheck::start();
            let (case, _) = standard_case("sched_move_out");
            let p = paths(&case);
            let result = enumerate_with(
                &case,
                vec![at(point, nth, move || {
                    fs::rename(p("root/d"), p("outside/d")).unwrap()
                })],
            );
            assert_eq!(
                result.unwrap_err(),
                EnumerationError::DirectoryBindingChanged,
                "{point:?} #{nth}"
            );
        }
    }

    #[test]
    fn replacement_directory_is_never_adopted() {
        let _leaks = LeakCheck::start();
        let (case, _) = standard_case("sched_replace");
        let p = paths(&case);
        let result = enumerate_with(
            &case,
            vec![at(SchedulePoint::EnumerateAfterAttempt, 1, move || {
                fs::rename(p("root/d"), p("outside/d")).unwrap();
                fs::create_dir(p("root/d")).unwrap();
                fs::write(p("root/d/f"), "replacement").unwrap();
            })],
        );
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
    }

    #[test]
    fn root_and_intermediate_changes_during_enumeration_refuse() {
        let _leaks = LeakCheck::start();
        let (case, _) = standard_case("sched_root");
        let p = paths(&case);
        let result = enumerate_with(
            &case,
            vec![at(SchedulePoint::EnumerateAfterAttempt, 1, move || {
                fs::rename(p("root"), p("old-root")).unwrap();
                fs::create_dir_all(p("root/d")).unwrap();
            })],
        );
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::Access(AccessError::RootBindingChanged)
        );
        fs::remove_dir_all(case.path("root")).unwrap();
        fs::rename(case.path("old-root"), case.path("root")).unwrap();

        let case = Case::new("sched_intermediate");
        case.file("root/a/d/f", "f");
        let root = case.authorize();
        let pin = pin_dir(&root, "a/d");
        let p = paths(&case);
        schedule(vec![at(
            SchedulePoint::EnumerateAfterAttempt,
            1,
            move || {
                fs::rename(p("root/a"), p("outside/a")).unwrap();
                fs::create_dir_all(p("root/a/d")).unwrap();
            },
        )]);
        let result = pin.enumerate(max(), &Cancellation::new());
        fi::clear_schedule_hook();
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
    }

    #[test]
    fn held_directory_deleted_during_enumeration_is_refused() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_delete");
        case.dir("root/d");
        let p = paths(&case);
        let result = enumerate_with(
            &case,
            vec![at(SchedulePoint::EnumerateAfterAttempt, 1, move || {
                fs::remove_dir(p("root/d")).unwrap()
            })],
        );
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
    }

    /// Documented limitation: a move out and back entirely between two
    /// checks is not observed, and a move after the final check is not
    /// either. The child request afterwards still refuses the detached
    /// directory.
    #[test]
    fn transient_and_late_moves_are_disclosed_limits() {
        let _leaks = LeakCheck::start();
        let (case, f) = standard_case("sched_transient");
        let p = paths(&case);
        let q = p.clone();
        let result = enumerate_with(
            &case,
            vec![
                at(SchedulePoint::EnumerateAfterReadOpen, 1, move || {
                    fs::rename(p("root/d"), p("outside/d")).unwrap()
                }),
                at(SchedulePoint::EnumerateAfterS2, 1, move || {
                    fs::rename(q("outside/d"), q("root/d")).unwrap()
                }),
            ],
        );
        assert_eq!(result.unwrap(), vec![Ok(f)]);

        let (case, _) = standard_case("sched_late");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        let p = paths(&case);
        schedule(vec![at(
            SchedulePoint::EnumerateBeforeRelease,
            1,
            move || fs::rename(p("root/d"), p("outside/d")).unwrap(),
        )]);
        let listing = pin.enumerate(max(), &Cancellation::new()).unwrap();
        fi::clear_schedule_hook();
        assert_eq!(listing.len(), 1, "moved after the final check: released");
        assert_eq!(
            listing.pin_regular(0).unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
    }

    /// The read handle is `.` of the **held** pin, not a re-resolution of the
    /// locator: while the original is moved out and a replacement sits at
    /// the name, the open still reaches the original. With both restored
    /// before S1, the listing shows the original's entries only.
    #[test]
    fn read_open_is_the_held_object_not_the_current_pathname() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_dot");
        let f = case.file("root/d/f", "original");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        let p = paths(&case);
        let q = p.clone();
        schedule(vec![
            at(SchedulePoint::EnumerateBeforeReadOpen, 1, move || {
                fs::rename(p("root/d"), p("outside/d")).unwrap();
                fs::create_dir(p("root/d")).unwrap();
                fs::write(p("root/d/g"), "replacement entry").unwrap();
            }),
            at(SchedulePoint::EnumerateAfterReadOpen, 1, move || {
                fs::rename(q("root/d"), q("outside/replacement")).unwrap();
                fs::rename(q("outside/d"), q("root/d")).unwrap();
            }),
        ]);
        let listing = pin.enumerate(max(), &Cancellation::new());
        fi::clear_schedule_hook();
        assert_eq!(pinned(&listing.unwrap()), vec![Ok(f)]);
    }

    /// A move out after the first attempt and back after the second is
    /// observed by the check that follows **each** attempt.
    #[test]
    fn move_between_attempts_is_observed_by_the_per_attempt_check() {
        let _leaks = LeakCheck::start();
        let (case, _) = standard_case("sched_per_attempt");
        let p = paths(&case);
        let q = p.clone();
        let result = enumerate_with(
            &case,
            vec![
                at(SchedulePoint::EnumerateAfterAttempt, 1, move || {
                    fs::rename(p("root/d"), p("outside/d")).unwrap()
                }),
                at(SchedulePoint::EnumerateAfterAttempt, 2, move || {
                    fs::rename(q("outside/d"), q("root/d")).unwrap()
                }),
            ],
        );
        assert_eq!(
            result.unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
    }

    /// Cancellation requested on entry refuses before the storage
    /// reservation (a planned allocation refusal is never reached) and before
    /// any open.
    #[test]
    fn precancelled_enumeration_allocates_and_opens_nothing() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_precancel");
        case.file("root/d/f", "f");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        fi::take_openat2_log();
        let before = fi::fd_counters();
        fi::refuse_next_allocation();
        let cancel = Cancellation::new();
        cancel.request();
        assert_eq!(
            pin.enumerate(max(), &cancel).unwrap_err(),
            EnumerationError::Cancelled
        );
        assert!(fi::take_openat2_log().is_empty());
        assert_eq!(fi::fd_counters().opened, before.opened);
        assert_eq!(fi::getdents_attempts(), 0);
        // The unconsumed refusal flag is thread-local and dies with the test
        // thread (each harness test starts with a reset).
    }

    // -- S0/S1/S2 (injected identity: forged evidence) ---------------------------

    #[test]
    fn s0_s1_s2_refuse_a_different_or_unlinked_object() {
        // Inspection order in enumerate: start check [root, dir, held],
        // S0 held, S1 read, S2 held.
        for (index, expected) in [
            (3usize, EnumerationError::DirectoryBindingChanged),
            (4, EnumerationError::Access(AccessError::ObjectChanged)),
            (5, EnumerationError::DirectoryBindingChanged),
        ] {
            let _leaks = LeakCheck::start();
            let case = Case::new("sched_s1");
            case.file("root/d/f", "f");
            let d = fs::symlink_metadata(case.path("root/d")).unwrap();
            let root = case.authorize();
            let pin = pin_dir(&root, "d");
            let mut plan = vec![None; index];
            plan.push(Some((d.dev(), d.ino() + 1)));
            fi::inject_identity(&plan);
            assert_eq!(
                pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
                expected,
                "inspection {index}"
            );
        }
    }

    #[test]
    fn directory_pin_policy_refusals_with_forged_evidence() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_policy");
        case.file("root/d/f", "f");
        let d = fs::symlink_metadata(case.path("root/d")).unwrap();
        let root = case.authorize();
        let pin = |root: &SecureRoot| {
            root.pin_directory(DirectoryTarget::Relative(&rel("d")))
                .map(|p| p.object_id())
        };
        // Pin inspections: [root binding, held, verifier, root binding, held].
        fi::inject_identity(&[None, Some((d.dev() + 1, d.ino()))]);
        assert_eq!(pin(&root), Err(AccessError::ConfinementRefused));
        fi::inject_identity(&[None, None, Some((d.dev(), d.ino() + 1))]);
        assert_eq!(pin(&root), Err(AccessError::ObjectChanged));
        fi::inject_identity(&[None, Some((0, d.ino()))]);
        assert_eq!(pin(&root), Err(AccessError::IdentityUnavailable));
        fi::inject_filesystem_magic(Some(0x0102_1994));
        assert_eq!(pin(&root), Err(AccessError::UnsupportedFilesystem));
        fi::inject_filesystem_magic(None);
        // Root target: `.` must be the held root. Inspections: [root
        // binding, held, root binding, held]. A forged identity on the first
        // sample only is caught by the final same-object check too; a
        // consistent one (both samples) only by the root identity check.
        let r = fs::symlink_metadata(case.path("root")).unwrap();
        let other = Some((r.dev(), r.ino() + 1));
        fi::inject_identity(&[None, other]);
        assert_eq!(
            root.pin_directory(DirectoryTarget::Root).unwrap_err(),
            AccessError::ObjectChanged
        );
        fi::inject_identity(&[None, other, None, other]);
        assert_eq!(
            root.pin_directory(DirectoryTarget::Root).unwrap_err(),
            AccessError::ObjectChanged
        );
    }

    #[test]
    fn directory_changed_during_pin_is_refused() {
        for point in [
            SchedulePoint::DirectoryPinAfterAcquire,
            SchedulePoint::DirectoryPinAfterVerify,
        ] {
            let _leaks = LeakCheck::start();
            let case = Case::new("sched_pin");
            case.file("root/d/f", "f");
            let root = case.authorize();
            let p = paths(&case);
            schedule(vec![at(point, 1, move || {
                fs::rename(p("root/d"), p("outside/d")).unwrap();
                symlink(p("outside/d"), p("root/d")).unwrap();
            })]);
            let result = root.pin_directory(DirectoryTarget::Relative(&rel("d")));
            fi::clear_schedule_hook();
            match point {
                // Verification no longer reaches the held object.
                SchedulePoint::DirectoryPinAfterAcquire => {
                    assert_eq!(result.unwrap_err(), AccessError::ObjectChanged)
                }
                // Root still binds; the pin was verified before the swap.
                // The first enumeration check then refuses it.
                _ => assert_eq!(
                    result
                        .unwrap()
                        .enumerate(max(), &Cancellation::new())
                        .unwrap_err(),
                    EnumerationError::DirectoryBindingChanged
                ),
            }
        }
    }

    // -- read open failures ------------------------------------------------------

    #[test]
    fn read_open_failures_are_coded_and_bounded() {
        for (plan, expected) in [
            (
                vec![Some(errno::EACCES)],
                EnumerationError::Access(AccessError::Inaccessible),
            ),
            (
                vec![Some(errno::EINTR); 3],
                EnumerationError::Access(AccessError::RetryExhausted),
            ),
            (
                vec![Some(errno::ENOSYS)],
                EnumerationError::Access(AccessError::SecurePrimitiveUnavailable),
            ),
        ] {
            let _leaks = LeakCheck::start();
            let case = Case::new("sched_read_open");
            case.file("root/d/f", "f");
            let root = case.authorize();
            let pin = pin_dir(&root, "d");
            // Start check: two binding opens, then the read open.
            let mut full = vec![None, None];
            full.extend(plan.iter().copied());
            fi::take_openat2_log();
            fi::inject_openat2(&full);
            assert_eq!(
                pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
                expected
            );
            let log = calls(&fi::take_openat2_log());
            assert_eq!(log.len(), 2 + plan.len());
            assert!(
                log[2..]
                    .iter()
                    .all(|c| *c == (READ_DIR_FLAGS, 0, FULL_MASK))
            );
        }
    }

    // -- allocation, cancellation and panic ---------------------------------------

    #[test]
    fn allocation_refusal_opens_nothing() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_alloc");
        case.file("root/d/f", "f");
        let root = case.authorize();
        let pin = pin_dir(&root, "d");
        fi::take_openat2_log();
        let before = fi::fd_counters();
        fi::refuse_next_allocation();
        assert_eq!(
            pin.enumerate(max(), &Cancellation::new()).unwrap_err(),
            EnumerationError::AllocationRefused
        );
        assert!(fi::take_openat2_log().is_empty(), "no open after refusal");
        assert_eq!(fi::fd_counters().opened, before.opened);
        assert_eq!(fi::getdents_attempts(), 0);
    }

    #[test]
    fn cancellation_at_each_stage() {
        // Before the start check: refusal (nothing read).
        // After the first attempt: Partial(Cancelled) with nothing kept.
        // Before release: Cancelled wins over a real end of stream.
        for (point, nth) in [
            (SchedulePoint::EnumerateBeforeStartCheck, 1),
            (SchedulePoint::EnumerateBeforeReadOpen, 1),
            (SchedulePoint::EnumerateAfterAttempt, 1),
            (SchedulePoint::EnumerateBeforeRelease, 1),
        ] {
            let _leaks = LeakCheck::start();
            let case = Case::new("sched_cancel");
            case.file("root/d/a", "a");
            case.file("root/d/b", "b");
            let root = case.authorize();
            let pin = pin_dir(&root, "d");
            let cancel = Rc::new(Cancellation::new());
            let flag = Rc::clone(&cancel);
            schedule(vec![at(point, nth, move || flag.request())]);
            let result = pin.enumerate(max(), &cancel);
            fi::clear_schedule_hook();
            match point {
                SchedulePoint::EnumerateBeforeStartCheck
                | SchedulePoint::EnumerateBeforeReadOpen => {
                    assert_eq!(
                        result.unwrap_err(),
                        EnumerationError::Cancelled,
                        "{point:?}"
                    )
                }
                SchedulePoint::EnumerateAfterAttempt => {
                    let listing = result.unwrap();
                    assert_eq!(listing.status().end(), StreamEnd::Cancelled);
                    assert_eq!(listing.status().coverage(), Coverage::Partial);
                    assert_eq!(listing.len(), 0);
                    assert_eq!(listing.counters().records_inspected(), 0);
                }
                _ => {
                    let listing = result.unwrap();
                    assert_eq!(listing.status().end(), StreamEnd::Cancelled);
                    assert_eq!(listing.status().coverage(), Coverage::Partial);
                    assert_eq!(listing.len(), 2, "kept candidates are released");
                }
            }
        }
    }

    const PANIC_POINTS: [SchedulePoint; 10] = [
        SchedulePoint::DirectoryPinAfterAcquire,
        SchedulePoint::DirectoryPinAfterVerify,
        SchedulePoint::EnumerateBeforeStartCheck,
        SchedulePoint::EnumerateBeforeReadOpen,
        SchedulePoint::EnumerateAfterReadOpen,
        SchedulePoint::EnumerateAfterS2,
        SchedulePoint::EnumerateAfterAttempt,
        SchedulePoint::EnumerateBeforeFinalCheck,
        SchedulePoint::EnumerateBeforeRelease,
        SchedulePoint::ChildBeforePin,
    ];

    fn panic_run(root: &SecureRoot, point: SchedulePoint) {
        fi::set_schedule_hook(move |p| {
            if p == point {
                panic!("ABF18 scheduled panic");
            }
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let listing = list(root, "d");
            let _ = listing.pin_regular(0);
        }));
        fi::clear_schedule_hook();
        assert!(result.is_err(), "{point:?} must be reached");
    }

    #[test]
    fn panic_at_every_point_closes_every_descriptor() {
        for point in PANIC_POINTS {
            let case = Case::new("sched_panic");
            case.file("root/d/f", "f");
            let root = case.authorize();
            fi::reset();
            panic_run(&root, point);
            let counters = fi::fd_counters();
            assert_eq!(counters.opened, counters.closed, "{point:?}: {counters:?}");
        }
        fi::reset();
    }

    // -- child authorization schedules -----------------------------------------

    /// Between the pre-check and the root-relative pin, the directory moves
    /// outside and a replacement directory with a same-named regular file
    /// takes its place. The root-relative pin can only reach the replacement
    /// (never the moved-out file through the stale directory); the post
    /// check then refuses and drops it.
    #[test]
    fn child_pin_is_anchored_at_the_root_not_the_held_directory() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_child_stale");
        let original = case.file("root/d/f", "original");
        let root = case.authorize();
        let listing = list(&root, "d");
        let p = paths(&case);
        let reached = Rc::new(Cell::new(None));
        let seen = Rc::clone(&reached);
        let q = p.clone();
        schedule(vec![
            at(SchedulePoint::ChildBeforePin, 1, move || {
                fs::rename(p("root/d"), p("outside/d")).unwrap();
                fs::create_dir(p("root/d")).unwrap();
                fs::write(p("root/d/f"), "replacement").unwrap();
            }),
            at(SchedulePoint::ChildAfterPin, 1, move || {
                seen.set(Some(ino(&q("root/d/f"))));
            }),
        ]);
        let result = listing.pin_regular(0);
        fi::clear_schedule_hook();
        assert_eq!(
            result.map(|p| p.evidence().object_id().inode()),
            Err(EnumerationError::DirectoryBindingChanged)
        );
        let replacement = reached
            .get()
            .expect("the root-relative pin succeeded first");
        assert_ne!(replacement, original);
    }

    #[test]
    fn child_moved_out_with_its_directory_before_the_pin_is_not_found() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_child_gone");
        case.file("root/d/f", "original");
        let root = case.authorize();
        let listing = list(&root, "d");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::ChildBeforePin, 1, move || {
            fs::rename(p("root/d"), p("outside/d")).unwrap()
        })]);
        let result = listing
            .pin_regular(0)
            .map(|p| p.evidence().object_id().inode());
        fi::clear_schedule_hook();
        // Resolved from the root: the name no longer exists there.
        assert_eq!(result, Err(EnumerationError::Access(AccessError::NotFound)));
    }

    /// The directory moves out before the child pin and back right after a
    /// successful one. Resolution from the root cannot find the name while
    /// it is outside; a resolution anchored at the held directory would
    /// have acquired the outside file and passed the restored post-check.
    #[test]
    fn child_resolution_during_a_transient_move_starts_from_the_root() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_child_transient");
        let original = case.file("root/d/f", "original");
        let root = case.authorize();
        let listing = list(&root, "d");
        let p = paths(&case);
        let q = p.clone();
        schedule(vec![
            at(SchedulePoint::ChildBeforePin, 1, move || {
                fs::rename(p("root/d"), p("outside/d")).unwrap()
            }),
            at(SchedulePoint::ChildAfterPin, 1, move || {
                fs::rename(q("outside/d"), q("root/d")).unwrap()
            }),
        ]);
        let result = listing
            .pin_regular(0)
            .map(|p| p.evidence().object_id().inode());
        fi::clear_schedule_hook();
        assert_eq!(result, Err(EnumerationError::Access(AccessError::NotFound)));
        assert_ne!(result, Ok(original));
    }

    #[test]
    fn child_replaced_by_outside_symlink_before_the_pin_is_refused() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_child_link");
        case.file("root/d/f", "original");
        let canary = case.file("outside/canary", "outside canary");
        let root = case.authorize();
        let listing = list(&root, "d");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::ChildBeforePin, 1, move || {
            fs::remove_file(p("root/d/f")).unwrap();
            symlink(p("outside/canary"), p("root/d/f")).unwrap();
        })]);
        let result = listing
            .pin_regular(0)
            .map(|p| p.evidence().object_id().inode());
        fi::clear_schedule_hook();
        assert_eq!(
            result,
            Err(EnumerationError::Access(AccessError::NotRegularFile(
                ObjectKind::Symlink
            )))
        );
        assert_ne!(result, Ok(canary));
    }

    #[test]
    fn child_directory_moved_after_the_pin_is_refused_and_dropped() {
        let _leaks = LeakCheck::start();
        let case = Case::new("sched_child_after");
        case.file("root/d/f", "original");
        let root = case.authorize();
        let listing = list(&root, "d");
        let p = paths(&case);
        schedule(vec![at(SchedulePoint::ChildAfterPin, 1, move || {
            fs::rename(p("root/d"), p("outside/d")).unwrap()
        })]);
        assert_eq!(
            listing.pin_regular(0).unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
        fi::clear_schedule_hook();
    }

    // -- ledger --------------------------------------------------------------------

    #[test]
    fn retained_ledger_uses_actual_capacities() {
        let _leaks = LeakCheck::start();
        let case = Case::new("ledger");
        case.file("root/dir-name/f", "f");
        let root = case.authorize();
        let listing = list(&root, "dir-name");
        let ledger = listing.harness_retained_ledger();
        eprintln!("ABF18-LEDGER native maximum-budget listing: {ledger:?}");
        assert!(ledger.arena_capacity >= 65_536);
        assert!(ledger.slots_capacity_bytes >= 512 * 8);
        // Locator "dir-name" + NUL, plus the inline listing size.
        assert!(ledger.fixed > "dir-name".len() + 1);
        assert_eq!(
            ledger.total,
            ledger.arena_capacity + ledger.slots_capacity_bytes + ledger.fixed
        );
        assert!(ledger.total <= abf_secure_fs::MAX_RETAINED_OUTPUT_BYTES);

        let small = pin_dir(&root, "dir-name")
            .enumerate(
                EnumerationBudget::new(8, 2, 16, 4).unwrap(),
                &Cancellation::new(),
            )
            .unwrap();
        let ledger = small.harness_retained_ledger();
        assert!(ledger.arena_capacity >= 16 && ledger.slots_capacity_bytes >= 16);
        assert!(ledger.total < 4096);
    }

    // -- closure (child process) ---------------------------------------------------

    pub(super) fn closure_harness_scenarios(
        case: &Case,
        root: &SecureRoot,
        probe_file: &Path,
        base: &[i32],
    ) {
        use super::closure::probe;
        fi::reset();

        // Injected getdents failure after a valid batch.
        let pin = pin_dir(root, "d");
        fi::inject_getdents(&[
            Some(DirentInjection::Batch(record(b"x", 1, 8))),
            Some(DirentInjection::Errno(errno::EIO)),
        ]);
        assert!(pin.enumerate(max(), &Cancellation::new()).is_err());
        assert_eq!(probe(probe_file), base, "after injected read error");

        // Mid-enumeration cancellation (released Partial listing).
        let pin = pin_dir(root, "d");
        let cancel = Rc::new(Cancellation::new());
        let flag = Rc::clone(&cancel);
        schedule(vec![at(
            SchedulePoint::EnumerateAfterAttempt,
            1,
            move || flag.request(),
        )]);
        let listing = pin.enumerate(max(), &cancel).unwrap();
        fi::clear_schedule_hook();
        assert_eq!(listing.status().end(), StreamEnd::Cancelled);
        drop(listing);
        assert_eq!(probe(probe_file), base, "after mid cancellation");

        // Allocation refusal.
        fi::refuse_next_allocation();
        assert!(
            pin_dir(root, "d")
                .enumerate(max(), &Cancellation::new())
                .is_err()
        );
        assert_eq!(probe(probe_file), base, "after allocation refusal");

        // Binding refusal during the scan.
        let p = paths(case);
        let q = p.clone();
        let pin = pin_dir(root, "d");
        schedule(vec![at(
            SchedulePoint::EnumerateAfterAttempt,
            1,
            move || fs::rename(p("root/d"), p("outside/d")).unwrap(),
        )]);
        assert!(pin.enumerate(max(), &Cancellation::new()).is_err());
        fi::clear_schedule_hook();
        fs::rename(q("outside/d"), q("root/d")).unwrap();
        assert_eq!(probe(probe_file), base, "after scheduled binding refusal");

        // Panic at every point.
        for point in PANIC_POINTS {
            panic_run(root, point);
            assert_eq!(probe(probe_file), base, "after panic at {point:?}");
        }
        fi::reset();
        eprintln!(
            "ABF18-CLOSURE harness scenarios: read error, cancel, allocation, binding, panics"
        );
    }
}
