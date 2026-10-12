//! M2A.2b — bounded single-directory discovery engine, in safe code.
//!
//! This module never touches the OS. It frames the bytes one `getdents64`
//! attempt returned, classifies each name, keeps accepted names in private
//! bounded storage and decides the discovery status. The Linux protocol
//! (`crate::directory`) supplies the attempts and the binding checks through
//! [`EntrySource`]; unit tests drive the same engine with synthetic batches.
//!
//! Framing (`getdents(2)`, `struct linux_dirent64`): `d_ino` u64 at 0,
//! `d_off` i64 at 8, `d_reclen` u16 at 16, `d_type` u8 at 18, `d_name` at 19.
//! Only `d_reclen` and `d_name` are decoded, from explicit byte slices;
//! `d_ino`, `d_off` and `d_type` are **never** read: they authorize, type,
//! identify or order nothing. There are no casts, no alignment-dependent
//! reads and no unbounded C-string scans. Any record that cannot be framed
//! refuses the **entire** listing, including the valid records before it.
//!
//! Hard limits per operation (README, "Directory enumeration"): one fixed
//! 8192-byte buffer, 4096 inspected records, 512 candidates, 255 bytes per
//! name, 65536 retained name bytes, 64 read attempts, 4160 scan steps, at
//! most two consecutive `EINTR` retries. Storage is reserved once, before
//! any I/O, and never grows. The limits bound this operation's accounted
//! payload; they do not bound process RSS, allocator metadata, kernel
//! memory, stack usage or wall-clock time.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{AccessError, EnumerationError};
use crate::path::{JOIN_SCRATCH_BYTES, JoinScratch, MAX_COMPONENT_BYTES};

/// Size of the fixed, initialized `getdents64` output buffer.
pub const READ_BUFFER_BYTES: usize = 8192;
/// Hard maximum of records inspected per enumeration, including `.`, `..`,
/// rejected names and duplicates.
pub const MAX_INSPECTED_RECORDS: usize = 4096;
/// Hard maximum of retained candidates per enumeration.
pub const MAX_CANDIDATES: usize = 512;
/// Hard maximum of retained name bytes per enumeration (no NUL, padding,
/// type, inode or cookie is retained).
pub const MAX_RETAINED_NAME_BYTES: usize = 65_536;
/// Hard maximum of `getdents64` attempts per enumeration, including
/// interrupted, failed and end-of-stream attempts.
pub const MAX_READ_ATTEMPTS: usize = 64;
/// At most this many consecutive `EINTR` retries; the next one is fatal.
pub const MAX_CONSECUTIVE_INTERRUPTS: usize = 2;
/// Hard maximum of read attempts plus inspected records.
pub const MAX_SCAN_STEPS: usize = MAX_READ_ATTEMPTS + MAX_INSPECTED_RECORDS;
/// Ceiling of the accounted retained output of one listing: actual storage
/// capacities, the retained locator and the listing's inline size.
pub const MAX_RETAINED_OUTPUT_BYTES: usize = 98_304;
/// Ceiling of the explicit temporary payload of one enumeration or child
/// request (fixed buffers and scratch), counted separately.
pub const MAX_TEMPORARY_PAYLOAD_BYTES: usize = 16_384;

/// Allowance for the inline size of a retained listing (capability, root
/// borrow, storage headers, status, counters). Checked at compile time
/// against the real type in `crate::directory`.
pub(crate) const INLINE_ALLOWANCE_BYTES: usize = 4096;

/// Explicit temporary payload of one enumeration: the read buffer and one
/// join scratch. The sort is in place (no scratch).
pub(crate) const ENUMERATION_TEMPORARY_BYTES: usize = READ_BUFFER_BYTES + JOIN_SCRATCH_BYTES;

const _: () = {
    assert!(MAX_SCAN_STEPS == 4160);
    assert!(ENUMERATION_TEMPORARY_BYTES <= MAX_TEMPORARY_PAYLOAD_BYTES);
    assert!(MAX_CANDIDATES <= MAX_INSPECTED_RECORDS);
    // Largest storage: arena + slots + locator (1025) + inline allowance.
    assert!(
        MAX_RETAINED_NAME_BYTES
            + MAX_CANDIDATES * std::mem::size_of::<Slot>()
            + JOIN_SCRATCH_BYTES
            + INLINE_ALLOWANCE_BYTES
            <= MAX_RETAINED_OUTPUT_BYTES
    );
};

// ---------------------------------------------------------------------------
// Budget and cancellation
// ---------------------------------------------------------------------------

/// Caller budget of one enumeration. Every value is in `1..=` its hard
/// maximum and `candidates <= inspected_records`. Fields are private; the
/// only constructors validate them. The buffer size, depth (one directory),
/// descriptor and memory ceilings are fixed and cannot be expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumerationBudget {
    records: usize,
    candidates: usize,
    name_bytes: usize,
    attempts: usize,
}

impl EnumerationBudget {
    /// All hard maximums.
    pub const fn maximum() -> Self {
        Self {
            records: MAX_INSPECTED_RECORDS,
            candidates: MAX_CANDIDATES,
            name_bytes: MAX_RETAINED_NAME_BYTES,
            attempts: MAX_READ_ATTEMPTS,
        }
    }

    /// Validates a budget. [`EnumerationError::InvalidBudget`] for a zero
    /// or above-maximum value, or more candidates than inspected records.
    pub fn new(
        inspected_records: usize,
        candidates: usize,
        retained_name_bytes: usize,
        read_attempts: usize,
    ) -> Result<Self, EnumerationError> {
        let within = |value: usize, max: usize| (1..=max).contains(&value);
        if !within(inspected_records, MAX_INSPECTED_RECORDS)
            || !within(candidates, MAX_CANDIDATES)
            || !within(retained_name_bytes, MAX_RETAINED_NAME_BYTES)
            || !within(read_attempts, MAX_READ_ATTEMPTS)
            || candidates > inspected_records
        {
            return Err(EnumerationError::InvalidBudget);
        }
        Ok(Self {
            records: inspected_records,
            candidates,
            name_bytes: retained_name_bytes,
            attempts: read_attempts,
        })
    }

    pub fn inspected_records(&self) -> usize {
        self.records
    }

    pub fn candidates(&self) -> usize {
        self.candidates
    }

    pub fn retained_name_bytes(&self) -> usize {
        self.name_bytes
    }

    pub fn read_attempts(&self) -> usize {
        self.attempts
    }
}

impl Default for EnumerationBudget {
    fn default() -> Self {
        Self::maximum()
    }
}

/// A cancellation request for an enumeration. It is only polled: before
/// storage is reserved, before every read attempt and every record, and
/// before the result is released. It cannot interrupt a system call already
/// in progress, and there is no callback.
#[derive(Debug, Default)]
pub struct Cancellation {
    requested: AtomicBool,
}

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Irreversible for this value.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Status and counters
// ---------------------------------------------------------------------------

/// Which budget stopped the discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BudgetKind {
    InspectedRecords,
    Candidates,
    NameBytes,
    ReadAttempts,
}

/// Why the discovery stream stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StreamEnd {
    /// `getdents64` actually returned 0 bytes. Only this is end of stream.
    Eof,
    /// A budget was reached; no extra read was made to look for the end.
    Budget(BudgetKind),
    /// Cancellation was observed after reading had started.
    Cancelled,
}

