//! Linux x86_64 GNU **LP64** ABI boundary. **The only module allowed to use
//! `unsafe`.** It is never compiled for GNU x32 (see `sys/mod.rs`).
//!
//! Every constant and layout below was checked against the installed headers
//! (hashes recorded in the ABF10/ABF11 reports):
//!
//! - `struct open_how`: `linux/openat2.h` — three `__u64` fields, 24 bytes;
//! - `RESOLVE_*`: `linux/openat2.h`;
//! - `__NR_openat2 = 437`: `asm/unistd_64.h`;
//! - `O_*`: `bits/fcntl-linux.h` and `asm-generic/fcntl.h` (x86_64 uses the
//!   generic values);
//! - `struct statfs`: glibc `bits/statfs.h` on LP64, 120 bytes;
//! - `EXT{2,3,4}_SUPER_MAGIC = 0xEF53`, `PROC_SUPER_MAGIC = 0x9FA0`:
//!   `linux/magic.h`;
//! - `__NR_readlinkat = 267`: `asm/unistd_64.h`;
//! - `__NR_getdents64 = 217`: `asm/unistd_64.h` (M2A.2b);
//! - `struct linux_dirent64` record framing (`getdents(2)`): `d_ino` u64 at
//!   0, `d_off` i64 at 8, `d_reclen` u16 at 16, `d_type` u8 at 18, `d_name`
//!   at 19. The record is parsed as bytes in safe code
//!   (`crate::enumeration`); no struct is ever cast onto the buffer;
//! - errno values: `asm-generic/errno-base.h` / `errno.h`
//!   (`EWOULDBLOCK == EAGAIN == 11`).
//!
//! Compile-time assertions pin the struct sizes, alignments and field
//! offsets. The unsafe operations are: the `openat2` system call, the
//! transfer of its returned descriptor into an `OwnedFd`, `fstatfs`, the
//! direct `readlinkat` system call (M2A.2a), the direct `getdents64` system
//! call (M2A.2b) and, in the test harness only, `fcntl(F_GETFL/F_GETFD)`.
//!
//! Ownership invariant: a raw descriptor returned by the kernel is wrapped
//! into an `OwnedFd` immediately, exactly once, before any other code runs,
//! and then moved into exactly one typed owner: [`Fd`] (always an `O_PATH`
//! descriptor), one of the private procfs admission states built on it
//! (module [`admission`]),
//! [`DirectoryPinFd`] (an `O_PATH|O_DIRECTORY` directory pin built on it),
//! [`ReadableFd`] (the only regular-file descriptor opened for reading) or
//! [`DirectoryReadFd`] (the only directory descriptor opened for reading).
//! The owner closes it on drop (including on every error path and during
//! unwinding, by ordinary Rust drop order). No function in this crate
//! returns, accepts or stores a raw descriptor number outside this module's
//! borrowed calls.
//!
//! Source-path resolutions (every name resolved beneath a held directory:
//! root, binding checks, regular-file pins, directory pins and the M2A.2b
//! directory self-open) use the full mask `BENEATH|NO_SYMLINKS|
//! NO_MAGICLINKS|NO_XDEV` (0x0F). The process-root anchor `/` uses 0x07
//! (`BENEATH` rejects every absolute path). The only other masks are the
//! private M2A.2a procfs exceptions below, which never resolve a source path.
//!
//! M2A.2a procfs exception (README, "Readable promotion"): the procfs opens
//! below take **no** caller-supplied path, flags or descriptor number. Their
//! names are constants or decimal numbers formatted here, and their
//! `open_how` values are private constants of the [`admission`] module. The
//! relaxed terminal resolution (`resolve = 0`) exists only in
//! [`admission::AdmittedFdDir::open_readable`], which accepts only the
//! admitted per-call `fd` directory of the calling thread and an admitted
//! regular source (F16-1): a candidate, a generic [`Fd`] or a task number
//! cannot be supplied. Directory enumeration (M2A.2b) uses **no** procfs
//! path and no relaxed mask.

use std::ffi::{CStr, c_char, c_int, c_long};
use std::fs::{File, Metadata};
use std::io;
use std::mem::{align_of, offset_of, size_of};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

// ---------------------------------------------------------------------------
// ABI selection guards
// ---------------------------------------------------------------------------

// The selector in `sys/mod.rs` already excludes these; this is a second,
// local guard in case a selector is ever widened by mistake.
#[cfg(any(not(target_pointer_width = "64"), target_abi = "x32"))]
compile_error!("abf-secure-fs: the handwritten Linux ABI is reviewed for LP64 x86_64 only");

// The `syscall` SAFETY argument and the `struct statfs` layout assume the
// LP64 x86_64 ABI: `long`, pointers and `usize` are 64 bits (full register
// width, so no variadic argument carries undefined upper bits), and `int` is
// 32 bits. GNU x32 (32-bit `long` and pointers) fails these at compile time.
const _: () = {
    assert!(size_of::<c_long>() == 8);
    assert!(size_of::<c_int>() == 4);
    assert!(size_of::<usize>() == 8);
    assert!(size_of::<*const c_char>() == 8);
    assert!(size_of::<*const OpenHow>() == 8);
    assert!(size_of::<*mut StatFs>() == 8);
    assert!(size_of::<*mut c_char>() == 8);
    assert!(size_of::<c_long>() == size_of::<usize>());
};

// ---------------------------------------------------------------------------
// ABI constants
// ---------------------------------------------------------------------------

/// `__NR_openat2` on x86_64 LP64 (`asm/unistd_64.h:354`). Not the x32 number
/// (`__X32_SYSCALL_BIT + 437` in `asm/unistd_x32.h`).
const SYS_OPENAT2: c_long = 437;
/// `__NR_readlinkat` on x86_64 LP64 (`asm/unistd_64.h:271`).
const SYS_READLINKAT: c_long = 267;
/// `__NR_getdents64` on x86_64 LP64 (`asm/unistd_64.h`). Not the x32 number
/// (`__X32_SYSCALL_BIT + 217`).
const SYS_GETDENTS64: c_long = 217;
/// `AT_FDCWD` (`linux/fcntl.h`). Only used with the constant absolute "/".
const AT_FDCWD: c_int = -100;

pub(crate) const O_RDONLY: u64 = 0;
pub(crate) const O_NONBLOCK: u64 = 0o4_000;
pub(crate) const O_PATH: u64 = 0o10_000_000;
pub(crate) const O_DIRECTORY: u64 = 0o200_000;
pub(crate) const O_NOFOLLOW: u64 = 0o400_000;
pub(crate) const O_CLOEXEC: u64 = 0o2_000_000;

pub(crate) const RESOLVE_NO_XDEV: u64 = 0x01;
pub(crate) const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
pub(crate) const RESOLVE_NO_SYMLINKS: u64 = 0x04;
pub(crate) const RESOLVE_BENEATH: u64 = 0x08;

/// `EXT2_SUPER_MAGIC == EXT3_SUPER_MAGIC == EXT4_SUPER_MAGIC`. The magic does
/// not distinguish ext2/3/4.
pub(crate) const EXT_SUPER_MAGIC: i64 = 0xEF53;
/// `PROC_SUPER_MAGIC`. Filesystem classification only: it does not
/// authenticate the mount or its PID namespace.
pub(crate) const PROC_SUPER_MAGIC: i64 = 0x9FA0;

pub(crate) const EPERM: i32 = 1;
pub(crate) const ENOENT: i32 = 2;
pub(crate) const EINTR: i32 = 4;
pub(crate) const E2BIG: i32 = 7;
pub(crate) const EAGAIN: i32 = 11;
pub(crate) const EACCES: i32 = 13;
pub(crate) const EXDEV: i32 = 18;
pub(crate) const ENOTDIR: i32 = 20;
pub(crate) const EINVAL: i32 = 22;
pub(crate) const ENOSYS: i32 = 38;
pub(crate) const ELOOP: i32 = 40;

/// `struct open_how` (`linux/openat2.h`). `mode` is always 0: no `O_CREAT`
/// or `O_TMPFILE` is ever used, and openat2 rejects a non-zero mode
/// otherwise.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenHow {
    pub(crate) flags: u64,
    pub(crate) mode: u64,
    pub(crate) resolve: u64,
}

const _: () = {
    assert!(size_of::<OpenHow>() == 24);
    assert!(align_of::<OpenHow>() == 8);
    assert!(offset_of!(OpenHow, flags) == 0);
    assert!(offset_of!(OpenHow, mode) == 8);
    assert!(offset_of!(OpenHow, resolve) == 16);
};

/// glibc `struct statfs` on x86_64 (`bits/statfs.h`, LP64; `__fsword_t` is
/// `long`, block/file counts are 64-bit, `__fsid_t` is `int[2]`). Only
/// `f_type` is read. The other fields exist so the kernel writes into a
/// correctly sized buffer.
#[repr(C)]
struct StatFs {
    f_type: i64,
    f_bsize: i64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: [c_int; 2],
    f_namelen: i64,
    f_frsize: i64,
    f_flags: i64,
    f_spare: [i64; 4],
}

