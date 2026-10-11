//! ABF M2A.1 — secure Linux file-object pinning (metadata only).
//!
//! A [`SecureRoot`] holds the directory **object** the caller authorized,
//! acquired with Linux `openat2` under a mandatory confinement mask on the
//! same ext-family mount as `/`. [`SecureRoot::pin_regular`] resolves a
//! validated relative path beneath that held object and returns a
//! [`PinnedRegularFile`]: an owned, opaque `O_PATH` handle to a single-link
//! regular file, plus handle-derived [`FileEvidence`].
//!
//! Scope and limits:
//!
//! - **Supported:** `x86_64-unknown-linux-gnu` only (64-bit pointers, the
//!   LP64 ABI; GNU x32 is excluded), with `openat2` (Linux ≥ 5.6) and an
//!   ext-family root mount. Everything else fails closed with
//!   [`AccessError::SecurePrimitiveUnavailable`] or
//!   [`AccessError::UnsupportedFilesystem`]. There is no weaker fallback.
//! - **Metadata only:** nothing reads file contents, enumerates directories,
//!   or opens an object for I/O. Readable promotion is not implemented.
//! - **Confinement comes from the kernel:** every resolution beneath a held
//!   directory uses `openat2` with the mandatory mask
//!   `BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`. The crate relies on the
//!   documented semantics of these flags, under a trusted kernel. The second
//!   (verification) resolution checks identity; it is not by itself a proof
//!   of confinement.
//! - **Object capability, not path membership:** replacements of the root
//!   path never redirect the held root, and observed binding changes are
//!   refused. A transient rename restored between checks is not guaranteed
//!   to be detected. Continuous path membership is **not** claimed, and
//!   nothing here establishes where file contents came from.
//! - No raw descriptors, `Read` implementations, absolute reopen paths or
//!   caller-supplied identities cross this API. Errors are codes only.
//!
//! Production collection stays BLOCKED. See `README.md`.

// Without a backend, the OS-facing helpers (descriptor-ready path forms,
// evidence construction, schedule points) are intentionally unused.
#![cfg_attr(
    not(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu",
        target_pointer_width = "64",
        not(target_abi = "x32"),
        not(abf_secure_fs_force_unsupported)
    )),
    allow(dead_code, unused_macros)
)]

// The `fault-injection` harness can forge kernel evidence (identity,
// filesystem magic). A Cargo feature can be enabled by `--all-features` or by
// any other crate in the dependency graph, so the feature alone is never
// enough: the dedicated cfg must also be set deliberately, by the test
// command itself.
#[cfg(all(feature = "fault-injection", not(abf_secure_fs_harness)))]
compile_error!(
    "abf-secure-fs: the `fault-injection` feature is a test-only harness that can forge \
     kernel evidence. It compiles only with `--cfg abf_secure_fs_harness` in an isolated \
     synthetic test build. Do not enable it with `--all-features` or from a dependency."
);

macro_rules! schedule_point {
    ($point:ident) => {
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        crate::linux::schedule::fire(crate::linux::schedule::SchedulePoint::$point);
    };
}

pub mod error;
pub mod path;
mod sys;

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
mod linux;

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
use linux as imp;

pub use error::{AccessError, ObjectKind, PathError};
pub use path::{
    MAX_COMPONENT_BYTES, MAX_RELATIVE_BYTES, MAX_RELATIVE_COMPONENTS, MAX_ROOT_BYTES,
    MAX_ROOT_COMPONENTS, RelativePath, check_root_path,
};

/// Handle-derived object identity: device and inode numbers, both concrete
/// (non-zero). It cannot be constructed outside this crate, so it is never
/// accepted as caller-supplied proof.
///
/// Identity is instantaneous evidence. An inode number can be reused after
/// the object is deleted. It does not identify a logical event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId {
    device: u64,
    inode: u64,
}

impl ObjectId {
    pub fn device(&self) -> u64 {
        self.device
    }

    pub fn inode(&self) -> u64 {
        self.inode
    }
}

/// Handle-derived metadata of a pinned regular file at the time it was
/// pinned (or reinspected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEvidence {
    object: ObjectId,
    link_count: u64,
    size: u64,
}

impl FileEvidence {
    pub(crate) fn new(object: ObjectId, link_count: u64, size: u64) -> Self {
        Self {
            object,
            link_count,
            size,
        }
    }

    pub fn object_id(&self) -> ObjectId {
        self.object
    }

    /// Always 1 for evidence returned by this crate.
    pub fn link_count(&self) -> u64 {
        self.link_count
    }

    /// Size in bytes reported by the handle metadata. Not a content read.
    pub fn size(&self) -> u64 {
        self.size
    }
}

