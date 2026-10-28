//! Privacy-minimized usage summary of one [`ClaudeBatch`] (ABF134E).
//!
//! The summary holds only enums, integers and the caller's own opaque labels (scan run,
//! environment, source). It carries no record, path, session/request/message identifier,
//! model name, timestamp or content, and its `Debug` shows nothing input-derived.
//!
//! # Counting semantics
//!
//! - **What is summed:** observed *record claims*. [`CountingBasis::ObservedRecordClaims`]
//!   and [`DuplicateIdentity::Unresolved`] are fixed fields of every summary: equal or
//!   repeated records are each counted, no deduplication is applied, and the sums are neither
//!   unique physical events nor billed consumption.
//! - **Components stay separate:** input, output, cache creation and cache read are four
//!   independent sums. Cache tokens are never added to input or output, and there is no
//!   combined total. Cache creation uses the parser's resolved claim
//!   (`ClaudeObservation::cache_creation`): a valid breakdown is used alone and the flat claim
//!   is ignored, so the two variants are never added; `cache_creation_flat_ignored` counts
//!   those records.
//! - **Which claims count:** every `Valid` claim of every observation, including a valid zero
//!   and claims of records whose other fields are invalid. Absent, null, wrong-type,
//!   out-of-range and unsupported-parent states are only counted in `states`, never as 0.
//! - **Value outcome:** `ReportedClaimSum` (at least one valid claim; an explicit zero is a
//!   sum of 0), `NoValidClaims`, `Indeterminate(NoObservations)` when the batch holds no
//!   observation at all, or `Overflow` when the checked `u64` sum overflows (the component
//!   then stays `Overflow`; it is never wrapped, saturated or zeroed).
//! - **Record diagnostics** (`records`) overlap and are not additive, like the counting
//!   crate's record diagnostics.
//! - **Coverage is carried, not judged:** batch completeness, file rejections by reason,
//!   skipped entries, parser rejections by code, oversized and blank lines and pending tails
//!   are all counted separately. A summary with any of these is a partial view: see
//!   [`Caveats`].
//! - **Scope:** one batch, one environment, one source. No Windows or Computer totals, and
//!   summaries are never merged here.
//!
//! The component value mirrors `abf-counting`'s `ComponentValue` vocabulary but is not that
//! crate's type: `abf-counting` consumes OpenCode-based synthetic envelopes and has no Claude
//! input, so reusing it would need a Claude adapter in other crates. Sums here are `u64`
//! because up to 4096 records of up to `u64::MAX` each can overflow it, which is reported.

use abf_parser::{Code, Field};