/// Whether the live discovery stream was exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Coverage {
    /// Actual end of stream, every returned record inspected, no name
    /// rejected by policy, final bindings valid, no cancellation. Describes a
    /// live stream, **not** a snapshot, a current inventory or the
    /// authorization of any child.
    Exhausted,
    /// Anything else.
    Partial,
}

/// Discovery status of a released listing. It cannot be constructed outside
/// this crate. There is no "complete" or "authorized" status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryStatus {
    end: StreamEnd,
    coverage: Coverage,
}

impl DiscoveryStatus {
    pub fn end(&self) -> StreamEnd {
        self.end
    }

    pub fn coverage(&self) -> Coverage {
        self.coverage
    }
}

/// Bounded counters of one enumeration. Counts reveal directory cardinality
/// and rejection facts, never names. Only this crate constructs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryCounters {
    read_attempts: usize,
    records_inspected: usize,
    navigation_records: usize,
    rejected_names: usize,
    candidates: usize,
    retained_name_bytes: usize,
}

impl DiscoveryCounters {
    const fn zero() -> Self {
        Self {
            read_attempts: 0,
            records_inspected: 0,
            navigation_records: 0,
            rejected_names: 0,
            candidates: 0,
            retained_name_bytes: 0,
        }
    }

    /// `getdents64` attempts, including interrupted and end-of-stream ones.
    pub fn read_attempts(&self) -> usize {
        self.read_attempts
    }

    /// Records inspected, including `.`, `..`, rejected names and
    /// duplicates.
    pub fn records_inspected(&self) -> usize {
        self.records_inspected
    }

    /// Exact `.` and `..` records (excluded, not a coverage loss).
    pub fn navigation_records(&self) -> usize {
        self.navigation_records
    }

    /// Names refused by the name policy (too long, not UTF-8, control or
    /// cross-OS characters, joined path too long or too deep).
    pub fn rejected_names(&self) -> usize {
        self.rejected_names
    }

    pub fn candidates(&self) -> usize {
        self.candidates
    }

    pub fn retained_name_bytes(&self) -> usize {
        self.retained_name_bytes
    }
}

/// Status released with a listing. Cancellation observed at release wins
/// over end of stream and budgets (a fatal error has already refused).
pub(crate) fn release_status(
    end: StreamEnd,
    counters: &DiscoveryCounters,
    cancelled_at_release: bool,
) -> DiscoveryStatus {
    let end = if cancelled_at_release {
        StreamEnd::Cancelled
    } else {
        end
    };
    let coverage = if end == StreamEnd::Eof && counters.rejected_names == 0 {
        Coverage::Exhausted
    } else {
        Coverage::Partial
    };
    DiscoveryStatus { end, coverage }
}

// ---------------------------------------------------------------------------
// Record framing (safe byte-slice parser)
// ---------------------------------------------------------------------------

/// Byte offset of `d_reclen` (u16, native endian).
const RECLEN_OFFSET: usize = 16;
/// Byte offset of `d_name`.
const NAME_OFFSET: usize = 19;
/// The 19-byte prefix plus at least the terminating NUL.
const MIN_RECORD_BYTES: usize = NAME_OFFSET + 1;

/// The record at `cursor` could not be framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed;

/// Frames the record starting at `cursor` in `batch` (exactly the returned
/// bytes). Returns the name (bytes before the first NUL **inside this
/// record**) and the strictly greater offset of the next record. Bytes after
/// the NUL are padding and are ignored, whatever their value.
pub(crate) fn frame_record(batch: &[u8], cursor: usize) -> Result<(&[u8], usize), Malformed> {
    let rest = batch.get(cursor..).ok_or(Malformed)?;
    if rest.len() < MIN_RECORD_BYTES {
        return Err(Malformed);
    }
    let reclen = match rest.get(RECLEN_OFFSET..RECLEN_OFFSET + 2) {
        Some(&[low, high]) => usize::from(u16::from_ne_bytes([low, high])),
        _ => return Err(Malformed),
    };
    if reclen < MIN_RECORD_BYTES {
        return Err(Malformed);
    }
    let record = rest.get(..reclen).ok_or(Malformed)?;
    let name_area = record.get(NAME_OFFSET..).ok_or(Malformed)?;
    let nul = name_area
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(Malformed)?;
    let name = name_area.get(..nul).ok_or(Malformed)?;
    if name.is_empty() || name.contains(&b'/') {
        return Err(Malformed);
    }
    let next = cursor.checked_add(reclen).ok_or(Malformed)?;
    Ok((name, next))
}

/// Policy decision for one framed name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameClass {
    /// Exactly `.` or `..`.
    Navigation,
    /// Refused by the name policy; counted, never retained or logged.
    Rejected,
    Accepted,
}

/// Exact `.`/`..` are navigation. Everything else must be at most 255 bytes,
/// strict UTF-8 and, joined after the directory locator, pass the unchanged
/// `RelativePath` rules (component and total limits, no control, `\` or
/// `:`), so that every candidate is later authorizable root-relatively.
pub(crate) fn classify_name(name: &[u8], scratch: &mut JoinScratch) -> NameClass {
    if name == b"." || name == b".." {
        return NameClass::Navigation;
    }
    if name.len() > MAX_COMPONENT_BYTES || std::str::from_utf8(name).is_err() {
        return NameClass::Rejected;
    }
    match scratch.join(name) {
        Ok(_) => NameClass::Accepted,
        Err(_) => NameClass::Rejected,
    }
}

// ---------------------------------------------------------------------------
// Bounded storage
// ---------------------------------------------------------------------------

/// A retained candidate: a range of the name arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    start: u32,
    len: u32,
}

const _: () = assert!(std::mem::size_of::<Slot>() == 8);

/// Private bounded storage: one name arena and one slot vector, reserved
/// exactly once before any I/O. Nothing is pushed past the reserved
/// capacity, so neither vector ever reallocates; a changed capacity is
/// detected and refused. `Debug` never prints names.
pub(crate) struct Store {
    arena: Vec<u8>,
    slots: Vec<Slot>,
    name_limit: usize,
    slot_limit: usize,
    arena_capacity: usize,
    slots_capacity: usize,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("candidates", &self.slots.len())
            .finish_non_exhaustive()
    }
}

/// Actual retained-output ledger of a listing, in bytes. Its parts are read
/// by the unit tests and the two-key harness only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(any(test, all(feature = "fault-injection", abf_secure_fs_harness))),
    allow(dead_code)
)]
pub(crate) struct Ledger {
    pub(crate) arena_capacity: usize,
    pub(crate) slots_capacity_bytes: usize,
    pub(crate) fixed: usize,
    pub(crate) total: usize,
}

