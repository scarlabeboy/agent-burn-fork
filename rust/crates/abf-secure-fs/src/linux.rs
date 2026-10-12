//! Linux protocol, in safe code over the `sys::linux_x86_64` ABI module.
//!
//! 1. Lexically validate the root ([`crate::path`]).
//! 2. Acquire the constant process root `/` (O_PATH|O_DIRECTORY|O_NOFOLLOW|
//!    O_CLOEXEC) and admit its filesystem by handle (`fstatfs` magic 0xEF53).
//! 3. Acquire the root **beneath that held anchor** with openat2 and the
//!    mandatory mask BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV. Require a
//!    directory on the anchor's device, with ext-family magic. Hold it.
//! 4. Root binding check: re-resolve the root path beneath the held anchor
//!    with the same flags. It must yield the held root object (dev, ino),
//!    otherwise `RootBindingChanged`.
//! 5. Pin: binding check, then one full-relative openat2 beneath the
//!    **held root** (O_PATH|O_NOFOLLOW|O_CLOEXEC, same mask). Apply the
//!    candidate policy to the handle (regular file, root's device, concrete
//!    ino, nlink == 1). Verify by a second full-relative resolution: the
//!    verifier handle must pass the same candidate policy **and** be the same
//!    (dev, ino). Then a final binding check and a final candidate policy on
//!    the held handle. The link count is therefore enforced on three samples:
//!    acquisition, verification and final inspection.
//!
//! No step here reads file contents, enumerates a directory, or opens an
//! object for I/O. No step has a fallback: an unavailable primitive or flag
//! is an error, and the flags are `const` values that no code path modifies.
//! Readable promotion of a pin (M2A.2a) is a separate protocol in
//! `crate::promotion`; it reuses the process-root constant and the bounded
//! retry helper here without changing them. Directory pinning and bounded
//! enumeration (M2A.2b) are a separate protocol in `crate::directory`; it
//! uses [`Root::open_directory_pin`] (full mask, beneath the held root) and
//! the unchanged [`Root::check_binding`] and [`Root::pin_regular`].
//!
//! What establishes what (README, "Security contract"):
//!
//! - **Resolution confinement** comes from the kernel: each resolution uses
//!   the mandatory mask, and the crate relies on the documented `openat2`
//!   semantics of those flags under a trusted kernel. The userspace checks
//!   here do not prove confinement and do not replace any flag.
//! - **Identity equality** of two independently confined acquisitions is
//!   what the verification adds. It catches some intervening changes.
//! - **Continuous path membership** and **content origin** are not
//!   established.

use crate::error::{AccessError, ObjectKind};
use crate::path::{RelativePath, RootPath, parse_root};
use crate::sys::linux_x86_64::{
    self as sys, DirectoryLocator, DirectoryPinFd, E2BIG, EACCES, EAGAIN, EINTR, EINVAL, ELOOP,
    ENOENT, ENOSYS, ENOTDIR, EPERM, EXDEV, EXT_SUPER_MAGIC, Fd, HandleMetadata, O_CLOEXEC,
    O_DIRECTORY, O_NOFOLLOW, O_PATH, OpenHow, RESOLVE_BENEATH, RESOLVE_NO_MAGICLINKS,
    RESOLVE_NO_SYMLINKS, RESOLVE_NO_XDEV,
};
use crate::{FileEvidence, ObjectId};
use std::ffi::CStr;
use std::fs::Metadata;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

/// Mandatory resolve mask for every resolution beneath a held directory.
pub(crate) const REQUIRED_RESOLVE: u64 =
    RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV;

/// The process-root anchor is the constant absolute path "/" with
/// `AT_FDCWD`. `RESOLVE_BENEATH` is not applicable there: it rejects every
/// absolute path by definition. The other three flags are kept. "/" has no
/// component to resolve, so they are defense in depth.
pub(crate) const PROCESS_ROOT_HOW: OpenHow = OpenHow {
    flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
    mode: 0,
    resolve: RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV,
};