/// An authorized root directory object. Owns its descriptors and closes them
/// on drop. Not `Clone`: a capability is never duplicated.
pub struct SecureRoot {
    inner: imp::Root,
}

impl std::fmt::Debug for SecureRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureRoot")
            .field("object", &self.inner.id())
            .finish_non_exhaustive()
    }
}

impl SecureRoot {
    /// Authorizes the directory currently bound at `root`, an explicit
    /// absolute path (no `~`, no variables, no normalization).
    ///
    /// The root must be reachable from `/` without symlinks, magic links or
    /// mount crossings, and be on the same ext-family mount as `/`.
    pub fn authorize(root: &str) -> Result<Self, AccessError> {
        imp::Root::authorize(root).map(|inner| Self { inner })
    }

    /// Identity of the held root directory object.
    pub fn object_id(&self) -> ObjectId {
        self.inner.id()
    }

    /// Checks that the root path still binds the originally authorized
    /// object. A replacement is reported as
    /// [`AccessError::RootBindingChanged`] and is never adopted. Passing
    /// shows the binding at the moment of the check only.
    pub fn check_binding(&self) -> Result<(), AccessError> {
        self.inner.check_binding()
    }

    /// Pins the single-link regular file at `relative` beneath the held root.
    ///
    /// Root binding is checked before and after the acquisition **by this
    /// method itself**. Callers do not need to call
    /// [`SecureRoot::check_binding`] for correctness. The returned handle
    /// keeps the original object; later replacements of the path do not
    /// affect it.
    pub fn pin_regular(&self, relative: &RelativePath) -> Result<PinnedRegularFile, AccessError> {
        let (inner, evidence) = self.inner.pin_regular(relative)?;
        Ok(PinnedRegularFile { inner, evidence })
    }
}

/// A pinned single-link regular file. Owns an `O_PATH` handle (no read or
/// write access) and closes it on drop. Not `Clone`.
pub struct PinnedRegularFile {
    inner: imp::Pinned,
    evidence: FileEvidence,
}

impl std::fmt::Debug for PinnedRegularFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedRegularFile")
            .field("evidence", &self.evidence)
            .finish_non_exhaustive()
    }
}

impl PinnedRegularFile {
    /// Evidence captured when the file was pinned.
    pub fn evidence(&self) -> &FileEvidence {
        &self.evidence
    }

    /// Fresh evidence from the held handle (never from a path). Fails if the
    /// held object now has more than one link or none.
    pub fn reinspect(&self) -> Result<FileEvidence, AccessError> {
        self.inner.reinspect()
    }
}

/// Compiled backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Backend {
    /// `x86_64-unknown-linux-gnu` with 64-bit pointers (LP64), openat2-based.
    /// Never selected for GNU x32.
    LinuxX86_64Gnu,
    /// No backend: every authorization fails with
    /// [`AccessError::SecurePrimitiveUnavailable`].
    Unavailable,
}

/// What this build implements. This is a compile-time description. Runtime
/// availability of `openat2` and of an admitted filesystem is established
/// only by [`SecureRoot::authorize`]. It never describes or attests a WSL
/// distribution or machine identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BackendCapabilities {
    pub backend: Backend,
    /// Root authorization and regular-file metadata pinning.
    pub metadata_pinning: bool,
    /// Resolve mask required for every resolution beneath a held directory
    /// (`RESOLVE_BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`), or 0 without a
    /// backend.
    pub required_resolve_mask: u64,
    /// Always false in M2A.1.
    pub readable_promotion: bool,
    /// Always false in M2A.1.
    pub directory_enumeration: bool,
    /// Always false: a Linux build does not establish that it runs in a
    /// particular WSL distribution.
    pub environment_attested: bool,
}

impl BackendCapabilities {
    pub const fn compiled() -> Self {
        #[cfg(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu",
            target_pointer_width = "64",
            not(target_abi = "x32"),
            not(abf_secure_fs_force_unsupported)
        ))]
        let (backend, metadata_pinning, required_resolve_mask) =
            (Backend::LinuxX86_64Gnu, true, linux::REQUIRED_RESOLVE);
        #[cfg(not(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu",
            target_pointer_width = "64",
            not(target_abi = "x32"),
            not(abf_secure_fs_force_unsupported)
        )))]
        let (backend, metadata_pinning, required_resolve_mask) = (Backend::Unavailable, false, 0);
        Self {
            backend,
            metadata_pinning,
            required_resolve_mask,
            readable_promotion: false,
            directory_enumeration: false,
            environment_attested: false,
        }
    }
}

/// Fail-closed stand-in for every other target (and for the
/// `abf_secure_fs_force_unsupported` cfg used to test this path). The types
/// are uninhabited: no root or pinned file can ever exist.
#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
)))]
mod imp {
    use crate::{AccessError, FileEvidence, ObjectId, RelativePath};

