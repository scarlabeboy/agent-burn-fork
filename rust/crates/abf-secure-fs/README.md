# abf-secure-fs (ABF M2A.1 + M2A.2a)

Secure **Linux file-object pinning** under an authorized root (metadata
only), and **experimental readable promotion** of a pinned file. It is a
standalone research foundation for later hardened collection work.

Status:

- M2A.1 pinning: implementation from conversations ABF11/ABF11B, corrected
  in ABF11C after the ABF12 audit, reviewed in ABF13 (PASS_WITH_FINDINGS).
- M2A.2a readable promotion: implementation candidate from conversation
  ABF15, following the ABF14 design. It requires an independent read-only
  security review before any use.

**Production collection: BLOCKED.** The default API reads no file contents
(only the two-key test harness can read bounded synthetic bytes),
enumerates no directories, has no Windows backend and is not integrated with
the M1 boundary (`abf-collection-boundary`), any parser or any application.

## What it provides

| Item | Purpose |
|---|---|
| `SecureRoot::authorize(&str)` | Acquires and **holds** the directory object currently bound at an explicit absolute root path. |
| `SecureRoot::check_binding()` | Checks that the root path still binds the held object. A replacement yields `RootBindingChanged` and is never adopted. |
| `SecureRoot::pin_regular(&RelativePath)` | Pins one single-link regular file beneath the held root. It performs its own binding checks; callers need not call `check_binding`. |
| `SecureRoot::object_id()` | Handle-derived identity of the held root. |
| `PinnedRegularFile::evidence()` | `FileEvidence` captured at pin time: `ObjectId` (device, inode), link count (always 1), size. |
| `PinnedRegularFile::reinspect()` | Fresh evidence from the **held handle**, never from a path. Fails if the object now has ≠ 1 link. |
| `PinnedRegularFile::promote(self)` | M2A.2a. **Consumes** the pin and returns a `ReadableRegularFile` for the **same** object, or a coded refusal (the pin and every acquired handle are then closed). |
| `ReadableRegularFile::evidence()` | Handle-derived evidence sampled from the new readable handle (S1), before any read. |
| `ReadableRegularFile::reinspect()` | Fresh evidence from **both** held handles; both must still be a regular file with the pinned identity and one link. |
| `RelativePath::new(&str)`, `check_root_path(&str)` | Pure, bounded lexical validation. |
| `BackendCapabilities::compiled()` | Honest compile-time description. Never attests a WSL distribution. |
| `AccessError`, `PathError`, `ObjectKind` | Code-only errors. No paths, names or OS message strings. `AccessError::PromotionWouldBlock` is new in M2A.2a. |

Nothing in the public API exposes a raw descriptor, a `File`, a
`Read`/`Write`/`Seek` implementation, `AsFd`/`AsRawFd`, `Deref`, a procfs
path, an absolute reopen path, open flags, or a way to supply an identity,
descriptor number or procfs anchor as proof. `SecureRoot`,
`PinnedRegularFile` and `ReadableRegularFile` are not `Clone`, have private
fields and no public constructor, and their `Debug` output never contains
paths or contents. The default API has **no** method that reads bytes.

## Supported platform

- `x86_64-unknown-linux-gnu` only, with Linux `openat2` (≥ 5.6) and the
  root on the **same ext-family mount as `/`** (fs magic `0xEF53`; this
  cannot distinguish ext2/3/4).
- The backend is selected by `target_os = "linux"`, `target_arch =
  "x86_64"`, `target_env = "gnu"`, `target_pointer_width = "64"` and
  `not(target_abi = "x32")`. GNU x32 (`x86_64-unknown-linux-gnux32`) also
  reports linux/x86_64/gnu, but has 32-bit `long` and pointers and a
  different syscall numbering, so it is **excluded**. The ABI module also
  asserts the LP64 widths at compile time.
