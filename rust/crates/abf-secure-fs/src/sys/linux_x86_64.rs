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
//! - errno values: `asm-generic/errno-base.h` / `errno.h`
//!   (`EWOULDBLOCK == EAGAIN == 11`).
//!
//! Compile-time assertions pin the struct sizes, alignments and field
//! offsets. The unsafe operations are: the `openat2` system call, the
//! transfer of its returned descriptor into an `OwnedFd`, `fstatfs`, the
//! direct `readlinkat` system call (M2A.2a) and, in the test harness only,
//! `fcntl(F_GETFL/F_GETFD)`.
//!
//! Ownership invariant: a raw descriptor returned by the kernel is wrapped
//! into an `OwnedFd` immediately, exactly once, before any other code runs,
//! and then moved into exactly one typed owner: [`Fd`] (always an `O_PATH`
//! descriptor), one of the private procfs capabilities built on it, or
//! [`ReadableFd`] (the only descriptor type opened for reading). The owner
//! closes it on drop (including on every error path and during unwinding,
//! by ordinary Rust drop order). No function in this crate returns, accepts
//! or stores a raw descriptor number outside this module's borrowed calls.
//!
//! M2A.2a procfs exception (README, "Readable promotion"): the procfs opens
//! below take **no** caller-supplied path, flags or descriptor number. Their
//! names are constants or decimal numbers formatted here, and their
//! `open_how` values are private constants. The relaxed terminal resolution
//! (`resolve = 0`) exists only in [`open_readable_leaf`], whose directory
//! argument can only come from the constant procfs chain.

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
    //   readable procfs leaf uses O_PATH, which does not open the object for
    //   I/O. The leaf's O_RDONLY|O_NONBLOCK open is reached only for the
    //   still-owned, previously type-checked regular source (see
    //   `open_readable_leaf`).
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
// M2A.2a: private procfs capabilities and the readable leaf
// ---------------------------------------------------------------------------

/// `proc` beneath the held process root "/". `RESOLVE_NO_XDEV` is omitted
/// **only here**, because the trusted proc mount is necessarily another
/// mount. BENEATH, NO_SYMLINKS and NO_MAGICLINKS are kept.
const PROC_MOUNT_HOW: OpenHow = OpenHow {
    flags: O_PATH | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC,
    mode: 0,
    resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
};

/// `thread-self` beneath the admitted proc root, full mask, not followed:
/// with `O_PATH|O_NOFOLLOW`, openat2 returns the final symlink itself
/// (documented exception), so only its metadata is ever used.
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
/// required: every confinement flag refuses the magic-link jump. This is the
/// **only** relaxed resolution in the crate. No O_PATH, O_NOFOLLOW, O_CREAT,
/// O_TRUNC, O_TMPFILE or write access; mode 0.
const PROC_FD_LEAF_HOW: OpenHow = OpenHow {
    flags: O_RDONLY | O_CLOEXEC | O_NONBLOCK,
    mode: 0,
    resolve: 0,
};

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

/// Opaque `Debug`, handle metadata and filesystem magic for a private
/// procfs capability. Its inner [`Fd`] is never handed out.
macro_rules! proc_capability {
    ($name:ident) => {
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }

        impl HandleMetadata for $name {
            fn metadata(&self) -> io::Result<Metadata> {
                self.fd.metadata()
            }
        }

        impl $name {
            pub(crate) fn filesystem_magic(&self) -> Result<i64, i32> {
                filesystem_magic(&self.fd)
            }
        }
    };
}

/// Candidate procfs root: only produced by [`open_proc_root`] from the
/// constant name `proc`. The promotion protocol admits it by handle.
pub(crate) struct ProcRoot {
    fd: Fd,
}

/// The not-followed `thread-self` link of a [`ProcRoot`]: only produced by
/// [`open_thread_self`].
pub(crate) struct ProcSelfLink {
    fd: Fd,
}

/// A task `fd` directory: only produced by [`open_task_fd_dir`] from a
/// [`ProcRoot`] and two numbers. The only directory type accepted by
/// [`open_readable_leaf`].
pub(crate) struct ProcFdDir {
    fd: Fd,
}

proc_capability!(ProcRoot);
proc_capability!(ProcSelfLink);
proc_capability!(ProcFdDir);