const _: () = {
    assert!(size_of::<StatFs>() == 120);
    assert!(align_of::<StatFs>() == 8);
    assert!(offset_of!(StatFs, f_type) == 0);
    assert!(offset_of!(StatFs, f_fsid) == 56);
    assert!(offset_of!(StatFs, f_spare) == 88);
};

impl StatFs {
    /// All-integer struct: the all-zero value is valid and needs no `unsafe`.
    const fn zeroed() -> Self {
        Self {
            f_type: 0,
            f_bsize: 0,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_fsid: [0; 2],
            f_namelen: 0,
            f_frsize: 0,
            f_flags: 0,
            f_spare: [0; 4],
        }
    }
}

// SAFETY: both symbols are provided by glibc on x86_64-unknown-linux-gnu with
// exactly these C signatures: `long syscall(long number, ...)` (unistd.h) and
// `int fstatfs(int fd, struct statfs *buf)` (sys/statfs.h; on LP64 glibc
// `fstatfs` and `fstatfs64` share the layout asserted above).
unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
    fn fstatfs(fd: c_int, buf: *mut StatFs) -> c_int;
}

// ---------------------------------------------------------------------------
// Owned descriptors
// ---------------------------------------------------------------------------

/// Handle-derived metadata of an owned descriptor (`statx(fd, "",
/// AT_EMPTY_PATH)` / `fstat`). Never opens or reads the object.
pub(crate) trait HandleMetadata {
    fn metadata(&self) -> io::Result<Metadata>;
}

/// Sole owner of one kernel descriptor obtained from `openat2`.
///
/// Internally stored as a `File` only to reach std's handle-based metadata
/// (`statx(fd, "", AT_EMPTY_PATH)` / `fstat`). The descriptor is always an
/// `O_PATH` descriptor ([`openat2_once`] refuses any `open_how` without
/// `O_PATH`): any read/write on it fails with `EBADF`, and no such call
/// exists in this crate. The `File` is never exposed. `Debug` is
/// deliberately opaque: std's `File` `Debug` resolves `/proc/self/fd/N` and
/// would print the path.
pub(crate) struct Fd {
    file: File,
}

impl std::fmt::Debug for Fd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fd(..)")
    }
}

impl Fd {
    /// Handle-derived metadata of the referenced object. Never opens the
    /// object, so FIFOs, sockets and devices are inspected without I/O.
    pub(crate) fn metadata(&self) -> io::Result<Metadata> {
        self.file.metadata()
    }

    fn raw(&self) -> c_int {
        self.file.as_raw_fd()
    }
}

impl HandleMetadata for Fd {
    fn metadata(&self) -> io::Result<Metadata> {
        Fd::metadata(self)
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        // `self.file` closes the descriptor right after this body runs.
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        fault::note_closed();
    }
}

/// Sole owner of the one descriptor type in this crate that is open for
/// reading: `O_RDONLY|O_CLOEXEC|O_NONBLOCK`, produced only by
/// [`open_readable_leaf`]. It is a fresh open file description (its own
/// offset and status flags), not a duplicate of the `O_PATH` source.
///
/// The `File` is never exposed: no `Read`/`Seek`/raw descriptor leaves this
/// module. Default builds have no method that reads bytes; the bounded
/// synthetic read exists only in the two-key test harness. `Debug` is
/// opaque (no `/proc/self/fd` path resolution).
pub(crate) struct ReadableFd {
    file: File,
}

impl std::fmt::Debug for ReadableFd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadableFd(..)")
    }
}

impl HandleMetadata for ReadableFd {
    fn metadata(&self) -> io::Result<Metadata> {
        self.file.metadata()
    }
}

impl Drop for ReadableFd {
    fn drop(&mut self) {
        // `self.file` closes the descriptor right after this body runs.
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        fault::note_closed();
    }
}

/// One `openat2` attempt anchored at a held directory. Returns the raw errno
/// on failure. No retry, no flag change: `how` is borrowed immutably and
/// passed through unchanged.
pub(crate) fn openat2_beneath(dir: &Fd, path: &CStr, how: &OpenHow) -> Result<Fd, i32> {
    openat2_once(dir.raw(), path, how)
}

/// One `openat2` attempt for the constant absolute path "/" (the process
/// root), with `AT_FDCWD`. Only this constant path can ever use `AT_FDCWD`.
pub(crate) fn openat2_process_root(how: &OpenHow) -> Result<Fd, i32> {
    openat2_once(AT_FDCWD, c"/", how)
}

/// One `openat2` attempt whose result is an [`Fd`]. Enforces the [`Fd`]
/// type invariant: an `open_how` without `O_PATH` is refused (`EINVAL`)
/// before any harness hook or system call, so an [`Fd`] can never be an I/O
/// descriptor.
fn openat2_once(dirfd: c_int, path: &CStr, how: &OpenHow) -> Result<Fd, i32> {
    if how.flags & O_PATH != O_PATH {
        return Err(EINVAL);
    }
    let owned = openat2_owned(dirfd, path, how)?;
    Ok(Fd {
        file: File::from(owned),
    })
}

/// The single raw `openat2` call site. The returned descriptor is wrapped
/// into an `OwnedFd` immediately; callers move it into exactly one typed
/// owner.
fn openat2_owned(dirfd: c_int, path: &CStr, how: &OpenHow) -> Result<OwnedFd, i32> {
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    if let Some(errno) = fault::before_openat2(how) {
        return Err(errno);
    }

    let path_ptr: *const c_char = path.as_ptr();
    let how_ptr: *const OpenHow = how;
    // SAFETY:
    // - `syscall` is the glibc variadic wrapper declared above. Each variadic
    //   argument is passed at full register width (c_long, pointers, usize)
    //   so that no upper-bit garbage reaches the kernel.
    // - `path_ptr` points to a NUL-terminated string borrowed from `path`,
    //   valid for the whole call.
    // - `how_ptr` points to a live, correctly laid out `struct open_how`
    //   (asserted above), borrowed for the whole call. The kernel only reads
    //   it, and `size` is its exact size.
    // - `dirfd` is either AT_FDCWD (only with the constant "/") or the
    //   descriptor of an `Fd` borrowed by the caller for the whole call, so it
    //   cannot be closed or reused meanwhile.
    // - The flags never include O_CREAT/O_TRUNC/O_TMPFILE/write access
    //   (memory safety does not depend on the flags). Every call except the
    //   readable procfs leaf and the directory self-open uses O_PATH, which
    //   does not open the object for I/O. The leaf's O_RDONLY|O_NONBLOCK
    //   open is reached only for the still-owned, previously type-checked
    //   regular source (see `admission::AdmittedFdDir::open_readable`); the
    //   directory self-open's
    //   O_RDONLY|O_DIRECTORY open only for "." of a held directory pin (see
    //   `open_directory_read`).
    let ret: c_long = unsafe {
        syscall(
            SYS_OPENAT2,
            c_long::from(dirfd),
            path_ptr,
            how_ptr,
            size_of::<OpenHow>(),
        )
    };
    if ret < 0 {
        // Read errno immediately; nothing in between can overwrite it.
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(errno);
    }
    let Ok(raw) = c_int::try_from(ret) else {
        // A kernel fd always fits in `int`. If that is ever violated there is
        // no valid descriptor number to take ownership of, so nothing is
        // wrapped and the call fails closed. Unreachable on Linux.
        return Err(0);
    };
    // SAFETY: `raw` is a fresh descriptor just returned by a successful
    // openat2. Nothing else in the process knows or owns it yet. Ownership
    // moves into `OwnedFd` exactly once, here, and from then on only the
    // single typed owner it is moved into (`Fd` or `ReadableFd`) closes it.
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    fault::note_opened();
    Ok(owned)
}

/// Filesystem magic (`f_type`) of the object behind `fd`.
pub(crate) fn filesystem_magic(fd: &Fd) -> Result<i64, i32> {
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    if let Some(magic) = fault::injected_magic() {
        return Ok(magic);
    }

    let mut buf = StatFs::zeroed();
    let buf_ptr: *mut StatFs = &mut buf;
    // SAFETY: `fd.raw()` is owned by a live borrowed `Fd`, and fstatfs works
    // on O_PATH descriptors. `buf_ptr` points to an exclusively borrowed,
    // initialized `StatFs` whose layout matches glibc's `struct statfs`
    // (asserted above). The callee writes at most `size_of::<StatFs>()` bytes.
    let ret = unsafe { fstatfs(fd.raw(), buf_ptr) };
    if ret != 0 {
        return Err(io::Error::last_os_error().raw_os_error().unwrap_or(0));
    }
    Ok(buf.f_type)
}

// ---------------------------------------------------------------------------
// M2A.2a: private procfs admission chain and the readable leaf (F16-1)
// ---------------------------------------------------------------------------

