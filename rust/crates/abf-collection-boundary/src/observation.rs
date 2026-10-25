//! Values emitted by the collection boundary.
//!
//! When the synthetic collector produces these values:
//! - string fields come only from allowlisted fixture keys whose values passed
//!   a narrow charset check;
//! - every rejection is an enumerated code;
//! - values that fail validation keep only their byte length.
//!
//! That is a property of collector-produced values, not of these types. The
//! structs have public, mutable fields (`non_exhaustive` blocks construction,
//! not mutation), and nothing re-validates them after collection. The
//! validation is also only syntactic: allowed identifiers, labels, model
//! strings, file names and relative paths can still contain sensitive
//! information. `Debug` output includes them and is not a safe export
//! format. Production redaction and export policy are deferred.

use std::io;

use crate::identity::{EnvironmentIdentity, Harness, OpaqueId, SourceInstance};
use crate::scope::{HostMatch, PathRejection, RootRejection};

/// A possibly-missing value. Non-`Value` states must never be read as zero.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field<T> {
    Value(T),
    /// The key was not present in the record.
    Absent,
    /// The key was present but its value was outside the accepted form.
    /// Only the value's byte length is kept.
    Unparseable {
        byte_length: usize,
    },
    /// The boundary does not collect this field.
    NotCollected,
}

impl<T> Field<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_unparseable(&self) -> bool {
        matches!(self, Self::Unparseable { .. })
    }
}

/// Capture time supplied by the caller. The boundary does not read a clock,
/// so identical inputs give identical outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaptureTime {
    NotRecorded,
    UnixMicros(i64),
}

/// One collection pass, identified by the caller.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScanRun {
    pub id: OpaqueId,
    pub capture_time: CaptureTime,
}

impl ScanRun {
    pub fn new(id: OpaqueId, capture_time: CaptureTime) -> Self {
        Self { id, capture_time }
    }
}

/// File-object evidence (I-FS) observed when the file was resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileObjectRef {
    Unix {
        dev: u64,
        ino: u64,
    },
    /// Not obtainable on this platform with the current implementation.
    Unavailable,
}

/// Where a record physically sat during one scan run.
///
/// This is a physical locator, **not** a cross-run occurrence identity:
/// without generation tracking (deferred), the same locator in two runs may
/// hold different bytes. It is therefore scoped by `scan_run`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct OccurrenceIdentity {
    pub scan_run: OpaqueId,
    pub environment: EnvironmentIdentity,
    pub source: SourceInstance,
    /// `/`-separated path relative to the authorized root.
    pub relative_path: String,
    pub file_object: FileObjectRef,
    /// 1-based physical line number in the file (the header is line 1).
    pub line_ordinal: u64,
    /// Byte offset of the record from the start of the file.
    pub byte_offset: u64,
    /// Record length in bytes, excluding the terminating newline.
    pub byte_length: u64,
}

/// Identifiers a record claims for itself. Equal claimed ids in different
/// occurrences are *not* treated as one logical event at this layer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ClaimedIds {
    pub message_id: Field<String>,
    pub request_id: Field<String>,
    pub session_id: Field<String>,
}

/// Raw token components as observed. No normalization, overlap model,
/// delta derivation or total reconciliation is applied.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct UsageComponents {
    pub input: Field<u64>,
    pub output: Field<u64>,
    pub cache_read: Field<u64>,
    pub cache_write: Field<u64>,
    pub reasoning: Field<u64>,
    pub total_reported: Field<u64>,
}