- Any other target (including GNU x32), or a build with
  `--cfg abf_secure_fs_force_unsupported`, compiles a fail-closed backend.
  Every authorization returns `SecurePrimitiveUnavailable` after lexical
  validation.
- Unsupported kernel, seccomp denial, missing flags, a root on another mount
  (separate `/home`, tmpfs `/tmp`, bind mounts, drvfs/9p), or a non-ext
  filesystem all fail closed. **There is no fallback** to `openat`,
  `canonicalize` or pathname opens.
- Readable promotion additionally requires the process's `/proc` to be a
  procfs mount (magic `0x9FA0`) that exposes `thread-self` as
  `<pid>/task/<tid>` and the per-task `fd` directory. Anything else (missing
  or non-procfs `/proc`, `hidepid`/LSM/seccomp denial, another spelling)
  fails closed with `SecurePrimitiveUnavailable` or `Inaccessible`.
- Running on a kernel named "WSL2" does **not** establish Ubuntu WSL2
  distribution identity. Nothing here detects or starts WSL.

## Protocol

| Step | Call | Flags | Resolve mask |
|---|---|---|---|
| Anchor | `openat2(AT_FDCWD, "/")` | `O_PATH\|O_DIRECTORY\|O_NOFOLLOW\|O_CLOEXEC` | `NO_SYMLINKS\|NO_MAGICLINKS\|NO_XDEV` (BENEATH is inapplicable to an absolute path) |
| Root | `openat2(anchor, root-without-leading-/)` | `O_PATH\|O_DIRECTORY\|O_NOFOLLOW\|O_CLOEXEC` | `BENEATH\|NO_SYMLINKS\|NO_MAGICLINKS\|NO_XDEV` |
| Binding check | same as Root, compared with the held root's `(dev, ino)` | same | same |
| Pin acquisition | `openat2(held root, full relative path)` | `O_PATH\|O_NOFOLLOW\|O_CLOEXEC` | `BENEATH\|NO_SYMLINKS\|NO_MAGICLINKS\|NO_XDEV` |
| Pin verification | second, independent full-relative resolution from the held root | same | same |

Admission: the anchor and root must both report fs magic `0xEF53` via
`fstatfs` on their handles. The root must be a directory on the anchor's
device.

Pin, in order:

1. binding check;
2. acquisition, then handle inspection: regular file (the `O_PATH|O_NOFOLLOW`
   final-symlink handle is explicitly refused), the root's device, non-zero
   inode, `nlink == 1`;
3. verification resolution: the verifier handle must pass the same
   candidate policy as step 2 (so an observed `nlink != 1` is refused there
   too) and reach the same `(dev, ino)`;
4. final binding check;
5. final handle inspection with the same candidate policy.

The link count is enforced on three samples: acquisition, verification and
final inspection.

Every descriptor is owned (RAII) and closed on every path. `EAGAIN`/`EINTR`
are retried at most twice (three attempts) with identical flags. `ENOSYS`,
`EPERM`, `EINVAL` and `E2BIG` map to `SecurePrimitiveUnavailable`; `ELOOP`
and `EXDEV` map to `ConfinementRefused`.

`O_PATH` never opens the object for I/O, so FIFOs, sockets and devices are
type-inspected without blocking or activation.

## Security contract (object capability, approved decision U2)

Four distinct properties. Only A and B are provided.

**A. Resolution confinement (provided by the kernel).** Every resolution
beneath a held directory (root acquisition, binding checks, pin acquisition
and pin verification) is one `openat2` call with the mandatory mask
`RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS|RESOLVE_NO_MAGICLINKS|RESOLVE_NO_XDEV`.
The masks are `const` and are never weakened, retried without a flag, or
replaced by a fallback. The crate relies on the documented semantics of
these flags. The installed `openat2(2)` manual says `RESOLVE_BENEATH` does
not let the resolution succeed if any component is not a descendant of
`dirfd`. It also documents `EXDEV` when an escape from the root is detected
during resolution, and `EAGAIN` when the kernel cannot ensure that a `..`
component did not escape. This is a **trusted-kernel assumption**. The crate
does not re-prove it, kernel source was not reviewed, and finite stress tests
are supplemental evidence, not a proof. Each flag is kept explicitly even
where the manual says another flag currently implies it.

