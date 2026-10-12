//! Pure, bounded lexical validation of roots and relative paths. No I/O, no
//! environment expansion, no normalization: a path is either accepted
//! exactly as spelled or rejected with a [`PathError`].
//!
//! Lexical acceptance is **not** confinement. Confinement comes from the
//! kernel (`openat2` resolve flags) at acquisition time. These rules only
//! remove spellings that are never legitimate, bound the work, and keep
//! cross-OS spellings out before any OS access.

use std::ffi::{CStr, CString};
use std::fmt;

use crate::error::PathError;

/// Maximum byte length of an authorized root, including the leading `/`.
pub const MAX_ROOT_BYTES: usize = 1024;
/// Maximum number of components of an authorized root.
pub const MAX_ROOT_COMPONENTS: usize = 32;
/// Maximum byte length of a relative candidate path.
pub const MAX_RELATIVE_BYTES: usize = 1024;
/// Maximum number of components of a relative candidate path.
pub const MAX_RELATIVE_COMPONENTS: usize = 32;
/// Maximum byte length of one component (Linux `NAME_MAX`).
pub const MAX_COMPONENT_BYTES: usize = 255;

/// Kernel pseudo-filesystems that are never authorizable roots. They are on
/// other mounts and would be refused by `RESOLVE_NO_XDEV` anyway; rejecting
/// them lexically gives a clearer code and avoids touching them.
const SYSTEM_NAMESPACES: [&str; 3] = ["proc", "sys", "dev"];

/// Pure lexical check of an authorized root spelling. It is what
/// [`crate::SecureRoot::authorize`] applies before any OS access.
pub fn check_root_path(root: &str) -> Result<(), PathError> {
    parse_root(root).map(|_| ())
}

/// A lexically validated relative candidate path: `/`-separated, no
/// empty/`.`/`..` components, no control characters, no `\` or `:`, bounded
/// length and depth.
///
/// `Debug` is opaque because the path can be sensitive.
#[derive(Clone)]
pub struct RelativePath {
    c_path: CString,
}

impl RelativePath {
    /// Validates `relative`. Never panics on malformed input.
    pub fn new(relative: &str) -> Result<Self, PathError> {
        check_relative(relative)?;
        let c_path = CString::new(relative).map_err(|_| PathError::InvalidCharacter)?;
        Ok(Self { c_path })
    }

    pub(crate) fn as_c_str(&self) -> &CStr {
        &self.c_path
    }

    /// The validated spelling, without the terminating NUL.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.c_path.as_bytes()
    }

    /// Heap bytes owned by this path: the exact boxed C string, including
    /// its NUL (`CString` stores a `Box<[u8]>` of exactly that length).
    pub(crate) fn heap_bytes(&self) -> usize {
        self.c_path.as_bytes_with_nul().len()
    }
}

/// The acceptance rules of [`RelativePath::new`], in the same order,
/// without allocating. Every rule is unchanged. (`CString::new` in `new`
/// can only fail on NUL, which the control-character rule already refuses.)
pub(crate) fn check_relative(relative: &str) -> Result<(), PathError> {
    if relative.is_empty() {
        return Err(PathError::Empty);
    }
    if relative.len() > MAX_RELATIVE_BYTES {
        return Err(PathError::TooLong);
    }
    if relative.contains(['\\', ':']) {
        return Err(PathError::CrossOsSeparator);
    }
    if relative.starts_with('/') {
        return Err(PathError::Absolute);
    }
    check_components(relative, MAX_RELATIVE_COMPONENTS)
}

/// Capacity of the fixed join scratch: the longest relative path plus a NUL.
pub(crate) const JOIN_SCRATCH_BYTES: usize = MAX_RELATIVE_BYTES + 1;

/// A fixed, zero-initialized buffer holding `parent/` (or nothing, for the
/// root directory) followed by one child name, to check the joined
/// root-relative spelling with [`check_relative`] **without allocating**.
/// It never escapes the crate and has an opaque `Debug`.
pub(crate) struct JoinScratch {
    buf: [u8; JOIN_SCRATCH_BYTES],
    prefix: usize,
}

