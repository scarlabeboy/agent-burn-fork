//! Synthetic fixture ingestion (format `abf-synthetic-fixture/1`).
//!
//! This is a test-only input format for exercising the boundary. It is not a
//! Claude Code, Codex or OpenCode log format, and real harness logs never
//! parse as fixtures because they lack the header line.
//!
//! # Format
//!
//! - File name ends with [`FIXTURE_EXTENSION`]; other files are never opened.
//! - UTF-8 text. Line 1 is exactly [`FIXTURE_HEADER`] followed by `\n`;
//!   otherwise the file is rejected after reading only the header bytes.
//! - Every further `\n`-terminated line is one record: tab-separated
//!   `key=value` pairs. Bytes after the last `\n` are counted as a pending
//!   tail and not parsed.
//! - Allowed keys: `message_id`, `request_id`, `session_id`,
//!   `model_provider`, `model`, `event_time`, `project_label`,
//!   `input_tokens`, `output_tokens`, `cache_read_tokens`,
//!   `cache_write_tokens`, `reasoning_tokens`, `total_tokens_reported`.
//! - Identifier-like values must match `[A-Za-z0-9._:/@-]{1,128}`,
//!   `event_time` must match `[0-9TZ:.+-]{1,40}`, token counts must be
//!   decimal `u64`. Otherwise the field is `Unparseable` (length only) and
//!   the record is `Partial`.
//! - A record is rejected if it has a prohibited key (content, credential,
//!   account, pricing or quota), an unknown key, a duplicate key, an empty
//!   value, control characters, or no usage component at all.
//!
//! # Scope of use
//!
//! For small, stable, trusted synthetic fixtures only.
//! - **Containment** is path-based and not race-free.
//! - **Reads** compare identity, length and modification time, but are not a
//!   coherent snapshot; rewrites preserving metadata can escape detection.
//! - **Memory:** at most 4096 record outcomes per call; input and directory
//!   limits still need to be kept small for these disposable fixtures.
//! - **Privacy:** allowlisted values can still be sensitive.
//!
//! See the crate README.

use std::{fs::File, io::Read};

use crate::observation::{
    AccountScope, ClaimedIds, CollectError, CollectionBatch, Completeness, Field, FileRejection,
    FileReport, FileStatus, Observation, ObservationOrigin, OccurrenceIdentity, ParseStatus,
    ProhibitedClass, RecordOutcome, RecordRejection, RejectedRecord, ScanRun, SkipReason,
    SkippedEntry, UsageComponents,
};
use crate::scope::{CollectionScope, PathRejection, ResolvedFile, file_object, hard_link_count};

pub const FIXTURE_FORMAT_VERSION: u32 = 1;
pub const FIXTURE_HEADER: &str = "abf-synthetic-fixture/1";
pub const FIXTURE_EXTENSION: &str = ".abf-fixture";

/// Maximum retained record outcomes per single-file call or entire root call.
/// Includes rejected records; exhaustion rejects the whole affected file.
pub const MAX_RECORD_OUTCOMES: usize = 4096;

const MAX_IDENTIFIER_LEN: usize = 128;
const MAX_EVENT_TIME_LEN: usize = 40;
const MAX_KEY_LEN: usize = 32;

const KEYS: [&str; 13] = [
    "message_id",
    "request_id",
    "session_id",
    "model_provider",
    "model",
    "event_time",
    "project_label",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "reasoning_tokens",
    "total_tokens_reported",
];
const MESSAGE_ID: usize = 0;
const REQUEST_ID: usize = 1;
const SESSION_ID: usize = 2;
const MODEL_PROVIDER: usize = 3;
const MODEL: usize = 4;
const EVENT_TIME: usize = 5;
const PROJECT_LABEL: usize = 6;
const INPUT_TOKENS: usize = 7;
const OUTPUT_TOKENS: usize = 8;
const CACHE_READ_TOKENS: usize = 9;
const CACHE_WRITE_TOKENS: usize = 10;
const REASONING_TOKENS: usize = 11;
const TOTAL_TOKENS_REPORTED: usize = 12;