**B. Identity of two confined acquisitions (provided by this crate).** The
pin acquisition and the verification resolution are independent, each under
A. The verifier must pass the same candidate policy and have the same
`(dev, ino)` as the held handle. The held handle stays open, so its inode
cannot be recycled meanwhile. This detects a held object that is no longer
bound at the path when the verification runs (moved or replaced file or
intermediate directory). The double acquisition is **not by itself a proof
of confinement**. Two equal identities would equally describe an object that
was never beneath the root. Excluding such an object is A's role. If any
kernel ever let both constrained resolutions return a never-inside object,
that would be a violation of the U2 authorization boundary, not an instance
of the accepted limitation C.

**C. Continuous path membership: NOT guaranteed.** The authorization pins
the **original directory object**. A later replacement of the root path
(real directory, symlink, mount, removal) never redirects the held root, and
observed binding changes are refused. A root or intermediate rename that is
restored between two checks is not guaranteed to be detected. A verified
object may be moved out of the root after verification, before or after the
pin returns. Both cases are demonstrated by tests. Pin evidence means
"resolved beneath the held root under A, and verified bound there under B,
at those moments".

**D. Content origin: NOT established.** An object renamed or copied **into**
the root is inside the root. Nothing here says where its bytes came from,
and no content is read.

Further limits:

- `nlink == 1` is a sampled policy, enforced at acquisition, verification
  and final inspection, and by `reinspect`. A hard link that is added and
  removed between samples is not observed. A link can appear right after the
  last sample. `nlink == 1` does not authenticate origin.
- Out of scope: same-user malware, injected code, mount-namespace
  administrators, a compromised kernel or glibc, and anyone who deliberately
  builds with the test harness cfg (below).

## Readable promotion (M2A.2a, experimental)

`PinnedRegularFile::promote(self) -> Result<ReadableRegularFile, AccessError>`
converts an existing, authorized `O_PATH` pin into a capability that owns
**both** the original `O_PATH` handle and a new `O_RDONLY` descriptor of the
**same** object. It uses a private procfs reopen of the still-owned
descriptor. It **never** resolves, `readlink`s or reopens a pathname of the
file, and it accepts no caller path, descriptor number, procfs anchor or
flags.

### Protocol

All steps are on the calling thread, synchronously, with a fresh anchor per
call (no cache, no cross-thread handoff).

| Step | Call | Flags | Resolve mask | Admission |
|---|---|---|---|---|
| S0 | `statx` on the original O_PATH handle | — | — | regular, pinned `(dev, ino)`, `nlink == 1` |
| Anchor | `openat2(AT_FDCWD, "/")` (unchanged constant) | `O_PATH\|O_DIRECTORY\|O_NOFOLLOW\|O_CLOEXEC` | `NO_SYMLINKS\|NO_MAGICLINKS\|NO_XDEV` (0x07) | — |
| Proc | `openat2(anchor, "proc")` | same | `BENEATH\|NO_SYMLINKS\|NO_MAGICLINKS` (0x0E): **only** place `NO_XDEV` is omitted, because procfs is another mount | `fstatfs` magic `0x9FA0`, directory; anchor dropped |
| Self link | `openat2(proc, "thread-self")` | `O_PATH\|O_NOFOLLOW\|O_CLOEXEC` (not followed) | full 0x0F | procfs magic, symlink, proc device |
| Self read | direct `readlinkat(link, "", buf, 32)` (syscall 267) | — | — | strict `<pid>/task/<tid>`; link dropped |
| Fd dir | `openat2(proc, "<pid>/task/<tid>/fd")`, reformatted from the two parsed numbers | `O_PATH\|O_DIRECTORY\|O_NOFOLLOW\|O_CLOEXEC` | full 0x0F | procfs magic, directory, proc device; proc dropped |
| Readable | `openat2(fd dir, "<n>")`, `<n>` = decimal number of the still-owned original descriptor, generated inside the ABI module | `O_RDONLY\|O_CLOEXEC\|O_NONBLOCK` (0o2004000), mode 0 | **0** (the single terminal magic-link exception) | fd dir dropped |
| S1 | `statx` on the **new** handle, before any read | — | — | regular, pinned `(dev, ino)`, `nlink == 1` |
| S2 | `statx` on the original handle | — | — | as S0 |

