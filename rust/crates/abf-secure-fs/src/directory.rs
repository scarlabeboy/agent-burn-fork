//! M2A.2b — directory capability and bounded single-directory enumeration,
//! in safe code over the `sys::linux_x86_64` ABI module. Protocol (README,
//! "Directory enumeration"):
//!
//! **Pin** ([`Directory::pin`]): root binding check; one acquisition beneath
//! the **held root** (`O_PATH|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`, mask
//! `BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`, of the literal `.` for the
//! root target or of the validated relative locator); directory policy on the
//! handle (directory, concrete identity, the root's device, ext-family
//! magic, `nlink > 0`); for a relative locator, an independent second
//! acquisition with identical parameters must pass the policy and reach the
//! same `(dev, ino)` (for the root target, the pin must be the held root
//! object); final root binding check; final policy on the held pin. The
//! regular-file rule `nlink == 1` does **not** apply to directories.
//!
//! **Binding** ([`Directory::check_binding`]): root binding check; for a
//! relative locator, a fresh full-mask acquisition of the original locator
//! must reach the held identity; the held pin must still be a linked
//! directory with that identity. A persistent change is
//! [`EnumerationError::DirectoryBindingChanged`]; a replacement is never
//! adopted. It is a sample: a move restored between two checks is not seen.
//!
//! **Read handle**: S0 (held pin policy), private self-open of the literal
//! `.` relative to the held pin (`O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC|
//! O_NONBLOCK`, mode 0, full mask; no procfs, no caller path or flags), S1
//! (the new handle: directory, the pinned identity and device, linked), S2
//! (held pin again). The handle is closed before the listing is released.
//!
//! **Enumeration** ([`Directory::enumerate`]): cancellation is polled first
//! (refusal before any allocation or open); storage is reserved once and
//! checked against the retained-output ceiling; binding checks run at start,
//! after the read open, after **every** `getdents64` attempt and before
//! release. Any binding, system-call, framing or storage refusal discards the
//! whole listing. Budget or cancellation stops release only what was kept,
//! as `Partial`, after the final checks pass.
//!
//! **Child authorization** ([`Listing::pin_regular`]): the candidate is
//! named by an ordinal of its own listing; the joined root-relative path is
//! revalidated; root and directory binding are checked; the **unchanged**
//! `Root::pin_regular` resolves the full path from the **held root**; root
//! and directory binding are checked again. The held directory descriptor is
//! never an anchor for a child, and a discovered name is never evidence:
//! the pinned object is whatever is currently, admissibly bound there.
//!
//! Descriptors: the root's two, the pin, the read handle and one transient
//! verification handle: at most five during enumeration; a listing keeps
//! three (with the root); a child request at most six.

use crate::enumeration::{
    self, Cancellation, DiscoveryCounters, DiscoveryStatus, EntrySource, EnumerationBudget,
    INLINE_ALLOWANCE_BYTES, Ledger, MAX_TEMPORARY_PAYLOAD_BYTES, READ_BUFFER_BYTES, ReadOutcome,
    Store,
};
use crate::error::{AccessError, EnumerationError, ObjectKind};
use crate::linux::{
    Inspection, MAX_OPEN_ATTEMPTS, OpenFailure, Pinned, Root, classify, classify_errno, inspect,
};
use crate::path::{JOIN_SCRATCH_BYTES, JoinScratch, RelativePath};
use crate::sys::linux_x86_64::{
    self as sys, DIRENT_BUFFER_BYTES, DirectoryLocator, DirectoryPinFd, DirectoryReadFd,
    DirentReadError, EAGAIN, EINTR, EXT_SUPER_MAGIC,
};
use crate::{FileEvidence, ObjectId};

const _: () = assert!(DIRENT_BUFFER_BYTES == READ_BUFFER_BYTES);

/// Explicit temporary payload of one child request: one join scratch and
/// the boxed C string of the revalidated `RelativePath` (at most 1025).
pub(crate) const CHILD_TEMPORARY_BYTES: usize = JOIN_SCRATCH_BYTES + JOIN_SCRATCH_BYTES;

