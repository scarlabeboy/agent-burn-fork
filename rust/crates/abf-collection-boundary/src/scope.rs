//! Explicit, caller-authorized collection scope and conservative path
//! resolution.
//!
//! The boundary never chooses a directory by itself: the caller passes an
//! absolute, canonical root. Each file is checked to resolve to a regular,
//! singly linked file below that root, and every component is checked with
//! `lstat`, so a symbolic link present at check time is refused.
//!
//! These checks are path-based and **not race-free**. A concurrent swap of
//! the root, an intermediate directory or the final file between a check and
//! the following use can make the boundary read outside data and emit it.
//! Production reuse remains blocked pending the prerequisites in the README.
//! Contract A V2 does not require absolute containment against concurrent swaps.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::identity::{EnvironmentIdentity, EnvironmentKind, SourceInstance};
use crate::observation::{CollectError, FileObjectRef, SkipReason, SkippedEntry};
use crate::synthetic::FIXTURE_EXTENSION;

const MAX_ROOT_LEN: usize = 4096;
const MAX_RELATIVE_LEN: usize = 1024;

/// Path syntax family of the host the boundary runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostFamily {
    Windows,
    Unix,
}

impl HostFamily {
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Whether the declared environment matches the host the code runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostMatch {
    Native,
    /// Declared environment differs from the host. Only meaningful for
    /// synthetic fixtures; recorded on every observation.
    Simulated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPolicy {
    /// The declared environment must be the host's own environment.
    RequireNative,
    /// Allow a synthetic simulation of the other environment on this host.
    AllowSimulated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RootRejection {
    Empty,
    TooLong,
    NonUtf8,
    NotAbsolute,
    TooBroad,
    EmptyComponent,
    RelativeComponent,
    InvalidCharacter,
    ReservedName,
    /// Path into the other OS's filesystem (`/mnt/<drive>`, `\\wsl$`, ...).
    CrossOsPath,
    UncOrDevicePath,
    Symlink,
    NotADirectory,
    /// The path is not identical to its canonical form (e.g. a symlinked
    /// ancestor).
    NotCanonical,
    /// The path now names a different file object than the one authorized.
    Replaced,
    Io(io::ErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeError {
    Root(RootRejection),
    EnvironmentHostMismatch,
    /// A [`CollectionLimits`] value is outside the accepted range.
    InvalidLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathRejection {
    /// The authorized root failed revalidation; nothing from it is emitted.
    RootChanged(RootRejection),
    Empty,
    TooLong,
    Absolute,
    Traversal,
    EmptyComponent,
    InvalidCharacter,
    TooDeep,
    NotFixtureExtension,
    SymlinkComponent,
    NotADirectoryComponent,
    Symlink,
    NotRegularFile,
    /// More than one hard link: the same object may also live outside the
    /// root, so it is refused.
    HardLinked,
    Io(io::ErrorKind),
}

/// Lexical root check for a given host family. Pure: performs no I/O, so the
/// Windows rules are testable on any host.
pub fn check_root_text(root: &str, host: HostFamily) -> Result<(), RootRejection> {
    if root.is_empty() {
        return Err(RootRejection::Empty);
    }
    if root.len() > MAX_ROOT_LEN {
        return Err(RootRejection::TooLong);
    }
    match host {
        HostFamily::Unix => check_unix_root(root),
        HostFamily::Windows => check_windows_root(root),
    }
}

fn check_root_component(component: &str) -> Result<(), RootRejection> {
    match component {
        "" => Err(RootRejection::EmptyComponent),
        "." | ".." => Err(RootRejection::RelativeComponent),
        _ if component.chars().any(char::is_control) || component.contains('\\') => {
            Err(RootRejection::InvalidCharacter)
        }
        _ => Ok(()),
    }
}

fn check_unix_root(root: &str) -> Result<(), RootRejection> {
    let rest = root.strip_prefix('/').ok_or(RootRejection::NotAbsolute)?;
    if rest.is_empty() {
        return Err(RootRejection::TooBroad);
    }
    let components: Vec<&str> = rest.split('/').collect();
    for component in &components {
        check_root_component(component)?;
    }
    // WSL default drvfs automount of Windows drives. A custom automount root
    // is not detected (documented limitation).
    if let ["mnt", drive, ..] = components.as_slice()
        && drive.len() == 1
        && drive.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(RootRejection::CrossOsPath);
    }
    Ok(())
}

fn check_windows_root(root: &str) -> Result<(), RootRejection> {
    let bytes = root.as_bytes();
    let is_separator = |b: u8| b == b'\\' || b == b'/';
    if bytes.len() >= 2 && is_separator(bytes[0]) && is_separator(bytes[1]) {
        let host = root[2..]
            .split(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        return Err(if host == "wsl$" || host == "wsl.localhost" {
            RootRejection::CrossOsPath
        } else {
            RootRejection::UncOrDevicePath
        });
    }
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && is_separator(bytes[2]);
    if !drive_absolute {
        return Err(RootRejection::NotAbsolute);
    }
    let rest = &root[3..];
    if rest.is_empty() {
        return Err(RootRejection::TooBroad);
    }
    for component in rest.split(['\\', '/']) {
        match component {
            "" => return Err(RootRejection::EmptyComponent),
            "." | ".." => return Err(RootRejection::RelativeComponent),
            _ => {}
        }
        if component.chars().any(char::is_control)
            || component.contains([':', '*', '?', '"', '<', '>', '|'])
            || component.ends_with(['.', ' '])
        {
            return Err(RootRejection::InvalidCharacter);
        }
        if is_reserved_windows_name(component) {
            return Err(RootRejection::ReservedName);
        }
    }
    Ok(())
}

fn is_reserved_windows_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        _ => {
            let bytes = stem.as_bytes();
            bytes.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && (b'1'..=b'9').contains(&bytes[3])
        }
    }
}

/// A caller-authorized collection root that passed lexical and filesystem
/// checks on this host. The directory's file-object identity is recorded at
/// authorization and rechecked by every collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedRoot {
    path: PathBuf,
    object: FileObjectRef,
}

impl AuthorizedRoot {
    /// Accepts only an absolute, canonical, existing directory that is not a
    /// symbolic link and has no symlinked ancestor. No expansion of `~` or
    /// environment variables is performed.
    pub fn new(path: &Path) -> Result<Self, RootRejection> {
        let text = path.to_str().ok_or(RootRejection::NonUtf8)?;
        check_root_text(text, HostFamily::current())?;
        let object = check_root_directory(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            object,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path-based recheck that the root is still the authorized, canonical,
    /// non-symlink directory object. This narrows but does not close the
    /// window for a concurrent replacement: it is not a race-free
    /// containment guarantee. On hosts where [`FileObjectRef`] is
    /// `Unavailable`, replacement by another real directory is not detected.
    pub(crate) fn revalidate(&self) -> Result<(), RootRejection> {
        if check_root_directory(&self.path)? == self.object {
            Ok(())
        } else {
            Err(RootRejection::Replaced)
        }
    }
}

fn check_root_directory(path: &Path) -> Result<FileObjectRef, RootRejection> {
    let metadata = fs::symlink_metadata(path).map_err(|e| RootRejection::Io(e.kind()))?;
    if is_redirection(&metadata) {
        return Err(RootRejection::Symlink);
    }
    if !metadata.is_dir() {
        return Err(RootRejection::NotADirectory);
    }
    let canonical = fs::canonicalize(path).map_err(|e| RootRejection::Io(e.kind()))?;
    if !is_same_canonical(path, &canonical) {
        return Err(RootRejection::NotCanonical);
    }
    Ok(file_object(&metadata))
}

fn is_same_canonical(given: &Path, canonical: &Path) -> bool {
    if cfg!(windows) {
        // `canonicalize` returns a verbatim `\\?\C:\...` path on Windows.
        canonical
            .to_str()
            .and_then(|c| c.strip_prefix(r"\\?\"))
            .is_some_and(|c| Some(c) == given.to_str())
    } else {
        canonical == given
    }
}

/// Upper bound accepted for [`CollectionLimits::max_file_bytes`].
pub const MAX_FILE_BYTES_CEILING: u64 = 256 * 1024 * 1024;
/// Upper bound accepted for [`CollectionLimits::max_entries`].
pub const MAX_ENTRIES_CEILING: usize = 1_000_000;
/// Upper bound accepted for [`CollectionLimits::max_depth`].
pub const MAX_DEPTH_CEILING: usize = 64;

/// Input bounds for one collection. Every value must be in `1..=<ceiling>`;
/// [`CollectionScope::with_limits`] rejects anything else.
///
/// These bound directory iteration and each individual input file. They are
/// not a byte budget for retained output: each record carries cloned
/// provenance and every file report is retained. Synthetic collection also
/// caps retained record outcomes at 4096 per call. Memory can still exceed
/// `max_file_bytes`. Use small, trusted synthetic fixtures only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionLimits {
    /// Per-file input size limit; not a batch memory budget.
    pub max_file_bytes: u64,
    /// Total directory entries (files, directories, anything) the walk may
    /// read. Exceeding it fails the collection. Bounds directory iteration
    /// only.
    pub max_entries: usize,
    /// Maximum number of components in a relative path.
    pub max_depth: usize,
}

impl CollectionLimits {
    fn is_valid(self) -> bool {
        (1..=MAX_FILE_BYTES_CEILING).contains(&self.max_file_bytes)
            && (1..=MAX_ENTRIES_CEILING).contains(&self.max_entries)
            && (1..=MAX_DEPTH_CEILING).contains(&self.max_depth)
    }
}

impl Default for CollectionLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024,
            max_entries: 10_000,
            max_depth: 16,
        }
    }
}

/// Everything one collection is allowed to touch, fixed up front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionScope {
    environment: EnvironmentIdentity,
    source: SourceInstance,
    root: AuthorizedRoot,
    host_match: HostMatch,
    limits: CollectionLimits,
}

impl CollectionScope {
    pub fn new(
        environment: EnvironmentIdentity,
        source: SourceInstance,
        root: AuthorizedRoot,
        policy: HostPolicy,
    ) -> Result<Self, ScopeError> {
        let host_match = if is_native_host(environment.kind()) {
            HostMatch::Native
        } else if policy == HostPolicy::AllowSimulated {
            HostMatch::Simulated
        } else {
            return Err(ScopeError::EnvironmentHostMismatch);
        };
        Ok(Self {
            environment,
            source,
            root,
            host_match,
            limits: CollectionLimits::default(),
        })
    }

    pub fn with_limits(mut self, limits: CollectionLimits) -> Result<Self, ScopeError> {
        if !limits.is_valid() {
            return Err(ScopeError::InvalidLimits);
        }
        self.limits = limits;
        Ok(self)
    }

    pub fn environment(&self) -> &EnvironmentIdentity {
        &self.environment
    }

    pub fn source(&self) -> &SourceInstance {
        &self.source
    }

    pub fn root(&self) -> &AuthorizedRoot {
        &self.root
    }

    pub fn host_match(&self) -> HostMatch {
        self.host_match
    }

    pub fn limits(&self) -> CollectionLimits {
        self.limits
    }

    /// Resolves `relative` (a `/`-separated path) to a regular file below the
    /// root, checking every component with `lstat`, so a symlink present at
    /// check time is refused. Path-based: a component swapped after its check
    /// is not detected.
    pub(crate) fn resolve_regular_file(
        &self,
        relative: &str,
    ) -> Result<ResolvedFile, PathRejection> {
        let components = validate_relative(relative, self.limits.max_depth)?;
        let (last, directories) = components
            .split_last()
            .expect("validated relative path is non-empty");
        let mut path = self.root.path.clone();
        for directory in directories {
            path.push(directory);
            let metadata = fs::symlink_metadata(&path).map_err(|e| PathRejection::Io(e.kind()))?;
            if is_redirection(&metadata) {
                return Err(PathRejection::SymlinkComponent);
            }
            if !metadata.is_dir() {
                return Err(PathRejection::NotADirectoryComponent);
            }
        }
        path.push(last);
        let metadata = fs::symlink_metadata(&path).map_err(|e| PathRejection::Io(e.kind()))?;
        if is_redirection(&metadata) {
            return Err(PathRejection::Symlink);
        }
        if !metadata.is_file() {
            return Err(PathRejection::NotRegularFile);
        }
        if hard_link_count(&metadata) > 1 {
            return Err(PathRejection::HardLinked);
        }
        Ok(ResolvedFile {
            path,
            relative: relative.to_owned(),
            object: file_object(&metadata),
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|e| PathRejection::Io(e.kind()))?,
        })
    }

    /// Lists candidate fixture files below the root without following
    /// symbolic links. Output is sorted and independent of directory order.
    ///
    /// At most `max_entries + 1` directory entries are ever yielded to the
    /// walk. If the tree holds more, the whole collection fails with
    /// [`CollectError::EntryLimitExceeded`] rather than returning a subset
    /// that would depend on directory iteration order. This bounds traversal
    /// only, not the memory of records later read from the candidates.
    pub(crate) fn enumerate(&self) -> Result<Enumeration, CollectError> {
        let mut enumeration = Enumeration::default();
        let mut pending = vec![(self.root.path.clone(), String::new(), 0usize)];
        let mut budget = self.limits.max_entries;
        while let Some((directory, prefix, depth)) = pending.pop() {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if depth == 0 => return Err(CollectError::RootUnreadable(error.kind())),
                Err(error) => {
                    enumeration.skip(Some(prefix), SkipReason::Io(error.kind()));
                    continue;
                }
            };
            let entries =
                take_bounded(entries, &mut budget).ok_or(CollectError::EntryLimitExceeded)?;
            let mut entries: Vec<fs::DirEntry> = entries
                .into_iter()
                .filter_map(|entry| {
                    entry
                        .map_err(|error| enumeration.skip(None, SkipReason::Io(error.kind())))
                        .ok()
                })
                .collect();
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries {
                let file_name = entry.file_name();
                let Some(name) = file_name.to_str() else {
                    enumeration.skip(None, SkipReason::NonUtf8Name);
                    continue;
                };
                if validate_component(name).is_err() {
                    enumeration.skip(None, SkipReason::InvalidName);
                    continue;
                }
                let relative = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                // Check all detectable redirections, including Windows reparse points.
                let metadata = match fs::symlink_metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        enumeration.skip(Some(relative), SkipReason::Io(error.kind()));
                        continue;
                    }
                };
                if is_redirection(&metadata) {
                    enumeration.skip(Some(relative), SkipReason::Symlink);
                } else if metadata.is_dir() {
                    if depth + 2 > self.limits.max_depth {
                        enumeration.skip(Some(relative), SkipReason::DepthLimit);
                    } else {
                        pending.push((entry.path(), relative, depth + 1));
                    }
                } else if !metadata.is_file() {
                    enumeration.skip(Some(relative), SkipReason::NotRegularFile);
                } else if has_fixture_extension(name) {
                    enumeration.candidates.push(relative);
                } else {
                    enumeration.skip(Some(relative), SkipReason::NotFixtureExtension);
                }
            }
        }
        enumeration.candidates.sort();
        enumeration.skipped.sort();
        Ok(enumeration)
    }
}