/// One attempt: the constant name `proc` beneath the held process root.
pub(crate) fn open_proc_root(process_root: &Fd) -> Result<ProcRoot, i32> {
    openat2_once(process_root.raw(), c"proc", &PROC_MOUNT_HOW).map(|fd| ProcRoot { fd })
}

/// One attempt: the constant name `thread-self` beneath `proc_root`, not
/// followed.
pub(crate) fn open_thread_self(proc_root: &ProcRoot) -> Result<ProcSelfLink, i32> {
    openat2_once(proc_root.fd.raw(), c"thread-self", &PROC_SELF_LINK_HOW)
        .map(|fd| ProcSelfLink { fd })
}

/// One attempt: `<pid>/task/<tid>/fd` beneath `proc_root`, formatted here
/// from two numbers in `1..=MAX_TASK_ID` (`EINVAL` otherwise).
pub(crate) fn open_task_fd_dir(proc_root: &ProcRoot, pid: u32, tid: u32) -> Result<ProcFdDir, i32> {
    if !(1..=MAX_TASK_ID).contains(&pid) || !(1..=MAX_TASK_ID).contains(&tid) {
        return Err(EINVAL);
    }
    let name = task_fd_dir_name(pid, tid).ok_or(EINVAL)?;
    let path = name.as_c_str().ok_or(EINVAL)?;
    openat2_once(proc_root.fd.raw(), path, &PROC_FD_DIR_HOW).map(|fd| ProcFdDir { fd })
}

/// Size of the fixed self-link buffer. A result that fills it is treated as
/// possibly truncated and refused.
pub(crate) const SELF_LINK_CAPACITY: usize = 32;

/// The bytes the kernel returned for a self link, `len < SELF_LINK_CAPACITY`.
/// `Debug` never prints them.
pub(crate) struct SelfLinkBytes {
    buf: [u8; SELF_LINK_CAPACITY],
    len: usize,
}

impl std::fmt::Debug for SelfLinkBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelfLinkBytes(..)")
    }
}

impl SelfLinkBytes {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkReadError {
    Errno(i32),
    /// The result filled the buffer (or exceeded it): possibly truncated.
    Truncated,
}

/// One direct `readlinkat(link, "", buf, 32)` system call on the owned,
/// not-followed self link. No libc wrapper, no fallback, no retry.
pub(crate) fn read_self_link(link: &ProcSelfLink) -> Result<SelfLinkBytes, LinkReadError> {
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
    // - `link.fd` is a live descriptor borrowed for the whole call.
    // - `path_ptr` is the static empty NUL-terminated string: the call acts
    //   on the O_PATH|O_NOFOLLOW link descriptor itself (documented
    //   readlinkat empty-path form).
    // - `buf_ptr` points to an exclusively borrowed, initialized array of
    //   exactly `SELF_LINK_CAPACITY` bytes; the kernel writes at most that
    //   many bytes and adds no NUL.
    let ret: c_long = unsafe {
        syscall(
            SYS_READLINKAT,
            c_long::from(link.fd.raw()),
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

/// One attempt: opens the generated decimal leaf of the still-owned `source`
/// descriptor beneath the task `fd` directory, with the fixed
/// `O_RDONLY|O_CLOEXEC|O_NONBLOCK`, mode 0, resolve 0.
///
/// The leaf name is formatted here from `source`'s own descriptor number
/// (at most ten digits, no `/`); it never comes from a caller, a string or a
/// `readlink` of the descriptor. `source` is borrowed for the whole call, so
/// that number stays bound to the same open description. The kernel follows
/// the fd entry's magic link to the object held by `source`, not to any
/// pathname.
pub(crate) fn open_readable_leaf(dir: &ProcFdDir, source: &Fd) -> Result<ReadableFd, i32> {
    let number = u32::try_from(source.raw()).map_err(|_| EINVAL)?;
    let mut name = ProcName::new();
    name.decimal(number).ok_or(EINVAL)?;
    let leaf = name.as_c_str().ok_or(EINVAL)?;
    let owned = openat2_owned(dir.fd.raw(), leaf, &PROC_FD_LEAF_HOW)?;
    Ok(ReadableFd {
        file: File::from(owned),
    })
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
/// opens/closes. It has no way to change flags or masks.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
pub(crate) mod fault {
    use super::{OpenHow, SELF_LINK_CAPACITY};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::ffi::c_long;

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
    }
}