/// Largest accepted PID/TID: the positive `int` range.
pub(crate) const MAX_TASK_ID: u32 = 2_147_483_647;

/// Capacity of a generated procfs name, including its NUL. The longest
/// generated name is `2147483647/task/2147483647/fd` (29 bytes).
const PROC_NAME_CAPACITY: usize = 32;

/// A NUL-terminated procfs name built **only** from constant literals and
/// decimal numbers, in a fixed zero-initialized buffer. It never holds
/// caller text.
struct ProcName {
    buf: [u8; PROC_NAME_CAPACITY],
    len: usize,
}

impl ProcName {
    const fn new() -> Self {
        Self {
            buf: [0; PROC_NAME_CAPACITY],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) -> Option<()> {
        // One byte always stays free for the terminating NUL.
        if self.len + 1 >= PROC_NAME_CAPACITY {
            return None;
        }
        self.buf[self.len] = byte;
        self.len += 1;
        Some(())
    }

    fn literal(&mut self, text: &'static str) -> Option<()> {
        text.bytes().try_for_each(|byte| self.push(byte))
    }

    fn decimal(&mut self, value: u32) -> Option<()> {
        let mut digits = [0_u8; 10];
        let mut rest = value;
        let mut count = 0;
        loop {
            digits[count] = b'0' + (rest % 10) as u8;
            count += 1;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        digits[..count]
            .iter()
            .rev()
            .try_for_each(|&digit| self.push(digit))
    }

    /// Fails (None) on an interior NUL; `buf[len]` is the NUL.
    fn as_c_str(&self) -> Option<&CStr> {
        CStr::from_bytes_with_nul(&self.buf[..=self.len]).ok()
    }
}

fn task_fd_dir_name(pid: u32, tid: u32) -> Option<ProcName> {
    let mut name = ProcName::new();
    name.decimal(pid)?;
    name.literal("/task/")?;
    name.decimal(tid)?;
    name.literal("/fd")?;
    Some(name)
}

/// Size of the fixed self-link buffer. A result that fills it is treated as
/// possibly truncated and refused.
pub(crate) const SELF_LINK_CAPACITY: usize = 32;

/// The bytes the kernel returned for a self link, `len < SELF_LINK_CAPACITY`.
/// `Debug` never prints them. Private to the ABI module: only the admission
/// chain below reads and parses a self link.
struct SelfLinkBytes {
    buf: [u8; SELF_LINK_CAPACITY],
    len: usize,
}

impl SelfLinkBytes {
    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkReadError {
    Errno(i32),
    /// The result filled the buffer (or exceeded it): possibly truncated.
    Truncated,
}

/// One direct `readlinkat(link, "", buf, 32)` system call on an owned,
/// not-followed link descriptor. No libc wrapper, no fallback, no retry.
/// Private: its only caller is [`admission::AdmittedSelfLink`].
fn read_self_link(link: &Fd) -> Result<SelfLinkBytes, LinkReadError> {
    let mut buf = [0_u8; SELF_LINK_CAPACITY];

    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    if let Some(injected) = fault::before_readlink(&mut buf) {
        return match injected {
            Ok(ret) => self_link_result(buf, ret),
            Err(errno) => Err(LinkReadError::Errno(errno)),
        };
    }

    let path_ptr: *const c_char = c"".as_ptr();
    let buf_ptr: *mut c_char = buf.as_mut_ptr().cast();
    // SAFETY:
    // - `syscall` is the glibc variadic wrapper declared above; every
    //   variadic argument is full register width (c_long, pointers, usize).
    // - `link` is a live descriptor borrowed for the whole call.
    // - `path_ptr` is the static empty NUL-terminated string: the call acts
    //   on the O_PATH|O_NOFOLLOW link descriptor itself (documented
    //   readlinkat empty-path form).
    // - `buf_ptr` points to an exclusively borrowed, initialized array of
    //   exactly `SELF_LINK_CAPACITY` bytes; the kernel writes at most that
    //   many bytes and adds no NUL.
    let ret: c_long = unsafe {
        syscall(
            SYS_READLINKAT,
            c_long::from(link.raw()),
            path_ptr,
            buf_ptr,
            SELF_LINK_CAPACITY,
        )
    };
    if ret < 0 {
        // Read errno immediately; nothing in between can overwrite it.
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(LinkReadError::Errno(errno));
    }
    self_link_result(buf, ret)
}

/// Checks the returned length before any slicing.
fn self_link_result(
    buf: [u8; SELF_LINK_CAPACITY],
    ret: c_long,
) -> Result<SelfLinkBytes, LinkReadError> {
    let Ok(len) = usize::try_from(ret) else {
        return Err(LinkReadError::Errno(0));
    };
    if len >= SELF_LINK_CAPACITY {
        return Err(LinkReadError::Truncated);
    }
    Ok(SelfLinkBytes { buf, len })
}

/// The private procfs admission chain of readable promotion (F16-1).
///
/// Every value below has **private fields** and is minted **only** by the
/// checked constructor of the previous state, inside this module. Code
/// outside it (including the rest of this ABI module, `crate::promotion`
/// and every other sibling) can hold, move and drop these values, but cannot
/// construct one, read its descriptor, or skip a state:
///
/// 1. [`ProcessRoot`]: the constant `/` with the unchanged
///    `crate::linux::PROCESS_ROOT_HOW` (mask 0x07). No path, flags or
///    descriptor input.
/// 2. [`CandidateProcRoot`]: the constant `proc` beneath a [`ProcessRoot`]
///    (mask 0x0E). It has no operation except [`CandidateProcRoot::admit`].
/// 3. [`AdmittedProcRoot`]: procfs magic, directory; its device is kept.
/// 4. [`CandidateSelfLink`]: the constant `thread-self` beneath **that**
///    admitted root, not followed (full mask). It borrows the root.
/// 5. [`AdmittedSelfLink`]: procfs magic, symlink, the root's device.
/// 6. [`CurrentTask`]: one bounded `readlinkat` of the admitted link,
///    parsed by the strict canonical grammar. It carries the borrowed
///    admitted root it came from: the task `fd` directory can only be
///    opened beneath that same root (one chain, not a lifetime coincidence).
///    No constructor accepts numbers: parsing never creates authority.
/// 7. [`CandidateFdDir`]: `<pid>/task/<tid>/fd` formatted here from the
///    task, beneath its own root (full mask), with that root's device.
/// 8. [`AdmittedFdDir`]: procfs magic, directory, the chain's device.
/// 9. [`AdmittedRegularSource`]: borrows a genuine [`Pinned`] and is minted
///    only by the existing S0 sample (`Pinned::reinspect`: regular file,
///    pinned identity, `nlink == 1`); no second S0 sample is taken.
///
/// The terminal relaxed open ([`AdmittedFdDir::open_readable`]) accepts
/// **only** an [`AdmittedFdDir`] and an [`AdmittedRegularSource`]; no
/// generic [`Fd`], candidate, raw number, path or flags. Its `open_how`
/// (`resolve = 0`) is a private constant of this module, so no other code
/// can name it.
///
/// Task-derived values ([`CandidateSelfLink`] onwards, except the source)
/// are `!Send` and `!Sync`: a calling thread's procfs anchor never moves to
/// another thread. Every value is created per call and dropped by the
/// caller; nothing is cached, cloned or duplicated. Admission is a sample of
/// type, magic and device at that moment: it classifies, it does not
/// authenticate the mount or prove an enduring state.
pub(crate) mod admission {
    use super::{
        Fd, LinkReadError, MAX_TASK_ID, O_CLOEXEC, O_DIRECTORY, O_NOFOLLOW, O_NONBLOCK, O_PATH,
        O_RDONLY, OpenHow, PROC_SUPER_MAGIC, ProcName, RESOLVE_BENEATH, RESOLVE_NO_MAGICLINKS,
        RESOLVE_NO_SYMLINKS, RESOLVE_NO_XDEV, ReadableFd, SelfLinkBytes, filesystem_magic,
        openat2_once, openat2_owned, openat2_process_root, read_self_link, task_fd_dir_name,
    };
    use crate::error::AccessError;
    use crate::linux::{PROCESS_ROOT_HOW, Pinned};
    use std::fs::{File, Metadata};
    use std::io;
    use std::marker::PhantomData;
    use std::mem::size_of;
    use std::os::unix::fs::MetadataExt;
    use std::rc::Rc;

    /// `proc` beneath the held process root "/". `RESOLVE_NO_XDEV` is
    /// omitted **only here**, because the trusted proc mount is necessarily
    /// another mount. BENEATH, NO_SYMLINKS and NO_MAGICLINKS are kept.
    const PROC_MOUNT_HOW: OpenHow = OpenHow {
        flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    };

    /// `thread-self` beneath the admitted proc root, full mask, not
    /// followed: with `O_PATH|O_NOFOLLOW`, openat2 returns the final symlink
    /// itself (documented exception), so only its metadata is ever used.
    const PROC_SELF_LINK_HOW: OpenHow = OpenHow {
        flags: O_PATH | O_NOFOLLOW | O_CLOEXEC,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV,
    };

    /// `<pid>/task/<tid>/fd` beneath the admitted proc root, full mask.
    const PROC_FD_DIR_HOW: OpenHow = OpenHow {
        flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV,
    };

    /// The terminal open of the generated `<fd>` leaf. `resolve = 0` is
    /// required: every confinement flag refuses the magic-link jump. This is
    /// the **only** relaxed resolution in the crate, and this private
    /// constant is nameable only inside this module. No O_PATH, O_NOFOLLOW,
    /// O_CREAT, O_TRUNC, O_TMPFILE or write access; mode 0.
    const PROC_FD_LEAF_HOW: OpenHow = OpenHow {
        flags: O_RDONLY | O_CLOEXEC | O_NONBLOCK,
        mode: 0,
        resolve: 0,
    };

    /// Zero-sized marker that makes a type `!Send` and `!Sync`.
    type ThreadBound = PhantomData<Rc<()>>;

    /// Refusal of one admission sample.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum AdmissionRefusal {
        /// `fstatfs` or `statx` failed with this errno.
        Errno(i32),
        /// Wrong magic, type or device, or metadata without an errno.
        NotAdmitted,
    }

    /// Refusal of the self-link read.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum TaskRefusal {
        Link(LinkReadError),
        /// Not exactly the canonical `<pid>/task/<tid>` spelling.
        Malformed,
    }

    /// Kind of a procfs handle, as far as admission cares.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Directory,
        Symlink,
        Other,
    }

