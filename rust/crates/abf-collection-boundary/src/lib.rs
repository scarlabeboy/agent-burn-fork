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
//! It is **not** a Claude Code, Codex or OpenCode parser, performs no
//! deduplication or canonicalization, and never discovers user directories
//! on its own. It is meant for small, stable, trusted synthetic fixtures. See
//! `README.md` for the contract, known limitations and deferred work.

#![forbid(unsafe_code)]

pub mod identity;
pub mod observation;
pub mod scope;
pub mod synthetic;

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