The self-link parser accepts exactly two canonical decimal numbers in
`1..=2147483647` (1 to 10 digits, no leading zero or sign) joined by the
literal `/task/`. An empty, full-buffer (possibly truncated), negative or
otherwise malformed result fails closed; there is no alternative format and
the payload is never stored or printed. The mandatory source-path masks
(0x0F beneath held directories, 0x07 for `/`) are unchanged; the procfs
`open_how` values are private constants of the ABI module and are never
available to source-path resolution.

Ownership: every successful system call result is wrapped into exactly one
owner immediately. `O_PATH` handles (`Fd`, and the private procfs
capabilities built on it) and the readable handle (`ReadableFd`) are
distinct private types; an `Fd` can never be opened without `O_PATH`. At
most three descriptors coexist during a promotion (including the original);
success retains exactly two. Every failure and unwind closes everything
acquired, including the consumed pin.

Retries and errors:

- procfs opens: at most three attempts on `EAGAIN`/`EINTR`;
- `readlinkat`: at most three attempts on `EINTR`;
- terminal open: at most three attempts on `EINTR`; **`EAGAIN`/`EWOULDBLOCK`
  is terminal** (`PromotionWouldBlock`, no retry, because each attempt could
  notify a lease holder again);
- missing/non-procfs `/proc`, a missing proc entry (`ENOENT`/`ENOTDIR`),
  an unrecognized self link, `ENOSYS`/`EPERM`/`EINVAL`/`E2BIG`:
  `SecurePrimitiveUnavailable`; `EACCES`: `Inaccessible`;
  `ELOOP`/`EXDEV`: `ConfinementRefused`; anything else: `UnexpectedOs(n)`;
- identity mismatch or zero links: `ObjectChanged`; more links:
  `MultipleLinks`; zero identity: `IdentityUnavailable`. No refusal is
  retried and no flag is ever removed.

### What it establishes, and what it does not

- **Same object:** the kernel follows the per-task fd entry of the
  still-owned descriptor to the object that descriptor holds (documented
  magic-link semantics, trusted kernel), and S1 checks the new handle's
  identity against the pin **before** any read. A rename, move out of the
  root, or replacement of the old name by a regular file, symlink, FIFO or
  socket cannot redirect it (tested natively).
- **Samples, not continuity:** `nlink == 1` and identity are checked at S0,
  S1 and S2 (and by `reinspect`). A link added and removed between samples,
  or added/an unlink made after S2, is not observed by the promotion.
  Statistics fields are not one atomic snapshot.
- **Not a snapshot:** size is evidence only; bytes can change during and
  after promotion. Nothing here establishes continuous namespace membership,
  immutable bytes, content origin or event identity. The pin stores no root
  capability; promotion exercises the existing object authority.
- **Side effects of an ordinary read open** are not zero: a read can update
  access times (mount policy), and opens/reads/closes can generate
  notifications. An `O_NONBLOCK` open can fail with `EWOULDBLOCK` **and**
  still start a write-lease break. Closing the readable descriptor releases
  POSIX record locks the same process holds on that file, so a collector must
  not share lock ownership with a writer. Promotion acquires no lock or
  lease, opens nothing for writing, creates/truncates nothing and does not
  restore timestamps. The coexistence target is **ordinary unlocked writers
  in separate processes** (tested natively), not every writer.