/// Each class lists exact keys and `<word>_` prefixes. Allowed keys are
/// checked first, so e.g. `message_id` is allowed while `message_text` is not.
const PROHIBITED: [(ProhibitedClass, &[&str]); 4] = [
    (
        ProhibitedClass::Credential,
        &[
            "token",
            "access_token",
            "refresh_token",
            "id_token",
            "session_token",
            "api_key",
            "apikey",
            "secret",
            "password",
            "credential",
            "credentials",
            "oauth",
            "auth",
            "authorization",
            "bearer",
            "cookie",
        ],
    ),
    (
        ProhibitedClass::AccountIdentity,
        &[
            "account",
            "email",
            "user",
            "user_id",
            "org",
            "organization",
            "subscription",
            "plan",
        ],
    ),
    (
        ProhibitedClass::PricingOrQuota,
        &[
            "cost",
            "price",
            "pricing",
            "billing",
            "usd",
            "quota",
            "rate_limit",
            "limit",
        ],
    ),
    (
        ProhibitedClass::Content,
        &[
            "prompt",
            "content",
            "message",
            "messages",
            "text",
            "response",
            "completion",
            "conversation",
            "transcript",
            "thinking",
            "reasoning",
            "summary",
            "body",
            "raw",
            "line",
            "cwd",
            "input",
            "output",
            "tool",
            "system",
            "attachment",
            "file",
        ],
    ),
];

/// Collects one fixture file named by a `/`-separated path relative to the
/// scope root. Path problems are returned as `Err` before any file is opened.
/// Read and header failures return `Ok` with a rejected [`FileStatus`].
///
/// The root is revalidated before resolution and again after the read; if
/// either check fails, the report is discarded (path-based, not race-free).
pub fn collect_synthetic_file(
    scope: &CollectionScope,
    run: &ScanRun,
    relative_path: &str,
) -> Result<FileReport, PathRejection> {
    let mut remaining_records = MAX_RECORD_OUTCOMES;
    collect_file_with_budget(scope, run, relative_path, &mut remaining_records)
}

fn collect_file_with_budget(
    scope: &CollectionScope,
    run: &ScanRun,
    relative_path: &str,
    remaining_records: &mut usize,
) -> Result<FileReport, PathRejection> {
    let revalidate = || {
        scope
            .root()
            .revalidate()
            .map_err(PathRejection::RootChanged)
    };
    revalidate()?;
    let resolved = scope.resolve_regular_file(relative_path)?;
    let report = read_fixture(scope, run, &resolved, remaining_records);
    revalidate()?;
    Ok(report)
}