const _: () = {
    assert!(CHILD_TEMPORARY_BYTES <= MAX_TEMPORARY_PAYLOAD_BYTES);
    // The real inline size of a retained listing fits its allowance.
    assert!(std::mem::size_of::<Listing<'static>>() <= INLINE_ALLOWANCE_BYTES);
};

/// Which directory to pin: the held root itself, or a validated locator
/// beneath it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Target<'a> {
    Root,
    Relative(&'a RelativePath),
}

/// The single retained copy of the original locator.
enum Locator {
    Root,
    Relative(RelativePath),
}

impl Locator {
    fn sys(&self) -> DirectoryLocator<'_> {
        match self {
            Self::Root => DirectoryLocator::HeldRoot,
            Self::Relative(path) => DirectoryLocator::Beneath(path),
        }
    }

    fn path(&self) -> Option<&RelativePath> {
        match self {
            Self::Root => None,
            Self::Relative(path) => Some(path),
        }
    }

    fn heap_bytes(&self) -> usize {
        self.path().map_or(0, RelativePath::heap_bytes)
    }
}

/// A pinned directory: borrows its original root, owns one `O_PATH`
/// directory pin and the original locator. Never exposes either.
pub(crate) struct Directory<'r> {
    root: &'r Root,
    pin: DirectoryPinFd,
    locator: Locator,
    id: ObjectId,
}

impl std::fmt::Debug for Directory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Directory")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Initial attempt plus at most two retries after `EAGAIN`/`EINTR`. The
/// closures take no parameters, so every attempt is identical.
fn bounded<T>(mut attempt: impl FnMut() -> Result<T, i32>) -> Result<T, OpenFailure> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(EAGAIN | EINTR) if attempts < MAX_OPEN_ATTEMPTS => {}
            Err(EAGAIN | EINTR) => return Err(OpenFailure::RetryExhausted),
            Err(errno) => return Err(OpenFailure::Errno(errno)),
        }
    }
}

fn open_pin(root: &Root, locator: &Locator) -> Result<DirectoryPinFd, OpenFailure> {
    bounded(|| root.open_directory_pin(locator.sys()))
}

/// Directory policy on a handle sample: directory, the root's device, at
/// least one link. (A concrete non-zero identity is enforced by `inspect`.)
fn directory_policy(root: &Root, info: Inspection) -> Result<Inspection, AccessError> {
    if info.kind != Some(ObjectKind::Directory) {
        return Err(AccessError::NotADirectory);
    }
    if info.id.device != root.id().device {
        return Err(AccessError::ConfinementRefused);
    }
    if info.nlink == 0 {
        return Err(AccessError::ObjectChanged);
    }
    Ok(info)
}

/// Policy plus ext-family filesystem magic, by handle.
fn admit_pin(root: &Root, pin: &DirectoryPinFd) -> Result<Inspection, AccessError> {
    let info = directory_policy(root, inspect(pin)?)?;
    let magic = pin.filesystem_magic().map_err(classify_errno)?;
    if magic != EXT_SUPER_MAGIC {
        return Err(AccessError::UnsupportedFilesystem);
    }
    Ok(info)
}

/// Re-resolving the original locator: anything showing it no longer leads to
/// a directory beneath the root on the same mount is a binding change.
fn classify_directory_binding(failure: OpenFailure) -> EnumerationError {
    match classify(failure) {
        AccessError::NotFound | AccessError::NotADirectory | AccessError::ConfinementRefused => {
            EnumerationError::DirectoryBindingChanged
        }
        other => EnumerationError::Access(other),
    }
}

