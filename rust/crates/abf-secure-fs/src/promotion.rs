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
    Inspection, MAX_OPEN_ATTEMPTS, OpenFailure, PROCESS_ROOT_HOW, Pinned, Target, classify,
    classify_errno, inspect, open_bounded,
};
use crate::sys::linux_x86_64::{
    self as sys, EAGAIN, EINTR, ENOENT, ENOTDIR, LinkReadError, MAX_TASK_ID, PROC_SUPER_MAGIC,
    ProcFdDir, ProcRoot, ReadableFd,
};
use crate::{FileEvidence, ObjectId};
use std::os::unix::fs::MetadataExt;

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
    // S0: the original handle, before any procfs work.
    pinned.reinspect()?;
    schedule_point!(PromoteAfterS0);

    let fd_dir = acquire_thread_fd_dir()?;
    schedule_point!(PromoteBeforeOpen);
    let readable = open_leaf(&fd_dir, &pinned)?;
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
/// the identical constant parameters (the closures take none).
fn proc_open_bounded<T>(mut attempt: impl FnMut() -> Result<T, i32>) -> Result<T, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(EAGAIN | EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(EAGAIN | EINTR) => return Err(AccessError::RetryExhausted),
            Err(errno) => return Err(classify_proc(OpenFailure::Errno(errno))),
        }
    }
}

/// Admits a procfs directory by handle: procfs magic, directory, and (when
/// given) the admitted proc device. Returns its device.
fn admit_proc_directory(
    magic: Result<i64, i32>,
    metadata: std::io::Result<std::fs::Metadata>,
    device: Option<u64>,
) -> Result<u64, AccessError> {
    let magic = magic.map_err(|errno| classify_proc(OpenFailure::Errno(errno)))?;
    if magic != PROC_SUPER_MAGIC {
        return Err(AccessError::SecurePrimitiveUnavailable);
    }
    let metadata = metadata_or_code(metadata)?;
    if !metadata.file_type().is_dir() {
        return Err(AccessError::SecurePrimitiveUnavailable);
    }
    if device.is_some_and(|device| device != metadata.dev()) {
        return Err(AccessError::SecurePrimitiveUnavailable);
    }
    Ok(metadata.dev())
}

fn metadata_or_code(
    metadata: std::io::Result<std::fs::Metadata>,
) -> Result<std::fs::Metadata, AccessError> {
    metadata.map_err(|error| match error.raw_os_error() {
        Some(errno) => classify_proc(OpenFailure::Errno(errno)),
        None => AccessError::SecurePrimitiveUnavailable,
    })
}

/// Acquires and admits this thread's procfs `fd` directory. Every handle
/// except the returned directory is dropped before return (and on error).
fn acquire_thread_fd_dir() -> Result<ProcFdDir, AccessError> {
    use sys::HandleMetadata;

    let anchor = open_bounded(Target::ProcessRoot, &PROCESS_ROOT_HOW).map_err(classify)?;
    let proc_root: ProcRoot = proc_open_bounded(|| sys::open_proc_root(&anchor))?;
    drop(anchor);
    let proc_device =
        admit_proc_directory(proc_root.filesystem_magic(), proc_root.metadata(), None)?;

    let (pid, tid) = {
        let link = proc_open_bounded(|| sys::open_thread_self(&proc_root))?;
        let magic = link
            .filesystem_magic()
            .map_err(|errno| classify_proc(OpenFailure::Errno(errno)))?;
        let metadata = metadata_or_code(link.metadata())?;
        if magic != PROC_SUPER_MAGIC
            || !metadata.file_type().is_symlink()
            || metadata.dev() != proc_device
        {
            return Err(AccessError::SecurePrimitiveUnavailable);
        }
        let bytes = read_self_link_bounded(&link)?;
        parse_thread_self(bytes.as_bytes()).ok_or(AccessError::SecurePrimitiveUnavailable)?
        // `link` and the payload are dropped here; the payload is never
        // stored or printed.
    };

    let fd_dir = proc_open_bounded(|| sys::open_task_fd_dir(&proc_root, pid, tid))?;
    drop(proc_root);
    admit_proc_directory(
        fd_dir.filesystem_magic(),
        fd_dir.metadata(),
        Some(proc_device),
    )?;
    Ok(fd_dir)
}