    /// The handle facts an admission decision uses.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Facts {
        kind: Kind,
        device: u64,
    }

    fn facts(metadata: io::Result<Metadata>) -> Result<Facts, AdmissionRefusal> {
        let metadata = metadata.map_err(|error| match error.raw_os_error() {
            Some(errno) => AdmissionRefusal::Errno(errno),
            None => AdmissionRefusal::NotAdmitted,
        })?;
        let file_type = metadata.file_type();
        let kind = if file_type.is_dir() {
            Kind::Directory
        } else if file_type.is_symlink() {
            Kind::Symlink
        } else {
            Kind::Other
        };
        Ok(Facts {
            kind,
            device: metadata.dev(),
        })
    }

    /// Procfs directory admission: magic first, then metadata, directory,
    /// and (when given) the chain's device. Returns the device.
    fn directory_verdict(
        magic: Result<i64, i32>,
        facts: Result<Facts, AdmissionRefusal>,
        device: Option<u64>,
    ) -> Result<u64, AdmissionRefusal> {
        let magic = magic.map_err(AdmissionRefusal::Errno)?;
        if magic != PROC_SUPER_MAGIC {
            return Err(AdmissionRefusal::NotAdmitted);
        }
        let facts = facts?;
        if facts.kind != Kind::Directory {
            return Err(AdmissionRefusal::NotAdmitted);
        }
        if device.is_some_and(|device| device != facts.device) {
            return Err(AdmissionRefusal::NotAdmitted);
        }
        Ok(facts.device)
    }

    /// Self-link admission: procfs magic, symlink, the chain's device.
    fn link_verdict(magic: i64, facts: Facts, device: u64) -> Result<(), AdmissionRefusal> {
        if magic != PROC_SUPER_MAGIC || facts.kind != Kind::Symlink || facts.device != device {
            return Err(AdmissionRefusal::NotAdmitted);
        }
        Ok(())
    }

    /// Strict parser for the kernel-generated `thread-self` target
    /// `<pid>/task/<tid>`: two canonical decimal numbers in `1..=2147483647`
    /// (1 to 10 digits, no leading zero, no sign) joined by the exact
    /// literal `/task/`, nothing else. Any other spelling, including an
    /// empty or possibly truncated result, is refused (`None`); there is no
    /// alternative format. The tuple it returns is **not** authority: only
    /// [`AdmittedSelfLink::read_current_task`] turns it into a
    /// [`CurrentTask`].
    fn parse_thread_self(bytes: &[u8]) -> Option<(u32, u32)> {
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

    macro_rules! opaque_debug {
        ($($name:ident $(<$life:lifetime>)?),+) => {$(
            impl std::fmt::Debug for $name $(<$life>)? {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str(concat!(stringify!($name), "(..)"))
                }
            }
        )+};
    }

    /// The constant process root `/`, opened only with the unchanged
    /// `PROCESS_ROOT_HOW`. Its only use is acquiring a [`CandidateProcRoot`].
    pub(crate) struct ProcessRoot {
        fd: Fd,
    }

    /// The constant `proc` beneath a [`ProcessRoot`], **not** admitted.
    /// Nothing accepts it except [`CandidateProcRoot::admit`].
    pub(crate) struct CandidateProcRoot {
        fd: Fd,
    }

    /// An admitted procfs root: procfs magic, directory; its device.
    pub(crate) struct AdmittedProcRoot {
        fd: Fd,
        device: u64,
    }

    /// The not-followed `thread-self` link beneath an admitted root, **not**
    /// admitted. It borrows that root.
    pub(crate) struct CandidateSelfLink<'proc> {
        fd: Fd,
        proc: &'proc AdmittedProcRoot,
        _thread: ThreadBound,
    }

    /// An admitted self link: procfs magic, symlink, the root's device.
    pub(crate) struct AdmittedSelfLink<'proc> {
        fd: Fd,
        proc: &'proc AdmittedProcRoot,
        _thread: ThreadBound,
    }

    /// The calling thread's task, as named by the kernel through the
    /// admitted self link of `proc`, read on this thread. It holds no
    /// descriptor. It cannot be built from numbers, cloned or sent.
    pub(crate) struct CurrentTask<'proc> {
        proc: &'proc AdmittedProcRoot,
        pid: u32,
        tid: u32,
        _thread: ThreadBound,
    }

    /// The task `fd` directory of a [`CurrentTask`], **not** admitted. It
    /// keeps its chain's proc device (the root itself may already be
    /// dropped).
    pub(crate) struct CandidateFdDir {
        fd: Fd,
        device: u64,
        _thread: ThreadBound,
    }

    /// An admitted task `fd` directory of the calling thread: procfs magic,
    /// directory, its chain's device. The only directory the readable leaf
    /// accepts.
    pub(crate) struct AdmittedFdDir {
        fd: Fd,
        _thread: ThreadBound,
    }

    /// A genuine pin that passed the S0 sample (regular file, pinned
    /// identity, `nlink == 1`) when this value was minted. It borrows the
    /// pin, so the pin's descriptor number stays bound to the same open
    /// description for as long as this value lives. The only source the
    /// readable leaf accepts.
    pub(crate) struct AdmittedRegularSource<'pin> {
        pinned: &'pin Pinned,
    }

    opaque_debug!(
        ProcessRoot,
        CandidateProcRoot,
        AdmittedProcRoot,
        CandidateSelfLink<'_>,
        AdmittedSelfLink<'_>,
        CurrentTask<'_>,
        CandidateFdDir,
        AdmittedFdDir,
        AdmittedRegularSource<'_>
    );

    impl ProcessRoot {
        /// One attempt: the constant absolute "/" with `AT_FDCWD` and the
        /// unchanged `PROCESS_ROOT_HOW` (mask 0x07).
        pub(crate) fn open() -> Result<Self, i32> {
            openat2_process_root(&PROCESS_ROOT_HOW).map(|fd| Self { fd })
        }

        /// One attempt: the constant name `proc` beneath this root (mask
        /// 0x0E).
        pub(crate) fn open_proc(&self) -> Result<CandidateProcRoot, i32> {
            openat2_once(self.fd.raw(), c"proc", &PROC_MOUNT_HOW).map(|fd| CandidateProcRoot { fd })
        }
    }

    impl CandidateProcRoot {
        /// Admits this handle as procfs (magic `0x9FA0`, directory). The
        /// candidate is consumed; on refusal its descriptor is closed.
        pub(crate) fn admit(self) -> Result<AdmittedProcRoot, AdmissionRefusal> {
            let magic = filesystem_magic(&self.fd);
            let facts = facts(self.fd.metadata());
            let device = directory_verdict(magic, facts, None)?;
            Ok(AdmittedProcRoot {
                fd: self.fd,
                device,
            })
        }
    }

    impl AdmittedProcRoot {
        /// One attempt: the constant name `thread-self` beneath this
        /// admitted root, not followed (full mask).
        pub(crate) fn open_thread_self(&self) -> Result<CandidateSelfLink<'_>, i32> {
            openat2_once(self.fd.raw(), c"thread-self", &PROC_SELF_LINK_HOW).map(|fd| {
                CandidateSelfLink {
                    fd,
                    proc: self,
                    _thread: PhantomData,
                }
            })
        }
    }

    impl<'proc> CandidateSelfLink<'proc> {
        /// Admits this handle as a procfs symlink on its root's device.
        /// Magic is sampled first; metadata only after it succeeded.
        pub(crate) fn admit(self) -> Result<AdmittedSelfLink<'proc>, AdmissionRefusal> {
            let magic = filesystem_magic(&self.fd).map_err(AdmissionRefusal::Errno)?;
            let facts = facts(self.fd.metadata())?;
            link_verdict(magic, facts, self.proc.device)?;
            Ok(AdmittedSelfLink {
                fd: self.fd,
                proc: self.proc,
                _thread: PhantomData,
            })
        }
    }

    impl<'proc> AdmittedSelfLink<'proc> {
        /// One direct `readlinkat` of this admitted link (no retry here),
        /// then the strict canonical parse. The payload is dropped here and
        /// never stored or printed.
        pub(crate) fn read_current_task(&self) -> Result<CurrentTask<'proc>, TaskRefusal> {
            let bytes: SelfLinkBytes = read_self_link(&self.fd).map_err(TaskRefusal::Link)?;
            let (pid, tid) = parse_thread_self(bytes.as_bytes()).ok_or(TaskRefusal::Malformed)?;
            Ok(CurrentTask {
                proc: self.proc,
                pid,
                tid,
                _thread: PhantomData,
            })
        }
    }

    impl CurrentTask<'_> {
        /// One attempt: `<pid>/task/<tid>/fd` formatted here from this task,
        /// beneath **this task's own** admitted root (full mask).
        pub(crate) fn open_fd_dir(&self) -> Result<CandidateFdDir, i32> {
            let name = task_fd_dir_name(self.pid, self.tid).ok_or(super::EINVAL)?;
            let path = name.as_c_str().ok_or(super::EINVAL)?;
            openat2_once(self.proc.fd.raw(), path, &PROC_FD_DIR_HOW).map(|fd| CandidateFdDir {
                fd,
                device: self.proc.device,
                _thread: PhantomData,
            })
        }
    }

    impl CandidateFdDir {
        /// Admits this handle as a procfs directory on its chain's device.
        pub(crate) fn admit(self) -> Result<AdmittedFdDir, AdmissionRefusal> {
            let magic = filesystem_magic(&self.fd);
            let facts = facts(self.fd.metadata());
            directory_verdict(magic, facts, Some(self.device))?;
            Ok(AdmittedFdDir {
                fd: self.fd,
                _thread: PhantomData,
            })
        }
    }

    impl<'pin> AdmittedRegularSource<'pin> {
        /// S0: the existing `Pinned::reinspect` sample of the held handle
        /// (regular file, pinned identity, `nlink == 1`), exactly once.
        pub(crate) fn admit(pinned: &'pin Pinned) -> Result<Self, AccessError> {
            pinned.reinspect()?;
            Ok(Self { pinned })
        }
    }

    impl AdmittedFdDir {
        /// One attempt: opens the generated decimal leaf of the admitted
        /// source's still-owned descriptor beneath this admitted directory,
        /// with the fixed `O_RDONLY|O_CLOEXEC|O_NONBLOCK`, mode 0, resolve 0.
        ///
        /// The leaf name is formatted here from the source's own descriptor
        /// number (at most ten digits, no `/`); it never comes from a
        /// caller, a string or a `readlink` of the descriptor. The source is
        /// borrowed for the whole call, so that number stays bound to the
        /// same open description. The kernel follows the fd entry's magic
        /// link to the object held by the pin, not to any pathname.
        pub(crate) fn open_readable(
            &self,
            source: &AdmittedRegularSource<'_>,
        ) -> Result<ReadableFd, i32> {
            let number = u32::try_from(source.pinned.handle().raw()).map_err(|_| super::EINVAL)?;
            let mut name = ProcName::new();
            name.decimal(number).ok_or(super::EINVAL)?;
            let leaf = name.as_c_str().ok_or(super::EINVAL)?;
            let owned = openat2_owned(self.fd.raw(), leaf, &PROC_FD_LEAF_HOW)?;
            Ok(ReadableFd {
                file: File::from(owned),
            })
        }
    }

    /// Inline control storage of one admission chain, checked against the
    /// 4096-byte ceiling. No admission value allocates.
    const CONTROL_BYTES: usize = size_of::<ProcessRoot>()
        + size_of::<CandidateProcRoot>()
        + size_of::<AdmittedProcRoot>()
        + size_of::<CandidateSelfLink<'static>>()
        + size_of::<AdmittedSelfLink<'static>>()
        + size_of::<CurrentTask<'static>>()
        + size_of::<CandidateFdDir>()
        + size_of::<AdmittedFdDir>()
        + size_of::<AdmittedRegularSource<'static>>()
        + size_of::<SelfLinkBytes>()
        + size_of::<ProcName>()
        + size_of::<ProcName>();

    const _: () = assert!(CONTROL_BYTES <= 4096);

    // Compile-time proof that the thread-bound values are neither `Send`
    // nor `Sync`: if one of them implemented the trait, both blanket impls
    // would apply and the `_` below would be ambiguous (a compile error).
    trait AmbiguousIfSend<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

    trait AmbiguousIfSync<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfSync<()> for T {}
    impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

    const _: fn() = || {
        <CandidateSelfLink<'static> as AmbiguousIfSend<_>>::check();
        <AdmittedSelfLink<'static> as AmbiguousIfSend<_>>::check();
        <CurrentTask<'static> as AmbiguousIfSend<_>>::check();
        <CandidateFdDir as AmbiguousIfSend<_>>::check();
        <AdmittedFdDir as AmbiguousIfSend<_>>::check();
        <CandidateSelfLink<'static> as AmbiguousIfSync<_>>::check();
        <AdmittedSelfLink<'static> as AmbiguousIfSync<_>>::check();
        <CurrentTask<'static> as AmbiguousIfSync<_>>::check();
        <CandidateFdDir as AmbiguousIfSync<_>>::check();
        <AdmittedFdDir as AmbiguousIfSync<_>>::check();
    };

    #[cfg(test)]
    mod tests {
        use super::*;

        // -- canonical task grammar (pure) ---------------------------------

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
                b"00/task/12",
                b"0/task/12",
                b"12/task/0",
                b"0/task/0",
                b"-1/task/12",
                b"12/task/-1",
                b"+1/task/12",
                b"12/task/+1",
                b" 12/task/12",
                b"12/task/12 ",
                b"12/task/12\n",
                b"12/task/12\0",
                b"12/task/1\x002",
                b"1\x00/task/12",
                b"./12/task/12",
                b"../12/task/12",
                b"12/task/../12",
                b"2147483648/task/1",
                b"1/task/2147483648",
                b"4294967295/task/1",
                b"4294967296/task/1",
                b"9999999999/task/1",
                b"12345678901/task/1",
                b"1/task/99999999999",
                b"99999999999999999999/task/1",
                b"\xd9\xa1/task/1",
                b"0x1/task/1",
                b"1e3/task/1",
            ];
            for spelling in refused {
                assert_eq!(parse_thread_self(spelling), None, "{spelling:?}");
            }
        }

        #[test]
        fn truncated_or_full_buffer_link_results_are_refused() {
            assert_eq!(
                super::super::self_link_result([b'1'; 32], 32).err(),
                Some(LinkReadError::Truncated)
            );
            assert_eq!(
                super::super::self_link_result([b'1'; 32], 33).err(),
                Some(LinkReadError::Truncated)
            );
            assert_eq!(
                super::super::self_link_result([b'1'; 32], -1).err(),
                Some(LinkReadError::Errno(0))
            );
            let mut buf = [0_u8; 32];
            buf[..8].copy_from_slice(b"1/task/1");
            let ok = super::super::self_link_result(buf, 8).unwrap();
            assert_eq!(ok.as_bytes(), b"1/task/1");
        }

        // -- admission verdicts (pure: synthetic facts, not kernel evidence) --

        const PROC: i64 = PROC_SUPER_MAGIC;
        const DIR: Facts = Facts {
            kind: Kind::Directory,
            device: 23,
        };
        const LINK: Facts = Facts {
            kind: Kind::Symlink,
            device: 23,
        };

        #[test]
        fn directory_admission_checks_magic_type_and_device_in_order() {
            assert_eq!(directory_verdict(Ok(PROC), Ok(DIR), None), Ok(23));
            assert_eq!(directory_verdict(Ok(PROC), Ok(DIR), Some(23)), Ok(23));
            let refused = Err(AdmissionRefusal::NotAdmitted);
            for magic in [0xEF53_i64, 0x0102_1994, 0, -1, PROC + 1] {
                assert_eq!(directory_verdict(Ok(magic), Ok(DIR), None), refused);
            }
            for kind in [Kind::Symlink, Kind::Other] {
                let facts = Facts { kind, device: 23 };
                assert_eq!(directory_verdict(Ok(PROC), Ok(facts), None), refused);
            }
            assert_eq!(directory_verdict(Ok(PROC), Ok(DIR), Some(24)), refused);
            // The magic sample wins over every later problem.
            assert_eq!(
                directory_verdict(Err(13), Err(AdmissionRefusal::Errno(2)), Some(9)),
                Err(AdmissionRefusal::Errno(13))
            );
            assert_eq!(
                directory_verdict(Ok(0), Err(AdmissionRefusal::Errno(2)), None),
                refused
            );
            assert_eq!(
                directory_verdict(Ok(PROC), Err(AdmissionRefusal::Errno(2)), None),
                Err(AdmissionRefusal::Errno(2))
            );
        }

        #[test]
        fn link_admission_checks_magic_type_and_device() {
            assert_eq!(link_verdict(PROC, LINK, 23), Ok(()));
            let refused = Err(AdmissionRefusal::NotAdmitted);
            assert_eq!(link_verdict(0xEF53, LINK, 23), refused);
            assert_eq!(link_verdict(PROC, DIR, 23), refused);
            let other = Facts {
                kind: Kind::Other,
                device: 23,
            };
            assert_eq!(link_verdict(PROC, other, 23), refused);
            assert_eq!(link_verdict(PROC, LINK, 24), refused);
        }

        #[test]
        fn metadata_errors_keep_their_errno() {
            let with_errno = facts(Err(io::Error::from_raw_os_error(13)));
            assert_eq!(with_errno, Err(AdmissionRefusal::Errno(13)));
            let without = facts(Err(io::Error::other("no errno")));
            assert_eq!(without, Err(AdmissionRefusal::NotAdmitted));
        }

        // -- bounded controls ------------------------------------------------

        #[test]
        fn admission_controls_are_small_and_inline() {
            // CONTROL_BYTES <= 4096 is a compile-time assertion above.
            // Task tokens hold no descriptor: a borrow, two numbers, a ZST.
            assert_eq!(size_of::<ThreadBound>(), 0);
            assert!(size_of::<CurrentTask<'static>>() <= 16);
            assert_eq!(size_of::<AdmittedRegularSource<'static>>(), 8);
            eprintln!("ABF26-ADMISSION control bytes: {CONTROL_BYTES}");
            for (name, bytes) in [
                ("ProcessRoot", size_of::<ProcessRoot>()),
                ("CandidateProcRoot", size_of::<CandidateProcRoot>()),
                ("AdmittedProcRoot", size_of::<AdmittedProcRoot>()),
                ("CandidateSelfLink", size_of::<CandidateSelfLink<'static>>()),
                ("AdmittedSelfLink", size_of::<AdmittedSelfLink<'static>>()),
                ("CurrentTask", size_of::<CurrentTask<'static>>()),
                ("CandidateFdDir", size_of::<CandidateFdDir>()),
                ("AdmittedFdDir", size_of::<AdmittedFdDir>()),
                (
                    "AdmittedRegularSource",
                    size_of::<AdmittedRegularSource<'static>>(),
                ),
                ("SelfLinkBytes", size_of::<SelfLinkBytes>()),
                ("ProcName", size_of::<ProcName>()),
            ] {
                eprintln!("ABF26-ADMISSION size {name}: {bytes}");
            }
        }

        // -- native chain on the real kernel (no harness, real procfs) -------

        /// Native: the whole chain on this thread. The task is the one the
        /// kernel names for this thread; the independent expected value is
        /// std's view of `/proc/thread-self` and this process id.
        #[test]
        fn native_chain_admits_this_threads_task() {
            let expected = std::fs::read_link("/proc/thread-self").unwrap();
            let process = ProcessRoot::open().unwrap();
            let candidate = process.open_proc().unwrap();
            drop(process);
            let proc = candidate.admit().unwrap();
            let dir = {
                let link = proc.open_thread_self().unwrap().admit().unwrap();
                let task = link.read_current_task().unwrap();
                drop(link);
                assert_eq!(task.pid, std::process::id());
                assert_eq!(
                    expected.to_str().unwrap(),
                    format!("{}/task/{}", task.pid, task.tid)
                );
                task.open_fd_dir().unwrap()
            };
            drop(proc);
            dir.admit().unwrap();
        }

        /// Native: a different thread names a different task through its own
        /// fresh chain (no cache, no shared token).
        #[test]
        fn each_thread_gets_its_own_task() {
            fn own_task() -> (u32, u32) {
                let process = ProcessRoot::open().unwrap();
                let proc = process.open_proc().unwrap().admit().unwrap();
                let link = proc.open_thread_self().unwrap().admit().unwrap();
                let task = link.read_current_task().unwrap();
                (task.pid, task.tid)
            }
            let here = own_task();
            let there = std::thread::spawn(own_task).join().unwrap();
            assert_eq!(here.0, there.0, "same process");
            assert_ne!(here.1, there.1, "different calling threads");
            assert_eq!(own_task(), here, "fresh chain, same thread");
        }

        /// Native: the process root is not procfs, so a candidate opened
        /// with the proc mount `open_how` beneath it would never be admitted
        /// as procfs; checked through the same verdict on the real `/`.
        #[test]
        fn native_process_root_is_not_admitted_as_procfs() {
            let process = ProcessRoot::open().unwrap();
            let verdict = directory_verdict(
                filesystem_magic(&process.fd),
                facts(process.fd.metadata()),
                None,
            );
            assert_eq!(verdict, Err(AdmissionRefusal::NotAdmitted));
        }

        /// Native: the generic `Fd` open paths cannot become an alternative
        /// readable leaf. With the relaxed leaf `open_how` (no `O_PATH`) they
        /// refuse with `EINVAL` before any system call, so the only readable
        /// procfs open is [`AdmittedFdDir::open_readable`].
        #[test]
        fn generic_open_paths_refuse_the_readable_leaf_how() {
            let process = ProcessRoot::open().unwrap();
            assert_eq!(
                super::super::openat2_beneath(&process.fd, c"proc", &PROC_FD_LEAF_HOW).err(),
                Some(super::super::EINVAL)
            );
            assert_eq!(
                super::super::openat2_process_root(&PROC_FD_LEAF_HOW).err(),
                Some(super::super::EINVAL)
            );
        }

        /// Injected (harness override of `fstatfs`, not kernel evidence): a
        /// candidate with a non-procfs magic is refused and closed.
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        #[test]
        fn injected_non_procfs_magic_refuses_each_candidate() {
            use super::super::fault;
            fault::reset();
            let process = ProcessRoot::open().unwrap();
            let candidate = process.open_proc().unwrap();
            fault::set_magic(Some(0xEF53));
            assert_eq!(
                candidate.admit().unwrap_err(),
                AdmissionRefusal::NotAdmitted
            );
            fault::set_magic(None);
            let proc = process.open_proc().unwrap().admit().unwrap();
            let link = proc.open_thread_self().unwrap();
            fault::set_magic(Some(0xEF53));
            assert_eq!(link.admit().unwrap_err(), AdmissionRefusal::NotAdmitted);
            fault::set_magic(None);
            let link = proc.open_thread_self().unwrap().admit().unwrap();
            let dir = link.read_current_task().unwrap().open_fd_dir().unwrap();
            fault::set_magic(Some(0xEF53));
            assert_eq!(dir.admit().unwrap_err(), AdmissionRefusal::NotAdmitted);
            fault::set_magic(None);
            drop(link);
            drop(proc);
            drop(process);
            let (opened, closed) = fault::counters();
            assert_eq!(opened, closed, "every candidate closed");
            fault::reset();
        }
    }
}

// ---------------------------------------------------------------------------
// M2A.2b: directory pins, the private directory self-open and getdents64
// ---------------------------------------------------------------------------

/// Directory pin acquisition and directory binding re-resolution beneath the
/// held root. Same flags and full mask as the root binding resolution.
const DIRECTORY_PIN_HOW: OpenHow = OpenHow {
    flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
    mode: 0,
    resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV,
};

/// The private self-open of a held directory pin: the literal `.`,
/// read-only directory, full mask. No `O_PATH`: this is the only directory
/// descriptor opened for reading. No write access, `O_CREAT`, `O_TRUNC` or
/// `O_TMPFILE`; mode 0.
const DIRECTORY_READ_HOW: OpenHow = OpenHow {
    flags: O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK,
    mode: 0,
    resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV,
};

/// Size of the fixed `getdents64` output buffer.
pub(crate) const DIRENT_BUFFER_BYTES: usize = 8192;

/// What a directory pin resolves beneath the held root: the root itself
/// (literal `.`) or a lexically validated relative path. Never a caller
/// C string, never an absolute path.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DirectoryLocator<'a> {
    HeldRoot,
    Beneath(&'a crate::path::RelativePath),
}

/// An `O_PATH|O_DIRECTORY` pin of a directory, only produced by
/// [`open_directory_pin`] (full mask beneath a held root). It is the only
/// argument [`open_directory_read`] accepts: a generic [`Fd`], a procfs
/// capability or a regular-file pin can never be self-opened for reading.
/// `Debug` is opaque.
pub(crate) struct DirectoryPinFd {
    fd: Fd,
}

impl std::fmt::Debug for DirectoryPinFd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DirectoryPinFd(..)")
    }
}

impl HandleMetadata for DirectoryPinFd {
    fn metadata(&self) -> io::Result<Metadata> {
        self.fd.metadata()
    }
}

impl DirectoryPinFd {
    pub(crate) fn filesystem_magic(&self) -> Result<i64, i32> {
        filesystem_magic(&self.fd)
    }
}

/// Sole owner of the one directory descriptor type opened for reading:
/// `O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC|O_NONBLOCK`, produced only by
/// [`open_directory_read`]. Its only I/O is [`read_directory_entries`]
/// (`getdents64`); no `File`, `Read`, `Seek` or raw descriptor leaves this
/// module. `Debug` is opaque.
pub(crate) struct DirectoryReadFd {
    file: File,
}

impl std::fmt::Debug for DirectoryReadFd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DirectoryReadFd(..)")
    }
}