fn is_native_host(kind: EnvironmentKind) -> bool {
    match kind {
        EnvironmentKind::WindowsNative => cfg!(windows),
        EnvironmentKind::Wsl2 => cfg!(target_os = "linux"),
    }
}

pub(crate) struct ResolvedFile {
    pub(crate) path: PathBuf,
    pub(crate) relative: String,
    pub(crate) object: FileObjectRef,
    pub(crate) len: u64,
    pub(crate) modified: std::time::SystemTime,
}

/// Pulls items while `budget` lasts, decrementing it per item. Returns `None`
/// as soon as one more item exists than the budget allows, so at most
/// `budget + 1` items are ever pulled from `items`.
fn take_bounded<I: Iterator>(items: I, budget: &mut usize) -> Option<Vec<I::Item>> {
    let mut taken = Vec::new();
    for item in items {
        *budget = budget.checked_sub(1)?;
        taken.push(item);
    }
    Some(taken)
}

#[derive(Default)]
pub(crate) struct Enumeration {
    pub(crate) candidates: Vec<String>,
    pub(crate) skipped: Vec<SkippedEntry>,
}

impl Enumeration {
    fn skip(&mut self, relative_path: Option<String>, reason: SkipReason) {
        self.skipped.push(SkippedEntry {
            relative_path,
            reason,
        });
    }
}