impl Store {
    /// Reserves the arena and slots for `budget`, then checks the **actual**
    /// capacities plus `fixed` (retained locator heap bytes and the
    /// listing's inline size) against [`MAX_RETAINED_OUTPUT_BYTES`].
    /// `refuse` models an allocation failure (harness/tests only).
    pub(crate) fn reserve(
        budget: &EnumerationBudget,
        fixed: usize,
        refuse: bool,
    ) -> Result<Self, EnumerationError> {
        if refuse {
            return Err(EnumerationError::AllocationRefused);
        }
        let mut arena = Vec::new();
        arena
            .try_reserve_exact(budget.name_bytes)
            .map_err(|_| EnumerationError::AllocationRefused)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(budget.candidates)
            .map_err(|_| EnumerationError::AllocationRefused)?;
        let store = Self {
            arena_capacity: arena.capacity(),
            slots_capacity: slots.capacity(),
            arena,
            slots,
            name_limit: budget.name_bytes,
            slot_limit: budget.candidates,
        };
        store.ledger(fixed)?;
        Ok(store)
    }

    /// The ledger from actual capacities; refuses above the ceiling or on
    /// arithmetic overflow.
    pub(crate) fn ledger(&self, fixed: usize) -> Result<Ledger, EnumerationError> {
        let arena_capacity = self.arena.capacity();
        let slots_capacity_bytes = self
            .slots
            .capacity()
            .checked_mul(std::mem::size_of::<Slot>())
            .ok_or(EnumerationError::AllocationRefused)?;
        let total = arena_capacity
            .checked_add(slots_capacity_bytes)
            .and_then(|sum| sum.checked_add(fixed))
            .ok_or(EnumerationError::AllocationRefused)?;
        if total > MAX_RETAINED_OUTPUT_BYTES {
            return Err(EnumerationError::AllocationRefused);
        }
        Ok(Ledger {
            arena_capacity,
            slots_capacity_bytes,
            fixed,
            total,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn name_bytes(&self) -> usize {
        self.arena.len()
    }

    /// Room for one more candidate of `len` bytes within the budget.
    fn fits(&self, len: usize) -> bool {
        self.slots.len() < self.slot_limit
            && self
                .arena
                .len()
                .checked_add(len)
                .is_some_and(|end| end <= self.name_limit)
    }

    /// Appends within the reserved capacity, then verifies that neither
    /// vector reallocated.
    fn push(&mut self, name: &[u8]) -> Result<(), EnumerationError> {
        if !self.fits(name.len())
            || self.slots.len() >= self.slots_capacity
            || self.arena.len() + name.len() > self.arena_capacity
        {
            return Err(EnumerationError::AllocationRefused);
        }
        let start =
            u32::try_from(self.arena.len()).map_err(|_| EnumerationError::AllocationRefused)?;
        let len = u32::try_from(name.len()).map_err(|_| EnumerationError::AllocationRefused)?;
        self.arena.extend_from_slice(name);
        self.slots.push(Slot { start, len });
        if self.arena.capacity() != self.arena_capacity
            || self.slots.capacity() != self.slots_capacity
        {
            return Err(EnumerationError::AllocationRefused);
        }
        Ok(())
    }

    /// Name bytes of candidate `ordinal`, if it exists.
    pub(crate) fn name(&self, ordinal: usize) -> Option<&[u8]> {
        let slot = self.slots.get(ordinal)?;
        let start = usize::try_from(slot.start).ok()?;
        let end = start.checked_add(usize::try_from(slot.len).ok()?)?;
        self.arena.get(start..end)
    }

    /// In-place iterative insertion sort of the slots by raw name bytes:
    /// no allocation, no recursion, no scratch. Duplicates are kept. At most
    /// `n * (n - 1) / 2` comparisons (130816 for 512 candidates), each over
    /// at most 255 bytes. Returns the number of comparisons.
    pub(crate) fn sort(&mut self) -> usize {
        let mut comparisons = 0;
        for unsorted in 1..self.slots.len() {
            let mut index = unsorted;
            while index > 0 {
                comparisons += 1;
                let (left, right) = (self.slots[index - 1], self.slots[index]);
                if self.slot_bytes(left) <= self.slot_bytes(right) {
                    break;
                }
                self.slots.swap(index - 1, index);
                index -= 1;
            }
        }
        comparisons
    }

    fn slot_bytes(&self, slot: Slot) -> &[u8] {
        let start = slot.start as usize;
        let end = start + slot.len as usize;
        self.arena.get(start..end).unwrap_or(&[])
    }

    #[cfg(test)]
    fn capacities(&self) -> (usize, usize) {
        (self.arena.capacity(), self.slots.capacity())
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Result of one read attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOutcome {
    /// Checked byte count, `<= READ_BUFFER_BYTES`. 0 is end of stream.
    Bytes(usize),
    /// `EINTR`.
    Interrupted,
}

/// The OS side of an enumeration.
pub(crate) trait EntrySource {
    /// Exactly one `getdents64` attempt. A fatal error discards everything.
    fn read(&mut self, buf: &mut [u8; READ_BUFFER_BYTES]) -> Result<ReadOutcome, EnumerationError>;

    /// Root and directory binding checks, run exactly once after **every**
    /// attempt (including interrupted, end-of-stream and fatal ones).
    fn check(&mut self) -> Result<(), EnumerationError>;
}

/// What the scan observed, before the final checks and release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scan {
    pub(crate) end: StreamEnd,
    pub(crate) counters: DiscoveryCounters,
}

/// A budget already reached, in tie order: inspected records, candidates,
/// retained name bytes. (Cancellation is polled first; attempts only when
/// another read would be needed.)
fn reached(
    counters: &DiscoveryCounters,
    store: &Store,
    budget: &EnumerationBudget,
) -> Option<BudgetKind> {
    if counters.records_inspected >= budget.records {
        Some(BudgetKind::InspectedRecords)
    } else if store.len() >= budget.candidates {
        Some(BudgetKind::Candidates)
    } else if store.name_bytes() >= budget.name_bytes {
        Some(BudgetKind::NameBytes)
    } else {
        None
    }
}

/// Runs the bounded scan. Cancellation observed before the first attempt is
/// a coded refusal. Fatal source, framing or storage errors return `Err`
/// and the caller discards everything. A reached ceiling stops before the
/// next action, without an extra end-of-stream probe; the last permitted
/// batch is still parsed, subject to the record, candidate, byte and
/// cancellation limits.
pub(crate) fn scan<S: EntrySource>(
    source: &mut S,
    budget: &EnumerationBudget,
    cancel: &Cancellation,
    store: &mut Store,
    scratch: &mut JoinScratch,
) -> Result<Scan, EnumerationError> {
    let mut buf = [0_u8; READ_BUFFER_BYTES];
    let mut counters = DiscoveryCounters::zero();
    let mut interrupts = 0;
    let end = 'scan: loop {
        if cancel.is_requested() {
            if counters.read_attempts == 0 {
                return Err(EnumerationError::Cancelled);
            }
            break 'scan StreamEnd::Cancelled;
        }
        if let Some(kind) = reached(&counters, store, budget) {
            break 'scan StreamEnd::Budget(kind);
        }
        if counters.read_attempts >= budget.attempts {
            break 'scan StreamEnd::Budget(BudgetKind::ReadAttempts);
        }

        counters.read_attempts += 1;
        // Exactly one binding check after EVERY attempt, including a fatal
        // one (F19-1). A fatal read error takes precedence over a check
        // error; otherwise a check error refuses. Nothing is released either
        // way.
        let read = source.read(&mut buf);
        let checked = source.check();
        let outcome = read?;
        checked?;
        let count = match outcome {
            ReadOutcome::Interrupted => {
                interrupts += 1;
                if interrupts > MAX_CONSECUTIVE_INTERRUPTS {
                    return Err(EnumerationError::Access(AccessError::RetryExhausted));
                }
                continue;
            }
            ReadOutcome::Bytes(0) => break 'scan StreamEnd::Eof,
            ReadOutcome::Bytes(count) => count,
        };
        interrupts = 0;

        let batch = buf
            .get(..count)
            .ok_or(EnumerationError::MalformedDirectoryData)?;
        let mut cursor = 0;
        while cursor < batch.len() {
            if cancel.is_requested() {
                break 'scan StreamEnd::Cancelled;
            }
            if let Some(kind) = reached(&counters, store, budget) {
                break 'scan StreamEnd::Budget(kind);
            }
            counters.records_inspected += 1;
            let (name, next) = frame_record(batch, cursor)
                .map_err(|Malformed| EnumerationError::MalformedDirectoryData)?;
            cursor = next;
            match classify_name(name, scratch) {
                NameClass::Navigation => counters.navigation_records += 1,
                NameClass::Rejected => counters.rejected_names += 1,
                NameClass::Accepted => {
                    if !store.fits(name.len()) {
                        // The slot count was checked before this record, so
                        // only the byte budget can be short: charged as
                        // inspected, nothing stored.
                        break 'scan StreamEnd::Budget(BudgetKind::NameBytes);
                    }
                    store.push(name)?;
                }
            }
        }
    };
    counters.candidates = store.len();
    counters.retained_name_bytes = store.name_bytes();
    let steps = counters.read_attempts + counters.records_inspected;
    if steps > MAX_SCAN_STEPS
        || counters.read_attempts > budget.attempts
        || counters.records_inspected > budget.records
    {
        // Unreachable by construction; refuse rather than release.
        return Err(EnumerationError::InvalidBudget);
    }
    Ok(Scan { end, counters })
}

// ---------------------------------------------------------------------------
// Byte-model tests (no OS access)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::RelativePath;
    use std::collections::VecDeque;

    /// Independent record builder: 8-byte aligned like the kernel, with
    /// arbitrary inode/offset/type bytes.
    fn record(name: &[u8], ino: u64, off: i64, kind: u8) -> Vec<u8> {
        let reclen = (19 + name.len() + 1).div_ceil(8) * 8;
        let mut out = Vec::with_capacity(reclen);
        out.extend_from_slice(&ino.to_ne_bytes());
        out.extend_from_slice(&off.to_ne_bytes());
        out.extend_from_slice(&(reclen as u16).to_ne_bytes());
        out.push(kind);
        out.extend_from_slice(name);
        out.resize(reclen, 0);
        out
    }

    fn rec(name: &[u8]) -> Vec<u8> {
        record(name, 7, 1, 8)
    }

    fn batch(names: &[&[u8]]) -> Vec<u8> {
        names.iter().flat_map(|n| rec(n)).collect()
    }

    #[derive(Debug, Clone)]
    enum Step {
        Bytes(Vec<u8>),
        Eintr,
        Fatal(EnumerationError),
        /// Return this raw count (> buffer) without writing.
        Count(usize),
    }

    /// Scripted source. After the script, every read is EOF. `cancel_after`
    /// requests cancellation after that many reads; `fail_check_at` fails
    /// the check after that read number (1-based).
    struct Model<'a> {
        steps: VecDeque<Step>,
        reads: usize,
        checks: usize,
        fail_check_at: Option<usize>,
        cancel_after: Option<(usize, &'a Cancellation)>,
    }

    impl<'a> Model<'a> {
        fn new(steps: Vec<Step>) -> Self {
            Self {
                steps: steps.into(),
                reads: 0,
                checks: 0,
                fail_check_at: None,
                cancel_after: None,
            }
        }
    }

    impl EntrySource for Model<'_> {
        fn read(
            &mut self,
            buf: &mut [u8; READ_BUFFER_BYTES],
        ) -> Result<ReadOutcome, EnumerationError> {
            self.reads += 1;
            let step = self.steps.pop_front().unwrap_or(Step::Bytes(Vec::new()));
            let outcome = match step {
                Step::Bytes(bytes) => {
                    assert!(bytes.len() <= READ_BUFFER_BYTES);
                    buf[..bytes.len()].copy_from_slice(&bytes);
                    Ok(ReadOutcome::Bytes(bytes.len()))
                }
                Step::Eintr => Ok(ReadOutcome::Interrupted),
                Step::Fatal(error) => Err(error),
                Step::Count(count) => Ok(ReadOutcome::Bytes(count)),
            };
            if let Some((after, cancel)) = self.cancel_after
                && self.reads == after
            {
                cancel.request();
            }
            outcome
        }

        fn check(&mut self) -> Result<(), EnumerationError> {
            self.checks += 1;
            if self.fail_check_at == Some(self.reads) {
                return Err(EnumerationError::DirectoryBindingChanged);
            }
            Ok(())
        }
    }

    struct Run {
        result: Result<Scan, EnumerationError>,
        store: Store,
        reads: usize,
        checks: usize,
    }

    fn drive_with(
        model: &mut Model<'_>,
        budget: EnumerationBudget,
        cancel: &Cancellation,
        parent: Option<&RelativePath>,
    ) -> Run {
        let mut store = Store::reserve(&budget, 0, false).unwrap();
        let mut scratch = JoinScratch::new(parent);
        let result = scan(model, &budget, cancel, &mut store, &mut scratch);
        Run {
            result,
            store,
            reads: model.reads,
            checks: model.checks,
        }
    }

    fn drive(steps: Vec<Step>, budget: EnumerationBudget) -> Run {
        drive_with(&mut Model::new(steps), budget, &Cancellation::new(), None)
    }

    fn names(store: &Store) -> Vec<Vec<u8>> {
        (0..store.len())
            .map(|i| store.name(i).unwrap().to_vec())
            .collect()
    }

    fn max() -> EnumerationBudget {
        EnumerationBudget::maximum()
    }

    fn status(run: &Run) -> DiscoveryStatus {
        let scan = run.result.unwrap();
        release_status(scan.end, &scan.counters, false)
    }

    // -- framing ----------------------------------------------------------

    #[test]
    fn frames_kernel_layout_offsets() {
        let bytes = record(b"abc", u64::MAX, -1, 4);
        assert_eq!(bytes.len(), 24);
        assert_eq!(&bytes[16..18], &24_u16.to_ne_bytes());
        assert_eq!(&bytes[19..22], b"abc");
        assert_eq!(frame_record(&bytes, 0), Ok((&b"abc"[..], 24)));
    }

    #[test]
    fn inode_offset_and_type_never_change_decisions() {
        let mut outcomes = Vec::new();
        for value in [0_u8, 1, 4, 8, 10, 12, 0x7f, 0x80, 0xff] {
            let ino = u64::from_ne_bytes([value; 8]);
            let off = i64::from_ne_bytes([value; 8]);
            let bytes: Vec<u8> = [&b"x"[..], b"."]
                .iter()
                .flat_map(|n| record(n, ino, off, value))
                .collect();
            let run = drive(vec![Step::Bytes(bytes)], max());
            outcomes.push((names(&run.store), run.result.unwrap()));
        }
        assert!(outcomes.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(outcomes[0].0, vec![b"x".to_vec()]);
    }

    #[test]
    fn every_truncation_of_a_valid_batch_is_malformed() {
        let full = batch(&[b"first", b"second-name"]);
        let boundaries = [0, rec(b"first").len(), full.len()];
        for cut in 1..full.len() {
            let result = drive(vec![Step::Bytes(full[..cut].to_vec())], max()).result;
            if boundaries.contains(&cut) {
                assert!(result.is_ok(), "cut {cut}");
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    EnumerationError::MalformedDirectoryData,
                    "cut {cut}"
                );
            }
        }
    }

    #[test]
    fn bad_record_lengths_are_malformed() {
        for reclen in [0_u16, 1, 19, 20, 4096, u16::MAX] {
            let mut bytes = rec(b"name");
            bytes[16..18].copy_from_slice(&reclen.to_ne_bytes());
            let result = frame_record(&bytes, 0);
            // reclen 20 frames a record whose name area holds only "n":
            // no NUL inside the record.
            assert_eq!(result, Err(Malformed), "reclen {reclen}");
        }
        // A 20-byte record whose single name byte is NUL: empty name.
        let mut empty = vec![0_u8; 20];
        empty[16..18].copy_from_slice(&20_u16.to_ne_bytes());
        assert_eq!(frame_record(&empty, 0), Err(Malformed));
        assert_eq!(frame_record(&[], usize::MAX), Err(Malformed));
    }

    #[test]
    fn nul_slash_and_empty_names_are_malformed() {
        let mut no_nul = rec(b"abcd");
        let len = no_nul.len();
        for byte in &mut no_nul[19..len] {
            *byte = b'a';
        }
        assert_eq!(frame_record(&no_nul, 0), Err(Malformed));
        assert_eq!(frame_record(&rec(b"a/b"), 0), Err(Malformed));
        assert_eq!(frame_record(&rec(b"/"), 0), Err(Malformed));
        // The NUL must be inside this record: a NUL in the next record does
        // not terminate this one.
        let mut joined = rec(b"abcd");
        let first_len = joined.len();
        for byte in &mut joined[19..first_len] {
            *byte = b'a';
        }
        joined.extend_from_slice(&rec(b"z"));
        assert_eq!(frame_record(&joined, 0), Err(Malformed));
    }

    #[test]
    fn padding_after_the_first_nul_is_ignored() {
        let mut bytes = rec(b"ab");
        let len = bytes.len();
        bytes[22..len].copy_from_slice(&b"SECRET-CANARY"[..len - 22]);
        assert_eq!(frame_record(&bytes, 0), Ok((&b"ab"[..], len)));
        let run = drive(vec![Step::Bytes(bytes)], max());
        assert_eq!(names(&run.store), vec![b"ab".to_vec()]);
    }

    #[test]
    fn unaligned_records_and_slices_are_framed_safely() {
        // reclen 21 (not a multiple of 8): a second record starts at 21.
        let mut first = vec![0_u8; 21];
        first[16..18].copy_from_slice(&21_u16.to_ne_bytes());
        first[19] = b'q';
        let mut bytes = first.clone();
        bytes.extend_from_slice(&rec(b"r"));
        assert_eq!(frame_record(&bytes, 0), Ok((&b"q"[..], 21)));
        assert_eq!(frame_record(&bytes, 21), Ok((&b"r"[..], 21 + 24)));
        // A sub-slice starting at an odd address.
        let mut shifted = vec![0xAA_u8];
        shifted.extend_from_slice(&bytes);
        assert_eq!(frame_record(&shifted[1..], 0), Ok((&b"q"[..], 21)));
    }

    #[test]
    fn malformed_tail_discards_the_valid_prefix() {
        let mut bytes = batch(&[b"good-one", b"good-two"]);
        bytes.extend_from_slice(&[0_u8; 19]);
        let run = drive(vec![Step::Bytes(bytes)], max());
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::MalformedDirectoryData
        );
    }