use crate::claude_jsonl::{
    ClaudeBatch, ClaudeFileRejection, ClaudeFileStatus, ClaudeLineOutcome, ClaudeTotals,
};
use crate::identity::{EnvironmentIdentity, OpaqueId, SourceInstance};
use crate::observation::Completeness;
use crate::scope::HostMatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountingBasis {
    /// Sums of claims as observed in records; every record counts, duplicates included.
    ObservedRecordClaims,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicateIdentity {
    /// Whether two records are the same physical event is not established.
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndeterminateReason {
    /// The batch contains no observation: nothing was collected, which is not zero usage.
    NoObservations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentValue {
    ReportedClaimSum(u64),
    /// Observations exist, but none carried a valid claim for this component.
    NoValidClaims,
    Indeterminate(IndeterminateReason),
    /// The checked sum exceeded `u64`.
    Overflow,
}

/// States of one component across all observations. They partition the observations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaimStates {
    pub valid: u64,
    /// Subset of `valid`: explicit zero claims.
    pub valid_zero: u64,
    pub absent: u64,
    pub null: u64,
    pub wrong_type: u64,
    pub out_of_range: u64,
    /// The parent object was missing or not an object, so no claim could be read.
    pub unsupported: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentSummary {
    pub value: ComponentValue,
    pub states: ClaimStates,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaudeComponents {
    pub input: ComponentSummary,
    pub output: ComponentSummary,
    /// Resolved claim: breakdown if valid, otherwise the flat claim; never both added.
    pub cache_creation: ComponentSummary,
    pub cache_read: ComponentSummary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordCounts {
    pub observations: u64,
    /// Parser status partition: `accepted + qualified + incomplete == observations`.
    pub accepted: u64,
    pub qualified: u64,
    pub incomplete: u64,
    /// Overlapping diagnostics over the four token components.
    pub with_valid_claim: u64,
    pub without_valid_claim: u64,
    pub with_invalid_claim: u64,
    pub with_null_claim: u64,
    pub with_missing_claim: u64,
    /// Records whose valid cache-creation breakdown made a valid flat claim irrelevant.
    pub cache_creation_flat_ignored: u64,
}

/// Parser rejections by code (every code has its own counter).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RejectionCounts {
    pub frame_budget: u64,
    pub string_budget: u64,
    pub depth_budget: u64,
    pub entry_budget: u64,
    pub result_budget: u64,
    pub invalid_limits: u64,
    pub invalid_utf8: u64,
    pub incomplete_json: u64,
    pub malformed_json: u64,
    pub trailing_data: u64,
    pub root_shape: u64,
    pub duplicate_key: u64,
}

impl RejectionCounts {
    pub fn total(&self) -> u64 {
        [
            self.frame_budget,
            self.string_budget,
            self.depth_budget,
            self.entry_budget,
            self.result_budget,
            self.invalid_limits,
            self.invalid_utf8,
            self.incomplete_json,
            self.malformed_json,
            self.trailing_data,
            self.root_shape,
            self.duplicate_key,
        ]
        .iter()
        .sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineSummary {
    pub terminated_lines: u64,
    pub rejected: RejectionCounts,
    pub oversized: u64,
    pub blank: u64,
    pub files_with_pending_tail: u64,
    pub pending_tail_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileSummary {
    pub read: u64,
    pub rejected: u64,
    pub path: u64,
    pub too_large: u64,
    pub byte_budget: u64,
    pub output_limit: u64,
    pub changed_during_read: u64,
    pub io: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipSummary {
    pub total: u64,
    /// Skips that may hide in-scope data (symlinks, depth limit, unreadable entries, ...).
    pub may_hide_data: u64,
    /// Out of scope by design (wrong extension, special files).
    pub out_of_scope: u64,
}

/// Why this summary is a partial view of the source. All `false` is not a completeness
/// proof: it only means none of these known gaps was observed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caveats {
    /// Traversal or reading was incomplete (the batch's own completeness).
    pub incomplete_batch: bool,
    /// Some lines were rejected by the parser or were oversized, so their usage is unknown.
    pub unparsed_lines: bool,
    /// Some file ended in an unterminated tail that was not parsed.
    pub pending_tails: bool,
    /// Some candidate file was rejected (changed, unreadable, over budget, ...).
    pub rejected_files: bool,
    /// Some entry that may hold data was skipped.
    pub skipped_in_scope: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeSummary {
    pub basis: CountingBasis,
    pub duplicates: DuplicateIdentity,
    pub scan_run: OpaqueId,
    pub environment: EnvironmentIdentity,
    pub source: SourceInstance,
    pub host_match: HostMatch,
    pub completeness: Completeness,
    pub components: ClaudeComponents,
    pub records: RecordCounts,
    pub lines: LineSummary,
    pub files: FileSummary,
    pub skipped: SkipSummary,
    pub caveats: Caveats,
}

pub(crate) struct Accumulator {
    sum: Option<u64>,
    states: ClaimStates,
}

impl Accumulator {
    pub(crate) fn new() -> Self {
        Self {
            sum: Some(0),
            states: ClaimStates::default(),
        }
    }
    pub(crate) fn add(&mut self, field: &Field<u64>) {
        let states = &mut self.states;
        match field {
            Field::Valid(value) => {
                states.valid += 1;
                if *value == 0 {
                    states.valid_zero += 1;
                }
                // Once overflowed the component stays overflowed.
                self.sum = self.sum.and_then(|sum| sum.checked_add(*value));
            }
            Field::Absent => states.absent += 1,
            Field::Null => states.null += 1,
            Field::WrongType => states.wrong_type += 1,
            Field::OutOfRange => states.out_of_range += 1,
            Field::Unsupported => states.unsupported += 1,
        }
    }
    pub(crate) fn finish(self, observations: u64) -> ComponentSummary {
        let value = match self.sum {
            None => ComponentValue::Overflow,
            Some(_) if observations == 0 => {
                ComponentValue::Indeterminate(IndeterminateReason::NoObservations)
            }
            Some(sum) if self.states.valid > 0 => ComponentValue::ReportedClaimSum(sum),
            Some(_) => ComponentValue::NoValidClaims,
        };
        ComponentSummary {
            value,
            states: self.states,
        }
    }
}

fn count_rejection(counts: &mut RejectionCounts, code: Code) {
    let counter = match code {
        Code::FrameBudget => &mut counts.frame_budget,
        Code::StringBudget => &mut counts.string_budget,
        Code::DepthBudget => &mut counts.depth_budget,
        Code::EntryBudget => &mut counts.entry_budget,
        Code::ResultBudget => &mut counts.result_budget,
        Code::InvalidLimits => &mut counts.invalid_limits,
        Code::InvalidUtf8 => &mut counts.invalid_utf8,
        Code::IncompleteJson => &mut counts.incomplete_json,
        Code::MalformedJson => &mut counts.malformed_json,
        Code::TrailingData => &mut counts.trailing_data,
        Code::RootShape => &mut counts.root_shape,
        Code::DuplicateKey => &mut counts.duplicate_key,
    };
    *counter += 1;
}

/// Summarizes one batch. Pure: no I/O, no clock, deterministic, and it never fails; every
/// gap is a field of the result.
pub fn summarize_claude_batch(batch: &ClaudeBatch) -> ClaudeSummary {
    let mut accumulators = [
        Accumulator::new(),
        Accumulator::new(),
        Accumulator::new(),
        Accumulator::new(),
    ];
    let mut records = RecordCounts::default();
    let mut lines = LineSummary::default();
    let mut files = FileSummary::default();

    for file in &batch.files {
        match file.status {
            ClaudeFileStatus::Read => files.read += 1,
            ClaudeFileStatus::Rejected(reason) => {
                files.rejected += 1;
                *match reason {
                    ClaudeFileRejection::Path(_) => &mut files.path,
                    ClaudeFileRejection::TooLarge => &mut files.too_large,
                    ClaudeFileRejection::ByteBudget => &mut files.byte_budget,
                    ClaudeFileRejection::OutputLimit => &mut files.output_limit,
                    ClaudeFileRejection::ChangedDuringRead => &mut files.changed_during_read,
                    ClaudeFileRejection::Io(_) => &mut files.io,
                } += 1;
            }
        }
        for line in &file.lines {
            match &line.outcome {
                ClaudeLineOutcome::Observation(observation) => {
                    records.observations += 1;
                    match observation.status {
                        abf_parser::Status::Accepted => records.accepted += 1,
                        abf_parser::Status::Qualified => records.qualified += 1,
                        abf_parser::Status::Incomplete => records.incomplete += 1,
                    }
                    let creation = observation.cache_creation();
                    let fields = [
                        &observation.input,
                        &observation.output,
                        &creation,
                        &observation.cache_read,
                    ];
                    for (accumulator, field) in accumulators.iter_mut().zip(fields) {
                        accumulator.add(field);
                    }
                    let any = |test: fn(&Field<u64>) -> bool| fields.iter().any(|f| test(f));
                    if any(|f| matches!(f, Field::Valid(_))) {
                        records.with_valid_claim += 1;
                    } else {
                        records.without_valid_claim += 1;
                    }
                    records.with_invalid_claim +=
                        u64::from(any(|f| matches!(f, Field::WrongType | Field::OutOfRange)));
                    records.with_null_claim += u64::from(any(|f| matches!(f, Field::Null)));
                    records.with_missing_claim +=
                        u64::from(any(|f| matches!(f, Field::Absent | Field::Unsupported)));
                    records.cache_creation_flat_ignored += u64::from(
                        observation.cache_creation_detail == Field::Valid(())
                            && matches!(observation.cache_creation_flat, Field::Valid(_))
                            && (matches!(observation.cache_creation_5m, Field::Valid(_))
                                || matches!(observation.cache_creation_1h, Field::Valid(_))),
                    );
                }
                ClaudeLineOutcome::Rejected(code) => count_rejection(&mut lines.rejected, *code),
                ClaudeLineOutcome::Oversized => {}
            }
        }
    }
    let ClaudeTotals {
        counts,
        files_with_pending_tail,
        pending_tail_bytes,
        ..
    } = batch.totals();
    lines.terminated_lines = counts.terminated_lines;
    lines.oversized = counts.oversized;
    lines.blank = counts.blank;
    lines.files_with_pending_tail = files_with_pending_tail;
    lines.pending_tail_bytes = pending_tail_bytes;

    let mut skipped = SkipSummary::default();
    for entry in &batch.skipped {
        skipped.total += 1;
        if entry.reason.makes_incomplete() {
            skipped.may_hide_data += 1;
        } else {
            skipped.out_of_scope += 1;
        }
    }

    let [input, output, cache_creation, cache_read] =
        accumulators.map(|accumulator| accumulator.finish(records.observations));
    let caveats = Caveats {
        incomplete_batch: batch.completeness == Completeness::Incomplete,
        unparsed_lines: lines.rejected.total() > 0 || lines.oversized > 0,
        pending_tails: lines.files_with_pending_tail > 0,
        rejected_files: files.rejected > 0,
        skipped_in_scope: skipped.may_hide_data > 0,
    };
    ClaudeSummary {
        basis: CountingBasis::ObservedRecordClaims,
        duplicates: DuplicateIdentity::Unresolved,
        scan_run: batch.scan_run.id.clone(),
        environment: batch.environment.clone(),
        source: batch.source.clone(),
        host_match: batch.host_match,
        completeness: batch.completeness,
        components: ClaudeComponents {
            input,
            output,
            cache_creation,
            cache_read,
        },
        records,
        lines,
        files,
        skipped,
        caveats,
    }
}

impl ClaudeBatch {
    /// See [`summarize_claude_batch`].
    pub fn summary(&self) -> ClaudeSummary {
        summarize_claude_batch(self)
    }
}

#[cfg(test)]
mod tests;
