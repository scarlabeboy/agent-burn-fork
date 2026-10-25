//! Claude-shaped JSONL collection below an explicit, caller-authorized root (ABF134D).
//!
//! Reads `*.jsonl` files recursively below the scope root, splits them into LF-terminated
//! lines with bounded buffering and hands each line to
//! [`abf_parser::parse_claude_long`]. Only allowlisted parser claims, line outcomes and
//! synthetic provenance are returned. Message content is never retained, and no path appears
//! in a result: files are identified by their ordinal in the sorted candidate list.
//!
//! Scope of use: disposable fictional trees. It does not discover directories, deduplicate,
//! normalize, persist, or establish physical provenance, and it has not been run on real
//! histories. The root must be an [`AuthorizedRoot`] built by the caller.
//!
//! # Guarantees and limits
//!
//! - **Containment** is the path-based, non-atomic check of this crate (Contract A V2):
//!   symbolic links, detected reparse points, hard links, traversal and non-regular files are
//!   refused or skipped and reported; a swap between check and open is not detected.
//! - **Reads** are streamed in 64 KiB chunks. A line over `max_line_bytes` is discarded as it
//!   streams (never buffered beyond the limit) and reported as `Oversized`. Each file's
//!   identity, length and modification time are compared before, during and after the read;
//!   growth, shrinkage or a metadata change rejects the whole file as `ChangedDuringRead` and
//!   emits nothing from it. This is not a coherent snapshot: rewrites that preserve the
//!   checked metadata are not detected.
//! - **Framing:** a record is a complete LF-terminated line. Trailing bytes without a final
//!   LF are counted in `pending_tail_bytes` and never parsed. Whitespace-only lines are
//!   counted as blank. A trailing CR is JSON whitespace.
//! - **Budgets:** files per batch, bytes per file (the scope's `max_file_bytes`), bytes per
//!   batch, line length and retained line outcomes are all explicit. Exceeding one rejects the
//!   affected file (or the batch, for the file count) with a coded reason; nothing is truncated
//!   and nothing becomes zero. Blank lines are counted, not retained.
//! - **Completeness** describes traversal and reading only. Rejected, oversized and blank lines
//!   and pending tails keep a batch `Complete` and must never be read as zero usage: inspect
//!   [`ClaudeBatch::totals`].

use std::{
    fs::File,
    io::{self, Read},
};

use abf_parser::{ClaudeObservation, ClaudeParseResult, Code, LongLimits, parse_claude_long};

use crate::identity::{EnvironmentIdentity, Harness, OpaqueId, SourceInstance};
use crate::observation::{CollectError, Completeness, FileObjectRef, ScanRun, SkippedEntry};
use crate::scope::{
    CollectionScope, HostMatch, PathRejection, ResolvedFile, file_object, hard_link_count,
};

/// Only files with this extension are candidates.
pub const CLAUDE_EXTENSION: &str = ".jsonl";
/// Ceilings for [`ClaudeReadLimits`].
pub const MAX_FILES_CEILING: usize = 4096;
pub const MAX_TOTAL_BYTES_CEILING: u64 = 1024 * 1024 * 1024;
pub const MAX_LINE_BYTES_CEILING: usize = LongLimits::MAX.record_bytes;
pub const MAX_RECORDS_CEILING: usize = 4096;

const CHUNK_BYTES: usize = 64 * 1024;

/// Per-batch budgets, each in `1..=<ceiling>`. The per-file byte limit, traversal depth and
/// directory-entry limit come from the scope's `CollectionLimits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaudeReadLimits {
    /// Candidate files. More candidates fail the batch: no order-dependent subset.
    pub max_files: usize,
    /// Sum of the sizes of files opened for reading in one batch.
    pub max_total_bytes: u64,
    /// Longest parsed line, excluding the LF; also the parser's record ceiling.
    pub max_line_bytes: usize,
    /// Retained line outcomes (observations, rejections, oversized lines) in one batch.
    pub max_records: usize,
}

impl ClaudeReadLimits {
    fn is_valid(self) -> bool {
        (1..=MAX_FILES_CEILING).contains(&self.max_files)
            && (1..=MAX_TOTAL_BYTES_CEILING).contains(&self.max_total_bytes)
            && (1..=MAX_LINE_BYTES_CEILING).contains(&self.max_line_bytes)
            && (1..=MAX_RECORDS_CEILING).contains(&self.max_records)
    }
}

