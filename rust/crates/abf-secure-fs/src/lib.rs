//! ABF M2A.1 — secure Linux file-object pinning, and ABF M2A.2a — readable
//! promotion of a pinned file (experimental, synthetic research only).
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
//! - **Pinning is metadata only:** pinning never reads file contents,
//!   enumerates directories, or opens an object for I/O.
//! - **Directory enumeration (M2A.2b):** [`SecureRoot::pin_directory`] pins
//!   the root or a validated directory beneath it as a [`PinnedDirectory`].
//!   [`PinnedDirectory::enumerate`] consumes it and performs **one** bounded
//!   `getdents64` discovery of that single directory (no recursion) through
//!   a private read-only self-open of `.` relative to the held pin, returning
//!   a [`DirectoryListing`]. Names stay private: candidates are opaque
//!   ordinals, and [`DirectoryListing::pin_regular`] authorizes one of them
//!   **afresh** through [`SecureRoot::pin_regular`]'s root-relative protocol.
//!   Discovery is not authorization, and no content is read. See
//!   `README.md`, "Directory enumeration".
//! - **Readable promotion (M2A.2a):** [`PinnedRegularFile::promote`]
//!   consumes a pin and returns a [`ReadableRegularFile`] that owns the
//!   original `O_PATH` handle **and** a new read-only descriptor of the
//!   **same** object, reopened through a private per-call procfs anchor of
//!   the current thread (never through a pathname of the file). The default
//!   API exposes no way to read bytes; only the two-key test harness has a
//!   bounded synthetic read. See `README.md`, "Readable promotion".
//! - **Confinement comes from the kernel:** every resolution of a source
//!   path beneath a held directory (root, binding checks, file and
//!   directory pins, the directory self-open) uses `openat2` with the
//!   mandatory mask `BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`; the
//!   process-root anchor `/` uses `NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`. The
//!   only other masks are the private procfs opens of readable promotion,
//!   which never resolve a source path (README). The crate relies on the
//!   documented semantics of these flags, under a trusted kernel. The second
//!   (verification) resolution checks identity; it is not by itself a proof
//!   of confinement.
//! - **Object capability, not path membership:** replacements of the root
//!   path never redirect the held root, and observed binding changes are
//!   refused. A transient rename restored between checks is not guaranteed
//!   to be detected. Continuous path membership is **not** claimed, and
//!   nothing here establishes where file contents came from.
//! - No raw descriptors, `File`, `Read`/`Seek` implementations, procfs
//!   paths, absolute reopen paths or caller-supplied identities cross this
//!   API. Errors are codes only.
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

mod enumeration;
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
mod promotion;

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
mod directory;

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
mod imp {
    pub(crate) use crate::directory::{Directory, Listing, Target as DirectoryPinTarget};
    pub(crate) use crate::linux::{Pinned, Root};
    pub(crate) use crate::promotion::Readable;
}

pub use enumeration::{
    BudgetKind, Cancellation, Coverage, DiscoveryCounters, DiscoveryStatus, EnumerationBudget,
    MAX_CANDIDATES, MAX_CONSECUTIVE_INTERRUPTS, MAX_INSPECTED_RECORDS, MAX_READ_ATTEMPTS,
    MAX_RETAINED_NAME_BYTES, MAX_RETAINED_OUTPUT_BYTES, MAX_SCAN_STEPS,
    MAX_TEMPORARY_PAYLOAD_BYTES, READ_BUFFER_BYTES, StreamEnd,
};
pub use error::{AccessError, EnumerationError, ObjectKind, PathError};
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

    /// M2A.2b. Pins the directory `target` beneath the held root (or the
    /// held root itself) as an `O_PATH` directory capability that borrows
    /// this root. Root binding is checked before and after; a relative
    /// target is verified by a second independent full-mask resolution.
    /// Directory policy: a directory with a concrete identity on the root's
    /// device and ext-family filesystem, with at least one link (the
    /// regular-file single-link rule does not apply to directories).
    pub fn pin_directory(
        &self,
        target: DirectoryTarget<'_>,
    ) -> Result<PinnedDirectory<'_>, AccessError> {
        let target = match target {
            DirectoryTarget::Root => imp::DirectoryPinTarget::Root,
            DirectoryTarget::Relative(path) => imp::DirectoryPinTarget::Relative(path),
        };
        imp::Directory::pin(&self.inner, target).map(|inner| PinnedDirectory { inner })
    }
}

