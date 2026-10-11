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