impl Default for ClaudeReadLimits {
    fn default() -> Self {
        Self {
            max_files: 64,
            max_total_bytes: 64 * 1024 * 1024,
            max_line_bytes: 1024 * 1024,
            max_records: 1024,
        }
    }
}

/// A collection that produced no batch. None of these may be read as "no usage".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeCollectError {
    InvalidLimits,
    /// The scope's source is not a Claude Code source.
    WrongHarness,
    /// Root unreadable or replaced, or more directory entries than the scope allows.
    Boundary(CollectError),
    /// More candidate files than [`ClaudeReadLimits::max_files`].
    FileLimitExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeFileRejection {
    /// The entry could not be resolved to a safe regular file (e.g. replaced after listing).
    Path(PathRejection),
    /// Larger than the scope's per-file limit.
    TooLarge,
    /// Not enough of the batch byte budget left to read this file.
    ByteBudget,
    /// Retained line outcome budget exhausted; nothing from this file is emitted.
    OutputLimit,
    /// Identity, length or modification time changed, or the byte count read differed from
    /// the length at open. Nothing from this file is emitted.
    ChangedDuringRead,
    Io(io::ErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeFileStatus {
    Read,
    Rejected(ClaudeFileRejection),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeLineOutcome {
    Observation(Box<ClaudeObservation>),
    /// The parser refused the line with a fixed code. This is not zero usage.
    Rejected(Code),
    /// Longer than `max_line_bytes`; discarded unparsed. This is not zero usage.
    Oversized,
}

/// One retained line outcome with its physical locator inside the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeLine {
    /// 1-based physical line number.
    pub line_ordinal: u64,
    /// Offset of the line start from the start of the file.
    pub byte_offset: u64,
    /// Line length in bytes, excluding the LF (the real length even if oversized).
    pub byte_length: u64,
    pub outcome: ClaudeLineOutcome,
}

/// Separate tallies of every LF-terminated line in a file that was read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaudeLineCounts {
    pub terminated_lines: u64,
    pub observations: u64,
    pub rejected: u64,
    pub oversized: u64,
    pub blank: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClaudeFileReport {
    /// Position in the sorted candidate list; the only file identifier in a result.
    pub file_ordinal: u32,
    pub file_object: FileObjectRef,
    pub status: ClaudeFileStatus,
    /// Retained outcomes in file order; empty unless `status` is `Read`.
    pub lines: Vec<ClaudeLine>,
    pub counts: ClaudeLineCounts,
    /// Bytes after the last LF: an unterminated, possibly in-progress record. Never parsed.
    pub pending_tail_bytes: u64,
}

/// Result of one root collection. Independent of every other batch; nothing merges them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClaudeBatch {
    pub scan_run: ScanRun,
    pub environment: EnvironmentIdentity,
    pub source: SourceInstance,
    pub host_match: HostMatch,
    pub files: Vec<ClaudeFileReport>,
    /// Sorted. Names are removed: only the reason is kept.
    pub skipped: Vec<SkippedEntry>,
    pub completeness: Completeness,
}

/// An observation with its complete synthetic provenance. Claimed ids stay claims.
#[derive(Clone, Debug)]
pub struct ClaudeRecordRef<'a> {
    pub scan_run: &'a OpaqueId,
    pub environment: &'a EnvironmentIdentity,
    pub source: &'a SourceInstance,
    pub host_match: HostMatch,
    pub file_ordinal: u32,
    pub file_object: FileObjectRef,
    pub line_ordinal: u64,
    pub byte_offset: u64,
    pub byte_length: u64,
    pub observation: &'a ClaudeObservation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaudeTotals {
    pub files_read: u64,
    pub files_rejected: u64,
    pub counts: ClaudeLineCounts,
    pub files_with_pending_tail: u64,
    pub pending_tail_bytes: u64,
}

