//! M2A.2a — readable promotion of a pinned regular file, in safe code over
//! the `sys::linux_x86_64` ABI module.
//!
//! [`promote`] consumes a [`Pinned`] O_PATH capability and returns a
//! [`Readable`] that owns **both** the original O_PATH handle and a new
//! `O_RDONLY|O_CLOEXEC|O_NONBLOCK` descriptor for the **same** object. No
//! pathname of the file is ever re-resolved, read with `readlink` or
//! reopened. Protocol (README, "Readable promotion"):
//!
//! 1. **S0**: inspect the original handle: regular file, its pinned
//!    `(dev, ino)`, `nlink == 1`.
//! 2. Fresh private procfs anchor for the **current thread**, per call:
//!    - constant `/` (unchanged process-root flags, mask
//!      `NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`);
//!    - constant `proc` beneath it (mask `BENEATH|NO_SYMLINKS|NO_MAGICLINKS`,
//!      the only place `NO_XDEV` is omitted); admitted by handle: procfs
//!      magic `0x9FA0`, directory; `/` is dropped;
//!    - constant `thread-self` beneath proc, `O_PATH|O_NOFOLLOW`, full mask;
//!      a procfs symlink on the proc device, never followed; one direct
//!      `readlinkat(link, "", buf[32])`; strict `<pid>/task/<tid>` parse;
//!      the link is dropped;
//!    - `<pid>/task/<tid>/fd` reformatted from the two numbers, beneath proc,
//!      full mask; admitted as a procfs directory on the proc device; proc
//!      is dropped.
//! 3. Terminal open of the decimal leaf of the still-owned original
//!    descriptor (generated inside the ABI module):
//!    `O_RDONLY|O_CLOEXEC|O_NONBLOCK`, mode 0, resolve 0. This magic-link
//!    exception exists only here. The task fd directory is then dropped.
//! 4. **S1**: inspect the readable handle **before** any read: regular file,
//!    same `(dev, ino)` as the pin, `nlink == 1`.
//! 5. **S2**: reinspect the original handle with the S0 policy.
//!
//! Steps 1–3 go through the sealed admission chain of the ABI module
//! (`sys::linux_x86_64::admission`, F16-1): S0 is the only way to obtain an
//! admitted source; each procfs handle is a candidate until its own checked
//! constructor admits it; the task is derived only from the admitted
//! `thread-self` link and opens its `fd` directory only beneath the same
//! admitted root; the terminal open accepts only the admitted `fd` directory
//! and the admitted source. This module cannot build any of those values
//! itself, pass a generic descriptor or task number, or name the relaxed
//! `open_how`.
//!
//! Any refusal drops the consumed pin and every handle acquired so far. At
//! most three descriptors coexist during the call (including the original);
//! success retains exactly two. Retries: proc anchor opens at most three
//! attempts on `EAGAIN`/`EINTR`; `readlinkat` at most three attempts on
//! `EINTR`; terminal open at most three attempts on `EINTR`, and
//! **`EAGAIN`/`EWOULDBLOCK` is terminal** (`PromotionWouldBlock`, no retry,
//! to avoid repeatedly initiating lease breaks). No flag is ever removed.
//!
//! Limits: the samples are not continuous (a link added and removed between
//! samples, or after S2, is not observed); sizes may differ between samples;
//! nothing here is a content snapshot, a namespace-membership proof or an
//! event identity. A normal read open can update atime, trigger
//! notifications and break write leases.

use crate::error::AccessError;
use crate::linux::{
    Inspection, MAX_OPEN_ATTEMPTS, OpenFailure, Pinned, classify, classify_errno, inspect,
};
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
use crate::sys::linux_x86_64::Fd;
use crate::sys::linux_x86_64::admission::{
    AdmissionRefusal, AdmittedFdDir, AdmittedRegularSource, AdmittedSelfLink, CurrentTask,
    ProcessRoot, TaskRefusal,
};
use crate::sys::linux_x86_64::{EAGAIN, EINTR, ENOENT, ENOTDIR, LinkReadError, ReadableFd};
use crate::{FileEvidence, ObjectId};

/// A readable capability: the consumed original pin (O_PATH) and the new
/// read-only descriptor of the same object. Both are owned and closed on
/// drop. Fields are private; nothing exposes either descriptor.
pub(crate) struct Readable {
    readable: ReadableFd,
    pinned: Pinned,
}

impl std::fmt::Debug for Readable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Readable")
            .field("id", &self.pinned.id())
            .finish_non_exhaustive()
    }
}