fn validate_component(component: &str) -> Result<(), PathRejection> {
    match component {
        "" => Err(PathRejection::EmptyComponent),
        "." | ".." => Err(PathRejection::Traversal),
        _ if component.chars().any(char::is_control) || component.contains(['\\', ':']) => {
            Err(PathRejection::InvalidCharacter)
        }
        _ => Ok(()),
    }
}

fn validate_relative(relative: &str, max_depth: usize) -> Result<Vec<&str>, PathRejection> {
    if relative.is_empty() {
        return Err(PathRejection::Empty);
    }
    if relative.len() > MAX_RELATIVE_LEN {
        return Err(PathRejection::TooLong);
    }
    if relative.starts_with(['/', '\\']) || relative.as_bytes().get(1) == Some(&b':') {
        return Err(PathRejection::Absolute);
    }
    let components: Vec<&str> = relative.split('/').collect();
    for component in &components {
        validate_component(component)?;
    }
    if components.len() > max_depth {
        return Err(PathRejection::TooDeep);
    }
    let last = components
        .last()
        .expect("split yields at least one component");
    if !has_fixture_extension(last) {
        return Err(PathRejection::NotFixtureExtension);
    }
    Ok(components)
}

fn has_fixture_extension(name: &str) -> bool {
    name.len() > FIXTURE_EXTENSION.len() && name.ends_with(FIXTURE_EXTENSION)
}

