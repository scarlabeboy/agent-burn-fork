//! ABF28 — one-shot bounded **synthetic research** read transaction, in safe
//! code over the `sys::linux_x86_64::research` ABI primitives.
//!
//! Compiled **only** in the deliberate two-key research build (feature
//! `fault-injection` **and** `--cfg abf_secure_fs_harness`) on the supported
//! target. Default and feature-only builds contain no content reader. The
//! two keys are a research access boundary: they do **not** prove that a
//! file holds synthetic bytes and are **not** user consent for real files.
//!
//! Authority (research policy A, ABF25): one transaction consumes one
//! already-promoted [`Readable`] (the held-object authority of its pin,
//! A23-2: the object may already be outside the root, before or after
//! promotion) and one opaque one-use [`ResearchReadPermit`]. Nothing here
//! checks or claims root membership, lineage, origin or immutable content.
//!
//! Ordered state machine (ABF25 `bounded-read-contract.md`):
//!
//! 1. entry: budget (validated by construction), cancellation, both entry
//!    samples (`fstat` of the original `O_PATH` handle, then of the readable
//!    handle), then the one fixed stage allocation;
//! 2. before each attempt: cancellation, budget credits, a positive request
//!    `min(request, remaining capture, remaining requested bytes)`, checked
//!    signed offset; attempt and requested bytes are charged **before** the
//!    system call and never refunded, and only while the post pair and the
//!    final pair of metadata credits stay reserved;
//! 3. after each returned attempt: the raw result is kept, **both** raw post
//!    samples are taken (the second even if the first failed), then the
//!    precedence below is applied;
//! 4. success (end of file or budget): both final samples, final
//!    cancellation, close both descriptors, then the **same** stage moves
//!    into the result.
//!
//! Every metadata pair debits its two `fstat` credits **before** the first
//! call. All counters use checked arithmetic; an accounting failure refuses
//! (no byte) instead of wrapping or saturating. Credits are charges, not a
//! record of completed calls: a panic between two actions can leave a credit
//! charged for a call that never ran.
//!
//! Per-attempt precedence: a kernel contract violation detected in the read
//! **or** in either post sample; fatal read error (including terminal
//! `EAGAIN`); ordinary post-sample refusal (original owner first, then
//! readable, then the change detector); `EINTR` retry exhaustion;
//! cancellation; budget stop; ordinary progress or end of file. The entry
//! and final pairs also rank a contract violation of either sample before
//! any ordinary refusal.
//!
//! The change detector is `(size, mtime, ctime)` from both owners: equal at
//! entry, after every attempt and at release. atime is excluded. Equality
//! does **not** establish an atomic snapshot or immutable bytes.

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::ObjectId;
use crate::enumeration::Cancellation;
use crate::error::{AccessError, ObjectKind, ResearchReadError};
use crate::linux::classify_errno;
use crate::promotion::Readable;
use crate::sys::linux_x86_64::fault::{self, ReadEvent};
use crate::sys::linux_x86_64::research::{
    self as sys, PreadResult, ReadStage, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT,
    S_IFREG, S_IFSOCK, STAGE_BYTES, StatFacts, StatFailure,
};
use crate::sys::linux_x86_64::{EAGAIN, EINTR, Fd, ReadableFd};

// ---------------------------------------------------------------------------
// Hard ceilings
// ---------------------------------------------------------------------------

/// Largest positive `pread64` request.
pub const MAX_REQUEST_BYTES: usize = sys::MAX_PREAD_WINDOW;
/// Largest captured (and returned) prefix, including bytes later discarded.
pub const MAX_CAPTURE_BYTES: usize = 65_536;
/// Largest cumulative requested I/O, charged before each attempt.
pub const MAX_REQUESTED_BYTES: usize = 262_144;
/// Largest number of `pread64` attempts, including `EINTR`, errors and EOF.
pub const MAX_PREAD_ATTEMPTS: usize = 64;
/// At most this many consecutive `EINTR` retry grants.
pub const MAX_CONSECUTIVE_EINTR_RETRIES: usize = 2;
/// At most this many `EINTR` retry grants per transaction.
pub const MAX_TOTAL_EINTR_RETRIES: usize = 8;
/// Direct `fstat` attempts: 2 at entry, 2 after each returned attempt, 2
/// final.
pub const MAX_FSTAT_ATTEMPTS: usize = 2 + 2 * MAX_PREAD_ATTEMPTS + 2;
/// Capacity of the single fixed staging allocation (charged in full even
/// for a smaller capture budget).
pub const STAGE_CAPACITY_BYTES: usize = STAGE_BYTES;
/// Ceiling of the explicit inline/control storage of one transaction.
pub const MAX_CONTROL_BYTES: usize = 4096;
/// Ceiling of retained payload plus control.
pub const MAX_RETAINED_BYTES: usize = 69_632;
/// Descriptors the transaction holds: the two already-promoted owners. It
/// opens and duplicates none.
pub const HELD_DESCRIPTORS: usize = 2;

const _: () = {
    assert!(MAX_REQUESTED_BYTES == MAX_PREAD_ATTEMPTS * MAX_REQUEST_BYTES);
    assert!(MAX_FSTAT_ATTEMPTS == 132);
    assert!(STAGE_CAPACITY_BYTES == MAX_CAPTURE_BYTES);
    assert!(MAX_RETAINED_BYTES == STAGE_CAPACITY_BYTES + MAX_CONTROL_BYTES);
    assert!(MAX_REQUEST_BYTES <= MAX_CAPTURE_BYTES);
    // Offsets stay far inside the signed 64-bit range.
    assert!((MAX_CAPTURE_BYTES as u64) < i64::MAX as u64);
};

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

/// Caller budget of one transaction. Every value is in `1..=` its hard
/// ceiling; a caller can only lower them. The stage capacity, the
/// descriptor count and the metadata allowance are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadBudget {
    request_bytes: usize,
    capture_bytes: usize,
    requested_bytes: usize,
    attempts: usize,
    consecutive_eintr: usize,
    total_eintr: usize,
}

impl ReadBudget {
    /// All hard ceilings.
    pub const fn maximum() -> Self {
        Self {
            request_bytes: MAX_REQUEST_BYTES,
            capture_bytes: MAX_CAPTURE_BYTES,
            requested_bytes: MAX_REQUESTED_BYTES,
            attempts: MAX_PREAD_ATTEMPTS,
            consecutive_eintr: MAX_CONSECUTIVE_EINTR_RETRIES,
            total_eintr: MAX_TOTAL_EINTR_RETRIES,
        }
    }

    /// Validates a reduced budget. [`ResearchReadError::InvalidBudget`] for
    /// a zero or above-ceiling value.
    pub fn new(
        request_bytes: usize,
        capture_bytes: usize,
        requested_bytes: usize,
        attempts: usize,
        consecutive_eintr_retries: usize,
        total_eintr_retries: usize,
    ) -> Result<Self, ResearchReadError> {
        let budget = Self {
            request_bytes,
            capture_bytes,
            requested_bytes,
            attempts,
            consecutive_eintr: consecutive_eintr_retries,
            total_eintr: total_eintr_retries,
        };
        budget.validate()?;
        Ok(budget)
    }

    fn validate(&self) -> Result<(), ResearchReadError> {
        let within = |value: usize, max: usize| (1..=max).contains(&value);
        if within(self.request_bytes, MAX_REQUEST_BYTES)
            && within(self.capture_bytes, MAX_CAPTURE_BYTES)
            && within(self.requested_bytes, MAX_REQUESTED_BYTES)
            && within(self.attempts, MAX_PREAD_ATTEMPTS)
            && within(self.consecutive_eintr, MAX_CONSECUTIVE_EINTR_RETRIES)
            && within(self.total_eintr, MAX_TOTAL_EINTR_RETRIES)
        {
            Ok(())
        } else {
            Err(ResearchReadError::InvalidBudget)
        }
    }

    pub fn request_bytes(&self) -> usize {
        self.request_bytes
    }

    pub fn capture_bytes(&self) -> usize {
        self.capture_bytes
    }

    pub fn requested_bytes(&self) -> usize {
        self.requested_bytes
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn consecutive_eintr_retries(&self) -> usize {
        self.consecutive_eintr
    }

    pub fn total_eintr_retries(&self) -> usize {
        self.total_eintr
    }
}

impl Default for ReadBudget {
    fn default() -> Self {
        Self::maximum()
    }
}

// ---------------------------------------------------------------------------
// Permit
// ---------------------------------------------------------------------------

thread_local! {
    static PERMIT_OUTSTANDING: Cell<bool> = const { Cell::new(false) };
}

/// Opaque one-use research permit: authorization of **one** research read
/// operation and its resource envelope. Not `Clone`/`Copy`/`Default`, no
/// public constructor or field; it cannot be built from an [`ObjectId`],
/// an ordinal, a name or a guessed token. Minted only by the two-key
/// harness coordinator (`fault_injection::mint_research_read_permit`), at
/// most one outstanding per thread, and `!Send`/`!Sync` so it stays on the
/// minting thread. A transaction consumes it in every outcome.
///
/// It is **not** provider/user consent, environment attestation or proof
/// that a file holds synthetic bytes.
pub struct ResearchReadPermit {
    _thread: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for ResearchReadPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResearchReadPermit(..)")
    }
}

impl ResearchReadPermit {
    /// `None` while another permit minted on this thread is alive.
    pub(crate) fn mint() -> Option<Self> {
        if PERMIT_OUTSTANDING.with(|outstanding| outstanding.replace(true)) {
            return None;
        }
        Some(Self {
            _thread: PhantomData,
        })
    }
}

impl Drop for ResearchReadPermit {
    fn drop(&mut self) {
        PERMIT_OUTSTANDING.with(|outstanding| outstanding.set(false));
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// Which budget stopped the observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadBudgetKind {
    /// No `pread64` attempt credit was left.
    Attempts,
    /// No requested-byte credit was left.
    RequestedBytes,
}

/// Why a released observation stopped. There is no "complete" end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObservationEnd {
    /// A positive `pread64` request actually returned 0 bytes at the
    /// advanced offset. A sampled end-of-file event, not durable EOF, a
    /// whole-file snapshot or complete usage.
    ObservedEof,
    /// The capture budget was filled. Incomplete by definition, even if the
    /// file is exactly that long: no lookahead read is made.
    CaptureLimit,
    /// An attempt or requested-byte budget ran out first. Incomplete.
    BudgetStop(ReadBudgetKind),
}