- **Trust boundary:** a trusted kernel, process root and procfs mount (magic
  is classification, not authentication); normal descriptor-table sharing
  between the process's threads for the pin's lifetime; no `fork`, `unshare`
  of the descriptor table, credential or namespace change during a call; no
  unsafe code that closes or rebinds owned descriptors. Hostile mount
  administrators, same-user malware and deliberate harness builds are out of
  scope.

## Building and testing (isolated, offline)

Standalone `[workspace]`, no dependencies, no build script. Only these
commands were used:

```sh
cargo fmt --check
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked
# Explicit test harness (feature AND cfg, isolated synthetic build only).
# rustdoc (doc-tests) reads RUSTDOCFLAGS, not RUSTFLAGS, so set both:
export RUSTFLAGS="--cfg abf_secure_fs_harness" RUSTDOCFLAGS="--cfg abf_secure_fs_harness"
cargo clippy --offline --locked --all-targets --features fault-injection -- -D warnings
cargo test --offline --locked --features fault-injection
# Fail-closed backend on this host (fresh shell):
export RUSTFLAGS="--cfg abf_secure_fs_force_unsupported" RUSTDOCFLAGS="--cfg abf_secure_fs_force_unsupported"
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --offline --locked
```

`cargo build/clippy/test --all-features` **without** `--cfg
abf_secure_fs_harness` is expected to fail with a `compile_error!`. This is
the intended fail-closed guard (see below), not a build defect.

Native tests require two **explicit** environment variables. There is no
fallback to a temp directory. The promotion tests also re-run single tests of
their own test binary as child processes (exact descriptor-closure checks and
a separate-process writer); they inherit the same variables:

- `ABF_SECURE_FS_FIXTURE_DIR`: an existing, test-owned directory on the
  same ext-family mount as `/`. Each test creates and removes its own
  `<dir>/<test>-<pid>/` case directory there.
- `ABF_SECURE_FS_FOREIGN_MOUNT_DIR`: an existing directory on a
  **different** mount (for example tmpfs). It is only resolved, never
  written, to check the `RESOLVE_NO_XDEV` refusal.

### Test harness (`fault-injection` + `abf_secure_fs_harness`)

The `fault-injection` feature adds a thread-local harness. It provides errno
injection that replaces real `openat2`, `readlinkat` or synthetic `pread`
calls, a procfs self-link payload override, an unbounded `open_how` log,
**filesystem-magic and identity overrides**, descriptor counters,
deterministic schedule hooks (including the promotion steps), descriptor
flag/offset probes of a promoted file, and the crate's **only** byte-reading
entry point, `ReadableRegularFile::harness_read_at` (at most 4096 bytes per
call, checked offset and length, fixed staging buffer, both handles checked
before and after, bytes returned only after the post-checks). The overrides
**forge kernel evidence**: they can make a refused root, a replaced root
binding, a different object or a non-procfs anchor look admitted. It cannot
change open flags or resolve masks.

The harness is therefore compiler-isolated with two keys:

- The feature alone does not compile. `compile_error!` fires unless
  `--cfg abf_secure_fs_harness` is also set. This covers `--all-features`,
  debug and release builds, and feature unification from any other crate in
  the dependency graph.
- The cfg is declared in `[lints.rust] unexpected_cfgs` and is used only by
  the isolated synthetic test commands above.
- Default builds contain no harness module, state, log, counter or hook.

A developer who deliberately sets both the feature and the harness cfg is
outside the trust boundary. That configuration exists to forge evidence in
tests.

## Not implemented (later gates, separately authorized)

A production read API, bounded reading and content coherence, directory
enumeration, the Windows backend, M1 integration, parsers, persistence and
UI. Readable promotion is an experimental M2A.2a research capability with no
production read path. No production readiness is claimed.