/// Root acquisition and binding re-resolution, beneath the held "/" anchor.
const DIRECTORY_HOW: OpenHow = OpenHow {
    flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
    mode: 0,
    resolve: REQUIRED_RESOLVE,
};

/// Candidate file acquisition beneath the held root. `O_PATH|O_NOFOLLOW`
/// returns a handle to a final symlink instead of failing (documented
/// openat2 exception), so the handle type is always checked explicitly.
const FILE_HOW: OpenHow = OpenHow {
    flags: O_PATH | O_NOFOLLOW | O_CLOEXEC,
    mode: 0,
    resolve: REQUIRED_RESOLVE,
};

/// One initial attempt plus at most two retries after `EAGAIN`/`EINTR`.
pub(crate) const MAX_OPEN_ATTEMPTS: u32 = 3;

pub(crate) enum OpenFailure {
    Errno(i32),
    RetryExhausted,
}

pub(crate) enum Target<'a> {
    ProcessRoot,
    Beneath(&'a Fd, &'a CStr),
}

/// Bounded retry. `how` is a `&'static` constant: every attempt passes the
/// identical flags and mask.
pub(crate) fn open_bounded(target: Target<'_>, how: &'static OpenHow) -> Result<Fd, OpenFailure> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let result = match target {
            Target::ProcessRoot => sys::openat2_process_root(how),
            Target::Beneath(dir, path) => sys::openat2_beneath(dir, path, how),
        };
        match result {
            Ok(fd) => return Ok(fd),
            Err(EAGAIN | EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(EAGAIN | EINTR) => return Err(OpenFailure::RetryExhausted),
            Err(errno) => return Err(OpenFailure::Errno(errno)),
        }
    }
}

pub(crate) fn classify(failure: OpenFailure) -> AccessError {
    match failure {
        OpenFailure::RetryExhausted => AccessError::RetryExhausted,
        OpenFailure::Errno(errno) => classify_errno(errno),
    }
}

pub(crate) fn classify_errno(errno: i32) -> AccessError {
    match errno {
        ENOSYS | EPERM | EINVAL | E2BIG => AccessError::SecurePrimitiveUnavailable,
        ELOOP | EXDEV => AccessError::ConfinementRefused,
        ENOENT => AccessError::NotFound,
        ENOTDIR => AccessError::NotADirectory,
        EACCES => AccessError::Inaccessible,
        other => AccessError::UnexpectedOs(other),
    }
}

/// Re-resolving the root path: anything showing that the path no longer
/// leads to a directory beneath "/" on the same mount is a binding change.
fn classify_binding(failure: OpenFailure) -> AccessError {
    match classify(failure) {
        AccessError::NotFound | AccessError::NotADirectory | AccessError::ConfinementRefused => {
            AccessError::RootBindingChanged
        }
        other => other,
    }
}

/// Handle-derived evidence. `kind == None` means regular file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inspection {
    pub(crate) id: ObjectId,
    pub(crate) kind: Option<ObjectKind>,
    pub(crate) nlink: u64,
    pub(crate) size: u64,
}

fn object_kind(metadata: &Metadata) -> Option<ObjectKind> {
    let file_type = metadata.file_type();
    if file_type.is_file() {
        None
    } else if file_type.is_dir() {
        Some(ObjectKind::Directory)
    } else if file_type.is_symlink() {
        Some(ObjectKind::Symlink)
    } else if file_type.is_fifo() {
        Some(ObjectKind::Fifo)
    } else if file_type.is_socket() {
        Some(ObjectKind::Socket)
    } else if file_type.is_char_device() {
        Some(ObjectKind::CharDevice)
    } else if file_type.is_block_device() {
        Some(ObjectKind::BlockDevice)
    } else {
        Some(ObjectKind::Unknown)
    }
}