#[cfg(unix)]
pub(crate) fn file_object(metadata: &fs::Metadata) -> FileObjectRef {
    use std::os::unix::fs::MetadataExt;
    FileObjectRef::Unix {
        dev: metadata.dev(),
        ino: metadata.ino(),
    }
}

#[cfg(not(unix))]
pub(crate) fn file_object(_metadata: &fs::Metadata) -> FileObjectRef {
    FileObjectRef::Unavailable
}

#[cfg(unix)]
pub(crate) fn hard_link_count(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink()
}

/// Hard-link counts need unstable std APIs on Windows; not detected there
/// yet (documented limitation).
#[cfg(not(unix))]
pub(crate) fn hard_link_count(_metadata: &fs::Metadata) -> u64 {
    1
}

/// Conservative Windows safeguard: refuse every detected reparse point,
/// including junctions. This is a path-time check, not an atomic no-follow open.
pub(crate) fn is_redirection(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::take_bounded;

    /// A long iterator stands in for a huge directory: the bound must hold
    /// without pulling it to the end.
    #[test]
    fn take_bounded_pulls_at_most_budget_plus_one() {
        let pulled = Cell::new(0usize);
        let huge = (0..10_000_000u32).inspect(|_| pulled.set(pulled.get() + 1));
        let mut budget = 3;
        assert_eq!(take_bounded(huge, &mut budget), None);
        assert_eq!(pulled.get(), 4);
    }

    #[test]
    fn take_bounded_accepts_exact_budget_and_shares_it() {
        let mut budget = 5;
        assert_eq!(take_bounded(0..3, &mut budget), Some(vec![0, 1, 2]));
        assert_eq!(budget, 2);
        assert_eq!(take_bounded(0..2, &mut budget), Some(vec![0, 1]));
        assert_eq!(budget, 0);
        assert_eq!(take_bounded(0..0, &mut budget), Some(vec![]));
        assert_eq!(take_bounded(0..1, &mut budget), None);
    }
}