/// The directory to pin with [`SecureRoot::pin_directory`]: the authorized
/// root itself (requested explicitly; never the process root), or a
/// validated relative path beneath it.
#[derive(Debug, Clone, Copy)]
pub enum DirectoryTarget<'a> {
    Root,
    Relative(&'a RelativePath),
}

/// A pinned directory (M2A.2b): owns one `O_PATH` directory handle and its
/// original root-relative locator, and borrows the [`SecureRoot`] that
/// authorized it, which therefore outlives it. Not `Clone`; no descriptor,
/// path or name is exposed, and `Debug` shows only the object identity.
pub struct PinnedDirectory<'root> {
    inner: imp::Directory<'root>,
}

impl std::fmt::Debug for PinnedDirectory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedDirectory")
            .field("object", &self.inner.id())
            .finish_non_exhaustive()
    }
}

impl<'root> PinnedDirectory<'root> {
    /// Handle-derived identity of the pinned directory.
    pub fn object_id(&self) -> ObjectId {
        self.inner.id()
    }

    /// One bounded discovery of this single directory. **Consumes the pin**:
    /// on success the listing keeps it; on refusal it is closed with every
    /// handle acquired by the call, and no name or counter is returned.
    ///
    /// Root and directory binding are checked at the start, after the
    /// private read open, after every `getdents64` attempt and before
    /// release. A binding, system-call, framing or storage refusal discards
    /// the whole listing. A reached budget or a cancellation observed after
    /// reading started returns only what was kept, with a `Partial` status.
    /// Cancellation requested before the call is
    /// [`EnumerationError::Cancelled`] before anything is allocated or
    /// opened. There is no cursor, continuation or retry; a new enumeration
    /// needs a new pin and may repeat or miss entries.
    pub fn enumerate(
        self,
        budget: EnumerationBudget,
        cancel: &Cancellation,
    ) -> Result<DirectoryListing<'root>, EnumerationError> {
        self.inner
            .enumerate(budget, cancel)
            .map(|inner| DirectoryListing { inner })
    }
}

/// The result of one bounded discovery: an opaque, bounded, sorted set of
/// candidates addressed by ordinal (`0..len()`), a status and counters. It
/// keeps the directory capability and the root borrow. Names, paths and
/// descriptors never leave it; `Debug` shows only counts, status and the
/// directory identity. A candidate is a **request**, not authorization.
pub struct DirectoryListing<'root> {
    inner: imp::Listing<'root>,
}

impl std::fmt::Debug for DirectoryListing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryListing")
            .field("directory", &self.inner.directory_id())
            .field("candidates", &self.inner.len())
            .field("status", &self.inner.status())
            .finish_non_exhaustive()
    }
}