/// Metadata of the held object (`statx`/`fstat` on the owned descriptor:
/// the O_PATH handle, or the M2A.2a readable handle). A zero device or inode
/// number is not a concrete identity and is never accepted.
pub(crate) fn inspect(fd: &impl HandleMetadata) -> Result<Inspection, AccessError> {
    let metadata = fd.metadata().map_err(|error| match error.raw_os_error() {
        Some(errno) => classify_errno(errno),
        None => AccessError::IdentityUnavailable,
    })?;
    let (device, inode) = (metadata.dev(), metadata.ino());
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    let (device, inode) = sys::fault::next_identity_override().unwrap_or((device, inode));
    if device == 0 || inode == 0 {
        return Err(AccessError::IdentityUnavailable);
    }
    Ok(Inspection {
        id: ObjectId { device, inode },
        kind: object_kind(&metadata),
        nlink: metadata.nlink(),
        size: metadata.size(),
    })
}

fn admit_filesystem(fd: &Fd) -> Result<(), AccessError> {
    let magic = sys::filesystem_magic(fd).map_err(classify_errno)?;
    if magic == EXT_SUPER_MAGIC {
        Ok(())
    } else {
        Err(AccessError::UnsupportedFilesystem)
    }
}

/// Authorized root capability: the held "/" anchor, the held root directory
/// object and its identity. The root path is kept only to re-check its
/// binding and is never exposed.
pub(crate) struct Root {
    anchor: Fd,
    root: Fd,
    path: RootPath,
    id: ObjectId,
}

impl std::fmt::Debug for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Root")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Pinned regular file: the held O_PATH handle and the identity it was
/// pinned with.
pub(crate) struct Pinned {
    fd: Fd,
    id: ObjectId,
}