impl HandleMetadata for DirectoryReadFd {
    fn metadata(&self) -> io::Result<Metadata> {
        self.file.metadata()
    }
}

impl Drop for DirectoryReadFd {
    fn drop(&mut self) {
        // `self.file` closes the descriptor right after this body runs.
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        fault::note_closed();
    }
}

/// One attempt: `O_PATH|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`, mode 0, full mask,
/// beneath the held `root` directory. `HeldRoot` resolves the literal `.`
/// (the held root object itself).
pub(crate) fn open_directory_pin(
    root: &Fd,
    locator: DirectoryLocator<'_>,
) -> Result<DirectoryPinFd, i32> {
    let path = match locator {
        DirectoryLocator::HeldRoot => c".",
        DirectoryLocator::Beneath(relative) => relative.as_c_str(),
    };
    openat2_once(root.raw(), path, &DIRECTORY_PIN_HOW).map(|fd| DirectoryPinFd { fd })
}

/// One attempt: the private self-open of `pin`, with the immutable literal
/// `.` and [`DIRECTORY_READ_HOW`] (full mask). `.` names the directory
/// object held by `pin` itself, never a pathname that could have been
/// rebound; `O_DIRECTORY` refuses any non-directory. The caller checks the
/// pin before (S0) and both handles after (S1/S2).
pub(crate) fn open_directory_read(pin: &DirectoryPinFd) -> Result<DirectoryReadFd, i32> {
    let owned = openat2_owned(pin.fd.raw(), c".", &DIRECTORY_READ_HOW)?;
    Ok(DirectoryReadFd {
        file: File::from(owned),
    })
}