/// The handle policy of S0, S1 and S2: regular file, the pinned identity,
/// exactly one link. Order of refusals matches `Pinned::reinspect`.
fn held_policy(info: Inspection, expected: ObjectId) -> Result<FileEvidence, AccessError> {
    if let Some(kind) = info.kind {
        return Err(AccessError::NotRegularFile(kind));
    }
    if info.id != expected {
        return Err(AccessError::ObjectChanged);
    }
    match info.nlink {
        1 => Ok(FileEvidence::new(info.id, info.nlink, info.size)),
        0 => Err(AccessError::ObjectChanged),
        _ => Err(AccessError::MultipleLinks),
    }
}

/// Converts `pinned` into a [`Readable`] for the same object, or refuses.
/// `pinned` is consumed in both cases. On success the returned evidence is
/// the S1 sample of the readable handle.
pub(crate) fn promote(pinned: Pinned) -> Result<(Readable, FileEvidence), AccessError> {
    // S0: the original handle, before any procfs work. Its success is the
    // only way to obtain the admitted source the terminal open accepts.
    let source = AdmittedRegularSource::admit(&pinned)?;
    schedule_point!(PromoteAfterS0);

    let fd_dir = acquire_thread_fd_dir()?;
    schedule_point!(PromoteBeforeOpen);
    let readable = open_leaf(&fd_dir, &source)?;
    drop(fd_dir);
    schedule_point!(PromoteAfterOpen);

    // S1: the new readable handle, before anything can read from it.
    let sample = held_policy(inspect(&readable)?, pinned.id())?;
    schedule_point!(PromoteAfterS1);

    // S2: the original handle again.
    pinned.reinspect()?;
    schedule_point!(PromoteAfterS2);

    Ok((Readable { readable, pinned }, sample))
}

impl Readable {
    /// Fresh evidence: both owned handles must still pass the S0/S1 policy
    /// for the pinned identity. Returns the readable handle's sample. Never
    /// consults a path or the root binding.
    pub(crate) fn reinspect(&self) -> Result<FileEvidence, AccessError> {
        self.pinned.reinspect()?;
        held_policy(inspect(&self.readable)?, self.pinned.id())
    }
}

// ---------------------------------------------------------------------------
// Procfs anchor
// ---------------------------------------------------------------------------

/// Proc-phase errno mapping: a missing or non-directory proc entry means the
/// required procfs support is unavailable, never permission to fall back.
fn classify_proc(failure: OpenFailure) -> AccessError {
    match failure {
        OpenFailure::Errno(ENOENT | ENOTDIR) => AccessError::SecurePrimitiveUnavailable,
        other => classify(other),
    }
}

/// Initial attempt plus at most two retries after `EAGAIN`/`EINTR`, with
/// the identical constant parameters (the closures take none). `classify`
/// maps any other errno: the unchanged process-root mapping for `/`, the
/// proc-phase mapping for every procfs open.
fn open_bounded<T>(
    mut attempt: impl FnMut() -> Result<T, i32>,
    classify: fn(OpenFailure) -> AccessError,
) -> Result<T, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(EAGAIN | EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(EAGAIN | EINTR) => return Err(AccessError::RetryExhausted),
            Err(errno) => return Err(classify(OpenFailure::Errno(errno))),
        }
    }
}

/// A refused admission sample: an errno from `fstatfs`/`statx` keeps the
/// proc-phase mapping; wrong magic, type or device is unavailable procfs.
fn refused(refusal: AdmissionRefusal) -> AccessError {
    match refusal {
        AdmissionRefusal::Errno(errno) => classify_proc(OpenFailure::Errno(errno)),
        AdmissionRefusal::NotAdmitted => AccessError::SecurePrimitiveUnavailable,
    }
}

/// Acquires and admits this thread's procfs `fd` directory through the
/// sealed admission chain (`sys::linux_x86_64::admission`). Every handle
/// except the returned directory is dropped before return (and on error).
/// At most two of these handles are open at once, plus the caller's pin.
fn acquire_thread_fd_dir() -> Result<AdmittedFdDir, AccessError> {
    let anchor = open_bounded(ProcessRoot::open, classify)?;
    let candidate = open_bounded(|| anchor.open_proc(), classify_proc)?;
    drop(anchor);
    let proc_root = candidate.admit().map_err(refused)?;

    let candidate = {
        let task = {
            let link = open_bounded(|| proc_root.open_thread_self(), classify_proc)?
                .admit()
                .map_err(refused)?;
            read_current_task_bounded(&link)?
            // `link` and the payload are dropped here; the payload is never
            // stored or printed.
        };
        open_bounded(|| task.open_fd_dir(), classify_proc)?
    };
    drop(proc_root);
    candidate.admit().map_err(refused)
}