impl std::fmt::Debug for Pinned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pinned")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Root {
    pub(crate) fn authorize(root: &str) -> Result<Self, AccessError> {
        let path = parse_root(root)?;

        let anchor = open_bounded(Target::ProcessRoot, &PROCESS_ROOT_HOW).map_err(classify)?;
        schedule_point!(AuthorizeAfterAnchor);
        admit_filesystem(&anchor)?;
        let anchor_info = inspect(&anchor)?;
        if anchor_info.kind != Some(ObjectKind::Directory) {
            return Err(AccessError::NotADirectory);
        }

        let held = open_bounded(Target::Beneath(&anchor, path.as_c_str()), &DIRECTORY_HOW)
            .map_err(classify)?;
        schedule_point!(AuthorizeAfterRootOpen);
        let info = inspect(&held)?;
        if info.kind != Some(ObjectKind::Directory) {
            return Err(AccessError::NotADirectory);
        }
        // Same superblock as "/": RESOLVE_NO_XDEV already refuses mount
        // crossings; this is an independent handle-based check.
        if info.id.device != anchor_info.id.device {
            return Err(AccessError::UnsupportedFilesystem);
        }
        admit_filesystem(&held)?;

        Ok(Self {
            anchor,
            root: held,
            path,
            id: info.id,
        })
    }

    pub(crate) fn id(&self) -> ObjectId {
        self.id
    }

    /// The root path must still resolve, under the mandatory mask beneath
    /// "/", to the originally authorized directory object.
    pub(crate) fn check_binding(&self) -> Result<(), AccessError> {
        let fresh = open_bounded(
            Target::Beneath(&self.anchor, self.path.as_c_str()),
            &DIRECTORY_HOW,
        )
        .map_err(classify_binding)?;
        let info = inspect(&fresh)?;
        if info.kind != Some(ObjectKind::Directory) || info.id != self.id {
            return Err(AccessError::RootBindingChanged);
        }
        Ok(())
    }

    /// Candidate policy on a handle: regular file, on the root's device,
    /// exactly one link.
    fn inspect_candidate(&self, fd: &Fd) -> Result<Inspection, AccessError> {
        let info = inspect(fd)?;
        if let Some(kind) = info.kind {
            return Err(AccessError::NotRegularFile(kind));
        }
        if info.id.device != self.id.device {
            return Err(AccessError::ConfinementRefused);
        }
        match info.nlink {
            1 => Ok(info),
            0 => Err(AccessError::ObjectChanged),
            _ => Err(AccessError::MultipleLinks),
        }
    }

    pub(crate) fn pin_regular(
        &self,
        relative: &RelativePath,
    ) -> Result<(Pinned, FileEvidence), AccessError> {
        let target = || Target::Beneath(&self.root, relative.as_c_str());

        schedule_point!(PinBeforeBindingCheck);
        self.check_binding()?;

        schedule_point!(PinBeforeAcquire);
        let held = open_bounded(target(), &FILE_HOW).map_err(classify)?;
        let first = self.inspect_candidate(&held)?;

        // Verification: a second, independent full-relative resolution from
        // the held root, under the same mandatory mask, must reach the same
        // object. It detects a held object that is no longer bound at this
        // path beneath the root when the verification runs, for example after
        // an intermediate directory or the file itself was moved or replaced
        // following the acquisition.
        //
        // This is an identity check, not a proof of confinement: if both
        // resolutions ever returned an object that was never beneath the
        // root, the identities would still be equal. Confinement of each
        // resolution rests on the kernel's documented RESOLVE_BENEATH /
        // NO_SYMLINKS / NO_MAGICLINKS / NO_XDEV semantics (trusted kernel).
        schedule_point!(PinAfterAcquire);
        let verify =
            open_bounded(target(), &FILE_HOW).map_err(|failure| match classify(failure) {
                AccessError::NotFound
                | AccessError::NotADirectory
                | AccessError::ConfinementRefused => AccessError::ObjectChanged,
                other => other,
            })?;
        // The verifier sample is subject to the same candidate policy as the
        // acquisition (an observed nlink != 1 is refused), before identity
        // equality is compared.
        let second = self.inspect_candidate(&verify);
        drop(verify);
        let second = second?;
        if second.id != first.id {
            return Err(AccessError::ObjectChanged);
        }

        schedule_point!(PinAfterVerify);
        self.check_binding()?;

        let last = self.inspect_candidate(&held)?;
        if last.id != first.id {
            return Err(AccessError::ObjectChanged);
        }
        let evidence = FileEvidence::new(last.id, last.nlink, last.size);
        Ok((
            Pinned {
                fd: held,
                id: last.id,
            },
            evidence,
        ))
    }

    /// M2A.2b: one attempt to acquire a directory pin beneath the **held
    /// root** (`O_PATH|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`, full mask). The
    /// held root descriptor itself never leaves this module; the caller
    /// (`crate::directory`) bounds retries and applies the directory policy.
    pub(crate) fn open_directory_pin(
        &self,
        locator: DirectoryLocator<'_>,
    ) -> Result<DirectoryPinFd, i32> {
        sys::open_directory_pin(&self.root, locator)
    }
}

impl Pinned {
    /// The held O_PATH handle, borrowed (M2A.2a promotion only).
    pub(crate) fn handle(&self) -> &Fd {
        &self.fd
    }

    /// The identity this file was pinned with.
    pub(crate) fn id(&self) -> ObjectId {
        self.id
    }

    /// M2A.2a: consumes the pin and converts it into a readable capability
    /// that keeps owning it (see `crate::promotion`).
    pub(crate) fn promote(self) -> Result<(crate::promotion::Readable, FileEvidence), AccessError> {
        crate::promotion::promote(self)
    }

    /// Fresh evidence from the **held** handle, never from a path. Policy is
    /// re-applied: still a regular file, same identity, exactly one link.
    pub(crate) fn reinspect(&self) -> Result<FileEvidence, AccessError> {
        let info = inspect(&self.fd)?;
        if let Some(kind) = info.kind {
            return Err(AccessError::NotRegularFile(kind));
        }
        if info.id != self.id {
            return Err(AccessError::ObjectChanged);
        }
        match info.nlink {
            1 => Ok(FileEvidence::new(info.id, info.nlink, info.size)),
            0 => Err(AccessError::ObjectChanged),
            _ => Err(AccessError::MultipleLinks),
        }
    }
}