impl DirectoryListing<'_> {
    /// Number of retained candidates.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    pub fn status(&self) -> DiscoveryStatus {
        self.inner.status()
    }

    pub fn counters(&self) -> DiscoveryCounters {
        self.inner.counters()
    }

    /// Identity of the enumerated directory (the held pin).
    pub fn directory_object_id(&self) -> ObjectId {
        self.inner.directory_id()
    }

    /// Authorizes candidate `ordinal` **afresh**: checks root and directory
    /// binding, then pins the joined root-relative path with the unchanged
    /// [`SecureRoot::pin_regular`] protocol from the held **root** (never
    /// from the directory handle), then checks both bindings again. The
    /// result is whatever single-link regular file is currently, admissibly
    /// bound there; nothing ties it to the entry seen during discovery.
    /// Directories, symlinks and special files are refused by that
    /// protocol. No content is read and nothing is promoted.
    pub fn pin_regular(&self, ordinal: usize) -> Result<PinnedRegularFile, EnumerationError> {
        let (inner, evidence) = self.inner.pin_regular(ordinal)?;
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

    /// Converts this pin into a [`ReadableRegularFile`] for the **same**
    /// object. **Consumes the pin** in every case: on success the readable
    /// capability keeps owning the original `O_PATH` handle; on failure the
    /// pin and every handle acquired during the call are closed, and the
    /// error carries no handle (pin again if needed).
    ///
    /// The new descriptor is opened `O_RDONLY|O_CLOEXEC|O_NONBLOCK` through
    /// a fresh private procfs anchor of the calling thread, from the held
    /// handle itself; no pathname of the file is resolved or reopened. The
    /// original handle (before and after) and the new handle (before any
    /// read) must each be a regular file with the pinned identity and
    /// exactly one link. A missing or unrecognized procfs is
    /// [`AccessError::SecurePrimitiveUnavailable`]; a non-blocking refusal
    /// (for example a conflicting write lease) is
    /// [`AccessError::PromotionWouldBlock`] and is never retried.
    ///
    /// The checks are samples, not continuous guarantees, and nothing here
    /// is a content snapshot. Opening for reading can update access times,
    /// generate notifications and break write leases (README).
    pub fn promote(self) -> Result<ReadableRegularFile, AccessError> {
        let (inner, evidence) = self.inner.promote()?;
        Ok(ReadableRegularFile { inner, evidence })
    }
}

/// A promoted regular file: owns the original pinned `O_PATH` handle and a
/// read-only descriptor of the same object, and closes both on drop. Not
/// `Clone`. Exposes no descriptor, `File`, `Read` or `Seek`; the default
/// API cannot read bytes (M2A.2a is a research capability, not a collector).
pub struct ReadableRegularFile {
    inner: imp::Readable,
    evidence: FileEvidence,
}

impl std::fmt::Debug for ReadableRegularFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadableRegularFile")
            .field("evidence", &self.evidence)
            .finish_non_exhaustive()
    }
}

impl ReadableRegularFile {
    /// Evidence sampled from the **readable** handle during promotion (S1),
    /// before any read. A sample, not a snapshot current at return.
    pub fn evidence(&self) -> &FileEvidence {
        &self.evidence
    }

    /// Fresh evidence from both held handles (never from a path): both must
    /// still be a regular file with the pinned identity and exactly one
    /// link. Returns the readable handle's sample.
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
    /// Resolve mask required for every source-path resolution beneath a
    /// held directory (`RESOLVE_BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`),
    /// or 0 without a backend. It does not describe the process-root anchor
    /// (`/`, 0x07) or the private procfs opens of readable promotion, which
    /// never resolve a source path.
    pub required_resolve_mask: u64,
    /// [`PinnedRegularFile::promote`] is implemented (M2A.2a, Linux
    /// backend). Runtime availability (procfs) is established only by
    /// `promote` itself.
    pub readable_promotion: bool,
    /// [`SecureRoot::pin_directory`] and bounded single-directory
    /// enumeration are implemented (M2A.2b, Linux backend).
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
        let (backend, metadata_pinning, required_resolve_mask, readable_promotion, enumeration) = (
            Backend::LinuxX86_64Gnu,
            true,
            linux::REQUIRED_RESOLVE,
            true,
            true,
        );
        #[cfg(not(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu",
            target_pointer_width = "64",
            not(target_abi = "x32"),
            not(abf_secure_fs_force_unsupported)
        )))]
        let (backend, metadata_pinning, required_resolve_mask, readable_promotion, enumeration) =
            (Backend::Unavailable, false, 0, false, false);
        Self {
            backend,
            metadata_pinning,
            required_resolve_mask,
            readable_promotion,
            directory_enumeration: enumeration,
            environment_attested: false,
        }
    }
}