/// At most three `readlinkat` attempts, retrying only `EINTR`.
fn read_self_link_bounded(link: &sys::ProcSelfLink) -> Result<sys::SelfLinkBytes, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match sys::read_self_link(link) {
            Ok(bytes) => return Ok(bytes),
            Err(LinkReadError::Errno(EINTR)) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(LinkReadError::Errno(EINTR)) => return Err(AccessError::RetryExhausted),
            Err(LinkReadError::Truncated) => return Err(AccessError::SecurePrimitiveUnavailable),
            Err(LinkReadError::Errno(errno)) => {
                return Err(classify_proc(OpenFailure::Errno(errno)));
            }
        }
    }
}

/// Strict parser for the kernel-generated `thread-self` target
/// `<pid>/task/<tid>`: two canonical decimal numbers in `1..=2147483647`
/// (1 to 10 digits, no leading zero, no sign) joined by the exact literal
/// `/task/`, nothing else. Any other spelling, including an empty or
/// possibly truncated result, is refused (`None`); there is no alternative
/// format.
pub(crate) fn parse_thread_self(bytes: &[u8]) -> Option<(u32, u32)> {
    const SEPARATOR: &[u8] = b"/task/";
    let split = bytes.iter().position(|&byte| byte == b'/')?;
    let (pid, rest) = bytes.split_at(split);
    let tid = rest.strip_prefix(SEPARATOR)?;
    Some((canonical_task_id(pid)?, canonical_task_id(tid)?))
}

fn canonical_task_id(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 10 || digits[0] == b'0' {
        return None;
    }
    let mut value: u64 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u64::from(byte - b'0');
    }
    let value = u32::try_from(value).ok()?;
    (1..=MAX_TASK_ID).contains(&value).then_some(value)
}

// ---------------------------------------------------------------------------
// Terminal open
// ---------------------------------------------------------------------------

/// At most three attempts, retrying only `EINTR`. `EAGAIN`/`EWOULDBLOCK` is
/// terminal and is never retried. A missing fd entry means procfs support is
/// unavailable; there is no fallback.
fn open_leaf(fd_dir: &ProcFdDir, pinned: &Pinned) -> Result<ReadableFd, AccessError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match sys::open_readable_leaf(fd_dir, pinned.handle()) {
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

#[cfg(test)]
mod tests {
    use super::parse_thread_self;

    #[test]
    fn accepts_only_canonical_kernel_spelling() {
        assert_eq!(parse_thread_self(b"1/task/1"), Some((1, 1)));
        assert_eq!(
            parse_thread_self(b"651101/task/651102"),
            Some((651_101, 651_102))
        );
        assert_eq!(
            parse_thread_self(b"2147483647/task/2147483647"),
            Some((2_147_483_647, 2_147_483_647))
        );
    }

    #[test]
    fn refuses_every_other_spelling() {
        let refused: &[&[u8]] = &[
            b"",
            b"/",
            b"self",
            b"thread-self",
            b"12",
            b"12/task",
            b"12/task/",
            b"/task/12",
            b"12/task/12/",
            b"12/task/12/fd",
            b"12/task//12",
            b"12//task/12",
            b"12/tasks/12",
            b"12/TASK/12",
            b"012/task/12",
            b"12/task/012",
            b"0/task/12",
            b"12/task/0",
            b"-1/task/12",
            b"+1/task/12",
            b" 12/task/12",
            b"12/task/12 ",
            b"12/task/12\n",
            b"12/task/1\x002",
            b"1\x00/task/12",
            b"./12/task/12",
            b"../12/task/12",
            b"12/task/../12",
            b"2147483648/task/1",
            b"1/task/2147483648",
            b"4294967296/task/1",
            b"12345678901/task/1",
            b"1/task/99999999999",
            b"\xd9\xa1/task/1",
        ];
        for spelling in refused {
            assert_eq!(parse_thread_self(spelling), None, "{spelling:?}");
        }
    }
}
