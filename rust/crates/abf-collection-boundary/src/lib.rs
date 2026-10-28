//! ABF M1 collection boundary.
//!
//! This crate is the first, deliberately narrow layer of an ABF collector:
//!
//! - an explicit [`identity::EnvironmentIdentity`] (Windows or one WSL distro),
//!   a [`identity::SourceInstance`] (one harness root) and a caller-chosen
//!   scan run;
//! - an explicit [`scope::CollectionScope`] over a caller-authorized root,
//!   with conservative rejection of traversal, symbolic links, hard links,
//!   non-regular files and cross-OS paths. These checks are path-based and
//!   **not race-free**, so production reuse is blocked;
//! - [`observation::Observation`] values built from allowlisted metadata
//!   fields. The checks are syntactic, not a redaction guarantee. The values
//!   keep physical occurrence, claimed native ids, environment, harness and
//!   model provider apart;
//! - a synthetic-only ingestion path ([`synthetic`]) over a documented fixture
//!   format.
//!
//! [`claude_jsonl`] adds a fictional Claude-shaped JSONL path (ABF134D) that delegates line
//! parsing to `abf-parser`.
//!
//! [`claude_replay`] optionally compares retained Claude observation claims within one
//! bounded batch, separately from the unchanged raw summary. It establishes no unique
//! physical-event or billed-consumption count.
//!
//! It is **not** a Codex or OpenCode parser, performs no
//! canonicalization, and never discovers user directories
//! on its own. It is meant for small, stable, trusted synthetic fixtures. See
//! `README.md` for the contract, known limitations and deferred work.

#![forbid(unsafe_code)]

pub mod claude_demo;
pub mod claude_jsonl;
pub mod claude_replay;
pub mod claude_summary;
pub mod identity;
pub mod observation;
pub mod scope;
pub mod synthetic;

pub use claude_jsonl::{
    CLAUDE_EXTENSION, ClaudeBatch, ClaudeCollectError, ClaudeFileRejection, ClaudeFileReport,
    ClaudeFileStatus, ClaudeLine, ClaudeLineCounts, ClaudeLineOutcome, ClaudeReadLimits,
    ClaudeRecordRef, ClaudeTotals, MAX_FILES_CEILING, MAX_LINE_BYTES_CEILING, MAX_RECORDS_CEILING,
    MAX_TOTAL_BYTES_CEILING, collect_claude_jsonl_file, collect_claude_jsonl_memory,
    collect_claude_jsonl_root,
};
pub use claude_summary::{
    Caveats, ClaimStates, ClaudeComponents, ClaudeSummary, ComponentSummary, ComponentValue,
    CountingBasis, DuplicateIdentity, FileSummary, IndeterminateReason, LineSummary, RecordCounts,
    RejectionCounts, SkipSummary, summarize_claude_batch,
};
pub use identity::{
    EnvironmentIdentity, EnvironmentInstance, EnvironmentKind, Harness, IdentityError, OpaqueId,
    SourceInstance, WslDistroName,
};
pub use observation::{
    AccountScope, CaptureTime, ClaimedIds, CollectError, CollectionBatch, Completeness, Field,
    FileObjectRef, FileRejection, FileReport, FileStatus, Observation, ObservationOrigin,
    OccurrenceIdentity, ParseStatus, ProhibitedClass, RecordOutcome, RecordRejection,
    RejectedRecord, ScanRun, SkipReason, SkippedEntry, UsageComponents,
};
pub use scope::{
    AuthorizedRoot, CollectionLimits, CollectionScope, HostFamily, HostMatch, HostPolicy,
    MAX_DEPTH_CEILING, MAX_ENTRIES_CEILING, MAX_FILE_BYTES_CEILING, PathRejection, RootRejection,
    ScopeError, check_root_text,
};
pub use synthetic::{
    FIXTURE_EXTENSION, FIXTURE_FORMAT_VERSION, FIXTURE_HEADER, collect_synthetic_file,
    collect_synthetic_root,
};