/// Fail-closed stand-in for every other target (and for the
/// `abf_secure_fs_force_unsupported` cfg used to test this path). The types
/// are uninhabited: no root, pinned or readable file can ever exist.
#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
)))]
mod imp {
    use crate::{
        AccessError, Cancellation, DiscoveryCounters, DiscoveryStatus, EnumerationBudget,
        EnumerationError, FileEvidence, ObjectId, RelativePath,
    };
    use std::convert::Infallible;
    use std::marker::PhantomData;

    #[derive(Debug)]
    pub(crate) enum Root {}

    #[derive(Debug)]
    pub(crate) enum Pinned {}

    #[derive(Debug)]
    pub(crate) enum Readable {}

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum DirectoryPinTarget<'a> {
        Root,
        Relative(&'a RelativePath),
    }

    /// Uninhabited: no root exists, so no directory can be pinned.
    #[derive(Debug)]
    pub(crate) struct Directory<'r> {
        never: Infallible,
        _root: PhantomData<&'r Root>,
    }

    /// Uninhabited, like [`Directory`].
    #[derive(Debug)]
    pub(crate) struct Listing<'r> {
        never: Infallible,
        _root: PhantomData<&'r Root>,
    }

    impl<'r> Directory<'r> {
        pub(crate) fn pin(
            root: &'r Root,
            _target: DirectoryPinTarget<'_>,
        ) -> Result<Self, AccessError> {
            match *root {}
        }

        pub(crate) fn id(&self) -> ObjectId {
            match self.never {}
        }

        pub(crate) fn enumerate(
            self,
            _budget: EnumerationBudget,
            _cancel: &Cancellation,
        ) -> Result<Listing<'r>, EnumerationError> {
            match self.never {}
        }
    }

    impl Listing<'_> {
        pub(crate) fn len(&self) -> usize {
            match self.never {}
        }

        pub(crate) fn status(&self) -> DiscoveryStatus {
            match self.never {}
        }

        pub(crate) fn counters(&self) -> DiscoveryCounters {
            match self.never {}
        }

        pub(crate) fn directory_id(&self) -> ObjectId {
            match self.never {}
        }

        pub(crate) fn pin_regular(
            &self,
            _ordinal: usize,
        ) -> Result<(Pinned, FileEvidence), EnumerationError> {
            match self.never {}
        }
    }

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

        pub(crate) fn promote(self) -> Result<(Readable, FileEvidence), AccessError> {
            match self {}
        }
    }

    impl Readable {
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
/// It can inject errnos **instead of** real `openat2`, `readlinkat` or
/// synthetic `pread` calls, replace the procfs self-link payload, record the
/// exact `open_how` of every attempt (an unbounded per-thread log), override
/// the reported filesystem magic or object identity, count descriptor
/// opens/closes and run schedule hooks. The overrides **forge** kernel
/// evidence: they can make a refused root, a replaced binding, a different
/// object or a non-procfs anchor look admitted. That is why the module
/// exists only in a deliberate harness build. It cannot change any open flag
/// or resolve mask. All state is thread-local.
///
/// The same two keys also compile the only byte-reading entry point,
/// [`ReadableRegularFile::harness_read_at`] (at most
/// [`fault_injection::MAX_SYNTHETIC_READ`] bytes per call), and descriptor
/// probes (status flags, offsets) used to verify the promotion against the
/// real kernel.
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
        pub const EIO: i32 = 5;
        pub const E2BIG: i32 = 7;
        pub const EBADF: i32 = 9;
        pub const EAGAIN: i32 = 11;
        /// Same value as `EAGAIN` on Linux.
        pub const EWOULDBLOCK: i32 = 11;
        pub const EACCES: i32 = 13;
        pub const ENOTDIR: i32 = 20;
        pub const EINVAL: i32 = 22;
        pub const EMFILE: i32 = 24;
        pub const ENOSYS: i32 = 38;
        pub const ELOOP: i32 = 40;
    }

    pub use crate::promotion::MAX_SYNTHETIC_READ;

    /// Refusal of [`crate::ReadableRegularFile::harness_read_at`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum SyntheticReadError {
        /// `limit` is 0 or above [`MAX_SYNTHETIC_READ`], or `offset + limit`
        /// overflows the signed file-offset range. Nothing was inspected or
        /// read.
        Range,
        /// A handle policy check (before or after the read) or the read
        /// itself failed. No bytes are returned.
        Access(crate::AccessError),
    }

    /// Real kernel descriptor flags of a promoted file (`fcntl`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DescriptorStatus {
        /// `F_GETFL` of the new readable descriptor.
        pub readable_status_flags: i32,
        /// `F_GETFD` of the new readable descriptor.
        pub readable_fd_flags: i32,
        /// `F_GETFL` of the retained original descriptor.
        pub original_status_flags: i32,
        /// `F_GETFD` of the retained original descriptor.
        pub original_fd_flags: i32,
    }

    /// A planned replacement for one procfs self-link `readlinkat` call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SelfLinkInjection {
        /// Answer with this errno instead of calling the kernel.
        Errno(i32),
        /// Answer with this payload instead of calling the kernel. It is
        /// truncated to the 32-byte buffer, and the returned length is the
        /// copied length (so a payload of 32 bytes or more models a
        /// possibly truncated kernel result).
        Payload(Vec<u8>),
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

    /// Plan for the next procfs self-link `readlinkat` calls on this
    /// thread: `Some(..)` answers that call instead of the kernel, `None`
    /// makes the real call.
    pub fn inject_self_link(plan: &[Option<SelfLinkInjection>]) {
        fault::set_link_plan(
            &plan
                .iter()
                .map(|entry| {
                    entry.clone().map(|injection| match injection {
                        SelfLinkInjection::Errno(errno) => fault::LinkInjection::Errno(errno),
                        SelfLinkInjection::Payload(bytes) => fault::LinkInjection::Payload(bytes),
                    })
                })
                .collect::<Vec<_>>(),
        );
    }

    /// `readlinkat` attempts (real or injected) since the last reset.
    pub fn self_link_reads() -> u64 {
        fault::link_calls()
    }

    /// Plan for the next synthetic `pread` calls on this thread:
    /// `Some(errno)` answers that call instead of the kernel.
    pub fn inject_read(plan: &[Option<i32>]) {
        fault::set_read_plan(plan);
    }

    /// Synthetic `pread` attempts (real or injected) since the last reset.
    pub fn read_attempts() -> u64 {
        fault::read_calls()
    }

    pub fn fd_counters() -> FdCounters {
        let (opened, closed) = fault::counters();
        FdCounters { opened, closed }
    }

    /// Highest number of descriptors wrapped on this thread and still open
    /// at the same time, since the last [`reset`] or [`reset_peak_descriptors`].
    pub fn peak_open_descriptors() -> u64 {
        fault::peak()
    }

    /// Restarts [`peak_open_descriptors`] from the currently open count.
    pub fn reset_peak_descriptors() {
        fault::reset_peak();
    }

    /// Largest accepted `getdents64` plan ([`inject_getdents`]).
    pub const MAX_GETDENTS_PLAN: usize = fault::MAX_DIRENT_PLAN;

    /// A planned replacement for one `getdents64` call (M2A.2b). Forged
    /// directory data: harness only.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum DirentInjection {
        /// Answer with this errno instead of calling the kernel.
        Errno(i32),
        /// Answer with these bytes (at most one 8192-byte buffer) instead of
        /// calling the kernel; the returned count is their length.
        Batch(Vec<u8>),
        /// Answer with this raw signed count without writing the buffer
        /// (models an impossible kernel count).
        Return(i64),
    }

    /// Plan for the next `getdents64` calls on this thread: `Some(..)`
    /// answers that call instead of the kernel, `None` makes the real call.
    /// Panics above [`MAX_GETDENTS_PLAN`] entries or for a batch larger than
    /// the buffer (bounded harness state).
    pub fn inject_getdents(plan: &[Option<DirentInjection>]) {
        fault::set_dirent_plan(
            &plan
                .iter()
                .map(|entry| {
                    entry.clone().map(|injection| match injection {
                        DirentInjection::Errno(errno) => fault::DirentInjection::Errno(errno),
                        DirentInjection::Batch(bytes) => fault::DirentInjection::Batch(bytes),
                        DirentInjection::Return(ret) => fault::DirentInjection::Return(ret),
                    })
                })
                .collect::<Vec<_>>(),
        );
    }

    /// `getdents64` attempts (real or injected) since the last reset.
    pub fn getdents_attempts() -> u64 {
        fault::dirent_calls()
    }

    /// `getdents64` plan entries not consumed yet.
    pub fn remaining_getdents_injections() -> usize {
        fault::remaining_dirent_plan()
    }

    /// The next enumeration on this thread fails its storage reservation.
    pub fn refuse_next_allocation() {
        fault::set_refuse_allocation(true);
    }

    /// Real `(F_GETFL, F_GETFD)` of the last directory read handle opened on
    /// this thread, sampled right after its open.
    pub fn last_directory_read_status() -> Option<(i32, i32)> {
        fault::directory_read_status()
    }

    /// Actual retained-output ledger of a listing, in bytes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RetainedLedger {
        /// Actual capacity of the name arena.
        pub arena_capacity: usize,
        /// Actual slot capacity times the 8-byte slot size.
        pub slots_capacity_bytes: usize,
        /// Retained locator heap bytes plus the listing's inline size.
        pub fixed: usize,
        pub total: usize,
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