    #[derive(Debug)]
    pub(crate) enum Root {}

    #[derive(Debug)]
    pub(crate) enum Pinned {}

    impl Root {
        pub(crate) fn authorize(root: &str) -> Result<Self, AccessError> {
            crate::path::check_root_path(root)?;
            Err(AccessError::SecurePrimitiveUnavailable)
        }

        pub(crate) fn id(&self) -> ObjectId {
            match *self {}
        }

        pub(crate) fn check_binding(&self) -> Result<(), AccessError> {
            match *self {}
        }

        pub(crate) fn pin_regular(
            &self,
            _relative: &RelativePath,
        ) -> Result<(Pinned, FileEvidence), AccessError> {
            match *self {}
        }
    }

    impl Pinned {
        pub(crate) fn reinspect(&self) -> Result<FileEvidence, AccessError> {
            match *self {}
        }
    }
}

/// Test-only harness: compiled only with the `fault-injection` feature
/// **and** `--cfg abf_secure_fs_harness`, on the supported target. A build
/// with the feature but without the cfg does not compile (see the
/// `compile_error!` above).
///
/// It can inject errnos **instead of** real `openat2` calls, record the
/// exact `open_how` of every attempt (an unbounded per-thread log), override
/// the reported filesystem magic or object identity, count descriptor
/// opens/closes and run schedule hooks. The overrides **forge** kernel
/// evidence: they can make a refused root, a replaced binding or a different
/// object look admitted. That is why the module exists only in a deliberate
/// harness build. It cannot change any open flag or resolve mask. All state
/// is thread-local.
#[cfg(all(
    feature = "fault-injection",
    abf_secure_fs_harness,
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
pub mod fault_injection {
    use crate::sys::linux_x86_64::fault;

    pub use crate::linux::schedule::SchedulePoint;

    /// Errno values for injection (Linux x86_64).
    pub mod errno {
        pub const EPERM: i32 = 1;
        pub const ENOENT: i32 = 2;
        pub const EINTR: i32 = 4;
        pub const E2BIG: i32 = 7;
        pub const EAGAIN: i32 = 11;
        pub const EINVAL: i32 = 22;
        pub const ENOSYS: i32 = 38;
    }

    /// One recorded `openat2` attempt: the exact `struct open_how` that was
    /// (or, if injected, would have been) passed to the kernel.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenAttempt {
        pub flags: u64,
        pub mode: u64,
        pub resolve: u64,
        /// `Some(errno)` if this attempt was answered by injection instead
        /// of the kernel.
        pub injected_errno: Option<i32>,
    }

    /// Descriptors wrapped and closed on this thread since the last reset.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FdCounters {
        pub opened: u64,
        pub closed: u64,
    }

    /// Plan for the next `openat2` calls on this thread, in order:
    /// `Some(errno)` answers that call with `errno` without calling the
    /// kernel, and `None` makes the real call.
    pub fn inject_openat2(plan: &[Option<i32>]) {
        fault::set_plan(plan);
    }

    /// Plan entries not consumed yet.
    pub fn remaining_injections() -> usize {
        fault::remaining_plan()
    }

    pub fn take_openat2_log() -> Vec<OpenAttempt> {
        fault::take_log()
            .into_iter()
            .map(|(how, injected_errno)| OpenAttempt {
                flags: how.flags,
                mode: how.mode,
                resolve: how.resolve,
                injected_errno,
            })
            .collect()
    }

    /// Report `magic` instead of the real `fstatfs` result.
    pub fn inject_filesystem_magic(magic: Option<i64>) {
        fault::set_magic(magic);
    }

    /// Plan for the next handle inspections on this thread, in order:
    /// `Some((device, inode))` replaces the identity the kernel reported for
    /// that inspection, and `None` keeps it. Used to reach the zero-identity
    /// and foreign-device refusals without creating devices or mounts.
    pub fn inject_identity(plan: &[Option<(u64, u64)>]) {
        fault::set_identity_plan(plan);
    }

    pub fn fd_counters() -> FdCounters {
        let (opened, closed) = fault::counters();
        FdCounters { opened, closed }
    }

    /// Runs `hook` synchronously at every [`SchedulePoint`] on this thread.
    /// The hook must not call `set_schedule_hook`/`clear_schedule_hook`.
    pub fn set_schedule_hook(hook: impl FnMut(SchedulePoint) + 'static) {
        crate::linux::schedule::set(Box::new(hook));
    }

    pub fn clear_schedule_hook() {
        crate::linux::schedule::clear();
    }

    /// Clears the plans, log, overrides, counters and hook.
    pub fn reset() {
        fault::reset();
        crate::linux::schedule::clear();
    }
}