impl<'r> Directory<'r> {
    pub(crate) fn pin(root: &'r Root, target: Target<'_>) -> Result<Self, AccessError> {
        let locator = match target {
            Target::Root => Locator::Root,
            Target::Relative(path) => Locator::Relative(path.clone()),
        };

        schedule_point!(DirectoryPinBeforeBindingCheck);
        root.check_binding()?;
        let held = open_pin(root, &locator).map_err(classify)?;
        let first = admit_pin(root, &held)?;
        schedule_point!(DirectoryPinAfterAcquire);

        match locator {
            // `.` of the held root must be the held root object.
            Locator::Root => {
                if first.id != root.id() {
                    return Err(AccessError::ObjectChanged);
                }
            }
            // Independent second full-mask acquisition of the same locator.
            Locator::Relative(_) => {
                let verify =
                    open_pin(root, &locator).map_err(|failure| match classify(failure) {
                        AccessError::NotFound
                        | AccessError::NotADirectory
                        | AccessError::ConfinementRefused => AccessError::ObjectChanged,
                        other => other,
                    })?;
                let second = admit_pin(root, &verify);
                drop(verify);
                if second?.id != first.id {
                    return Err(AccessError::ObjectChanged);
                }
            }
        }

        schedule_point!(DirectoryPinAfterVerify);
        root.check_binding()?;
        let last = admit_pin(root, &held)?;
        if last.id != first.id {
            return Err(AccessError::ObjectChanged);
        }
        Ok(Self {
            root,
            pin: held,
            locator,
            id: last.id,
        })
    }

    pub(crate) fn id(&self) -> ObjectId {
        self.id
    }

    /// The held pin is still a linked directory with the pinned identity.
    fn held_sample(&self) -> Result<(), EnumerationError> {
        let info = inspect(&self.pin)?;
        if info.kind != Some(ObjectKind::Directory) || info.id != self.id || info.nlink == 0 {
            return Err(EnumerationError::DirectoryBindingChanged);
        }
        Ok(())
    }

    /// Root binding, then the original locator binding (relative target),
    /// then the held pin. Each verification handle is dropped before the
    /// next is opened.
    pub(crate) fn check_binding(&self) -> Result<(), EnumerationError> {
        self.root.check_binding()?;
        if let Locator::Relative(_) = self.locator {
            let fresh = open_pin(self.root, &self.locator).map_err(classify_directory_binding)?;
            let info = inspect(&fresh);
            drop(fresh);
            let info = info?;
            if info.kind != Some(ObjectKind::Directory) || info.id != self.id {
                return Err(EnumerationError::DirectoryBindingChanged);
            }
        }
        self.held_sample()
    }

    /// S0, private `.` self-open, S1, S2.
    fn open_read(&self) -> Result<DirectoryReadFd, EnumerationError> {
        self.held_sample()?;
        schedule_point!(EnumerateBeforeReadOpen);
        let read = bounded(|| sys::open_directory_read(&self.pin))
            .map_err(|failure| EnumerationError::Access(classify(failure)))?;
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        sys::fault::note_directory_read_status(read.descriptor_status().ok());
        schedule_point!(EnumerateAfterReadOpen);

        let info = directory_policy(self.root, inspect(&read)?)?;
        if info.id != self.id {
            return Err(EnumerationError::Access(AccessError::ObjectChanged));
        }

        self.held_sample()?;
        schedule_point!(EnumerateAfterS2);
        Ok(read)
    }

    /// One bounded enumeration. Consumes the pin; the listing keeps it.
    pub(crate) fn enumerate(
        self,
        budget: EnumerationBudget,
        cancel: &Cancellation,
    ) -> Result<Listing<'r>, EnumerationError> {
        if cancel.is_requested() {
            return Err(EnumerationError::Cancelled);
        }
        #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
        let refuse = sys::fault::take_refuse_allocation();
        #[cfg(not(all(feature = "fault-injection", abf_secure_fs_harness)))]
        let refuse = false;
        let fixed = self
            .locator
            .heap_bytes()
            .checked_add(std::mem::size_of::<Listing<'static>>())
            .ok_or(EnumerationError::AllocationRefused)?;
        let mut store = Store::reserve(&budget, fixed, refuse)?;

        schedule_point!(EnumerateBeforeStartCheck);
        if cancel.is_requested() {
            return Err(EnumerationError::Cancelled);
        }
        self.check_binding()?;
        let read = self.open_read()?;
        self.check_binding()?;

        let mut scratch = JoinScratch::new(self.locator.path());
        let scan = {
            let mut source = KernelSource {
                directory: &self,
                read: &read,
            };
            enumeration::scan(&mut source, &budget, cancel, &mut store, &mut scratch)?
        };
        drop(read);

        schedule_point!(EnumerateBeforeFinalCheck);
        self.check_binding()?;
        schedule_point!(EnumerateBeforeRelease);
        let status = enumeration::release_status(scan.end, &scan.counters, cancel.is_requested());
        store.sort();
        let ledger = store.ledger(fixed)?;
        Ok(Listing {
            directory: self,
            store,
            status,
            counters: scan.counters,
            ledger,
        })
    }
}