/// Harness-only probes of a promoted file (feature `fault-injection` **and**
/// `--cfg abf_secure_fs_harness`). Not present in any ordinary build.
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
impl ReadableRegularFile {
    /// The only byte-reading entry point of the crate, for synthetic test
    /// files: one positioned read of at most `limit` bytes
    /// (`1..=MAX_SYNTHETIC_READ`) at `offset`, staged in a fixed buffer.
    /// Both handles are checked before and after; bytes are returned only
    /// if the post-checks pass. A short read is returned as short (it says
    /// nothing about the rest of the file). Does not move the offset.
    pub fn harness_read_at(
        &self,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<u8>, fault_injection::SyntheticReadError> {
        use crate::promotion::SyntheticReadFailure;
        self.inner
            .read_synthetic(offset, limit)
            .map_err(|failure| match failure {
                SyntheticReadFailure::Range => fault_injection::SyntheticReadError::Range,
                SyntheticReadFailure::Access(error) => {
                    fault_injection::SyntheticReadError::Access(error)
                }
            })
    }

    /// Real `F_GETFL`/`F_GETFD` of both owned descriptors. Panics on an
    /// `fcntl` failure (test probe only).
    pub fn harness_descriptor_status(&self) -> fault_injection::DescriptorStatus {
        let (readable_status_flags, readable_fd_flags) =
            self.inner.readable_status().expect("fcntl on readable");
        let (original_status_flags, original_fd_flags) =
            self.inner.original_status().expect("fcntl on original");
        fault_injection::DescriptorStatus {
            readable_status_flags,
            readable_fd_flags,
            original_status_flags,
            original_fd_flags,
        }
    }

    /// Current offset of the readable description (`lseek SEEK_CUR`).
    pub fn harness_offset(&self) -> u64 {
        self.inner.offset().expect("lseek on readable")
    }

    /// Sets the readable description's offset (`lseek SEEK_SET`); reads
    /// nothing. Returns the new offset.
    pub fn harness_seek(&self, to: u64) -> u64 {
        self.inner.seek_to(to).expect("lseek on readable")
    }
}

/// Harness-only ledger probe of a listing (feature `fault-injection` **and**
/// `--cfg abf_secure_fs_harness`). Not present in any ordinary build.
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
impl DirectoryListing<'_> {
    /// Actual capacities and fixed parts accounted for this listing.
    pub fn harness_retained_ledger(&self) -> fault_injection::RetainedLedger {
        let ledger = self.inner.ledger();
        fault_injection::RetainedLedger {
            arena_capacity: ledger.arena_capacity,
            slots_capacity_bytes: ledger.slots_capacity_bytes,
            fixed: ledger.fixed,
            total: ledger.total,
        }
    }
}