impl fmt::Debug for JoinScratch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JoinScratch(..)")
    }
}

impl JoinScratch {
    /// `parent` is a validated directory locator (`None` for the root). A
    /// validated parent has at most 1024 bytes, so `parent/` always fits the
    /// 1025-byte buffer; when it leaves no room, every join is `TooLong`.
    pub(crate) fn new(parent: Option<&RelativePath>) -> Self {
        let mut scratch = Self {
            buf: [0; JOIN_SCRATCH_BYTES],
            prefix: 0,
        };
        if let Some(parent) = parent {
            let bytes = parent.as_bytes();
            let len = bytes.len().min(MAX_RELATIVE_BYTES);
            scratch.buf[..len].copy_from_slice(&bytes[..len]);
            scratch.buf[len] = b'/';
            scratch.prefix = len + 1;
        }
        scratch
    }

    /// Joins `name` after the prefix and returns the joined spelling if it
    /// passes [`check_relative`]. Bounded: at most 1024 bytes are copied and
    /// checked. The scratch is reused; nothing is retained.
    pub(crate) fn join(&mut self, name: &[u8]) -> Result<&str, PathError> {
        let end = self
            .prefix
            .checked_add(name.len())
            .ok_or(PathError::TooLong)?;
        if end > MAX_RELATIVE_BYTES {
            return Err(PathError::TooLong);
        }
        self.buf[self.prefix..end].copy_from_slice(name);
        let joined =
            std::str::from_utf8(&self.buf[..end]).map_err(|_| PathError::InvalidCharacter)?;
        check_relative(joined)?;
        Ok(joined)
    }
}

impl fmt::Debug for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RelativePath(..)")
    }
}

/// Validated root, stored **without** its leading `/`, ready to resolve
/// beneath the held process-root anchor.
pub(crate) struct RootPath {
    beneath_slash: CString,
}

impl RootPath {
    pub(crate) fn as_c_str(&self) -> &CStr {
        &self.beneath_slash
    }
}

impl fmt::Debug for RootPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RootPath(..)")
    }
}

pub(crate) fn parse_root(root: &str) -> Result<RootPath, PathError> {
    if root.is_empty() {
        return Err(PathError::Empty);
    }
    if root.len() > MAX_ROOT_BYTES {
        return Err(PathError::TooLong);
    }
    if root.contains(['\\', ':']) {
        return Err(PathError::CrossOsSeparator);
    }
    let rest = root.strip_prefix('/').ok_or(PathError::NotAbsolute)?;
    if rest.is_empty() {
        return Err(PathError::TooBroad);
    }
    check_components(rest, MAX_ROOT_COMPONENTS)?;

    let mut components = rest.split('/');
    let first = components.next().unwrap_or_default();
    if SYSTEM_NAMESPACES.contains(&first) {
        return Err(PathError::SystemNamespace);
    }
    if first == "mnt"
        && let Some(drive) = components.next()
        && drive.len() == 1
        && drive.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(PathError::CrossOsPath);
    }

    let beneath_slash = CString::new(rest).map_err(|_| PathError::InvalidCharacter)?;
    Ok(RootPath { beneath_slash })
}

/// Checks every component. The total length has already been bounded, and
/// the component count is checked while iterating, so the work is bounded
/// by the input length.
fn check_components(path: &str, max_components: usize) -> Result<(), PathError> {
    for (index, component) in path.split('/').enumerate() {
        if index >= max_components {
            return Err(PathError::TooDeep);
        }
        match component {
            "" => return Err(PathError::EmptyComponent),
            "." => return Err(PathError::DotComponent),
            ".." => return Err(PathError::ParentComponent),
            _ => {}
        }
        if component.len() > MAX_COMPONENT_BYTES {
            return Err(PathError::ComponentTooLong);
        }
        if component.chars().any(char::is_control) {
            return Err(PathError::InvalidCharacter);
        }
    }
    Ok(())
}
