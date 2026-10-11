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
//! - `EXT{2,3,4}_SUPER_MAGIC = 0xEF53`: `linux/magic.h`;
//! - errno values: `asm-generic/errno-base.h` / `errno.h`.
//!
//! Compile-time assertions pin the struct sizes, alignments and field
//! offsets. Only three operations are unsafe: the `openat2` system call, the
//! transfer of its returned descriptor into an owning [`Fd`], and `fstatfs`.
//!
//! Ownership invariant: a raw descriptor returned by the kernel is wrapped
//! into an [`Fd`] immediately, exactly once, before any other code runs.
//! [`Fd`] is the sole owner and closes it on drop (including on every error
//! path, by ordinary Rust drop order). No function in this crate returns,
//! accepts or stores a raw descriptor number outside this module's borrowed
//! calls.

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
    assert!(size_of::<c_long>() == size_of::<usize>());
};

// ---------------------------------------------------------------------------
// ABI constants
// ---------------------------------------------------------------------------

/// `__NR_openat2` on x86_64 LP64 (`asm/unistd_64.h:354`). Not the x32 number
/// (`__X32_SYSCALL_BIT + 437` in `asm/unistd_x32.h`).
const SYS_OPENAT2: c_long = 437;
/// `AT_FDCWD` (`linux/fcntl.h`). Only used with the constant absolute "/".
const AT_FDCWD: c_int = -100;

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
// Owned descriptor
// ---------------------------------------------------------------------------

/// Sole owner of one kernel descriptor obtained from `openat2`.
///
/// Internally stored as a `File` only to reach std's handle-based metadata
/// (`statx(fd, "", AT_EMPTY_PATH)` / `fstat`). The descriptor is always an
/// `O_PATH` descriptor: any read/write on it fails with `EBADF`, and no such
/// call exists in this crate. The `File` is never exposed. `Debug` is
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

impl Drop for Fd {
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

fn openat2_once(dirfd: c_int, path: &CStr, how: &OpenHow) -> Result<Fd, i32> {
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
    // - The flags never include O_CREAT/O_TRUNC/write access. With O_PATH the
    //   kernel does not open the object for I/O, so this call cannot block on
    //   a FIFO or activate a device.
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
    // moves into `OwnedFd` exactly once, here, and from then on only `Fd`
    // closes it.
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    fault::note_opened();
    Ok(Fd {
        file: File::from(owned),
    })
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

/// Test-only harness instrumentation. It is compiled only with the
/// `fault-injection` feature **and** `--cfg abf_secure_fs_harness` (a build
/// with only the feature fails to compile), and is thread-local, so parallel
/// tests do not interfere. It can force an errno **instead of** calling
/// openat2, record the exact `open_how` of every attempt, override the
/// reported filesystem magic or object identity (forged evidence, harness
/// only), and count descriptor opens/closes. It has no way to change flags or
/// masks.
#[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
pub(crate) mod fault {
    use super::OpenHow;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    thread_local! {
        static PLAN: RefCell<VecDeque<Option<i32>>> = const { RefCell::new(VecDeque::new()) };
        static LOG: RefCell<Vec<(OpenHow, Option<i32>)>> = const { RefCell::new(Vec::new()) };
        static MAGIC: Cell<Option<i64>> = const { Cell::new(None) };
        static IDENTITY: RefCell<VecDeque<Option<(u64, u64)>>> = const { RefCell::new(VecDeque::new()) };
        static OPENED: Cell<u64> = const { Cell::new(0) };
        static CLOSED: Cell<u64> = const { Cell::new(0) };
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
    }
}