/// Coherence of a released observation. The only value: no snapshot is
/// established, whatever the change detector showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Coherence {
    /// Mutable observation; snapshot not established. Positive reads can
    /// span different content moments even when the detector matched.
    MutableObservation,
}

/// Numeric accounting of one released observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCounters {
    pread_attempts: usize,
    eintr_retries: usize,
    requested_bytes: usize,
    captured_bytes: usize,
    fstat_attempts: usize,
}

impl ReadCounters {
    const fn zero() -> Self {
        Self {
            pread_attempts: 0,
            eintr_retries: 0,
            requested_bytes: 0,
            captured_bytes: 0,
            fstat_attempts: 0,
        }
    }

    /// `pread64` attempts, including interrupted and end-of-file ones
    /// (each charged before its call).
    pub fn pread_attempts(&self) -> usize {
        self.pread_attempts
    }

    /// `EINTR` retry grants used.
    pub fn eintr_retries(&self) -> usize {
        self.eintr_retries
    }

    /// Requested bytes charged (repeats at the same offset included).
    pub fn requested_bytes(&self) -> usize {
        self.requested_bytes
    }

    pub fn captured_bytes(&self) -> usize {
        self.captured_bytes
    }

    /// Direct `fstat` attempts (charged per pair before the first call; in
    /// a released observation every charged attempt ran).
    pub fn fstat_attempts(&self) -> usize {
        self.fstat_attempts
    }
}

/// A released bounded observation: the validated prefix (in the very stage
/// allocation the kernel wrote into), its end reason, the held object's
/// identity and counters. `Debug` never shows bytes. Dropping it zeroes the
/// stage (best effort) before freeing it.
///
/// It describes the **acquired object** at sampled moments only: no current
/// location, membership, lineage, origin or immutable content.
pub struct PrefixObservation {
    stage: ReadStage,
    len: usize,
    end: ObservationEnd,
    object: ObjectId,
    counters: ReadCounters,
}

impl std::fmt::Debug for PrefixObservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrefixObservation")
            .field("object", &self.object)
            .field("len", &self.len)
            .field("end", &self.end)
            .finish_non_exhaustive()
    }
}

impl PrefixObservation {
    /// The released prefix, starting at file offset 0.
    pub fn bytes(&self) -> &[u8] {
        &self.stage.bytes()[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn end(&self) -> ObservationEnd {
        self.end
    }

    pub fn coherence(&self) -> Coherence {
        Coherence::MutableObservation
    }

    /// Handle-derived identity of the observed (held) object.
    pub fn object_id(&self) -> ObjectId {
        self.object
    }

    pub fn counters(&self) -> ReadCounters {
        self.counters
    }
}

/// Explicit inline/control storage of one transaction: the result (with the
/// stage owner), the permit, the budget, the consumed readable owner, the
/// engine source and state, two retained samples and one kernel `stat`
/// buffer.
pub(crate) const CONTROL_BYTES: usize = std::mem::size_of::<PrefixObservation>()
    + std::mem::size_of::<ResearchReadPermit>()
    + std::mem::size_of::<ReadBudget>()
    + std::mem::size_of::<Readable>()
    + std::mem::size_of::<KernelSource<'static>>()
    + std::mem::size_of::<Loop>()
    + 2 * std::mem::size_of::<Tuple>()
    + 2 * std::mem::size_of::<Result<StatFacts, StatFailure>>()
    + 144;

const _: () = {
    assert!(CONTROL_BYTES <= MAX_CONTROL_BYTES);
    assert!(STAGE_CAPACITY_BYTES + CONTROL_BYTES <= MAX_RETAINED_BYTES);
};

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Which held owner a sample comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Original,
    Readable,
}

/// The OS side of a transaction (the kernel, or a model in unit tests).
pub(crate) trait Source {
    /// Exactly one `pread64` attempt into `window` at `offset`.
    fn pread(&mut self, window: &mut [u8], offset: i64) -> PreadResult;
    /// Exactly one direct `fstat` attempt of `owner`.
    fn fstat(&mut self, owner: Owner) -> Result<StatFacts, StatFailure>;
}

/// The single staging buffer the engine writes into.
pub(crate) trait Stage {
    fn buf(&mut self) -> &mut [u8; STAGE_BYTES];
}

impl Stage for ReadStage {
    fn buf(&mut self) -> &mut [u8; STAGE_BYTES] {
        self.bytes_mut()
    }
}

/// The change detector: size, mtime, ctime (validated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tuple {
    size: u64,
    mtime: (i64, u64),
    ctime: (i64, u64),
}

const NANOS_PER_SECOND: u64 = 1_000_000_000;

fn kind_of(mode: u32) -> Option<ObjectKind> {
    match mode & S_IFMT {
        S_IFREG => None,
        S_IFDIR => Some(ObjectKind::Directory),
        S_IFLNK => Some(ObjectKind::Symlink),
        S_IFIFO => Some(ObjectKind::Fifo),
        S_IFSOCK => Some(ObjectKind::Socket),
        S_IFCHR => Some(ObjectKind::CharDevice),
        S_IFBLK => Some(ObjectKind::BlockDevice),
        _ => Some(ObjectKind::Unknown),
    }
}

/// Held-owner policy on one sample, in the order of `Pinned::reinspect`:
/// errno, regular file, concrete identity, the expected identity, exactly
/// one link; then the detector fields must be valid.
pub(crate) fn admit(
    sample: Result<StatFacts, StatFailure>,
    expected: ObjectId,
) -> Result<Tuple, ResearchReadError> {
    let facts = match sample {
        Ok(facts) => facts,
        Err(StatFailure::Errno(errno)) => {
            return Err(ResearchReadError::Metadata(classify_errno(errno)));
        }
        Err(StatFailure::Contract) => return Err(ResearchReadError::KernelContractViolation),
    };
    if let Some(kind) = kind_of(facts.mode) {
        return Err(ResearchReadError::Metadata(AccessError::NotRegularFile(
            kind,
        )));
    }
    if facts.device == 0 || facts.inode == 0 {
        return Err(ResearchReadError::Metadata(
            AccessError::IdentityUnavailable,
        ));
    }
    if facts.device != expected.device() || facts.inode != expected.inode() {
        return Err(ResearchReadError::Metadata(AccessError::ObjectChanged));
    }
    match facts.nlink {
        1 => {}
        0 => return Err(ResearchReadError::Metadata(AccessError::ObjectChanged)),
        _ => return Err(ResearchReadError::Metadata(AccessError::MultipleLinks)),
    }
    let Ok(size) = u64::try_from(facts.size) else {
        return Err(ResearchReadError::InvalidMetadata);
    };
    if facts.mtime_nsec >= NANOS_PER_SECOND || facts.ctime_nsec >= NANOS_PER_SECOND {
        return Err(ResearchReadError::InvalidMetadata);
    }
    Ok(Tuple {
        size,
        mtime: (facts.mtime_sec, facts.mtime_nsec),
        ctime: (facts.ctime_sec, facts.ctime_nsec),
    })
}

/// Classification of one returned attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadClass {
    Progress(usize),
    Eof,
    Interrupted,
    WouldBlock,
    Fatal(i32),
    Contract,
}

/// Validates the raw result against the positive `request` before any
/// slicing: only `-1` with a positive errno is an error; a negative value
/// other than `-1`, `-1` without errno, or a count above the request is a
/// contract violation (never clamped); `0` is end of file.
pub(crate) fn classify_read(raw: PreadResult, request: usize) -> ReadClass {
    match raw {
        PreadResult::Errno(EINTR) => ReadClass::Interrupted,
        PreadResult::Errno(EAGAIN) => ReadClass::WouldBlock,
        PreadResult::Errno(errno) if errno <= 0 => ReadClass::Contract,
        PreadResult::Errno(errno) => ReadClass::Fatal(errno),
        PreadResult::Returned(0) => ReadClass::Eof,
        PreadResult::Returned(count) => match usize::try_from(count) {
            Ok(count) if count <= request => ReadClass::Progress(count),
            _ => ReadClass::Contract,
        },
    }
}

/// The next positive request and its checked signed offset, or the budget
/// that ends the observation. `filled` is the validated prefix length.
pub(crate) fn next_request(
    budget: &ReadBudget,
    filled: usize,
    attempts: usize,
    requested: usize,
) -> Result<Result<(usize, i64), ObservationEnd>, ResearchReadError> {
    let remaining_capture = budget
        .capture_bytes
        .checked_sub(filled)
        .ok_or(ResearchReadError::InvalidBudget)?;
    if remaining_capture == 0 {
        return Ok(Err(ObservationEnd::CaptureLimit));
    }
    if attempts >= budget.attempts {
        return Ok(Err(ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)));
    }
    let remaining_requested = budget
        .requested_bytes
        .checked_sub(requested)
        .ok_or(ResearchReadError::InvalidBudget)?;
    if remaining_requested == 0 {
        return Ok(Err(ObservationEnd::BudgetStop(
            ReadBudgetKind::RequestedBytes,
        )));
    }
    let request = budget
        .request_bytes
        .min(remaining_capture)
        .min(remaining_requested);
    let offset = checked_offset(filled, request).ok_or(ResearchReadError::InvalidBudget)?;
    Ok(Ok((request, offset)))
}

/// `offset` as a signed file offset such that `offset + request` still fits
/// `i64` and the window ends inside the stage.
pub(crate) fn checked_offset(offset: usize, request: usize) -> Option<i64> {
    if request == 0 || request > MAX_REQUEST_BYTES {
        return None;
    }
    let end = offset.checked_add(request)?;
    if end > STAGE_BYTES {
        return None;
    }
    let signed = i64::try_from(offset).ok()?;
    signed.checked_add(i64::try_from(request).ok()?)?;
    Some(signed)
}

