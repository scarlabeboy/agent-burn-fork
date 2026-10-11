# abf-secure-fs (ABF M2A.1)

Secure **Linux file-object pinning** under an authorized root, **metadata
only**. It is a standalone research foundation for later hardened collection
work.

Status: implementation candidate from gate M2A.1 (conversations ABF11 and
ABF11B, corrected in ABF11C after the ABF12 independent audit). It requires
another independent read-only security review before any use.

**Production collection: BLOCKED.** This crate reads no file contents,
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
| `RelativePath::new(&str)`, `check_root_path(&str)` | Pure, bounded lexical validation. |
| `BackendCapabilities::compiled()` | Honest compile-time description. Never attests a WSL distribution. |
| `AccessError`, `PathError`, `ObjectKind` | Code-only errors. No paths, names or OS message strings. |

Nothing in the public API exposes a raw descriptor, a `File`, a `Read`
implementation, an absolute reopen path, or a way to supply an identity as
proof. `SecureRoot` and `PinnedRegularFile` are not `Clone`, and their
`Debug` output never contains paths.

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
fallback to a temp directory:

- `ABF_SECURE_FS_FIXTURE_DIR`: an existing, test-owned directory on the
  same ext-family mount as `/`. Each test creates and removes its own
  `<dir>/<test>-<pid>/` case directory there.
- `ABF_SECURE_FS_FOREIGN_MOUNT_DIR`: an existing directory on a
  **different** mount (for example tmpfs). It is only resolved, never
  written, to check the `RESOLVE_NO_XDEV` refusal.

### Test harness (`fault-injection` + `abf_secure_fs_harness`)

The `fault-injection` feature adds a thread-local harness. It provides errno
injection that replaces real `openat2` calls, an unbounded `open_how` log,
**filesystem-magic and identity overrides**, descriptor counters and
deterministic schedule hooks. The overrides **forge kernel evidence**: they
can make a refused root, a replaced root binding or a different object look
admitted. It cannot change open flags or resolve masks.

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

Readable promotion of a pinned object, directory enumeration, bounded
reading and coherence, the Windows backend, M1 integration, parsers,
persistence and UI. No production readiness is claimed.
