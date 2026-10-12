//! Code-only errors. No variant carries a path, a name, an OS message string
//! or any caller-supplied text. `Display` prints fixed strings only.

use std::fmt;

/// Lexical rejection of a root or relative path, before any OS access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PathError {
    Empty,
    /// More bytes than the documented maximum.
    TooLong,
    /// More components than the documented maximum.
    TooDeep,
    /// A single component exceeds `MAX_COMPONENT_BYTES`.
    ComponentTooLong,
    /// A root must start with `/`.
    NotAbsolute,
    /// A relative path must not start with `/`.
    Absolute,
    /// The root `/` itself is never authorizable.
    TooBroad,
    /// `//`, a leading or trailing `/` on a relative path, or a trailing `/`.
    EmptyComponent,
    /// A `.` component.
    DotComponent,
    /// A `..` component.
    ParentComponent,
    /// NUL or another control character.
    InvalidCharacter,
    /// `\` or `:`: Windows drive, UNC, device or stream spellings.
    CrossOsSeparator,
    /// `/mnt/<drive letter>`: WSL default Windows-drive mount.
    CrossOsPath,
    /// `/proc`, `/sys` or `/dev`: kernel pseudo-filesystems.
    SystemNamespace,
}

impl PathError {
    const fn code(self) -> &'static str {
        match self {
            Self::Empty => "path: empty",
            Self::TooLong => "path: too long",
            Self::TooDeep => "path: too many components",
            Self::ComponentTooLong => "path: component too long",
            Self::NotAbsolute => "path: root is not absolute",
            Self::Absolute => "path: relative path is absolute",
            Self::TooBroad => "path: root is too broad",
            Self::EmptyComponent => "path: empty component",
            Self::DotComponent => "path: '.' component",
            Self::ParentComponent => "path: '..' component",
            Self::InvalidCharacter => "path: invalid character",
            Self::CrossOsSeparator => "path: cross-OS separator",
            Self::CrossOsPath => "path: cross-OS mount path",
            Self::SystemNamespace => "path: kernel pseudo-filesystem",
        }
    }
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for PathError {}

/// Kind of a refused non-regular object, from handle metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObjectKind {
    Directory,
    Symlink,
    Fifo,
    Socket,
    CharDevice,
    BlockDevice,
    Unknown,
}

/// Coded failure of an authorization, binding check, pin or readable
/// promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AccessError {
    /// The path was rejected lexically; no OS access happened.
    Path(PathError),
    /// `openat2` or a required flag/mask is unavailable or denied
    /// (`ENOSYS`, `EPERM`, `EINVAL`, `E2BIG`), or the platform has no
    /// backend. For readable promotion, also: procfs missing or not
    /// recognized (magic, type, device), a missing proc entry, or a
    /// malformed, unrecognized or possibly truncated `thread-self` link.
    /// Never answered with a weaker fallback.
    SecurePrimitiveUnavailable,
    /// The anchor or root is not on the admitted ext-family filesystem, or
    /// its device does not match.
    UnsupportedFilesystem,
    /// The authorized root path no longer binds the originally authorized
    /// directory object (replaced, removed, turned into a link or mount).
    RootBindingChanged,
    /// Resolution refused by confinement: a symlink (`ELOOP`), a mount
    /// crossing (`EXDEV`) or an object on another device.
    ConfinementRefused,
    /// `ENOENT`.
    NotFound,
    /// A component that must be a directory is not one (`ENOTDIR`, or the
    /// acquired root is not a directory).
    NotADirectory,
    /// The pinned candidate is not a regular file.
    NotRegularFile(ObjectKind),
    /// Link count is not exactly 1: the object may also be reachable
    /// elsewhere.
    MultipleLinks,
    /// The object at the path, or the held object, changed between the
    /// acquisition and its verification (different identity or zero links).
    ObjectChanged,
    /// The kernel did not provide a concrete object identity.
    IdentityUnavailable,
    /// `EACCES`.
    Inaccessible,
    /// `EAGAIN`/`EINTR` persisted through the bounded retries.
    RetryExhausted,
    /// The read-only open of a promotion was refused with
    /// `EAGAIN`/`EWOULDBLOCK` (for example a conflicting write lease). It
    /// is never retried: each attempt could notify the lease holder again.
    PromotionWouldBlock,
    /// Any other errno, as a number only.
    UnexpectedOs(i32),
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => fmt::Display::fmt(path, f),
            Self::SecurePrimitiveUnavailable => f.write_str("secure primitive unavailable"),
            Self::UnsupportedFilesystem => f.write_str("unsupported filesystem"),
            Self::RootBindingChanged => f.write_str("root binding changed"),
            Self::ConfinementRefused => f.write_str("confinement refused"),
            Self::NotFound => f.write_str("not found"),
            Self::NotADirectory => f.write_str("not a directory"),
            Self::NotRegularFile(kind) => write!(f, "not a regular file ({kind:?})"),
            Self::MultipleLinks => f.write_str("multiple links"),
            Self::ObjectChanged => f.write_str("object changed"),
            Self::IdentityUnavailable => f.write_str("identity unavailable"),
            Self::Inaccessible => f.write_str("inaccessible"),
            Self::RetryExhausted => f.write_str("retry exhausted"),
            Self::PromotionWouldBlock => f.write_str("promotion would block"),
            Self::UnexpectedOs(errno) => write!(f, "unexpected os error {errno}"),
        }
    }
}

impl std::error::Error for AccessError {}