/// `fstat` credits of one metadata pair (both owners).
const PAIR_CREDITS: usize = 2;

/// Mutable loop state (inline control storage). Every update is checked and
/// all-or-nothing: on an accounting failure nothing is changed and the
/// transaction refuses with [`ResearchReadError::InvalidBudget`].
#[derive(Debug)]
pub(crate) struct Loop {
    filled: usize,
    consecutive_eintr: usize,
    total_eintr: usize,
    counters: ReadCounters,
}

impl Loop {
    pub(crate) const fn new() -> Self {
        Self {
            filled: 0,
            consecutive_eintr: 0,
            total_eintr: 0,
            counters: ReadCounters::zero(),
        }
    }

    /// Debits one attempt and `request` requested bytes **before** the
    /// system call. Refused (nothing debited) unless the request is within
    /// its budget, both cumulative credits fit, and the post pair **plus**
    /// the reserved final pair of metadata credits still fit. Returns the
    /// attempt ordinal.
    fn charge_attempt(
        &mut self,
        budget: &ReadBudget,
        request: usize,
    ) -> Result<usize, ResearchReadError> {
        let attempts = self
            .counters
            .pread_attempts
            .checked_add(1)
            .filter(|&attempts| attempts <= budget.attempts);
        let requested = self
            .counters
            .requested_bytes
            .checked_add(request)
            .filter(|&requested| requested <= budget.requested_bytes);
        let reserve = PAIR_CREDITS
            .checked_mul(2)
            .and_then(|reserve| self.counters.fstat_attempts.checked_add(reserve))
            .filter(|&reserved| reserved <= MAX_FSTAT_ATTEMPTS);
        match (attempts, requested, reserve) {
            (Some(attempts), Some(requested), Some(_))
                if (1..=budget.request_bytes).contains(&request) =>
            {
                self.counters.pread_attempts = attempts;
                self.counters.requested_bytes = requested;
                Ok(attempts)
            }
            _ => Err(ResearchReadError::InvalidBudget),
        }
    }

    /// Debits the two `fstat` credits of one pair **before** its first call
    /// (never refunded). Returns the charged total.
    fn charge_metadata_pair(&mut self) -> Result<usize, ResearchReadError> {
        let charged = self
            .counters
            .fstat_attempts
            .checked_add(PAIR_CREDITS)
            .filter(|&charged| charged <= MAX_FSTAT_ATTEMPTS)
            .ok_or(ResearchReadError::InvalidBudget)?;
        self.counters.fstat_attempts = charged;
        Ok(charged)
    }

    /// Counts one returned `EINTR`; `true` when it exceeds the consecutive
    /// or total grant budget (`RetryExhausted`).
    fn count_interrupt(&mut self, budget: &ReadBudget) -> Result<bool, ResearchReadError> {
        let (Some(consecutive), Some(total)) = (
            self.consecutive_eintr.checked_add(1),
            self.total_eintr.checked_add(1),
        ) else {
            return Err(ResearchReadError::InvalidBudget);
        };
        self.consecutive_eintr = consecutive;
        self.total_eintr = total;
        Ok(consecutive > budget.consecutive_eintr || total > budget.total_eintr)
    }

    /// Records one granted `EINTR` retry before the retried attempt.
    fn grant_retry(&mut self) -> Result<(), ResearchReadError> {
        self.counters.eintr_retries = self
            .counters
            .eintr_retries
            .checked_add(1)
            .ok_or(ResearchReadError::InvalidBudget)?;
        Ok(())
    }

    /// Advances the validated prefix by a positive count within the capture
    /// budget and ends the run of consecutive interrupts.
    fn advance(&mut self, budget: &ReadBudget, count: usize) -> Result<(), ResearchReadError> {
        self.filled = self
            .filled
            .checked_add(count)
            .filter(|&filled| filled <= budget.capture_bytes)
            .ok_or(ResearchReadError::InvalidBudget)?;
        self.consecutive_eintr = 0;
        Ok(())
    }
}

/// What the scan observed after its final samples and cancellation poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scan {
    pub(crate) len: usize,
    pub(crate) end: ObservationEnd,
    pub(crate) counters: ReadCounters,
}

/// Both raw owner samples of one pair, before any policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PairSample {
    original: Result<StatFacts, StatFailure>,
    readable: Result<StatFacts, StatFailure>,
}

/// Both owner samples, unconditionally (the second even when the first
/// failed). The two `fstat` credits are debited and persisted **before** the
/// first call. No policy is applied here: see [`judge_pair`].
fn sample_pair<S: Source>(
    source: &mut S,
    state: &mut Loop,
) -> Result<PairSample, ResearchReadError> {
    let charged = state.charge_metadata_pair()?;
    let original = source.fstat(Owner::Original);
    fault::read_event(ReadEvent::AfterMetadata(event_count(
        charged.saturating_sub(1),
    )));
    let readable = source.fstat(Owner::Readable);
    fault::read_event(ReadEvent::AfterMetadata(event_count(charged)));
    Ok(PairSample { original, readable })
}

/// Policy of one pair: a kernel-contract violation of **either** sample
/// first (an ordinary refusal of the original owner never masks one of the
/// readable owner), then the original owner, then the readable owner, then
/// their detector equality.
pub(crate) fn judge_pair(pair: PairSample, expected: ObjectId) -> Result<Tuple, ResearchReadError> {
    if matches!(pair.original, Err(StatFailure::Contract))
        || matches!(pair.readable, Err(StatFailure::Contract))
    {
        return Err(ResearchReadError::KernelContractViolation);
    }
    let original = admit(pair.original, expected)?;
    let readable = admit(pair.readable, expected)?;
    if original != readable {
        return Err(ResearchReadError::ContentChanged);
    }
    Ok(original)
}

/// Steps 1–3 of the per-attempt precedence: a kernel-contract violation of
/// the read **or** of its post pair; then a fatal read error or terminal
/// `EAGAIN` (primary over an ordinary post refusal); then the ordinary post
/// refusal. `Ok` leaves progress, end of file and `EINTR` to the later steps.
pub(crate) fn attempt_precedence(
    class: ReadClass,
    post: Result<(), ResearchReadError>,
) -> Result<(), ResearchReadError> {
    if class == ReadClass::Contract || post == Err(ResearchReadError::KernelContractViolation) {
        return Err(ResearchReadError::KernelContractViolation);
    }
    match class {
        ReadClass::WouldBlock => Err(ResearchReadError::WouldBlock),
        ReadClass::Fatal(errno) => Err(ResearchReadError::ReadFailed(classify_errno(errno))),
        ReadClass::Progress(_) | ReadClass::Eof | ReadClass::Interrupted | ReadClass::Contract => {
            post
        }
    }
}

fn event_count(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The whole transaction short of descriptor closing and result assembly.
/// On `Ok` the final samples passed and no cancellation was observed; the
/// stage is returned with the validated prefix length. On `Err` the stage
/// (if allocated) has already been dropped.
pub(crate) fn transact<S: Source, B: Stage>(
    source: &mut S,
    allocate: impl FnOnce() -> Option<B>,
    expected: ObjectId,
    budget: &ReadBudget,
    cancel: &Cancellation,
) -> Result<(B, Scan), ResearchReadError> {
    transact_in(source, allocate, expected, budget, cancel, &mut Loop::new())
}

/// [`transact`] over caller-held loop state, so that unit tests can inspect
/// the charged credits after a refusal or an unwind.
pub(crate) fn transact_in<S: Source, B: Stage>(
    source: &mut S,
    allocate: impl FnOnce() -> Option<B>,
    expected: ObjectId,
    budget: &ReadBudget,
    cancel: &Cancellation,
    state: &mut Loop,
) -> Result<(B, Scan), ResearchReadError> {
    // 1. Pure validation first: it wins over a simultaneous cancellation.
    budget.validate()?;
    if cancel.is_requested() {
        return Err(ResearchReadError::Cancelled);
    }
    let baseline = judge_pair(sample_pair(source, state)?, expected)?;
    fault::read_event(ReadEvent::EntrySampled);
    let mut stage = allocate().ok_or(ResearchReadError::AllocationRefused)?;
    fault::read_event(ReadEvent::StageAllocated);

    let end = loop {
        // 2. Before each attempt.
        if cancel.is_requested() {
            return Err(ResearchReadError::Cancelled);
        }
        let (request, offset) = match next_request(
            budget,
            state.filled,
            state.counters.pread_attempts,
            state.counters.requested_bytes,
        )? {
            Ok(next) => next,
            Err(end) => break end,
        };
        let window_end = state
            .filled
            .checked_add(request)
            .ok_or(ResearchReadError::InvalidBudget)?;
        let attempt = event_count(state.charge_attempt(budget, request)?);
        fault::read_event(ReadEvent::BeforeAttempt(attempt));
        let window = stage
            .buf()
            .get_mut(state.filled..window_end)
            .ok_or(ResearchReadError::InvalidBudget)?;
        let raw = source.pread(window, offset);
        fault::read_event(ReadEvent::AfterAttempt(attempt));

        // 3. Both raw post samples, before any decision (their credits were
        //    reserved when the attempt was charged).
        let pair = sample_pair(source, state)?;
        let post = judge_pair(pair, expected).and_then(|tuple| {
            if tuple == baseline {
                Ok(())
            } else {
                Err(ResearchReadError::ContentChanged)
            }
        });
        let class = classify_read(raw, request);
        attempt_precedence(class, post)?;
        let filled = u64::try_from(state.filled).map_err(|_| ResearchReadError::InvalidBudget)?;
        match class {
            ReadClass::Progress(count) => {
                let end = u64::try_from(count)
                    .ok()
                    .and_then(|count| filled.checked_add(count))
                    .ok_or(ResearchReadError::InvalidBudget)?;
                if end > baseline.size {
                    return Err(ResearchReadError::ContentChanged);
                }
            }
            ReadClass::Eof if filled < baseline.size => {
                return Err(ResearchReadError::ContentChanged);
            }
            _ => {}
        }
        if class == ReadClass::Interrupted && state.count_interrupt(budget)? {
            return Err(ResearchReadError::RetryExhausted);
        }
        if cancel.is_requested() {
            return Err(ResearchReadError::Cancelled);
        }
        match class {
            ReadClass::Interrupted => state.grant_retry()?,
            ReadClass::Eof => break ObservationEnd::ObservedEof,
            ReadClass::Progress(count) => {
                state.advance(budget, count)?;
                if state.filled == budget.capture_bytes {
                    break ObservationEnd::CaptureLimit;
                }
            }
            ReadClass::WouldBlock | ReadClass::Fatal(_) | ReadClass::Contract => {
                return Err(ResearchReadError::KernelContractViolation);
            }
        }
    };

    // 4. Final samples (their two credits stayed reserved by every charged
    //    attempt), then the final cancellation poll.
    if judge_pair(sample_pair(source, state)?, expected)? != baseline {
        return Err(ResearchReadError::ContentChanged);
    }
    fault::read_event(ReadEvent::BeforeFinalCancel);
    if cancel.is_requested() {
        return Err(ResearchReadError::Cancelled);
    }
    state.counters.captured_bytes = state.filled;
    Ok((
        stage,
        Scan {
            len: state.filled,
            end,
            counters: state.counters,
        },
    ))
}

/// The kernel side: the readable handle for `pread64`, both held owners for
/// direct `fstat`.
pub(crate) struct KernelSource<'a> {
    readable: &'a ReadableFd,
    original: &'a Fd,
}