/// Failure of one `getdents64` attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirentReadError {
    Errno(i32),
    /// The kernel reported more bytes than the buffer holds (or a count
    /// that is not a valid length). Nothing is sliced.
    Overlong,
}

/// One direct `getdents64(dir, buf, 8192)` system call. No libc wrapper, no
/// retry, no buffer growth. Returns the checked byte count (`0` is EOF).
pub(crate) fn read_directory_entries(
    dir: &DirectoryReadFd,
    buf: &mut [u8; DIRENT_BUFFER_BYTES],
) -> Result<usize, DirentReadError> {
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    if let Some(injected) = fault::before_getdents(buf) {
        return match injected {
            Ok(ret) => dirent_count(ret),
            Err(errno) => Err(DirentReadError::Errno(errno)),
        };
    }

    let buf_ptr: *mut u8 = buf.as_mut_ptr();
    // SAFETY:
    // - `syscall` is the glibc variadic wrapper declared above; every
    //   variadic argument is full register width (c_long, pointer, usize).
    // - `dir.file` is a live descriptor owned by `dir`, which is borrowed for
    //   the whole call, so it cannot be closed or reused meanwhile.
    // - `buf_ptr` points to an exclusively borrowed, initialized array of
    //   exactly `DIRENT_BUFFER_BYTES` bytes, and that exact count is passed:
    //   the kernel writes at most that many bytes. No struct is cast onto it.
    let ret: c_long = unsafe {
        syscall(
            SYS_GETDENTS64,
            c_long::from(dir.file.as_raw_fd()),
            buf_ptr,
            DIRENT_BUFFER_BYTES,
        )
    };
    if ret < 0 {
        // Read errno immediately; nothing in between can overwrite it.
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(DirentReadError::Errno(errno));
    }
    dirent_count(ret)
}