    #[test]
    fn malformed_later_batch_discards_earlier_batches() {
        let mut bad = rec(b"x");
        bad[16..18].copy_from_slice(&3_u16.to_ne_bytes());
        let run = drive(
            vec![Step::Bytes(batch(&[b"a", b"b"])), Step::Bytes(bad)],
            max(),
        );
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::MalformedDirectoryData
        );
    }

    #[test]
    fn overlong_count_is_malformed() {
        let run = drive(vec![Step::Count(READ_BUFFER_BYTES + 1)], max());
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::MalformedDirectoryData
        );
    }

    #[test]
    fn full_buffer_record_is_accepted() {
        // One record spanning the whole 8192-byte buffer (long padding).
        let mut bytes = vec![0_u8; READ_BUFFER_BYTES];
        bytes[16..18].copy_from_slice(&(READ_BUFFER_BYTES as u16).to_ne_bytes());
        bytes[19] = b'm';
        let run = drive(vec![Step::Bytes(bytes)], max());
        assert_eq!(names(&run.store), vec![b"m".to_vec()]);
    }

    // -- names --------------------------------------------------------------

    #[test]
    fn name_policy_rejects_and_counts() {
        let long_ok = vec![b'a'; 255];
        let long_bad = vec![b'b'; 256];
        let names_in: Vec<&[u8]> = vec![
            b".",
            b"..",
            b"...",
            &long_ok,
            &long_bad,
            b"\xff\xfe",
            b"ctl\x01",
            b"tab\tname",
            b"del\x7f",
            b"c:drive",
            b"back\\slash",
            "\u{85}next-line".as_bytes(),
            "caf\u{e9}".as_bytes(),
            b"ok",
        ];
        let run = drive(vec![Step::Bytes(batch(&names_in))], max());
        let scan = run.result.unwrap();
        assert_eq!(scan.counters.navigation_records, 2);
        assert_eq!(scan.counters.rejected_names, 8);
        assert_eq!(scan.counters.candidates, 4);
        assert_eq!(scan.counters.records_inspected, 14);
        let mut expected = vec![
            b"...".to_vec(),
            long_ok.clone(),
            "caf\u{e9}".as_bytes().to_vec(),
            b"ok".to_vec(),
        ];
        expected.sort();
        let mut got = names(&run.store);
        got.sort();
        assert_eq!(got, expected);
        // EOF with rejections is Partial.
        let status = release_status(scan.end, &scan.counters, false);
        assert_eq!(status.end(), StreamEnd::Eof);
        assert_eq!(status.coverage(), Coverage::Partial);
    }

    #[test]
    fn dots_alone_do_not_degrade_coverage() {
        let run = drive(vec![Step::Bytes(batch(&[b".", b"..", b"a"]))], max());
        assert_eq!(status(&run).coverage(), Coverage::Exhausted);
        assert_eq!(status(&run).end(), StreamEnd::Eof);
    }

    #[test]
    fn joined_path_limits_apply() {
        // Parent of 31 components: one more component is fine (32), but
        // the join must also fit 1024 bytes.
        let parent_text = vec!["d"; 31].join("/");
        let parent = RelativePath::new(&parent_text).unwrap();
        let mut model = Model::new(vec![Step::Bytes(batch(&[b"ok", &[b'x'; 255]]))]);
        let run = drive_with(&mut model, max(), &Cancellation::new(), Some(&parent));
        let scan = run.result.unwrap();
        assert_eq!(
            (scan.counters.candidates, scan.counters.rejected_names),
            (2, 0)
        );

        let deep_text = vec!["d"; 32].join("/");
        let deep = RelativePath::new(&deep_text).unwrap();
        let mut model = Model::new(vec![Step::Bytes(batch(&[b"ok"]))]);
        let run = drive_with(&mut model, max(), &Cancellation::new(), Some(&deep));
        let scan = run.result.unwrap();
        assert_eq!(
            (scan.counters.candidates, scan.counters.rejected_names),
            (0, 1)
        );

        let wide_text = vec!["p".repeat(255); 4].join("/");
        let wide = RelativePath::new(&wide_text).unwrap();
        assert_eq!(wide_text.len(), 1023);
        let mut model = Model::new(vec![Step::Bytes(batch(&[b"z"]))]);
        let run = drive_with(&mut model, max(), &Cancellation::new(), Some(&wide));
        // "…/z" is 1025 bytes: rejected.
        assert_eq!(run.result.unwrap().counters.rejected_names, 1);
    }

    #[test]
    fn duplicates_are_kept_and_sorted() {
        let run = drive(
            vec![
                Step::Bytes(batch(&[b"b", b"a", b"b"])),
                Step::Bytes(batch(&[b"a", b"c"])),
            ],
            max(),
        );
        let mut store = run.store;
        store.sort();
        assert_eq!(
            names(&store),
            vec![
                b"a".to_vec(),
                b"a".to_vec(),
                b"b".to_vec(),
                b"b".to_vec(),
                b"c".to_vec()
            ]
        );
    }

    // -- streams, EOF and errors --------------------------------------------

    #[test]
    fn eof_is_only_a_zero_byte_read() {
        let run = drive(
            vec![Step::Bytes(batch(&[b"a"])), Step::Bytes(Vec::new())],
            max(),
        );
        let status = status(&run);
        assert_eq!(status.end(), StreamEnd::Eof);
        assert_eq!(status.coverage(), Coverage::Exhausted);
        assert_eq!(run.reads, 2);
        // A fatal error after a valid batch is never EOF and discards it.
        let run = run_fatal_after_batch(EnumerationError::Access(AccessError::NotFound));
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::Access(AccessError::NotFound)
        );
    }

    fn run_fatal_after_batch(error: EnumerationError) -> Run {
        drive(
            vec![Step::Bytes(batch(&[b"a", b"b"])), Step::Fatal(error)],
            max(),
        )
    }

    #[test]
    fn interrupts_are_bounded_and_counted() {
        let ok = drive(
            vec![Step::Eintr, Step::Eintr, Step::Bytes(batch(&[b"a"]))],
            max(),
        );
        let scan = ok.result.unwrap();
        assert_eq!(scan.counters.read_attempts, 4); // 2 EINTR, batch, EOF
        assert_eq!(ok.checks, 4, "a check after every attempt");

        let exhausted = drive(vec![Step::Eintr, Step::Eintr, Step::Eintr], max());
        assert_eq!(
            exhausted.result.unwrap_err(),
            EnumerationError::Access(AccessError::RetryExhausted)
        );
        assert_eq!(exhausted.reads, 3);

        // The counter is consecutive: a batch in between resets it.
        let reset = drive(
            vec![
                Step::Eintr,
                Step::Eintr,
                Step::Bytes(batch(&[b"a"])),
                Step::Eintr,
                Step::Eintr,
            ],
            max(),
        );
        assert!(reset.result.is_ok());
    }

    #[test]
    fn binding_failure_after_any_attempt_discards_everything() {
        for at in 1..=3 {
            let mut model = Model::new(vec![
                Step::Bytes(batch(&[b"a"])),
                Step::Bytes(batch(&[b"b"])),
            ]);
            model.fail_check_at = Some(at);
            let run = drive_with(&mut model, max(), &Cancellation::new(), None);
            assert_eq!(
                run.result.unwrap_err(),
                EnumerationError::DirectoryBindingChanged,
                "check after read {at}"
            );
        }
    }

    // -- budgets ------------------------------------------------------------

    #[test]
    fn budget_construction_is_validated() {
        assert!(EnumerationBudget::new(4096, 512, 65_536, 64).is_ok());
        assert_eq!(EnumerationBudget::maximum(), EnumerationBudget::default());
        for (r, c, b, a) in [
            (0, 1, 1, 1),
            (1, 0, 1, 1),
            (1, 1, 0, 1),
            (1, 1, 1, 0),
            (4097, 1, 1, 1),
            (4096, 513, 1, 1),
            (4096, 1, 65_537, 1),
            (4096, 1, 1, 65),
            (2, 3, 1, 1),
            (usize::MAX, 1, 1, 1),
            (1, 1, usize::MAX, 1),
        ] {
            assert_eq!(
                EnumerationBudget::new(r, c, b, a),
                Err(EnumerationError::InvalidBudget),
                "{r} {c} {b} {a}"
            );
        }
    }

    #[test]
    fn exact_candidate_limit_is_partial_without_an_eof_probe() {
        let budget = EnumerationBudget::new(4096, 3, 65_536, 64).unwrap();
        let run = drive(vec![Step::Bytes(batch(&[b"a", b"b", b"c"]))], budget);
        let status = status(&run);
        assert_eq!(status.end(), StreamEnd::Budget(BudgetKind::Candidates));
        assert_eq!(status.coverage(), Coverage::Partial);
        assert_eq!(run.reads, 1, "no extra read to discover EOF");
    }

    #[test]
    fn inspected_record_limit_stops_before_the_next_record() {
        let budget = EnumerationBudget::new(2, 2, 65_536, 64).unwrap();
        let run = drive(vec![Step::Bytes(batch(&[b".", b"a", b"b"]))], budget);
        let scan = run.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::InspectedRecords));
        assert_eq!(scan.counters.records_inspected, 2);
        assert_eq!(names(&run.store), vec![b"a".to_vec()]);
    }

    #[test]
    fn name_byte_limit_charges_the_record_and_stores_nothing() {
        let budget = EnumerationBudget::new(4096, 512, 5, 64).unwrap();
        let run = drive(vec![Step::Bytes(batch(&[b"abc", b"defg", b"h"]))], budget);
        let scan = run.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::NameBytes));
        assert_eq!(scan.counters.records_inspected, 2);
        assert_eq!(names(&run.store), vec![b"abc".to_vec()]);
        // Exactly full: stops before the next record.
        let budget = EnumerationBudget::new(4096, 512, 3, 64).unwrap();
        let run2 = drive(vec![Step::Bytes(batch(&[b"abc", b"d"]))], budget);
        let scan = run2.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::NameBytes));
        assert_eq!(scan.counters.records_inspected, 1);
    }

    #[test]
    fn attempt_limit_guards_the_next_read_not_the_last_batch() {
        let budget = EnumerationBudget::new(4096, 512, 65_536, 2).unwrap();
        let run = drive(
            vec![
                Step::Bytes(batch(&[b"a"])),
                Step::Bytes(batch(&[b"b", b"c"])),
            ],
            budget,
        );
        let scan = run.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::ReadAttempts));
        assert_eq!(scan.counters.candidates, 3, "last permitted batch parsed");
        assert_eq!(run.reads, 2);
    }

    #[test]
    fn tie_order_prefers_records_then_candidates_then_bytes() {
        // All three reached by the same record.
        let budget = EnumerationBudget::new(1, 1, 1, 64).unwrap();
        let run = drive(vec![Step::Bytes(batch(&[b"a"]))], budget);
        assert_eq!(
            run.result.unwrap().end,
            StreamEnd::Budget(BudgetKind::InspectedRecords)
        );
        let budget = EnumerationBudget::new(2, 1, 1, 64).unwrap();
        let run = drive(vec![Step::Bytes(batch(&[b"a"]))], budget);
        assert_eq!(
            run.result.unwrap().end,
            StreamEnd::Budget(BudgetKind::Candidates)
        );
    }

    #[test]
    fn maximum_scan_is_4160_steps() {
        // 64 batches of 64 navigation records: 64 attempts + 4096 records,
        // never a candidate, and no EOF read.
        let nav: Vec<&[u8]> = (0..64)
            .map(|i| if i % 2 == 0 { &b"."[..] } else { &b".."[..] })
            .collect();
        let steps = (0..64).map(|_| Step::Bytes(batch(&nav))).collect();
        let run = drive(steps, max());
        let scan = run.result.unwrap();
        assert_eq!(scan.counters.read_attempts, 64);
        assert_eq!(scan.counters.records_inspected, 4096);
        assert_eq!(
            scan.counters.read_attempts + scan.counters.records_inspected,
            MAX_SCAN_STEPS
        );
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::InspectedRecords));
        assert_eq!(run.reads, 64);
    }

    #[test]
    fn repeated_batches_terminate_within_the_attempt_budget() {
        let steps = (0..200).map(|_| Step::Bytes(batch(&[b"."]))).collect();
        let run = drive(steps, max());
        let scan = run.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Budget(BudgetKind::ReadAttempts));
        assert_eq!(run.reads, 64);
    }

    // -- cancellation ---------------------------------------------------------

    #[test]
    fn cancellation_before_the_first_read_is_a_refusal() {
        let cancel = Cancellation::new();
        cancel.request();
        let mut model = Model::new(vec![Step::Bytes(batch(&[b"a"]))]);
        let run = drive_with(&mut model, max(), &cancel, None);
        assert_eq!(run.result.unwrap_err(), EnumerationError::Cancelled);
        assert_eq!(run.reads, 0);
    }

    #[test]
    fn cancellation_mid_scan_is_partial_with_what_was_kept() {
        let cancel = Cancellation::new();
        let mut model = Model::new(vec![
            Step::Bytes(batch(&[b"a", b"b"])),
            Step::Bytes(batch(&[b"c"])),
        ]);
        model.cancel_after = Some((1, &cancel));
        let run = drive_with(&mut model, max(), &cancel, None);
        let scan = run.result.unwrap();
        // Polled before the first record of the batch.
        assert_eq!(scan.end, StreamEnd::Cancelled);
        assert_eq!(scan.counters.records_inspected, 0);
        assert_eq!(run.reads, 1);
        let status = release_status(scan.end, &scan.counters, true);
        assert_eq!(status.coverage(), Coverage::Partial);
    }

    #[test]
    fn cancellation_at_release_wins_over_eof_and_budget() {
        let run = drive(vec![Step::Bytes(batch(&[b"a"]))], max());
        let scan = run.result.unwrap();
        assert_eq!(scan.end, StreamEnd::Eof);
        let status = release_status(scan.end, &scan.counters, true);
        assert_eq!(status.end(), StreamEnd::Cancelled);
        assert_eq!(status.coverage(), Coverage::Partial);
        let status = release_status(
            StreamEnd::Budget(BudgetKind::Candidates),
            &scan.counters,
            true,
        );
        assert_eq!(status.end(), StreamEnd::Cancelled);
    }

    #[test]
    fn fatal_error_wins_over_cancellation() {
        let cancel = Cancellation::new();
        let mut model = Model::new(vec![Step::Fatal(EnumerationError::Access(
            AccessError::Inaccessible,
        ))]);
        model.cancel_after = Some((1, &cancel));
        let run = drive_with(&mut model, max(), &cancel, None);
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::Access(AccessError::Inaccessible)
        );
        // F19-1: the fatal attempt is still followed by its binding check.
        assert_eq!((run.reads, run.checks), (1, 1));
    }

    // -- F19-1: one binding check after every attempt, fatal ones included ----

    const EIO: EnumerationError = EnumerationError::Access(AccessError::UnexpectedOs(5));
    const EAGAIN: EnumerationError = EnumerationError::Access(AccessError::UnexpectedOs(11));
    const ENOENT: EnumerationError = EnumerationError::Access(AccessError::NotFound);

    #[test]
    fn fatal_first_attempt_is_checked_exactly_once() {
        // EIO, ENOENT, EAGAIN (terminal, never retried) and the overlong-count
        // refusal as `KernelSource::read` maps them.
        for error in [
            EIO,
            ENOENT,
            EAGAIN,
            EnumerationError::MalformedDirectoryData,
        ] {
            let run = drive(vec![Step::Fatal(error)], max());
            assert_eq!(run.result.unwrap_err(), error, "{error:?}");
            assert_eq!((run.reads, run.checks), (1, 1), "{error:?}");
            assert_eq!(run.store.len(), 0);
        }
    }

    #[test]
    fn overlong_count_is_checked_before_refusal() {
        let run = drive(vec![Step::Count(READ_BUFFER_BYTES + 1)], max());
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::MalformedDirectoryData
        );
        assert_eq!((run.reads, run.checks), (1, 1));
    }

    #[test]
    fn fatal_second_read_after_a_valid_batch_is_checked_and_releases_nothing() {
        let run = drive(
            vec![Step::Bytes(batch(&[b"a", b"b"])), Step::Fatal(EIO)],
            max(),
        );
        assert_eq!(run.result.unwrap_err(), EIO);
        assert_eq!((run.reads, run.checks), (2, 2));
    }

    #[test]
    fn fatal_read_error_takes_precedence_over_a_failing_check() {
        for error in [EIO, ENOENT, EAGAIN] {
            let mut model = Model::new(vec![Step::Fatal(error)]);
            model.fail_check_at = Some(1);
            let run = drive_with(&mut model, max(), &Cancellation::new(), None);
            assert_eq!(run.result.unwrap_err(), error, "{error:?}");
            assert_eq!((run.reads, run.checks), (1, 1), "check still executed");
        }
        // Later attempt.
        let mut model = Model::new(vec![Step::Bytes(batch(&[b"a"])), Step::Fatal(EIO)]);
        model.fail_check_at = Some(2);
        let run = drive_with(&mut model, max(), &Cancellation::new(), None);
        assert_eq!(run.result.unwrap_err(), EIO);
        assert_eq!((run.reads, run.checks), (2, 2));
    }

    #[test]
    fn successful_read_with_failing_check_refuses_immediately() {
        let mut model = Model::new(vec![
            Step::Bytes(batch(&[b"a"])),
            Step::Bytes(batch(&[b"b"])),
        ]);
        model.fail_check_at = Some(1);
        let run = drive_with(&mut model, max(), &Cancellation::new(), None);
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::DirectoryBindingChanged
        );
        assert_eq!((run.reads, run.checks), (1, 1), "no further attempt");
    }

    #[test]
    fn interrupts_then_batch_keep_one_check_per_attempt() {
        let run = drive(
            vec![Step::Eintr, Step::Eintr, Step::Bytes(batch(&[b"a"]))],
            max(),
        );
        let scan = run.result.unwrap();
        assert_eq!(scan.counters.read_attempts, 4);
        assert_eq!((run.reads, run.checks), (4, 4));
        assert_eq!(scan.end, StreamEnd::Eof);
        // Three interrupts: fatal after its own check.
        let run = drive(vec![Step::Eintr, Step::Eintr, Step::Eintr], max());
        assert_eq!(
            run.result.unwrap_err(),
            EnumerationError::Access(AccessError::RetryExhausted)
        );
        assert_eq!((run.reads, run.checks), (3, 3));
    }

    #[test]
    fn eof_attempt_is_checked_once() {
        let run = drive(vec![Step::Bytes(Vec::new())], max());
        let scan = run.result.unwrap();
        assert_eq!((run.reads, run.checks), (1, 1));
        let status = release_status(scan.end, &scan.counters, false);
        assert_eq!(status.end(), StreamEnd::Eof);
        assert_eq!(status.coverage(), Coverage::Exhausted);
    }

    #[test]
    fn cancellation_never_turns_a_fatal_read_into_a_partial_listing() {
        // Cancellation requested during a valid first read, then fatal second
        // read: the scan stops as Cancelled before the second read. With the
        // cancellation during a fatal read, the fatal error wins.
        for (steps, expected) in [
            (vec![Step::Fatal(EIO)], Err(EIO)),
            (
                vec![Step::Bytes(batch(&[b"a"])), Step::Fatal(EIO)],
                Ok(StreamEnd::Cancelled),
            ),
        ] {
            let cancel = Cancellation::new();
            let mut model = Model::new(steps);
            model.cancel_after = Some((1, &cancel));
            let run = drive_with(&mut model, max(), &cancel, None);
            assert_eq!(run.result.map(|scan| scan.end), expected);
            assert_eq!(run.reads, run.checks);
        }
    }

    #[test]
    fn every_attempt_in_mixed_scripts_has_exactly_one_check() {
        let scripts = vec![
            vec![Step::Eintr, Step::Fatal(EIO)],
            vec![
                Step::Bytes(batch(&[b"a"])),
                Step::Eintr,
                Step::Fatal(ENOENT),
            ],
            vec![Step::Bytes(batch(&[b"a"])), Step::Bytes(Vec::new())],
            vec![
                Step::Eintr,
                Step::Bytes(batch(&[b"."])),
                Step::Fatal(EAGAIN),
            ],
            (0..70).map(|_| Step::Bytes(batch(&[b"."]))).collect(),
        ];
        for steps in scripts {
            let run = drive(steps, max());
            assert_eq!(run.reads, run.checks, "{:?}", run.result);
            assert!(run.reads <= MAX_READ_ATTEMPTS);
        }
    }

    // -- storage ------------------------------------------------------------

    #[test]
    fn storage_is_reserved_once_and_never_grows() {
        let mut model = Model::new(
            (0..8)
                .map(|batch_index| {
                    let names: Vec<Vec<u8>> = (0..64)
                        .map(|i| {
                            format!("{batch_index:02}-{i:02}-{}", "n".repeat(120)).into_bytes()
                        })
                        .collect();
                    let refs: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
                    let bytes = batch(&refs[..48]);
                    Step::Bytes(bytes)
                })
                .collect(),
        );
        let budget = max();
        let mut store = Store::reserve(&budget, 0, false).unwrap();
        let before = store.capacities();
        assert!(before.0 >= MAX_RETAINED_NAME_BYTES && before.1 >= MAX_CANDIDATES);
        let mut scratch = JoinScratch::new(None);
        let scan = scan(
            &mut model,
            &budget,
            &Cancellation::new(),
            &mut store,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(store.capacities(), before);
        assert_eq!(scan.counters.candidates, 384);
        let comparisons = store.sort();
        assert!(comparisons <= 384 * 383 / 2);
        assert_eq!(store.capacities(), before);
        let ledger = store.ledger(0).unwrap();
        assert_eq!(ledger.arena_capacity, before.0);
        assert_eq!(ledger.slots_capacity_bytes, before.1 * 8);
        assert!(ledger.total <= MAX_RETAINED_OUTPUT_BYTES);
    }

    #[test]
    fn worst_case_sort_is_bounded() {
        let names: Vec<Vec<u8>> = (0..512)
            .rev()
            .map(|i| format!("{i:04}").into_bytes())
            .collect();
        let budget = max();
        let mut store = Store::reserve(&budget, 0, false).unwrap();
        for name in &names {
            store.push(name).unwrap();
        }
        assert_eq!(store.sort(), 512 * 511 / 2);
        let sorted = (0..512)
            .map(|i| store.name(i).unwrap().to_vec())
            .collect::<Vec<_>>();
        let mut expected = names.clone();
        expected.sort();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn reservation_refusals() {
        assert_eq!(
            Store::reserve(&max(), 0, true).unwrap_err(),
            EnumerationError::AllocationRefused
        );
        // A fixed part that pushes the ledger above the ceiling.
        assert_eq!(
            Store::reserve(&max(), MAX_RETAINED_OUTPUT_BYTES, false).unwrap_err(),
            EnumerationError::AllocationRefused
        );
        assert_eq!(
            Store::reserve(&max(), usize::MAX, false).unwrap_err(),
            EnumerationError::AllocationRefused
        );
        // Actual standard-library capacity refusal.
        let mut huge: Vec<u8> = Vec::new();
        assert!(huge.try_reserve_exact(usize::MAX).is_err());
    }

    #[test]
    fn push_beyond_the_reservation_is_refused_not_grown() {
        let budget = EnumerationBudget::new(4096, 2, 4, 64).unwrap();
        let mut store = Store::reserve(&budget, 0, false).unwrap();
        let before = store.capacities();
        store.push(b"ab").unwrap();
        store.push(b"cd").unwrap();
        assert_eq!(
            store.push(b"e").unwrap_err(),
            EnumerationError::AllocationRefused
        );
        assert_eq!(store.capacities(), before);
        assert_eq!(store.name(2), None);
        assert_eq!(store.name(usize::MAX), None);
    }

    #[test]
    fn debug_output_never_contains_names() {
        let run = drive(vec![Step::Bytes(batch(&[b"SECRET-NAME-CANARY"]))], max());
        let text = format!("{:?} {:?}", run.store, run.result);
        assert!(!text.contains("SECRET"), "{text}");
        let scratch = JoinScratch::new(Some(&RelativePath::new("SECRET-DIR").unwrap()));
        assert!(!format!("{scratch:?}").contains("SECRET"));
    }
}