impl Source for KernelSource<'_> {
    fn pread(&mut self, window: &mut [u8], offset: i64) -> PreadResult {
        sys::pread_window(self.readable, window, offset)
    }

    fn fstat(&mut self, owner: Owner) -> Result<StatFacts, StatFailure> {
        match owner {
            Owner::Original => sys::fstat_original(self.original),
            Owner::Readable => sys::fstat_readable(self.readable),
        }
    }
}

/// The transaction: consumes `readable` and `permit` in every outcome. On
/// success both descriptors are closed **before** the stage moves (not
/// copied) into the result.
pub(crate) fn research_read_prefix(
    readable: Readable,
    permit: ResearchReadPermit,
    budget: ReadBudget,
    cancel: &Cancellation,
) -> Result<PrefixObservation, ResearchReadError> {
    let (stage, scan, object) = {
        let (readable_fd, original, object) = readable.reader_owners();
        let mut source = KernelSource {
            readable: readable_fd,
            original,
        };
        let (stage, scan) = transact(&mut source, ReadStage::allocate, object, &budget, cancel)?;
        (stage, scan, object)
    };
    drop(readable);
    fault::read_event(ReadEvent::OwnersClosed);
    drop(permit);
    Ok(PrefixObservation {
        stage,
        len: scan.len,
        end: scan.end,
        object,
        counters: scan.counters,
    })
}