/// The kernel side of one scan: `getdents64` on the private read handle,
/// and binding plus read-handle checks after each attempt.
struct KernelSource<'a, 'r> {
    directory: &'a Directory<'r>,
    read: &'a DirectoryReadFd,
}

impl EntrySource for KernelSource<'_, '_> {
    fn read(&mut self, buf: &mut [u8; READ_BUFFER_BYTES]) -> Result<ReadOutcome, EnumerationError> {
        let result = sys::read_directory_entries(self.read, buf);
        schedule_point!(EnumerateAfterAttempt);
        match result {
            Ok(count) => Ok(ReadOutcome::Bytes(count)),
            Err(DirentReadError::Errno(EINTR)) => Ok(ReadOutcome::Interrupted),
            // ENOENT, EAGAIN and every other errno are fatal, never EOF.
            Err(DirentReadError::Errno(errno)) => {
                Err(EnumerationError::Access(classify_errno(errno)))
            }
            Err(DirentReadError::Overlong) => Err(EnumerationError::MalformedDirectoryData),
        }
    }

    fn check(&mut self) -> Result<(), EnumerationError> {
        self.directory.check_binding()?;
        let info = inspect(self.read)?;
        if info.kind != Some(ObjectKind::Directory) || info.id != self.directory.id {
            return Err(EnumerationError::DirectoryBindingChanged);
        }
        Ok(())
    }
}

/// A released listing: the consumed directory capability (root borrow, pin,
/// locator), the bounded private names and the status. Names never leave
/// the crate; candidates are addressed by ordinal.
pub(crate) struct Listing<'r> {
    directory: Directory<'r>,
    store: Store,
    status: DiscoveryStatus,
    counters: DiscoveryCounters,
    /// Checked at release; read only by the two-key harness.
    #[cfg_attr(
        not(all(feature = "fault-injection", abf_secure_fs_harness)),
        allow(dead_code)
    )]
    ledger: Ledger,
}

impl std::fmt::Debug for Listing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listing")
            .field("directory", &self.directory.id)
            .field("candidates", &self.store.len())
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Listing<'_> {
    pub(crate) fn len(&self) -> usize {
        self.store.len()
    }

    pub(crate) fn status(&self) -> DiscoveryStatus {
        self.status
    }

    pub(crate) fn counters(&self) -> DiscoveryCounters {
        self.counters
    }

    pub(crate) fn directory_id(&self) -> ObjectId {
        self.directory.id
    }

    #[cfg(all(feature = "fault-injection", abf_secure_fs_harness))]
    pub(crate) fn ledger(&self) -> Ledger {
        self.ledger
    }

    /// Fresh root-relative authorization of candidate `ordinal`.
    pub(crate) fn pin_regular(
        &self,
        ordinal: usize,
    ) -> Result<(Pinned, FileEvidence), EnumerationError> {
        let name = self
            .store
            .name(ordinal)
            .ok_or(EnumerationError::OrdinalOutOfRange)?;
        let mut scratch = JoinScratch::new(self.directory.locator.path());
        let joined = scratch
            .join(name)
            .map_err(|error| EnumerationError::Access(AccessError::Path(error)))?;
        let relative = RelativePath::new(joined)
            .map_err(|error| EnumerationError::Access(AccessError::Path(error)))?;

        schedule_point!(ChildBeforeBindingCheck);
        self.directory.check_binding()?;
        schedule_point!(ChildBeforePin);
        let pinned = self.directory.root.pin_regular(&relative)?;
        schedule_point!(ChildAfterPin);
        self.directory.check_binding()?;
        Ok(pinned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_inline_size_fits_the_allowance() {
        let inline = std::mem::size_of::<Listing<'static>>();
        assert!(inline <= INLINE_ALLOWANCE_BYTES, "{inline}");
        // Recorded for the ABF18 ledger.
        eprintln!("ABF18-LEDGER inline Listing bytes: {inline}");
    }
}