/// Collects every fixture file below the scope root.
///
/// The root is revalidated before enumeration, around every file and at the
/// end; any failure discards the whole collection (path-based, not
/// race-free).
pub fn collect_synthetic_root(
    scope: &CollectionScope,
    run: &ScanRun,
) -> Result<CollectionBatch, CollectError> {
    let revalidate = || scope.root().revalidate().map_err(CollectError::RootChanged);
    revalidate()?;
    let enumeration = scope.enumerate()?;
    let mut skipped = enumeration.skipped;
    let mut files = Vec::with_capacity(enumeration.candidates.len());
    let mut remaining_records = MAX_RECORD_OUTCOMES;
    for relative in enumeration.candidates {
        match collect_file_with_budget(scope, run, &relative, &mut remaining_records) {
            Ok(report) => files.push(report),
            Err(PathRejection::RootChanged(rejection)) => {
                return Err(CollectError::RootChanged(rejection));
            }
            // The entry changed between listing and resolution.
            Err(rejection) => skipped.push(SkippedEntry {
                relative_path: Some(relative),
                reason: SkipReason::Path(rejection),
            }),
        }
    }
    revalidate()?;
    skipped.sort();
    let incomplete = skipped.iter().any(|entry| entry.reason.makes_incomplete())
        || files.iter().any(|file| file.status != FileStatus::Read);
    Ok(CollectionBatch {
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

fn read_fixture(
    scope: &CollectionScope,
    run: &ScanRun,
    resolved: &ResolvedFile,
    remaining_records: &mut usize,
) -> FileReport {
    let mut report = FileReport {
        relative_path: resolved.relative.clone(),
        file_object: resolved.object,
        status: FileStatus::Read,
        records: Vec::new(),
        pending_tail_bytes: 0,
    };
    match read_bounded(resolved, scope.limits().max_file_bytes) {
        Ok(bytes) => {
            let Some(body) = bytes.get(FIXTURE_HEADER.len() + 1..) else {
                report.status = FileStatus::Rejected(FileRejection::NotSyntheticFixture);
                return report;
            };
            let records = body
                .iter()
                .filter(|&&byte| byte == b'\n')
                .take(*remaining_records + 1)
                .count();
            if records > *remaining_records {
                report.status = FileStatus::Rejected(FileRejection::OutputLimit);
            } else {
                *remaining_records -= records;
                parse_records(&bytes, scope, run, &mut report);
            }
        }
        Err(rejection) => report.status = FileStatus::Rejected(rejection),
    }
    report
}

fn read_bounded(resolved: &ResolvedFile, max_bytes: u64) -> Result<Vec<u8>, FileRejection> {
    let io = |error: std::io::Error| FileRejection::Io(error.kind());
    if resolved.len > max_bytes || max_bytes < (FIXTURE_HEADER.len() + 1) as u64 {
        return Err(FileRejection::TooLarge);
    }
    let mut file = File::open(&resolved.path).map_err(io)?;
    let before = file.metadata().map_err(io)?;
    if !before.is_file()
        || file_object(&before) != resolved.object
        || before.len() != resolved.len
        || before.modified().map_err(io)? != resolved.modified
    {
        return Err(FileRejection::ChangedDuringRead);
    }
    if hard_link_count(&before) > 1 {
        return Err(FileRejection::Path(PathRejection::HardLinked));
    }

    let header_len = FIXTURE_HEADER.len() + 1;
    let mut bytes = vec![0u8; header_len];
    match file.read_exact(&mut bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(FileRejection::NotSyntheticFixture);
        }
        Err(error) => return Err(io(error)),
    }
    if &bytes[..FIXTURE_HEADER.len()] != FIXTURE_HEADER.as_bytes() || bytes[header_len - 1] != b'\n'
    {
        return Err(FileRejection::NotSyntheticFixture);
    }

    let remaining = max_bytes
        .saturating_add(1)
        .saturating_sub(header_len as u64);
    (&mut file)
        .take(remaining)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(FileRejection::TooLarge);
    }
    let after = file.metadata().map_err(io)?;
    if file_object(&after) != resolved.object
        || after.len() != before.len()
        || after.modified().map_err(io)? != resolved.modified
        || bytes.len() as u64 != before.len()
    {
        return Err(FileRejection::ChangedDuringRead);
    }
    Ok(bytes)
}

fn parse_records(bytes: &[u8], scope: &CollectionScope, run: &ScanRun, report: &mut FileReport) {
    let header_len = FIXTURE_HEADER.len() + 1;
    let mut rest = &bytes[header_len..];
    let mut offset = header_len as u64;
    let mut ordinal = 1u64;
    while !rest.is_empty() {
        let Some(newline) = rest.iter().position(|&b| b == b'\n') else {
            report.pending_tail_bytes = rest.len() as u64;
            break;
        };
        ordinal += 1;
        let occurrence = OccurrenceIdentity {
            scan_run: run.id.clone(),
            environment: scope.environment().clone(),
            source: scope.source().clone(),
            relative_path: report.relative_path.clone(),
            file_object: report.file_object,
            line_ordinal: ordinal,
            byte_offset: offset,
            byte_length: newline as u64,
        };
        report
            .records
            .push(parse_record(&rest[..newline], occurrence, scope, run));
        offset += newline as u64 + 1;
        rest = &rest[newline + 1..];
    }
}

fn parse_record(
    line: &[u8],
    occurrence: OccurrenceIdentity,
    scope: &CollectionScope,
    run: &ScanRun,
) -> RecordOutcome {
    let values = match parse_fields(line) {
        Ok(values) => values,
        Err(reason) => return RecordOutcome::Rejected(RejectedRecord { occurrence, reason }),
    };
    let usage = UsageComponents {
        input: count_field(values[INPUT_TOKENS]),
        output: count_field(values[OUTPUT_TOKENS]),
        cache_read: count_field(values[CACHE_READ_TOKENS]),
        cache_write: count_field(values[CACHE_WRITE_TOKENS]),
        reasoning: count_field(values[REASONING_TOKENS]),
        total_reported: count_field(values[TOTAL_TOKENS_REPORTED]),
    };
    if usage.all().iter().all(|field| **field == Field::Absent) {
        return RecordOutcome::Rejected(RejectedRecord {
            occurrence,
            reason: RecordRejection::NoUsageComponents,
        });
    }
    let claimed_ids = ClaimedIds {
        message_id: identifier_field(values[MESSAGE_ID]),
        request_id: identifier_field(values[REQUEST_ID]),
        session_id: identifier_field(values[SESSION_ID]),
    };
    let model_provider = identifier_field(values[MODEL_PROVIDER]);
    let model = identifier_field(values[MODEL]);
    let event_time_raw = event_time_field(values[EVENT_TIME]);
    let project_label = identifier_field(values[PROJECT_LABEL]);

    let partial = usage.all().iter().any(|field| field.is_unparseable())
        || [
            &claimed_ids.message_id,
            &claimed_ids.request_id,
            &claimed_ids.session_id,
            &model_provider,
            &model,
            &event_time_raw,
            &project_label,
        ]
        .iter()
        .any(|field| field.is_unparseable());

    RecordOutcome::Accepted(Box::new(Observation {
        occurrence,
        claimed_ids,
        model_provider,
        model,
        event_time_raw,
        project_label,
        usage,
        parse_status: if partial {
            ParseStatus::Partial
        } else {
            ParseStatus::Complete
        },
        account_scope: AccountScope::NotCollected,
        capture_time: run.capture_time,
        origin: ObservationOrigin::SyntheticFixture {
            format_version: FIXTURE_FORMAT_VERSION,
            host_match: scope.host_match(),
        },
    }))
}

fn parse_fields(line: &[u8]) -> Result<[Option<&str>; KEYS.len()], RecordRejection> {
    let text = std::str::from_utf8(line).map_err(|_| RecordRejection::InvalidUtf8)?;
    if text.is_empty() {
        return Err(RecordRejection::EmptyRecord);
    }
    if text.chars().any(|c| c.is_control() && c != '\t') {
        return Err(RecordRejection::ControlCharacter);
    }
    let pairs = text
        .split('\t')
        .map(|pair| pair.split_once('=').ok_or(RecordRejection::MalformedPair))
        .collect::<Result<Vec<_>, _>>()?;
    // Privacy classification wins over every other key error.
    if let Some(class) = pairs.iter().find_map(|(key, _)| prohibited_class(key)) {
        return Err(RecordRejection::ProhibitedField(class));
    }
    let mut values = [None; KEYS.len()];
    for (key, value) in pairs {
        if key.is_empty()
            || key.len() > MAX_KEY_LEN
            || !key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        {
            return Err(RecordRejection::InvalidKey);
        }
        let Some(index) = KEYS.iter().position(|known| *known == key) else {
            return Err(RecordRejection::UnknownField);
        };
        if value.is_empty() {
            return Err(RecordRejection::EmptyValue);
        }
        if values[index].replace(value).is_some() {
            return Err(RecordRejection::DuplicateField(KEYS[index]));
        }
    }
    Ok(values)
}

fn prohibited_class(key: &str) -> Option<ProhibitedClass> {
    let key = key.to_ascii_lowercase();
    if KEYS.contains(&key.as_str()) {
        return None;
    }
    PROHIBITED.iter().find_map(|(class, words)| {
        words
            .iter()
            .any(|word| {
                key == *word
                    || key
                        .strip_prefix(word)
                        .is_some_and(|rest| rest.starts_with('_'))
                    || key
                        .strip_suffix(word)
                        .is_some_and(|rest| rest.ends_with('_'))
            })
            .then_some(*class)
    })
}

fn identifier_field(value: Option<&str>) -> Field<String> {
    text_field(value, MAX_IDENTIFIER_LEN, |b| {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'@' | b'-')
    })
}

fn event_time_field(value: Option<&str>) -> Field<String> {
    text_field(value, MAX_EVENT_TIME_LEN, |b| {
        b.is_ascii_digit() || matches!(b, b'T' | b'Z' | b':' | b'.' | b'+' | b'-')
    })
}

fn text_field(value: Option<&str>, max_len: usize, allowed: impl Fn(u8) -> bool) -> Field<String> {
    match value {
        None => Field::Absent,
        Some(value) if value.len() <= max_len && value.bytes().all(allowed) => {
            Field::Value(value.to_owned())
        }
        Some(value) => Field::Unparseable {
            byte_length: value.len(),
        },
    }
}

fn count_field(value: Option<&str>) -> Field<u64> {
    let Some(value) = value else {
        return Field::Absent;
    };
    // `u64::from_str` alone would also accept a leading `+`.
    match value.parse::<u64>() {
        Ok(count) if value.bytes().all(|b| b.is_ascii_digit()) => Field::Value(count),
        _ => Field::Unparseable {
            byte_length: value.len(),
        },
    }
}

#[cfg(test)]
mod tests;