/// At most three `readlinkat` attempts, retrying only `EINTR`.
fn read_current_task_bounded<'proc>(
    link: &AdmittedSelfLink<'proc>,
) -> Result<CurrentTask<'proc>, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match link.read_current_task() {
            Ok(task) => return Ok(task),
            Err(TaskRefusal::Link(LinkReadError::Errno(EINTR))) if attempts < MAX_OPEN_ATTEMPTS => {
            }
            Err(TaskRefusal::Link(LinkReadError::Errno(EINTR))) => {
                return Err(AccessError::RetryExhausted);
            }
            Err(TaskRefusal::Link(LinkReadError::Truncated) | TaskRefusal::Malformed) => {
                return Err(AccessError::SecurePrimitiveUnavailable);
            }
            Err(TaskRefusal::Link(LinkReadError::Errno(errno))) => {
                return Err(classify_proc(OpenFailure::Errno(errno)));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal open
// ---------------------------------------------------------------------------

/// At most three attempts, retrying only `EINTR`. `EAGAIN`/`EWOULDBLOCK` is
/// terminal and is never retried. A missing fd entry means procfs support is
/// unavailable; there is no fallback. Only an admitted `fd` directory and an
/// admitted source can reach the relaxed open.
fn open_leaf(
    fd_dir: &AdmittedFdDir,
    source: &AdmittedRegularSource<'_>,
) -> Result<ReadableFd, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match fd_dir.open_readable(source) {
            Ok(readable) => return Ok(readable),
            Err(EAGAIN) => return Err(AccessError::PromotionWouldBlock),
            Err(EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(EINTR) => return Err(AccessError::RetryExhausted),
            Err(ENOENT) => return Err(AccessError::SecurePrimitiveUnavailable),
            Err(errno) => return Err(classify_errno(errno)),
        }
    }
}

// ---------------------------------------------------------------------------
// Harness-only bounded synthetic read
// ---------------------------------------------------------------------------

/// Largest synthetic read per call (harness only; not a scanner budget).
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
pub const MAX_SYNTHETIC_READ: usize = 4096;

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntheticReadFailure {
    /// `limit` is 0 or above [`MAX_SYNTHETIC_READ`], or `offset + limit`
    /// overflows the signed 64-bit file offset range.
    Range,
    Access(AccessError),
}

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
impl Readable {
    /// One bounded positioned read of at most `limit` bytes at `offset`,
    /// staged in a fixed buffer. Both handles must pass the policy before
    /// and after the read; bytes are returned only after the post-checks.
    /// A short read is returned as short. `EINTR` is retried at most twice;
    /// every other error is coded.
    pub(crate) fn read_synthetic(
        &self,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<u8>, SyntheticReadFailure> {
        if limit == 0 || limit > MAX_SYNTHETIC_READ {
            return Err(SyntheticReadFailure::Range);
        }
        let end = offset
            .checked_add(limit as u64)
            .ok_or(SyntheticReadFailure::Range)?;
        if end > i64::MAX as u64 {
            return Err(SyntheticReadFailure::Range);
        }

        self.reinspect().map_err(SyntheticReadFailure::Access)?;
        schedule_point!(ReadBeforeRead);
        let mut stage = [0_u8; MAX_SYNTHETIC_READ];
        let mut attempts = 0;
        let count = loop {
            attempts += 1;
            match self.readable.read_at(&mut stage[..limit], offset) {
                Ok(count) => break count,
                Err(EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
                Err(EINTR) => {
                    return Err(SyntheticReadFailure::Access(AccessError::RetryExhausted));
                }
                Err(errno) => return Err(SyntheticReadFailure::Access(classify_errno(errno))),
            }
        };
        schedule_point!(ReadAfterRead);
        self.reinspect().map_err(SyntheticReadFailure::Access)?;
        Ok(stage[..count.min(limit)].to_vec())
    }

    /// ABF28: the two held owners and the pinned identity, borrowed for the
    /// research read transaction (`crate::read`), which consumes this
    /// capability and drops it (closing both) before releasing any byte.
    /// No descriptor leaves the crate.
    pub(crate) fn reader_owners(&self) -> (&ReadableFd, &Fd, ObjectId) {
        (&self.readable, self.pinned.handle(), self.pinned.id())
    }

    pub(crate) fn readable_status(&self) -> Result<(i32, i32), i32> {
        self.readable.descriptor_status()
    }

    pub(crate) fn original_status(&self) -> Result<(i32, i32), i32> {
        self.pinned.handle().descriptor_status()
    }

    pub(crate) fn offset(&self) -> Result<u64, i32> {
        self.readable.offset()
    }

    pub(crate) fn seek_to(&self, to: u64) -> Result<u64, i32> {
        self.readable.seek_to(to)
    }
}