/// Checks the signed return before any conversion or slicing.
fn dirent_count(ret: c_long) -> Result<usize, DirentReadError> {
    let Ok(count) = usize::try_from(ret) else {
        return Err(DirentReadError::Overlong);
    };
    if count > DIRENT_BUFFER_BYTES {
        return Err(DirentReadError::Overlong);
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Harness-only descriptor probes (feature `fault-injection` AND
// `--cfg abf_secure_fs_harness`). Default builds contain none of them.
// ---------------------------------------------------------------------------

// SAFETY: glibc on x86_64-unknown-linux-gnu provides
// `int fcntl(int fd, int cmd, ...)` (fcntl.h). Only F_GETFL and F_GETFD,
// which take no third argument, are used.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
unsafe extern "C" {
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
}

/// `(F_GETFL status flags, F_GETFD descriptor flags)` of a live descriptor.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
fn descriptor_status(file: &File) -> Result<(i32, i32), i32> {
    const F_GETFD: c_int = 1;
    const F_GETFL: c_int = 3;
    // SAFETY: `file` is borrowed for the whole call, so its descriptor is
    // live. F_GETFL only reads descriptor state and takes no third argument.
    let status = unsafe { fcntl(file.as_raw_fd(), F_GETFL) };
    if status < 0 {
        return Err(io::Error::last_os_error().raw_os_error().unwrap_or(0));
    }
    // SAFETY: as above, for F_GETFD.
    let fd_flags = unsafe { fcntl(file.as_raw_fd(), F_GETFD) };
    if fd_flags < 0 {
        return Err(io::Error::last_os_error().raw_os_error().unwrap_or(0));
    }
    Ok((status, fd_flags))
}

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
impl Fd {
    pub(crate) fn descriptor_status(&self) -> Result<(i32, i32), i32> {
        descriptor_status(&self.file)
    }
}

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
impl DirectoryReadFd {
    pub(crate) fn descriptor_status(&self) -> Result<(i32, i32), i32> {
        descriptor_status(&self.file)
    }
}

#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
impl ReadableFd {
    pub(crate) fn descriptor_status(&self) -> Result<(i32, i32), i32> {
        descriptor_status(&self.file)
    }

    /// One positioned read (`pread64`) into `buf`; does not move the
    /// description's offset. The caller validates `offset` and bounds `buf`.
    pub(crate) fn read_at(&self, buf: &mut [u8], offset: u64) -> Result<usize, i32> {
        use std::os::unix::fs::FileExt;
        if let Some(errno) = fault::before_read() {
            return Err(errno);
        }
        self.file
            .read_at(buf, offset)
            .map_err(|error| error.raw_os_error().unwrap_or(0))
    }

    /// Current offset of this open file description (`lseek SEEK_CUR`).
    pub(crate) fn offset(&self) -> Result<u64, i32> {
        use std::io::Seek;
        (&self.file)
            .stream_position()
            .map_err(|error| error.raw_os_error().unwrap_or(0))
    }

    /// Moves this description's offset (`lseek SEEK_SET`). Reads nothing.
    pub(crate) fn seek_to(&self, to: u64) -> Result<u64, i32> {
        use std::io::{Seek, SeekFrom};
        (&self.file)
            .seek(SeekFrom::Start(to))
            .map_err(|error| error.raw_os_error().unwrap_or(0))
    }
}

/// Test-only harness instrumentation. It is compiled only with the
/// `fault-injection` feature **and** `--cfg abf_secure_fs_harness` (a build
/// with only the feature fails to compile), and is thread-local, so parallel
/// tests do not interfere. It can force an errno **instead of** calling
/// openat2, readlinkat or pread, replace the self-link payload, record the
/// exact `open_how` of every attempt, override the reported filesystem magic
/// or object identity (forged evidence, harness only), and count descriptor
/// opens/closes. It has no way to change flags or masks. M2A.2b adds a
/// **bounded** `getdents64` answer plan (at most [`MAX_DIRENT_PLAN`]
/// entries, each batch at most one buffer), an allocation-refusal flag and
/// the kernel status flags of the last directory read descriptor.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
pub(crate) mod fault {
    use super::{DIRENT_BUFFER_BYTES, OpenHow, SELF_LINK_CAPACITY};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::ffi::c_long;

    /// Largest accepted `getdents64` answer plan.
    pub(crate) const MAX_DIRENT_PLAN: usize = 128;

    /// A planned replacement for one `getdents64` call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum DirentInjection {
        Errno(i32),
        /// Copied into the buffer; the returned count is its length
        /// (at most one buffer).
        Batch(Vec<u8>),
        /// Returned as the raw signed count; the buffer is not written.
        Return(i64),
    }

    /// A planned replacement for one `readlinkat` call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum LinkInjection {
        Errno(i32),
        /// Copied into the fixed buffer, truncated to its capacity; the
        /// returned length is the copied length (kernel truncation model).
        Payload(Vec<u8>),
    }

    thread_local! {
        static PLAN: RefCell<VecDeque<Option<i32>>> = const { RefCell::new(VecDeque::new()) };
        static LOG: RefCell<Vec<(OpenHow, Option<i32>)>> = const { RefCell::new(Vec::new()) };
        static MAGIC: Cell<Option<i64>> = const { Cell::new(None) };
        static IDENTITY: RefCell<VecDeque<Option<(u64, u64)>>> = const { RefCell::new(VecDeque::new()) };
        static OPENED: Cell<u64> = const { Cell::new(0) };
        static CLOSED: Cell<u64> = const { Cell::new(0) };
        static LINK_PLAN: RefCell<VecDeque<Option<LinkInjection>>> = const { RefCell::new(VecDeque::new()) };
        static LINK_CALLS: Cell<u64> = const { Cell::new(0) };
        static READ_PLAN: RefCell<VecDeque<Option<i32>>> = const { RefCell::new(VecDeque::new()) };
        static READ_CALLS: Cell<u64> = const { Cell::new(0) };
        static DIRENT_PLAN: RefCell<VecDeque<Option<DirentInjection>>> = const { RefCell::new(VecDeque::new()) };
        static DIRENT_CALLS: Cell<u64> = const { Cell::new(0) };
        static REFUSE_ALLOCATION: Cell<bool> = const { Cell::new(false) };
        static DIRECTORY_READ_STATUS: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
        static PEAK: Cell<u64> = const { Cell::new(0) };
    }

    /// Counts every getdents64 attempt and answers it from the plan, if a
    /// planned entry is `Some`.
    pub(crate) fn before_getdents(
        buf: &mut [u8; DIRENT_BUFFER_BYTES],
    ) -> Option<Result<c_long, i32>> {
        DIRENT_CALLS.with(|c| c.set(c.get() + 1));
        match DIRENT_PLAN.with(|p| p.borrow_mut().pop_front()).flatten()? {
            DirentInjection::Errno(errno) => Some(Err(errno)),
            DirentInjection::Batch(bytes) => {
                // Bounded at plan time; re-checked here.
                let len = bytes.len().min(DIRENT_BUFFER_BYTES);
                buf[..len].copy_from_slice(&bytes[..len]);
                Some(Ok(len as c_long))
            }
            DirentInjection::Return(ret) => Some(Ok(ret)),
        }
    }

    /// Replaces the plan. Panics (harness misuse) above [`MAX_DIRENT_PLAN`]
    /// entries or for a batch larger than one buffer.
    pub(crate) fn set_dirent_plan(plan: &[Option<DirentInjection>]) {
        assert!(plan.len() <= MAX_DIRENT_PLAN, "getdents plan too long");
        assert!(
            plan.iter().all(|entry| match entry {
                Some(DirentInjection::Batch(bytes)) => bytes.len() <= DIRENT_BUFFER_BYTES,
                _ => true,
            }),
            "getdents batch larger than the buffer"
        );
        DIRENT_PLAN.with(|p| *p.borrow_mut() = plan.iter().cloned().collect());
    }

    pub(crate) fn dirent_calls() -> u64 {
        DIRENT_CALLS.with(Cell::get)
    }

    pub(crate) fn remaining_dirent_plan() -> usize {
        DIRENT_PLAN.with(|p| p.borrow().len())
    }

    /// Makes the next enumeration storage reservation fail.
    pub(crate) fn set_refuse_allocation(refuse: bool) {
        REFUSE_ALLOCATION.with(|r| r.set(refuse));
    }

    /// Consumes the allocation-refusal flag.
    pub(crate) fn take_refuse_allocation() -> bool {
        REFUSE_ALLOCATION.with(|r| r.replace(false))
    }

    pub(crate) fn note_directory_read_status(status: Option<(i32, i32)>) {
        DIRECTORY_READ_STATUS.with(|s| s.set(status));
    }

    pub(crate) fn directory_read_status() -> Option<(i32, i32)> {
        DIRECTORY_READ_STATUS.with(Cell::get)
    }

    /// Counts every readlinkat attempt and answers it from the plan, if a
    /// planned entry is `Some`.
    pub(crate) fn before_readlink(
        buf: &mut [u8; SELF_LINK_CAPACITY],
    ) -> Option<Result<c_long, i32>> {
        LINK_CALLS.with(|c| c.set(c.get() + 1));
        match LINK_PLAN.with(|p| p.borrow_mut().pop_front()).flatten()? {
            LinkInjection::Errno(errno) => Some(Err(errno)),
            LinkInjection::Payload(payload) => {
                let len = payload.len().min(SELF_LINK_CAPACITY);
                buf[..len].copy_from_slice(&payload[..len]);
                Some(Ok(len as c_long))
            }
        }
    }

    pub(crate) fn set_link_plan(plan: &[Option<LinkInjection>]) {
        LINK_PLAN.with(|p| *p.borrow_mut() = plan.iter().cloned().collect());
    }

    pub(crate) fn link_calls() -> u64 {
        LINK_CALLS.with(Cell::get)
    }

    /// Counts every synthetic pread attempt and answers it from the plan.
    pub(crate) fn before_read() -> Option<i32> {
        READ_CALLS.with(|c| c.set(c.get() + 1));
        READ_PLAN.with(|p| p.borrow_mut().pop_front()).flatten()
    }

    pub(crate) fn set_read_plan(plan: &[Option<i32>]) {
        READ_PLAN.with(|p| *p.borrow_mut() = plan.iter().copied().collect());
    }

    pub(crate) fn read_calls() -> u64 {
        READ_CALLS.with(Cell::get)
    }

    pub(crate) fn before_openat2(how: &OpenHow) -> Option<i32> {
        let injected = PLAN.with(|p| p.borrow_mut().pop_front()).flatten();
        LOG.with(|l| l.borrow_mut().push((*how, injected)));
        injected
    }

    pub(crate) fn injected_magic() -> Option<i64> {
        MAGIC.with(Cell::get)
    }

    /// Next planned `(device, inode)` override for a handle inspection, if
    /// any. `None` entries (and an empty plan) keep the real values.
    pub(crate) fn next_identity_override() -> Option<(u64, u64)> {
        IDENTITY.with(|p| p.borrow_mut().pop_front()).flatten()
    }

    pub(crate) fn set_identity_plan(plan: &[Option<(u64, u64)>]) {
        IDENTITY.with(|p| *p.borrow_mut() = plan.iter().copied().collect());
    }

    pub(crate) fn note_opened() {
        OPENED.with(|c| c.set(c.get() + 1));
        let live = OPENED
            .with(Cell::get)
            .saturating_sub(CLOSED.with(Cell::get));
        PEAK.with(|p| p.set(p.get().max(live)));
    }

    /// Highest number of descriptors wrapped on this thread and not yet
    /// closed, since the last reset (or [`reset_peak`]).
    pub(crate) fn peak() -> u64 {
        PEAK.with(Cell::get)
    }

    /// Restarts the high-water mark from the currently live count.
    pub(crate) fn reset_peak() {
        let live = OPENED
            .with(Cell::get)
            .saturating_sub(CLOSED.with(Cell::get));
        PEAK.with(|p| p.set(live));
    }

    pub(crate) fn note_closed() {
        CLOSED.with(|c| c.set(c.get() + 1));
    }

    pub(crate) fn set_plan(plan: &[Option<i32>]) {
        PLAN.with(|p| *p.borrow_mut() = plan.iter().copied().collect());
    }

    pub(crate) fn remaining_plan() -> usize {
        PLAN.with(|p| p.borrow().len())
    }

    pub(crate) fn take_log() -> Vec<(OpenHow, Option<i32>)> {
        LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    pub(crate) fn set_magic(magic: Option<i64>) {
        MAGIC.with(|m| m.set(magic));
    }

    pub(crate) fn counters() -> (u64, u64) {
        (OPENED.with(Cell::get), CLOSED.with(Cell::get))
    }

    pub(crate) fn reset() {
        set_plan(&[]);
        take_log();
        set_magic(None);
        set_identity_plan(&[]);
        OPENED.with(|c| c.set(0));
        CLOSED.with(|c| c.set(0));
        set_link_plan(&[]);
        LINK_CALLS.with(|c| c.set(0));
        set_read_plan(&[]);
        READ_CALLS.with(|c| c.set(0));
        set_dirent_plan(&[]);
        DIRENT_CALLS.with(|c| c.set(0));
        set_refuse_allocation(false);
        note_directory_read_status(None);
        PEAK.with(|p| p.set(0));
    }
}