// ---------------------------------------------------------------------------
// Pure engine tests (synthetic model source; no OS access)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const DEV: u64 = 41;
    const INO: u64 = 97;

    fn id() -> ObjectId {
        ObjectId {
            device: DEV,
            inode: INO,
        }
    }

    fn facts(size: i64) -> StatFacts {
        StatFacts {
            device: DEV,
            inode: INO,
            nlink: 1,
            mode: S_IFREG | 0o600,
            size,
            mtime_sec: 1_700_000_000,
            mtime_nsec: 5,
            ctime_sec: 1_700_000_000,
            ctime_nsec: 7,
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Step {
        /// Serve the next bytes of the model file (up to the window).
        Serve,
        /// Serve at most this many bytes.
        Short(usize),
        Raw(PreadResult),
    }

    /// Model file with scripted reads and scripted samples. After the
    /// script every read serves real model bytes. `cancel_after_read`
    /// requests cancellation right after that read number (1-based).
    struct Model<'c> {
        content: Vec<u8>,
        steps: VecDeque<Step>,
        samples: VecDeque<Option<Result<StatFacts, StatFailure>>>,
        base: StatFacts,
        reads: usize,
        stats: Vec<Owner>,
        offsets: Vec<(i64, usize)>,
        cancel_after_read: Option<(usize, &'c Cancellation)>,
        /// Panic instead of performing this read / `fstat` (1-based), so
        /// that the call never runs.
        panic_on_read: Option<usize>,
        panic_on_stat: Option<usize>,
    }

    impl<'c> Model<'c> {
        fn new(content: &[u8], steps: Vec<Step>) -> Self {
            Self {
                content: content.to_vec(),
                steps: steps.into(),
                samples: VecDeque::new(),
                base: facts(content.len() as i64),
                reads: 0,
                stats: Vec::new(),
                offsets: Vec::new(),
                cancel_after_read: None,
                panic_on_read: None,
                panic_on_stat: None,
            }
        }
    }

    impl Source for Model<'_> {
        fn pread(&mut self, window: &mut [u8], offset: i64) -> PreadResult {
            if self.panic_on_read == Some(self.reads + 1) {
                panic!("ABF30 scheduled model panic before a read");
            }
            self.reads += 1;
            self.offsets.push((offset, window.len()));
            let step = self.steps.pop_front().unwrap_or(Step::Serve);
            let serve = |window: &mut [u8], limit: usize| {
                let start = (offset as usize).min(self.content.len());
                let n = window.len().min(limit).min(self.content.len() - start);
                window[..n].copy_from_slice(&self.content[start..start + n]);
                PreadResult::Returned(n as i64)
            };
            let result = match step {
                Step::Serve => serve(window, usize::MAX),
                Step::Short(limit) => serve(window, limit),
                Step::Raw(raw) => raw,
            };
            if let Some((after, cancel)) = self.cancel_after_read
                && after == self.reads
            {
                cancel.request();
            }
            result
        }

        fn fstat(&mut self, owner: Owner) -> Result<StatFacts, StatFailure> {
            if self.panic_on_stat == Some(self.stats.len() + 1) {
                panic!("ABF30 scheduled model panic before an fstat");
            }
            self.stats.push(owner);
            self.samples.pop_front().flatten().unwrap_or(Ok(self.base))
        }
    }

    struct Run {
        result: Result<(Box<[u8; STAGE_BYTES]>, Scan), ResearchReadError>,
        reads: usize,
        stats: usize,
        offsets: Vec<(i64, usize)>,
        allocated: bool,
    }

    fn drive_with(model: &mut Model<'_>, budget: ReadBudget, cancel: &Cancellation) -> Run {
        let allocated = Cell::new(false);
        let result = transact(
            model,
            || {
                allocated.set(true);
                Some(Box::new([0_u8; STAGE_BYTES]))
            },
            id(),
            &budget,
            cancel,
        );
        Run {
            result,
            reads: model.reads,
            stats: model.stats.len(),
            offsets: model.offsets.clone(),
            allocated: allocated.get(),
        }
    }

    fn drive(content: &[u8], steps: Vec<Step>) -> Run {
        drive_with(
            &mut Model::new(content, steps),
            ReadBudget::maximum(),
            &Cancellation::new(),
        )
    }

    /// The refusal of `run`. An unexpected release fails with fixed text:
    /// the model stage is never formatted into test output.
    fn refused(run: &Run) -> ResearchReadError {
        match &run.result {
            Ok((_, scan)) => panic!("expected a refusal, released {} bytes", scan.len),
            Err(error) => *error,
        }
    }

    fn ok(run: &Run) -> (&[u8], Scan) {
        let (stage, scan) = run.result.as_ref().unwrap();
        (&stage[..scan.len], *scan)
    }

    impl Stage for Box<[u8; STAGE_BYTES]> {
        fn buf(&mut self) -> &mut [u8; STAGE_BYTES] {
            self
        }
    }

    fn content(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    // -- ends and counters ---------------------------------------------------

    #[test]
    fn empty_file_needs_one_positive_request_returning_zero() {
        let run = drive(b"", vec![]);
        let (bytes, scan) = ok(&run);
        assert!(bytes.is_empty());
        assert_eq!(scan.end, ObservationEnd::ObservedEof);
        assert_eq!((run.reads, run.stats), (1, 6));
        assert_eq!(run.offsets, vec![(0, 4096)]);
        assert_eq!(scan.counters.fstat_attempts, 6);
        assert_eq!(scan.counters.requested_bytes, 4096);
    }

    #[test]
    fn exact_capture_is_capture_limit_without_an_eof_probe() {
        let data = content(MAX_CAPTURE_BYTES);
        let run = drive(&data, vec![]);
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes, &data[..]);
        assert_eq!(scan.end, ObservationEnd::CaptureLimit);
        assert_eq!(run.reads, 16, "no N+1 lookahead");
        assert_eq!(run.stats, 2 + 2 * 16 + 2);
    }

    #[test]
    fn one_byte_short_of_capture_ends_with_a_one_byte_eof_request() {
        let data = content(MAX_CAPTURE_BYTES - 1);
        let run = drive(&data, vec![]);
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes, &data[..]);
        assert_eq!(scan.end, ObservationEnd::ObservedEof);
        assert_eq!(run.reads, 17);
        assert_eq!(*run.offsets.last().unwrap(), (65_535, 1));
    }

    #[test]
    fn short_reads_are_progress_never_eof() {
        let data = content(10_000);
        let run = drive(
            &data,
            vec![Step::Short(1), Step::Short(3000), Step::Short(7)],
        );
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes, &data[..]);
        assert_eq!(scan.end, ObservationEnd::ObservedEof);
        assert_eq!(run.offsets[..3], [(0, 4096), (1, 4096), (3001, 4096)]);
    }

    #[test]
    fn attempt_and_requested_budgets_stop_without_eof() {
        let data = content(20_000);
        let budget = ReadBudget::new(4096, 65_536, 262_144, 2, 2, 8).unwrap();
        let run = drive_with(&mut Model::new(&data, vec![]), budget, &Cancellation::new());
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes, &data[..8192]);
        assert_eq!(
            scan.end,
            ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)
        );
        assert_eq!(run.reads, 2);

        let budget = ReadBudget::new(1000, 65_536, 2500, 64, 2, 8).unwrap();
        let run = drive_with(&mut Model::new(&data, vec![]), budget, &Cancellation::new());
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes, &data[..2500]);
        assert_eq!(
            scan.end,
            ObservationEnd::BudgetStop(ReadBudgetKind::RequestedBytes)
        );
        assert_eq!(run.offsets, vec![(0, 1000), (1000, 1000), (2000, 500)]);
        assert_eq!(scan.counters.requested_bytes, 2500);
    }

    #[test]
    fn reduced_capture_reports_capture_limit_even_at_the_true_end() {
        let data = content(100);
        let budget = ReadBudget::new(4096, 100, 262_144, 64, 2, 8).unwrap();
        let run = drive_with(&mut Model::new(&data, vec![]), budget, &Cancellation::new());
        assert_eq!(ok(&run).1.end, ObservationEnd::CaptureLimit);
        assert_eq!(run.reads, 1);
        let budget = ReadBudget::new(4096, 101, 262_144, 64, 2, 8).unwrap();
        let run = drive_with(&mut Model::new(&data, vec![]), budget, &Cancellation::new());
        assert_eq!(ok(&run).1.end, ObservationEnd::ObservedEof);
        assert_eq!(run.reads, 2);
    }

    #[test]
    fn requested_bytes_are_charged_for_failed_and_interrupted_attempts() {
        let data = content(10);
        let run = drive(
            &data,
            vec![
                Step::Raw(PreadResult::Errno(EINTR)),
                Step::Raw(PreadResult::Errno(EINTR)),
            ],
        );
        let (_, scan) = ok(&run);
        assert_eq!(scan.counters.pread_attempts, 4);
        // E, E, 10 bytes, then the EOF request: all four charged in full.
        assert_eq!(scan.counters.requested_bytes, 4 * 4096);
        assert_eq!(scan.counters.eintr_retries, 2);
        assert_eq!(run.offsets[..3], [(0, 4096), (0, 4096), (0, 4096)]);
    }

    // -- EINTR --------------------------------------------------------------

    #[test]
    fn eintr_grants_are_bounded_consecutively_and_in_total() {
        let eintr = Step::Raw(PreadResult::Errno(EINTR));
        let run = drive(&content(10), vec![eintr, eintr, eintr]);
        assert_eq!(refused(&run), ResearchReadError::RetryExhausted);
        assert_eq!((run.reads, run.stats), (3, 8), "checked after each attempt");

        let mut steps = Vec::new();
        for _ in 0..4 {
            steps.extend([eintr, eintr, Step::Short(1)]);
        }
        let run = drive(&content(100), steps.clone());
        assert_eq!(
            ok(&run).1.counters.eintr_retries,
            8,
            "eight grants are fine"
        );
        steps.push(eintr);
        let run = drive(&content(100), steps);
        assert_eq!(refused(&run), ResearchReadError::RetryExhausted);
        assert_eq!(run.reads, 13, "the ninth would-be retry is fatal");
    }

    // -- precedence ---------------------------------------------------------

    fn post_failure(
        read: Step,
        original: Option<Result<StatFacts, StatFailure>>,
        readable: Option<Result<StatFacts, StatFailure>>,
    ) -> Run {
        let mut model = Model::new(&content(10), vec![read]);
        model.samples = VecDeque::from(vec![None, None, original, readable]);
        drive_with(&mut model, ReadBudget::maximum(), &Cancellation::new())
    }

    #[test]
    fn contract_violation_wins_and_both_samples_still_run() {
        for raw in [
            PreadResult::Returned(4097),
            PreadResult::Returned(-2),
            PreadResult::Returned(i64::MIN),
            PreadResult::Errno(0),
            PreadResult::Errno(-5),
        ] {
            let run = post_failure(
                Step::Raw(raw),
                Some(Err(StatFailure::Errno(5))),
                Some(Err(StatFailure::Errno(13))),
            );
            assert_eq!(
                refused(&run),
                ResearchReadError::KernelContractViolation,
                "{raw:?}"
            );
            assert_eq!((run.reads, run.stats), (1, 4), "{raw:?}");
        }
    }

    #[test]
    fn fatal_read_wins_over_failed_post_samples_which_still_run() {
        let run = post_failure(
            Step::Raw(PreadResult::Errno(5)),
            Some(Err(StatFailure::Errno(13))),
            Some(Err(StatFailure::Errno(2))),
        );
        assert_eq!(
            refused(&run),
            ResearchReadError::ReadFailed(AccessError::UnexpectedOs(5))
        );
        assert_eq!((run.reads, run.stats), (1, 4));
        let run = post_failure(Step::Raw(PreadResult::Errno(EAGAIN)), None, None);
        assert_eq!(refused(&run), ResearchReadError::WouldBlock);
        assert_eq!(run.reads, 1, "EAGAIN is never retried");
    }

    #[test]
    fn original_owner_refusal_is_reported_before_the_readable_one() {
        let run = post_failure(
            Step::Serve,
            Some(Err(StatFailure::Errno(13))),
            Some(Err(StatFailure::Errno(2))),
        );
        assert_eq!(
            refused(&run),
            ResearchReadError::Metadata(AccessError::Inaccessible)
        );
        assert_eq!(run.stats, 4, "the readable owner was still sampled");
        let run = post_failure(Step::Serve, None, Some(Err(StatFailure::Errno(2))));
        assert_eq!(
            refused(&run),
            ResearchReadError::Metadata(AccessError::NotFound)
        );
    }

    #[test]
    fn post_sample_refusal_wins_over_eintr_exhaustion_and_cancellation() {
        let eintr = Step::Raw(PreadResult::Errno(EINTR));
        let cancel = Cancellation::new();
        let mut model = Model::new(&content(10), vec![eintr, eintr, eintr]);
        let mut changed = facts(10);
        changed.size = 11;
        model.samples = VecDeque::from(vec![
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Ok(changed)),
        ]);
        model.cancel_after_read = Some((3, &cancel));
        let run = drive_with(&mut model, ReadBudget::maximum(), &cancel);
        assert_eq!(refused(&run), ResearchReadError::ContentChanged);
    }

    #[test]
    fn eintr_exhaustion_wins_over_cancellation_which_wins_over_eof_and_budget() {
        let eintr = Step::Raw(PreadResult::Errno(EINTR));
        let cancel = Cancellation::new();
        let mut model = Model::new(&content(10), vec![eintr, eintr, eintr]);
        model.cancel_after_read = Some((3, &cancel));
        let run = drive_with(&mut model, ReadBudget::maximum(), &cancel);
        assert_eq!(refused(&run), ResearchReadError::RetryExhausted);

        // Cancellation after the EOF attempt, and after the capture-filling
        // attempt: no release.
        for (data, after) in [(content(10), 2), (content(MAX_CAPTURE_BYTES), 16)] {
            let cancel = Cancellation::new();
            let mut model = Model::new(&data, vec![]);
            model.cancel_after_read = Some((after, &cancel));
            let run = drive_with(&mut model, ReadBudget::maximum(), &cancel);
            assert_eq!(refused(&run), ResearchReadError::Cancelled);
            assert_eq!(run.reads, after);
            assert_eq!(run.stats, 2 + 2 * after, "no final samples after it");
        }
    }

    #[test]
    fn cancellation_never_releases_a_partial_prefix() {
        let cancel = Cancellation::new();
        let mut model = Model::new(&content(20_000), vec![]);
        model.cancel_after_read = Some((1, &cancel));
        let run = drive_with(&mut model, ReadBudget::maximum(), &cancel);
        assert_eq!(refused(&run), ResearchReadError::Cancelled);
        assert_eq!(run.reads, 1);
    }

    #[test]
    fn entry_cancellation_allocates_and_samples_nothing() {
        let cancel = Cancellation::new();
        cancel.request();
        let run = drive_with(
            &mut Model::new(&content(10), vec![]),
            ReadBudget::maximum(),
            &cancel,
        );
        assert_eq!(refused(&run), ResearchReadError::Cancelled);
        assert_eq!((run.reads, run.stats, run.allocated), (0, 0, false));
    }

    #[test]
    fn invalid_budget_wins_over_entry_cancellation() {
        let cancel = Cancellation::new();
        cancel.request();
        let bad = ReadBudget {
            attempts: 0,
            ..ReadBudget::maximum()
        };
        let run = drive_with(&mut Model::new(&content(10), vec![]), bad, &cancel);
        assert_eq!(refused(&run), ResearchReadError::InvalidBudget);
        assert_eq!((run.reads, run.stats), (0, 0));
    }

    #[test]
    fn allocation_refusal_happens_after_entry_samples_and_reads_nothing() {
        let mut model = Model::new(&content(10), vec![]);
        let result = transact(
            &mut model,
            || None::<Box<[u8; STAGE_BYTES]>>,
            id(),
            &ReadBudget::maximum(),
            &Cancellation::new(),
        );
        assert_eq!(result.err(), Some(ResearchReadError::AllocationRefused));
        assert_eq!((model.reads, model.stats.len()), (0, 2));
    }

    // -- change detector ----------------------------------------------------

    #[test]
    fn detector_changes_refuse_the_whole_stage() {
        let changes: [fn(&mut StatFacts); 5] = [
            |f| f.size += 1,
            |f| f.size -= 1,
            |f| f.mtime_nsec += 1,
            |f| f.ctime_sec += 1,
            |f| f.ctime_nsec += 1,
        ];
        for (index, change) in changes.iter().enumerate() {
            // After the first attempt, on either owner, and at release.
            for slot in [2, 3, 6, 7] {
                let mut model = Model::new(&content(10), vec![]);
                let mut changed = facts(10);
                change(&mut changed);
                let mut samples = vec![None; 8];
                samples[slot] = Some(Ok(changed));
                model.samples = samples.into();
                let run = drive_with(&mut model, ReadBudget::maximum(), &Cancellation::new());
                assert_eq!(
                    refused(&run),
                    ResearchReadError::ContentChanged,
                    "change {index} at sample {slot}"
                );
            }
        }
        // atime is not part of the detector (it is not even sampled).
    }

    #[test]
    fn owners_disagreeing_at_entry_refuse_before_allocation() {
        let mut model = Model::new(&content(10), vec![]);
        let mut changed = facts(10);
        changed.mtime_sec += 1;
        model.samples = VecDeque::from(vec![None, Some(Ok(changed))]);
        let run = drive_with(&mut model, ReadBudget::maximum(), &Cancellation::new());
        assert_eq!(refused(&run), ResearchReadError::ContentChanged);
        assert!(!run.allocated);
    }

    #[test]
    fn progress_beyond_baseline_size_or_early_eof_is_content_changed() {
        // The model serves 20 bytes while every sample still claims 10.
        let mut model = Model::new(&content(20), vec![]);
        model.base = facts(10);
        let run = drive_with(&mut model, ReadBudget::maximum(), &Cancellation::new());
        assert_eq!(refused(&run), ResearchReadError::ContentChanged);
        // EOF at 5 while the samples claim 10.
        let mut model = Model::new(&content(5), vec![]);
        model.base = facts(10);
        let run = drive_with(&mut model, ReadBudget::maximum(), &Cancellation::new());
        assert_eq!(refused(&run), ResearchReadError::ContentChanged);
        // A shorter prefix at a budget end is permitted (explicitly
        // incomplete).
        let mut model = Model::new(&content(10), vec![]);
        let budget = ReadBudget::new(4, 65_536, 262_144, 1, 2, 8).unwrap();
        let run = drive_with(&mut model, budget, &Cancellation::new());
        assert_eq!(
            ok(&run).1.end,
            ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)
        );
    }

    // -- policy -------------------------------------------------------------

    #[test]
    fn owner_policy_refusals_in_order() {
        let base = facts(10);
        let expect = |f: StatFacts, error: ResearchReadError| {
            assert_eq!(admit(Ok(f), id()).unwrap_err(), error, "{f:?}");
        };
        expect(
            StatFacts {
                mode: S_IFDIR | 0o700,
                device: 0,
                ..base
            },
            ResearchReadError::Metadata(AccessError::NotRegularFile(ObjectKind::Directory)),
        );
        for (mode, kind) in [
            (S_IFLNK, ObjectKind::Symlink),
            (S_IFIFO, ObjectKind::Fifo),
            (S_IFSOCK, ObjectKind::Socket),
            (S_IFCHR, ObjectKind::CharDevice),
            (S_IFBLK, ObjectKind::BlockDevice),
            (0, ObjectKind::Unknown),
        ] {
            expect(
                StatFacts { mode, ..base },
                ResearchReadError::Metadata(AccessError::NotRegularFile(kind)),
            );
        }
        expect(
            StatFacts { device: 0, ..base },
            ResearchReadError::Metadata(AccessError::IdentityUnavailable),
        );
        expect(
            StatFacts { inode: 0, ..base },
            ResearchReadError::Metadata(AccessError::IdentityUnavailable),
        );
        expect(
            StatFacts {
                inode: INO + 1,
                ..base
            },
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        );
        expect(
            StatFacts {
                device: DEV + 1,
                ..base
            },
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        );
        expect(
            StatFacts { nlink: 0, ..base },
            ResearchReadError::Metadata(AccessError::ObjectChanged),
        );
        expect(
            StatFacts { nlink: 2, ..base },
            ResearchReadError::Metadata(AccessError::MultipleLinks),
        );
        expect(
            StatFacts { size: -1, ..base },
            ResearchReadError::InvalidMetadata,
        );
        expect(
            StatFacts {
                mtime_nsec: NANOS_PER_SECOND,
                ..base
            },
            ResearchReadError::InvalidMetadata,
        );
        expect(
            StatFacts {
                ctime_nsec: u64::MAX,
                ..base
            },
            ResearchReadError::InvalidMetadata,
        );
        assert_eq!(
            admit(Err(StatFailure::Contract), id()).unwrap_err(),
            ResearchReadError::KernelContractViolation
        );
        assert!(admit(Ok(base), id()).is_ok());
    }

    // -- arithmetic ---------------------------------------------------------

    #[test]
    fn offsets_and_windows_are_checked() {
        assert_eq!(checked_offset(0, 4096), Some(0));
        assert_eq!(checked_offset(61_440, 4096), Some(61_440));
        assert_eq!(checked_offset(61_441, 4096), None, "window past the stage");
        assert_eq!(checked_offset(65_535, 1), Some(65_535));
        assert_eq!(checked_offset(65_536, 1), None);
        assert_eq!(checked_offset(0, 0), None);
        assert_eq!(checked_offset(0, 4097), None);
        assert_eq!(checked_offset(usize::MAX, 1), None);
        assert_eq!(checked_offset(i64::MAX as usize, 1), None);
        assert_eq!(
            classify_read(PreadResult::Returned(4096), 4096),
            ReadClass::Progress(4096)
        );
        assert_eq!(
            classify_read(PreadResult::Returned(4097), 4096),
            ReadClass::Contract
        );
        assert_eq!(
            classify_read(PreadResult::Returned(i64::MAX), 4096),
            ReadClass::Contract
        );
        assert_eq!(
            classify_read(PreadResult::Returned(-1), 1),
            ReadClass::Contract
        );
    }

    #[test]
    fn next_request_respects_every_credit() {
        let max = ReadBudget::maximum();
        assert_eq!(next_request(&max, 0, 0, 0), Ok(Ok((4096, 0))));
        assert_eq!(
            next_request(&max, 65_000, 15, 61_440),
            Ok(Ok((536, 65_000)))
        );
        assert_eq!(
            next_request(&max, 65_536, 16, 65_536),
            Ok(Err(ObservationEnd::CaptureLimit))
        );
        assert_eq!(
            next_request(&max, 0, 64, 0),
            Ok(Err(ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)))
        );
        assert_eq!(
            next_request(&max, 0, 63, 262_144),
            Ok(Err(ObservationEnd::BudgetStop(
                ReadBudgetKind::RequestedBytes
            )))
        );
        assert_eq!(
            next_request(&max, 65_537, 0, 0),
            Err(ResearchReadError::InvalidBudget)
        );
        assert_eq!(
            next_request(&max, 0, 0, 262_145),
            Err(ResearchReadError::InvalidBudget)
        );
    }

    #[test]
    fn worst_case_counts_stay_within_ceilings() {
        // 64 one-byte short reads of a large file: attempts end first.
        let steps = vec![Step::Short(1); 64];
        let run = drive(&content(100_000), steps);
        let (bytes, scan) = ok(&run);
        assert_eq!(bytes.len(), 64);
        assert_eq!(
            scan.end,
            ObservationEnd::BudgetStop(ReadBudgetKind::Attempts)
        );
        assert_eq!(scan.counters.pread_attempts, MAX_PREAD_ATTEMPTS);
        assert_eq!(scan.counters.fstat_attempts, MAX_FSTAT_ATTEMPTS);
        assert_eq!(scan.counters.requested_bytes, MAX_REQUESTED_BYTES);
        assert_eq!(run.stats, MAX_FSTAT_ATTEMPTS);
    }

    #[test]
    fn control_storage_is_small_and_inline() {
        eprintln!("ABF28-LEDGER control bytes: {CONTROL_BYTES}");
        for (name, bytes) in [
            (
                "PrefixObservation",
                std::mem::size_of::<PrefixObservation>(),
            ),
            (
                "ResearchReadPermit",
                std::mem::size_of::<ResearchReadPermit>(),
            ),
            ("ReadBudget", std::mem::size_of::<ReadBudget>()),
            ("Readable", std::mem::size_of::<Readable>()),
            ("KernelSource", std::mem::size_of::<KernelSource<'static>>()),
            ("Loop", std::mem::size_of::<Loop>()),
            ("Tuple", std::mem::size_of::<Tuple>()),
            (
                "StatSampleResult",
                std::mem::size_of::<Result<StatFacts, StatFailure>>(),
            ),
        ] {
            eprintln!("ABF28-LEDGER size {name}: {bytes}");
        }
        // CONTROL_BYTES <= MAX_CONTROL_BYTES is a compile-time assertion.
        assert_eq!(std::mem::size_of::<ResearchReadPermit>(), 0);
    }

    // -- ABF30: kernel-contract precedence (A29-1) ---------------------------

    const EIO: i32 = 5;
    const ENOENT: i32 = 2;
    const EACCES: i32 = 13;

    type Sample = Option<Result<StatFacts, StatFailure>>;

    const CONTRACT: Sample = Some(Err(StatFailure::Contract));

    fn errno(errno: i32) -> Sample {
        Some(Err(StatFailure::Errno(errno)))
    }

    fn with(change: fn(&mut StatFacts)) -> Sample {
        let mut sample = facts(10);
        change(&mut sample);
        Some(Ok(sample))
    }

    /// Runs `content` with the scripted reads and samples; `None` samples
    /// serve the model's base facts.
    fn scripted(content: &[u8], steps: Vec<Step>, samples: Vec<Sample>, budget: ReadBudget) -> Run {
        let mut model = Model::new(content, steps);
        model.samples = samples.into();
        drive_with(&mut model, budget, &Cancellation::new())
    }

    /// Independent oracle of one pair, written as an explicit rule over
    /// named samples (not via `judge_pair`): a contract violation of either
    /// owner; else the original owner's ordinary refusal; else the readable
    /// owner's; else detector equality.
    #[test]
    fn pair_policy_ranks_either_contract_before_any_ordinary_refusal() {
        use ResearchReadError::{ContentChanged, KernelContractViolation, Metadata};
        let named: [(&str, Result<StatFacts, StatFailure>); 6] = [
            ("ok", Ok(facts(10))),
            ("eacces", Err(StatFailure::Errno(EACCES))),
            ("enoent", Err(StatFailure::Errno(ENOENT))),
            ("contract", Err(StatFailure::Contract)),
            (
                "nlink2",
                Ok(StatFacts {
                    nlink: 2,
                    ..facts(10)
                }),
            ),
            ("grown", Ok(facts(11))),
        ];
        let ordinary = |name: &str| match name {
            "eacces" => Some(Metadata(AccessError::Inaccessible)),
            "enoent" => Some(Metadata(AccessError::NotFound)),
            "nlink2" => Some(Metadata(AccessError::MultipleLinks)),
            _ => None,
        };
        for (o_name, original) in named {
            for (r_name, readable) in named {
                let expected = if o_name == "contract" || r_name == "contract" {
                    Err(KernelContractViolation)
                } else if let Some(error) = ordinary(o_name) {
                    Err(error)
                } else if let Some(error) = ordinary(r_name) {
                    Err(error)
                } else if o_name == r_name {
                    Ok(())
                } else {
                    Err(ContentChanged)
                };
                let got = judge_pair(PairSample { original, readable }, id()).map(|_| ());
                assert_eq!(got, expected, "original {o_name}, readable {r_name}");
            }
        }
    }

    /// Independent oracle of steps 1–3 for every read class against every
    /// kind of post outcome.
    #[test]
    fn attempt_precedence_table() {
        use ResearchReadError::{
            ContentChanged, InvalidMetadata, KernelContractViolation, Metadata, ReadFailed,
            WouldBlock,
        };
        let classes = [
            ("progress", ReadClass::Progress(10)),
            ("eof", ReadClass::Eof),
            ("eintr", ReadClass::Interrupted),
            ("eagain", ReadClass::WouldBlock),
            ("eio", ReadClass::Fatal(EIO)),
            ("contract", ReadClass::Contract),
        ];
        let posts = [
            ("ok", Ok(())),
            ("contract", Err(KernelContractViolation)),
            ("metadata", Err(Metadata(AccessError::Inaccessible))),
            ("changed", Err(ContentChanged)),
            ("invalid", Err(InvalidMetadata)),
        ];
        for (c_name, class) in classes {
            for (p_name, post) in posts {
                let expected = if c_name == "contract" || p_name == "contract" {
                    Err(KernelContractViolation)
                } else if c_name == "eagain" {
                    Err(WouldBlock)
                } else if c_name == "eio" {
                    Err(ReadFailed(AccessError::UnexpectedOs(EIO)))
                } else {
                    post
                };
                assert_eq!(
                    attempt_precedence(class, post),
                    expected,
                    "read {c_name}, post {p_name}"
                );
            }
        }
    }

    /// A29-1 counterexamples A–C and their neighbours, through the whole
    /// transaction: exactly one read, both post samples, no release.
    #[test]
    fn post_contract_outranks_fatal_eagain_and_ordinary_post_refusals() {
        use ResearchReadError::{KernelContractViolation as K, Metadata, ReadFailed, WouldBlock};
        let raw = |raw| Step::Raw(raw);
        let eio = raw(PreadResult::Errno(EIO));
        let eagain = raw(PreadResult::Errno(EAGAIN));
        let cases: [(&str, Step, Sample, Sample, ResearchReadError); 16] = [
            ("A eio+second-contract", eio, None, CONTRACT, K),
            ("eio+first-contract", eio, CONTRACT, None, K),
            (
                "eio+eacces+second-contract",
                eio,
                errno(EACCES),
                CONTRACT,
                K,
            ),
            ("B eagain+first-contract", eagain, CONTRACT, None, K),
            ("eagain+second-contract", eagain, None, CONTRACT, K),
            (
                "C progress+eacces+second-contract",
                Step::Serve,
                errno(EACCES),
                CONTRACT,
                K,
            ),
            (
                "progress+first-contract+enoent",
                Step::Serve,
                CONTRACT,
                errno(ENOENT),
                K,
            ),
            ("progress+both-contract", Step::Serve, CONTRACT, CONTRACT, K),
            (
                "eintr+eacces+second-contract",
                raw(PreadResult::Errno(EINTR)),
                errno(EACCES),
                CONTRACT,
                K,
            ),
            (
                "negative+ordinary",
                raw(PreadResult::Returned(-2)),
                errno(EACCES),
                errno(ENOENT),
                K,
            ),
            (
                "overcount+ordinary",
                raw(PreadResult::Returned(4097)),
                errno(EACCES),
                errno(ENOENT),
                K,
            ),
            (
                "errno0+second-contract",
                raw(PreadResult::Errno(0)),
                None,
                CONTRACT,
                K,
            ),
            (
                "eio+ordinary stays fatal",
                eio,
                errno(EACCES),
                errno(ENOENT),
                ReadFailed(AccessError::UnexpectedOs(EIO)),
            ),
            (
                "eio+detector change stays fatal",
                eio,
                with(|f| f.size += 1),
                None,
                ReadFailed(AccessError::UnexpectedOs(EIO)),
            ),
            (
                "eagain+ordinary stays terminal",
                eagain,
                errno(EACCES),
                None,
                WouldBlock,
            ),
            (
                "progress+ordinary original first",
                Step::Serve,
                errno(EACCES),
                errno(ENOENT),
                Metadata(AccessError::Inaccessible),
            ),
        ];
        for (name, step, original, readable, expected) in cases {
            let run = post_failure(step, original, readable);
            assert_eq!(refused(&run), expected, "{name}");
            assert_eq!(
                (run.reads, run.stats),
                (1, 4),
                "{name}: both post samples, no new read"
            );
            assert!(run.allocated, "{name}");
        }
        // A genuine zero-byte EOF is followed by both samples too.
        for (readable, expected) in [
            (CONTRACT, K),
            (errno(ENOENT), Metadata(AccessError::Inaccessible)),
        ] {
            let run = scripted(
                b"",
                vec![],
                vec![None, None, errno(EACCES), readable],
                ReadBudget::maximum(),
            );
            assert_eq!(refused(&run), expected, "eof");
            assert_eq!((run.reads, run.stats), (1, 4), "eof");
        }
    }

    /// Entry and final pairs rank a contract violation of either owner first
    /// and release nothing; ordinary-only pairs stay original-owner first.
    #[test]
    fn entry_and_final_pairs_rank_contract_first() {
        use ResearchReadError::{KernelContractViolation as K, Metadata};
        let nlink2 = with(|f| f.nlink = 2);
        for (name, original, readable, expected) in [
            ("first-contract", CONTRACT, errno(EACCES), K),
            ("second-contract", errno(EACCES), CONTRACT, K),
            ("nlink2+second-contract", nlink2, CONTRACT, K),
            ("both-contract", CONTRACT, CONTRACT, K),
            (
                "ordinary original first",
                errno(EACCES),
                errno(ENOENT),
                Metadata(AccessError::Inaccessible),
            ),
        ] {
            let run = scripted(
                &content(10),
                vec![],
                vec![original, readable],
                ReadBudget::maximum(),
            );
            assert_eq!(refused(&run), expected, "entry {name}");
            assert_eq!(
                (run.reads, run.stats, run.allocated),
                (0, 2, false),
                "entry {name}"
            );
        }
        // 10 bytes: one progress read and one EOF read (samples 0..6), then
        // the final pair (samples 6 and 7) after bytes were staged.
        for (name, original, readable, expected) in [
            ("first-contract", CONTRACT, with(|f| f.mtime_sec += 1), K),
            ("eio+second-contract", errno(EIO), CONTRACT, K),
            ("nlink2+second-contract", nlink2, CONTRACT, K),
            (
                "ordinary original first",
                errno(EACCES),
                errno(ENOENT),
                Metadata(AccessError::Inaccessible),
            ),
        ] {
            let mut samples = vec![None; 6];
            samples.extend([original, readable]);
            let run = scripted(&content(10), vec![], samples, ReadBudget::maximum());
            assert_eq!(refused(&run), expected, "final {name}");
            assert_eq!(
                (run.reads, run.stats, run.allocated),
                (2, 8, true),
                "final {name}"
            );
        }
    }

    /// After positive progress a post contract still outranks a pending
    /// cancellation and `EINTR` exhaustion; ordinary refusals keep their
    /// place above them.
    #[test]
    fn contract_after_progress_outranks_exhaustion_and_cancellation() {
        use ResearchReadError::{KernelContractViolation as K, Metadata};
        // Cancellation requested right after read 2; its post pair is
        // samples 4 and 5.
        for (readable, expected) in [
            (CONTRACT, K),
            (errno(ENOENT), Metadata(AccessError::NotFound)),
            (None, ResearchReadError::Cancelled),
        ] {
            let cancel = Cancellation::new();
            let mut model = Model::new(&content(20_000), vec![]);
            model.samples = vec![None, None, None, None, None, readable].into();
            model.cancel_after_read = Some((2, &cancel));
            let run = drive_with(&mut model, ReadBudget::maximum(), &cancel);
            assert_eq!(refused(&run), expected);
            assert_eq!((run.reads, run.stats), (2, 6));
        }
        // The third consecutive EINTR would exhaust the grants; a contract
        // in its post pair (samples 6 and 7) wins.
        let eintr = Step::Raw(PreadResult::Errno(EINTR));
        let mut samples = vec![None; 6];
        samples.extend([errno(EACCES), CONTRACT]);
        let run = scripted(
            &content(10),
            vec![eintr, eintr, eintr],
            samples,
            ReadBudget::maximum(),
        );
        assert_eq!(refused(&run), K);
        assert_eq!((run.reads, run.stats), (3, 8));
    }

    /// End-of-file and budget-limit adjacency: the last funded attempt and
    /// the final pair after a capture or budget end still rank a contract
    /// first.
    #[test]
    fn contract_at_eof_and_budget_ends_is_primary() {
        let k = ResearchReadError::KernelContractViolation;
        let tail = |pad: usize, original: Sample, readable: Sample| {
            let mut samples = vec![None; pad];
            samples.extend([original, readable]);
            samples
        };
        // Attempt budget 1: progress, then the final pair (samples 4, 5).
        let one = ReadBudget::new(4096, 65_536, 262_144, 1, 2, 8).unwrap();
        let run = scripted(&content(100), vec![], tail(4, errno(EACCES), CONTRACT), one);
        assert_eq!((refused(&run), run.reads, run.stats), (k, 1, 6));
        // Capture 100 of 100 bytes: capture limit, then the final pair.
        let capture = ReadBudget::new(4096, 100, 262_144, 64, 2, 8).unwrap();
        let run = scripted(&content(100), vec![], tail(4, CONTRACT, None), capture);
        assert_eq!((refused(&run), run.reads, run.stats), (k, 1, 6));
        // The last funded attempt returns a genuine zero (samples 4, 5).
        let two = ReadBudget::new(4096, 65_536, 262_144, 2, 2, 8).unwrap();
        let run = scripted(&content(100), vec![], tail(4, errno(EACCES), CONTRACT), two);
        assert_eq!((refused(&run), run.reads, run.stats), (k, 2, 6));
        // The same budgets without injected failures still release.
        for budget in [one, capture, two] {
            let run = scripted(&content(100), vec![], vec![], budget);
            assert_eq!(ok(&run).0, &content(100)[..]);
        }
    }

    // -- ABF30: checked, precharged accounting (A29-2) -----------------------

    fn snapshot(state: &Loop) -> (usize, usize, usize, ReadCounters) {
        (
            state.filled,
            state.consecutive_eintr,
            state.total_eintr,
            state.counters,
        )
    }

    fn drive_in(
        model: &mut Model<'_>,
        budget: ReadBudget,
        state: &mut Loop,
    ) -> Option<ResearchReadError> {
        transact_in(
            model,
            || Some(Box::new([0_u8; STAGE_BYTES])),
            id(),
            &budget,
            &Cancellation::new(),
            state,
        )
        .err()
    }

    /// Credits stay debited after every refusal: nothing is refunded for a
    /// fatal error, `EAGAIN`, an impossible count, an ordinary post refusal
    /// or a contract violation.
    #[test]
    fn credits_are_never_refunded_on_refusal() {
        for (name, step, samples, expected) in [
            (
                "eio",
                Step::Raw(PreadResult::Errno(EIO)),
                vec![],
                ResearchReadError::ReadFailed(AccessError::UnexpectedOs(EIO)),
            ),
            (
                "eagain",
                Step::Raw(PreadResult::Errno(EAGAIN)),
                vec![],
                ResearchReadError::WouldBlock,
            ),
            (
                "negative",
                Step::Raw(PreadResult::Returned(-2)),
                vec![],
                ResearchReadError::KernelContractViolation,
            ),
            (
                "post-refusal",
                Step::Serve,
                vec![None, None, errno(EACCES)],
                ResearchReadError::Metadata(AccessError::Inaccessible),
            ),
            (
                "post-contract",
                Step::Serve,
                vec![None, None, None, CONTRACT],
                ResearchReadError::KernelContractViolation,
            ),
        ] {
            let mut model = Model::new(&content(10), vec![step]);
            model.samples = samples.into();
            let mut state = Loop::new();
            assert_eq!(
                drive_in(&mut model, ReadBudget::maximum(), &mut state),
                Some(expected),
                "{name}"
            );
            let counters = state.counters;
            assert_eq!(
                (
                    counters.pread_attempts,
                    counters.requested_bytes,
                    counters.fstat_attempts
                ),
                (1, 4096, 4),
                "{name}"
            );
            assert_eq!((model.reads, model.stats.len()), (1, 4), "{name}");
            assert_eq!((state.filled, counters.eintr_retries), (0, 0), "{name}");
        }
        // EINTR exhaustion: three charged attempts, two granted retries,
        // three counted interrupts.
        let eintr = Step::Raw(PreadResult::Errno(EINTR));
        let mut model = Model::new(&content(10), vec![eintr, eintr, eintr]);
        let mut state = Loop::new();
        assert_eq!(
            drive_in(&mut model, ReadBudget::maximum(), &mut state),
            Some(ResearchReadError::RetryExhausted)
        );
        assert_eq!(
            snapshot(&state),
            (
                0,
                3,
                3,
                ReadCounters {
                    pread_attempts: 3,
                    eintr_retries: 2,
                    requested_bytes: 3 * 4096,
                    captured_bytes: 0,
                    fstat_attempts: 8,
                }
            )
        );
    }

    /// A panic between two actions leaves the credit of the call that never
    /// ran charged (debit precedes the call); the counters are charges, not
    /// a record of completed system calls.
    #[test]
    fn a_panic_between_actions_leaves_its_credit_charged() {
        // (panic before read n, panic before fstat n, charged (pread,
        // requested, fstat), calls that ran (reads, fstats)).
        type Point = (
            Option<usize>,
            Option<usize>,
            (usize, usize, usize),
            (usize, usize),
        );
        let points: [Point; 6] = [
            (None, Some(1), (0, 0, 2), (0, 0)),
            (None, Some(2), (0, 0, 2), (0, 1)),
            (Some(1), None, (1, 4096, 2), (0, 2)),
            (None, Some(3), (1, 4096, 4), (1, 2)),
            (None, Some(4), (1, 4096, 4), (1, 3)),
            // 10 bytes: the final pair starts at fstat 7.
            (None, Some(7), (2, 8192, 8), (2, 6)),
        ];
        for (read, stat, charged, ran) in points {
            let mut model = Model::new(&content(10), vec![]);
            model.panic_on_read = read;
            model.panic_on_stat = stat;
            let mut state = Loop::new();
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drive_in(&mut model, ReadBudget::maximum(), &mut state)
            }));
            assert!(unwound.is_err(), "{read:?} {stat:?}");
            let counters = state.counters;
            assert_eq!(
                (
                    counters.pread_attempts,
                    counters.requested_bytes,
                    counters.fstat_attempts
                ),
                charged,
                "{read:?} {stat:?}"
            );
            assert_eq!((model.reads, model.stats.len()), ran, "{read:?} {stat:?}");
        }
    }

    /// An attempt is charged only while its post pair and the final pair
    /// stay reserved; a pair charge never exceeds the ceiling.
    #[test]
    fn attempt_charges_keep_the_final_pair_reserved() {
        let max = ReadBudget::maximum();
        let mut state = Loop::new();
        state.counters.fstat_attempts = MAX_FSTAT_ATTEMPTS - 4;
        assert_eq!(state.charge_attempt(&max, 4096), Ok(1));
        state.counters.fstat_attempts = MAX_FSTAT_ATTEMPTS - 3;
        let before = snapshot(&state);
        assert_eq!(
            state.charge_attempt(&max, 4096),
            Err(ResearchReadError::InvalidBudget)
        );
        assert_eq!(snapshot(&state), before, "nothing debited on refusal");

        state.counters.fstat_attempts = MAX_FSTAT_ATTEMPTS - 2;
        assert_eq!(state.charge_metadata_pair(), Ok(MAX_FSTAT_ATTEMPTS));
        let before = snapshot(&state);
        assert_eq!(
            state.charge_metadata_pair(),
            Err(ResearchReadError::InvalidBudget)
        );
        assert_eq!(snapshot(&state), before);
    }

    /// Ceiling edges of each checked update, and refusal (never wrapping,
    /// saturating or panicking) at the arithmetic limit, with nothing
    /// changed.
    #[test]
    fn checked_counters_refuse_at_ceilings_and_overflow() {
        let max = ReadBudget::maximum();
        let refuse =
            |state: &mut Loop, update: &dyn Fn(&mut Loop) -> Result<(), ResearchReadError>| {
                let before = snapshot(state);
                assert_eq!(update(state), Err(ResearchReadError::InvalidBudget));
                assert_eq!(snapshot(state), before);
            };
        let attempt = |request: usize| {
            move |s: &mut Loop| {
                s.charge_attempt(&ReadBudget::maximum(), request)
                    .map(|_| ())
            }
        };

        let mut state = Loop::new();
        state.counters.pread_attempts = MAX_PREAD_ATTEMPTS - 1;
        assert_eq!(state.charge_attempt(&max, 1), Ok(MAX_PREAD_ATTEMPTS));
        refuse(&mut state, &attempt(1));
        let mut state = Loop::new();
        state.counters.pread_attempts = usize::MAX;
        refuse(&mut state, &attempt(1));

        let mut state = Loop::new();
        state.counters.requested_bytes = MAX_REQUESTED_BYTES - 4096;
        assert_eq!(state.charge_attempt(&max, 4096), Ok(1));
        refuse(&mut state, &attempt(1));
        let mut state = Loop::new();
        state.counters.requested_bytes = usize::MAX;
        refuse(&mut state, &attempt(1));
        let mut state = Loop::new();
        refuse(&mut state, &attempt(0));
        refuse(&mut state, &attempt(MAX_REQUEST_BYTES + 1));
        refuse(&mut state, &attempt(usize::MAX));

        let mut state = Loop::new();
        state.counters.fstat_attempts = usize::MAX;
        refuse(&mut state, &attempt(1));
        refuse(&mut state, &|s| s.charge_metadata_pair().map(|_| ()));
        state.counters.fstat_attempts = usize::MAX - 1;
        refuse(&mut state, &|s| s.charge_metadata_pair().map(|_| ()));

        let mut state = Loop::new();
        assert_eq!(state.count_interrupt(&max), Ok(false));
        assert_eq!(state.count_interrupt(&max), Ok(false));
        assert_eq!(state.count_interrupt(&max), Ok(true), "third consecutive");
        let mut state = Loop::new();
        state.total_eintr = MAX_TOTAL_EINTR_RETRIES;
        assert_eq!(state.count_interrupt(&max), Ok(true), "ninth in total");
        let mut state = Loop::new();
        state.consecutive_eintr = usize::MAX;
        refuse(&mut state, &|s| {
            s.count_interrupt(&ReadBudget::maximum()).map(|_| ())
        });
        let mut state = Loop::new();
        state.total_eintr = usize::MAX;
        refuse(&mut state, &|s| {
            s.count_interrupt(&ReadBudget::maximum()).map(|_| ())
        });

        let mut state = Loop::new();
        state.counters.eintr_retries = usize::MAX;
        refuse(&mut state, &|s| s.grant_retry());

        let mut state = Loop::new();
        state.filled = MAX_CAPTURE_BYTES - 1;
        state.consecutive_eintr = 2;
        refuse(&mut state, &|s| s.advance(&ReadBudget::maximum(), 2));
        assert_eq!(state.advance(&max, 1), Ok(()));
        assert_eq!(
            (state.filled, state.consecutive_eintr),
            (MAX_CAPTURE_BYTES, 0)
        );
        let mut state = Loop::new();
        state.filled = usize::MAX;
        refuse(&mut state, &|s| s.advance(&ReadBudget::maximum(), 1));
    }
}