impl ClaudeBatch {
    pub fn observations(&self) -> impl Iterator<Item = ClaudeRecordRef<'_>> {
        self.files.iter().flat_map(move |file| {
            file.lines
                .iter()
                .filter_map(move |line| match &line.outcome {
                    ClaudeLineOutcome::Observation(observation) => Some(ClaudeRecordRef {
                        scan_run: &self.scan_run.id,
                        environment: &self.environment,
                        source: &self.source,
                        host_match: self.host_match,
                        file_ordinal: file.file_ordinal,
                        file_object: file.file_object,
                        line_ordinal: line.line_ordinal,
                        byte_offset: line.byte_offset,
                        byte_length: line.byte_length,
                        observation,
                    }),
                    _ => None,
                })
        })
    }

    pub fn totals(&self) -> ClaudeTotals {
        let mut totals = ClaudeTotals::default();
        for file in &self.files {
            match file.status {
                ClaudeFileStatus::Read => totals.files_read += 1,
                ClaudeFileStatus::Rejected(_) => totals.files_rejected += 1,
            }
            totals.counts.terminated_lines += file.counts.terminated_lines;
            totals.counts.observations += file.counts.observations;
            totals.counts.rejected += file.counts.rejected;
            totals.counts.oversized += file.counts.oversized;
            totals.counts.blank += file.counts.blank;
            if file.pending_tail_bytes > 0 {
                totals.files_with_pending_tail += 1;
                totals.pending_tail_bytes += file.pending_tail_bytes;
            }
        }
        totals
    }
}

/// Collects every `*.jsonl` file below the scope root. The scope's source must be a Claude
/// Code source. The root is revalidated before enumeration, around every file and at the end;
/// any failure discards the whole collection (path-based, not race-free).
pub fn collect_claude_jsonl_root(
    scope: &CollectionScope,
    run: &ScanRun,
    limits: ClaudeReadLimits,
) -> Result<ClaudeBatch, ClaudeCollectError> {
    if !limits.is_valid() {
        return Err(ClaudeCollectError::InvalidLimits);
    }
    if scope.source().harness() != Harness::ClaudeCode {
        return Err(ClaudeCollectError::WrongHarness);
    }
    let revalidate = || {
        scope
            .root()
            .revalidate()
            .map_err(|rejection| ClaudeCollectError::Boundary(CollectError::RootChanged(rejection)))
    };
    revalidate()?;
    let enumeration = scope
        .enumerate_with(CLAUDE_EXTENSION)
        .map_err(ClaudeCollectError::Boundary)?;
    if enumeration.candidates.len() > limits.max_files {
        return Err(ClaudeCollectError::FileLimitExceeded);
    }
    let parser = LongLimits {
        record_bytes: limits.max_line_bytes,
        ..LongLimits::MAX
    };
    let mut remaining_bytes = limits.max_total_bytes;
    let mut remaining_records = limits.max_records;
    let mut files = Vec::with_capacity(enumeration.candidates.len());
    for (index, relative) in enumeration.candidates.iter().enumerate() {
        let file_ordinal =
            u32::try_from(index).map_err(|_| ClaudeCollectError::FileLimitExceeded)?;
        revalidate()?;
        let mut report = ClaudeFileReport {
            file_ordinal,
            file_object: FileObjectRef::Unavailable,
            status: ClaudeFileStatus::Read,
            lines: Vec::new(),
            counts: ClaudeLineCounts::default(),
            pending_tail_bytes: 0,
        };
        match scope.resolve_regular_file_with(relative, CLAUDE_EXTENSION) {
            Err(rejection) => {
                report.status = ClaudeFileStatus::Rejected(ClaudeFileRejection::Path(rejection));
            }
            Ok(resolved) => {
                report.file_object = resolved.object;
                let outcome = if resolved.len > scope.limits().max_file_bytes {
                    Err(ClaudeFileRejection::TooLarge)
                } else if resolved.len > remaining_bytes {
                    Err(ClaudeFileRejection::ByteBudget)
                } else {
                    remaining_bytes -= resolved.len;
                    read_lines(&resolved, &limits, parser, remaining_records)
                };
                match outcome {
                    Ok(scanned) => {
                        remaining_records -= scanned.lines.len();
                        report.lines = scanned.lines;
                        report.counts = scanned.counts;
                        report.pending_tail_bytes = scanned.pending_tail_bytes;
                    }
                    Err(rejection) => report.status = ClaudeFileStatus::Rejected(rejection),
                }
            }
        }
        revalidate()?;
        files.push(report);
    }
    revalidate()?;
    let mut skipped: Vec<SkippedEntry> = enumeration
        .skipped
        .into_iter()
        .map(|entry| SkippedEntry {
            relative_path: None,
            reason: entry.reason,
        })
        .collect();
    skipped.sort();
    let incomplete = skipped.iter().any(|entry| entry.reason.makes_incomplete())
        || files
            .iter()
            .any(|file| file.status != ClaudeFileStatus::Read);
    Ok(ClaudeBatch {
        scan_run: run.clone(),
        environment: scope.environment().clone(),
        source: scope.source().clone(),
        host_match: scope.host_match(),
        files,
        skipped,
        completeness: if incomplete {
            Completeness::Incomplete
        } else {
            Completeness::Complete
        },
    })
}