impl UsageComponents {
    pub(crate) fn all(&self) -> [&Field<u64>; 6] {
        [
            &self.input,
            &self.output,
            &self.cache_read,
            &self.cache_write,
            &self.reasoning,
            &self.total_reported,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParseStatus {
    Complete,
    /// At least one present field was [`Field::Unparseable`].
    Partial,
}

/// Account identity is never collected by this boundary. Verified or
/// declared account association is a separate, later concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccountScope {
    NotCollected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObservationOrigin {
    /// Parsed from an `abf-synthetic-fixture` file. Never a real harness log.
    SyntheticFixture {
        format_version: u32,
        host_match: HostMatch,
    },
}

/// One accepted usage observation, built from allowlisted fixture fields only.
/// See the module docs for what this does and does not guarantee.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Observation {
    pub occurrence: OccurrenceIdentity,
    pub claimed_ids: ClaimedIds,
    /// Underlying model provider as stated by the record; never inferred
    /// from the harness or the model name.
    pub model_provider: Field<String>,
    pub model: Field<String>,
    /// Raw event time string (charset-checked, not parsed).
    pub event_time_raw: Field<String>,
    pub project_label: Field<String>,
    pub usage: UsageComponents,
    pub parse_status: ParseStatus,
    pub account_scope: AccountScope,
    pub capture_time: CaptureTime,
    pub origin: ObservationOrigin,
}

impl Observation {
    pub fn harness(&self) -> Harness {
        self.occurrence.source.harness()
    }
}

/// Category of a prohibited field. The field name and value are not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProhibitedClass {
    Content,
    Credential,
    AccountIdentity,
    PricingOrQuota,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordRejection {
    InvalidUtf8,
    EmptyRecord,
    ControlCharacter,
    MalformedPair,
    InvalidKey,
    EmptyValue,
    UnknownField,
    DuplicateField(&'static str),
    ProhibitedField(ProhibitedClass),
    NoUsageComponents,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct RejectedRecord {
    pub occurrence: OccurrenceIdentity,
    pub reason: RecordRejection,
}

/// Every physical record line yields exactly one outcome; rejected records
/// are retained as diagnostics, never silently dropped.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordOutcome {
    Accepted(Box<Observation>),
    Rejected(RejectedRecord),
}

impl RecordOutcome {
    pub fn occurrence(&self) -> &OccurrenceIdentity {
        match self {
            Self::Accepted(observation) => &observation.occurrence,
            Self::Rejected(rejected) => &rejected.occurrence,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileRejection {
    Path(PathRejection),
    NotSyntheticFixture,
    TooLarge,
    /// Retained record budget exhausted; nothing from this file is emitted.
    OutputLimit,
    /// A limited read-change check failed:
    /// - the opened handle was not a regular file;
    /// - on Unix only, its file identity differed from the resolved one;
    /// - its length or modification time changed from resolution through
    ///   `open` to the end of the read;
    /// - the bytes read did not equal the length at `open`.
    ///
    /// Nothing from the file is emitted. This is not a coherent snapshot:
    /// rewrites preserving the checked metadata are not detected.
    ChangedDuringRead,
    Io(io::ErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileStatus {
    Read,
    Rejected(FileRejection),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct FileReport {
    pub relative_path: String,
    pub file_object: FileObjectRef,
    pub status: FileStatus,
    pub records: Vec<RecordOutcome>,
    /// Bytes after the last newline (an unterminated, possibly in-progress
    /// record). They are not parsed or counted in this run and do not affect
    /// [`Completeness`]. A non-zero value must not be read as zero usage.
    pub pending_tail_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
    Symlink,
    NotRegularFile,
    NonUtf8Name,
    InvalidName,
    NotFixtureExtension,
    DepthLimit,
    Path(PathRejection),
    Io(io::ErrorKind),
}

impl SkipReason {
    /// Whether this skip means in-scope data may have been missed. Only
    /// entries that can never be fixtures (wrong extension, sockets, FIFOs,
    /// devices) are out of scope.
    pub fn makes_incomplete(self) -> bool {
        !matches!(self, Self::NotFixtureExtension | Self::NotRegularFile)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct SkippedEntry {
    /// `None` when the name is not valid UTF-8 or not an acceptable name.
    pub relative_path: Option<String>,
    pub reason: SkipReason,
}

/// Traversal and read completeness of a batch, not usage completeness.
///
/// `Complete` means enumeration finished without a skip that may hide data,
/// and every candidate fixture was read (`FileStatus::Read`) and split into
/// newline-terminated records. It is not proof that all usage was collected,
/// parsed or counted. A `Complete` batch can contain rejected records,
/// `Partial` observations and non-zero `pending_tail_bytes`, and can even
/// have zero observations.
///
/// Neither `Incomplete`, nor rejected records, nor pending tails may ever be
/// read as "no usage".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Completeness {
    Complete,
    Incomplete,
}

/// A collection that produced no batch. None of these may be read as "no
/// usage".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectError {
    RootUnreadable(io::ErrorKind),
    /// The authorized root failed revalidation before, during or after the
    /// collection. Everything read in this collection is discarded.
    RootChanged(RootRejection),
    /// The tree holds more directory entries than
    /// [`crate::scope::CollectionLimits::max_entries`].
    EntryLimitExceeded,
}

/// Result of one root collection. Batches from different scopes or runs are
/// independent values; this crate offers no operation that merges them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CollectionBatch {
    pub scan_run: ScanRun,
    pub environment: EnvironmentIdentity,
    pub source: SourceInstance,
    pub host_match: HostMatch,
    /// Sorted by relative path.
    pub files: Vec<FileReport>,
    /// Sorted.
    pub skipped: Vec<SkippedEntry>,
    pub completeness: Completeness,
}

impl CollectionBatch {
    pub fn observations(&self) -> impl Iterator<Item = &Observation> {
        self.files
            .iter()
            .flat_map(|file| &file.records)
            .filter_map(|record| match record {
                RecordOutcome::Accepted(observation) => Some(observation.as_ref()),
                RecordOutcome::Rejected(_) => None,
            })
    }

    pub fn rejected_records(&self) -> impl Iterator<Item = &RejectedRecord> {
        self.files
            .iter()
            .flat_map(|file| &file.records)
            .filter_map(|record| match record {
                RecordOutcome::Accepted(_) => None,
                RecordOutcome::Rejected(rejected) => Some(rejected),
            })
    }
}