impl From<PathError> for AccessError {
    fn from(error: PathError) -> Self {
        Self::Path(error)
    }
}

/// Coded refusal of a directory enumeration (M2A.2b) or of the
/// authorization of one discovered child. Like [`AccessError`], no variant
/// carries a name, a path, an OS message or caller text.
///
/// The two operations fail differently:
///
/// - a failed `PinnedDirectory::enumerate` has **consumed** the directory
///   pin, returns **no** listing and discards every staged candidate;
/// - a failed `DirectoryListing::pin_regular` returns **no** child pin, only
///   **borrows** the listing and leaves it available; a later request
///   authorizes afresh (and can still refuse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EnumerationError {
    /// Cancellation was observed before the first `getdents64` attempt.
    /// If it was already requested on entry, nothing new was allocated or
    /// opened (the consumed pin is still dropped). If it was first observed
    /// later, the listing storage may already have been reserved and the
    /// private read handle opened; both are released. Cancellation observed
    /// after an attempt is not this error: it yields a `Partial` listing
    /// that may have zero candidates.
    Cancelled,
    /// A budget value is zero, above its hard maximum, or inconsistent
    /// (more candidates than inspected records).
    InvalidBudget,
    /// Reserving the bounded listing storage failed, or its actual capacity
    /// would exceed the retained-output ceiling or changed unexpectedly.
    AllocationRefused,
    /// The kernel returned directory data that cannot be framed safely (or a
    /// count larger than the buffer). Nothing, not even a valid prefix, is
    /// kept.
    MalformedDirectoryData,
    /// The directory's original root-relative locator no longer binds the
    /// held directory object (moved, renamed, replaced, removed, turned into
    /// a link or mount). Never adopted.
    DirectoryBindingChanged,
    /// The ordinal does not name a candidate of this listing. The listing
    /// stays available.
    OrdinalOutOfRange,
    /// A root binding, acquisition, handle-policy or system-call refusal.
    Access(AccessError),
}

impl fmt::Display for EnumerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("enumeration cancelled"),
            Self::InvalidBudget => f.write_str("invalid enumeration budget"),
            Self::AllocationRefused => f.write_str("enumeration storage refused"),
            Self::MalformedDirectoryData => f.write_str("malformed directory data"),
            Self::DirectoryBindingChanged => f.write_str("directory binding changed"),
            Self::OrdinalOutOfRange => f.write_str("ordinal out of range"),
            Self::Access(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for EnumerationError {}

impl From<AccessError> for EnumerationError {
    fn from(error: AccessError) -> Self {
        Self::Access(error)
    }
}

/// ABF28: coded refusal of the research read transaction
/// (`ReadableRegularFile::research_read_prefix`). Present **only** in the
/// deliberate two-key research build (feature `fault-injection` **and**
/// `--cfg abf_secure_fs_harness`) on the supported target.
///
/// Every refusal has consumed the readable capability and the permit,
/// closed both held descriptors and disposed the whole private stage: **no
/// byte** of the file is returned, not even a prefix read before the
/// refusal. Like the other errors, no variant carries a name, path, OS
/// message, content or caller text. A refusal never means "empty" or "zero
/// usage".
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResearchReadError {
    /// A budget value is zero or above its hard ceiling, or an internal
    /// accounting bound would be exceeded. Nothing was sampled or read.
    InvalidBudget,
    /// Cancellation was observed (at entry, before an attempt, after an
    /// attempt or before release). Earlier progress is discarded.
    Cancelled,
    /// The single fixed stage could not be allocated. No read happened.
    AllocationRefused,
    /// `pread64` or `fstat` returned a value outside the documented kernel
    /// contract (negative other than `-1`, `-1` without errno, a count above
    /// the request). Never clamped.
    KernelContractViolation,
    /// A fatal `pread64` errno (classified as for opens; `EIO` is
    /// `UnexpectedOs(5)`). It wins over failing post-attempt samples.
    ReadFailed(AccessError),
    /// `pread64` failed with `EAGAIN`/`EWOULDBLOCK`. Terminal: never
    /// retried, waited for or reopened.
    WouldBlock,
    /// `EINTR` exceeded the consecutive or total retry grants.
    RetryExhausted,
    /// A held-owner sample was refused: `fstat` errno, not a regular file,
    /// no concrete identity, a different identity, zero or several links.
    Metadata(AccessError),
    /// A sample carried an impossible size or timestamp.
    InvalidMetadata,
    /// The size/mtime/ctime change detector differed between the owners or
    /// from the entry baseline, the read progressed beyond the baseline
    /// size, or end of file was observed before it.
    ContentChanged,
}

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
impl fmt::Display for ResearchReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBudget => f.write_str("research read: invalid budget"),
            Self::Cancelled => f.write_str("research read: cancelled"),
            Self::AllocationRefused => f.write_str("research read: stage allocation refused"),
            Self::KernelContractViolation => {
                f.write_str("research read: kernel contract violation")
            }
            Self::ReadFailed(error) => write!(f, "research read: read failed ({error})"),
            Self::WouldBlock => f.write_str("research read: would block"),
            Self::RetryExhausted => f.write_str("research read: retry exhausted"),
            Self::Metadata(error) => write!(f, "research read: metadata refused ({error})"),
            Self::InvalidMetadata => f.write_str("research read: invalid metadata"),
            Self::ContentChanged => f.write_str("research read: content changed"),
        }
    }
}

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
impl std::error::Error for ResearchReadError {}