#[derive(Debug)]
struct Scanned {
    lines: Vec<ClaudeLine>,
    counts: ClaudeLineCounts,
    pending_tail_bytes: u64,
}

// Line state across chunks. `buffer` holds a line only while it is within `max_line_bytes`;
// once exceeded it is dropped and only the length keeps counting.
struct LineState {
    buffer: Vec<u8>,
    length: u64,
    start: u64,
    oversized: bool,
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

fn read_lines(
    resolved: &ResolvedFile,
    limits: &ClaudeReadLimits,
    parser: LongLimits,
    record_budget: usize,
) -> Result<Scanned, ClaudeFileRejection> {
    let io = |error: io::Error| ClaudeFileRejection::Io(error.kind());
    let mut file = File::open(&resolved.path).map_err(io)?;
    let before = file.metadata().map_err(io)?;
    if !before.is_file()
        || file_object(&before) != resolved.object
        || before.len() != resolved.len
        || before.modified().map_err(io)? != resolved.modified
    {
        return Err(ClaudeFileRejection::ChangedDuringRead);
    }
    if hard_link_count(&before) > 1 {
        return Err(ClaudeFileRejection::Path(PathRejection::HardLinked));
    }

    let mut scanned = Scanned {
        lines: Vec::new(),
        counts: ClaudeLineCounts::default(),
        pending_tail_bytes: 0,
    };
    let mut state = LineState {
        buffer: Vec::new(),
        length: 0,
        start: 0,
        oversized: false,
    };
    let mut chunk = vec![0u8; CHUNK_BYTES];
    // One byte beyond the recorded length detects growth without reading further.
    let mut reader = (&mut file).take(resolved.len.saturating_add(1));
    let mut total = 0u64;
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        };
        total += read as u64;
        if total > resolved.len {
            return Err(ClaudeFileRejection::ChangedDuringRead);
        }
        let mut rest = &chunk[..read];
        while !rest.is_empty() {
            let (piece, terminated) = match rest.iter().position(|&byte| byte == b'\n') {
                Some(index) => (&rest[..index], true),
                None => (rest, false),
            };
            state.length += piece.len() as u64;
            if !state.oversized {
                if state.buffer.len() + piece.len() > limits.max_line_bytes {
                    state.oversized = true;
                    state.buffer = Vec::new();
                } else {
                    state.buffer.extend_from_slice(piece);
                }
            }
            if terminated {
                finish_line(&mut state, &mut scanned, parser, record_budget)?;
                rest = &rest[piece.len() + 1..];
            } else {
                rest = &[];
            }
        }
    }
    let after = file.metadata().map_err(io)?;
    if total != resolved.len
        || file_object(&after) != resolved.object
        || after.len() != resolved.len
        || after.modified().map_err(io)? != resolved.modified
    {
        return Err(ClaudeFileRejection::ChangedDuringRead);
    }
    scanned.pending_tail_bytes = state.length;
    Ok(scanned)
}

fn finish_line(
    state: &mut LineState,
    scanned: &mut Scanned,
    parser: LongLimits,
    record_budget: usize,
) -> Result<(), ClaudeFileRejection> {
    scanned.counts.terminated_lines += 1;
    let line_ordinal = scanned.counts.terminated_lines;
    let outcome = if state.oversized {
        scanned.counts.oversized += 1;
        Some(ClaudeLineOutcome::Oversized)
    } else if is_blank(&state.buffer) {
        scanned.counts.blank += 1;
        None
    } else {
        Some(match parse_claude_long(&state.buffer, None, parser) {
            ClaudeParseResult::Observation(observation) => {
                scanned.counts.observations += 1;
                ClaudeLineOutcome::Observation(Box::new(observation))
            }
            ClaudeParseResult::Rejected(code) => {
                scanned.counts.rejected += 1;
                ClaudeLineOutcome::Rejected(code)
            }
        })
    };
    if let Some(outcome) = outcome {
        if scanned.lines.len() >= record_budget {
            return Err(ClaudeFileRejection::OutputLimit);
        }
        scanned.lines.push(ClaudeLine {
            line_ordinal,
            byte_offset: state.start,
            byte_length: state.length,
            outcome,
        });
    }
    state.start += state.length + 1;
    state.length = 0;
    state.oversized = false;
    state.buffer.clear();
    Ok(())
}

#[cfg(test)]
mod tests;