/// Test-only deterministic schedule points (feature `fault-injection`). A
/// hook runs synchronously at each point on the calling thread, so a test can
/// mutate its own fixture tree at an exact step of the real protocol.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
pub(crate) mod schedule {
    use std::cell::RefCell;

    /// Steps of [`super::Root::authorize`], [`super::Root::pin_regular`],
    /// readable promotion (`crate::promotion`) and the harness-only bounded
    /// synthetic read at which a test hook runs.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum SchedulePoint {
        /// After "/" is acquired, before the root is acquired.
        AuthorizeAfterAnchor,
        /// After the root is acquired, before it is inspected.
        AuthorizeAfterRootOpen,
        /// Before the first root binding check of a pin.
        PinBeforeBindingCheck,
        /// After the first binding check, before the candidate is acquired.
        PinBeforeAcquire,
        /// After the candidate is acquired and inspected, before verification.
        PinAfterAcquire,
        /// After verification, before the final binding check.
        PinAfterVerify,
        /// Promotion: after the S0 sample of the original handle, before the
        /// procfs anchor is acquired.
        PromoteAfterS0,
        /// Promotion: procfs anchor admitted, right before the terminal
        /// readable open.
        PromoteBeforeOpen,
        /// Promotion: readable handle owned, before its S1 sample.
        PromoteAfterOpen,
        /// Promotion: after S1, before the S2 sample of the original handle.
        PromoteAfterS1,
        /// Promotion: after S2, before the readable capability is returned.
        PromoteAfterS2,
        /// Synthetic read: after the pre-read checks, before the read.
        ReadBeforeRead,
        /// Synthetic read: after the read, before the post-read checks.
        ReadAfterRead,
        /// Directory pin: before its first root binding check.
        DirectoryPinBeforeBindingCheck,
        /// Directory pin: acquired and admitted, before verification.
        DirectoryPinAfterAcquire,
        /// Directory pin: verified, before the final root binding check.
        DirectoryPinAfterVerify,
        /// Enumeration: storage reserved, before the start binding check.
        EnumerateBeforeStartCheck,
        /// Enumeration: after S0, before the private `.` read open.
        EnumerateBeforeReadOpen,
        /// Enumeration: read handle owned, before its S1 sample.
        EnumerateAfterReadOpen,
        /// Enumeration: after S2, before the post-open binding check.
        EnumerateAfterS2,
        /// Enumeration: after every `getdents64` attempt, before its
        /// binding check.
        EnumerateAfterAttempt,
        /// Enumeration: read handle closed, before the final binding check.
        EnumerateBeforeFinalCheck,
        /// Enumeration: after the final binding check, before release.
        EnumerateBeforeRelease,
        /// Child authorization: before the pre-pin binding check.
        ChildBeforeBindingCheck,
        /// Child authorization: after the pre-pin binding check, before the
        /// root-relative `pin_regular`.
        ChildBeforePin,
        /// Child authorization: after `pin_regular`, before the post-pin
        /// binding check.
        ChildAfterPin,
    }

    type Hook = Box<dyn FnMut(SchedulePoint)>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    pub(crate) fn set(hook: Hook) {
        HOOK.with(|h| *h.borrow_mut() = Some(hook));
    }

    pub(crate) fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    pub(crate) fn fire(point: SchedulePoint) {
        // Take the hook out while it runs, so it can perform filesystem
        // operations without a RefCell re-borrow. A hook must not call
        // `set`/`clear` itself; it is restored afterwards.
        let taken = HOOK.with(|h| h.borrow_mut().take());
        if let Some(mut hook) = taken {
            hook(point);
            HOOK.with(|h| {
                let mut slot = h.borrow_mut();
                if slot.is_none() {
                    *slot = Some(hook);
                }
            });
        }
    }
}
