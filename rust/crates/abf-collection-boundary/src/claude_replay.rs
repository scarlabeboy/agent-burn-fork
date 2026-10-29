//! ABF140B: source-scoped exact replay of retained observation claims, not raw JSON,
//! physical events or billed usage. Content is deliberately not retained or compared.
//!
//! One batch only. Eligibility requires explicit assistant/non-sidechain evidence,
//! bounded nonempty UUID/session/request/message/model/timestamp claims, Accepted status,
//! no diagnostics and four valid resolved token components. Missing/invalid evidence and
//! sidechains remain unresolved. UUID is an opaque source claim, not authentication.
//! Equal UUIDs or a session/message pair are candidate relations only:
//! every related observation must have identical retained evidence. Any difference or
//! ineligible peer invalidates the group, including preceding exact copies. No lifecycle
//! supersession, larger-value preference, transitive inference or cross-batch identity.
//!
//! The comparison's sole allocation holds at most 4096 borrowed observation pointers;
//! the existing raw summary separately copies bounded caller labels. No identity
//! strings or records are cloned. Pair comparisons are O(n^2), each over bounded fields.
//! Failure/exhaustion emits no partial replay result. `raw` always preserves coverage;
//! Evaluated means the bounded comparison ran, never that source coverage is complete.

use abf_parser::{ClaudeObservation, Field, Status};

use crate::claude_summary::Accumulator;
use crate::{ClaudeBatch, ClaudeComponents, ClaudeSummary, Harness, MAX_RECORDS_CEILING};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayBasis {
    SourceScopedExactObservationClaimsV1,
}

/// Overlapping reason counts; use `unresolved_observations` for the partition count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnresolvedCounts {
    pub invalid_identity: u64,
    pub unsupported_record_type: u64,
    pub sidechain: u64,
    pub invalid_claims: u64,
    pub conflicting_evidence: u64,
}

/// ABF142: overlapping counts of observations with at least one conflicting peer.
/// Each observation contributes at most one to each counter, regardless of peer count.
/// Relation counts cover conflicting peers only; they are not unique pairs or events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConflictCounts {
    pub ineligible_peer: u64,
    pub identity_metadata: u64,
    pub token_fields: u64,
    pub parser_evidence: u64,
    pub uuid_related: u64,
    pub session_message_related: u64,
}

/// ABF143: overlapping observation counts over metadata-only conflicting pairs.
/// Matching claims are unconfirmed candidates, never a deduplication decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateCounts {
    pub matching_request: u64,
    pub request_mismatch: u64,
    pub missing_identity: u64,
    pub other_metadata: u64,
}

// Called only for metadata-only conflicts with identical token and parser evidence.
fn candidate_category(a: &ClaudeObservation, b: &ClaudeObservation) -> usize {
    if !identity_valid(a) || !identity_valid(b) {
        return 2;
    }
    if eligible(a)
        && eligible(b)
        && same_text(&a.session_id, &b.session_id)
        && same_text(&a.message_id, &b.message_id)
        && same_text(&a.model, &b.model)
        && a.correlation == b.correlation
        && a.message == b.message
    {
        // Only UUID, timestamp and (for the mismatch category) request may differ.
        return usize::from(!same_text(&a.request_id, &b.request_id));
    }
    3
}

// Exhaustive destructuring makes new retained fields require an attribution decision.
// These differences explain the existing full equality decision; they never replace it.
fn evidence_differences(a: &ClaudeObservation, b: &ClaudeObservation) -> [bool; 3] {
    let ClaudeObservation {
        identity,
        correlation,
        session_id,
        request_id,
        timestamp,
        message,
        message_id,
        model,
        usage,
        input,
        output,
        cache_read,
        cache_creation_flat,
        cache_creation_detail,
        cache_creation_5m,
        cache_creation_1h,
        status,
        diagnostics,
    } = a;
    [
        identity != &b.identity
            || correlation != &b.correlation
            || session_id != &b.session_id
            || request_id != &b.request_id
            || timestamp != &b.timestamp
            || message != &b.message
            || message_id != &b.message_id
            || model != &b.model,
        usage != &b.usage
            || input != &b.input
            || output != &b.output
            || cache_read != &b.cache_read
            || cache_creation_flat != &b.cache_creation_flat
            || cache_creation_detail != &b.cache_creation_detail
            || cache_creation_5m != &b.cache_creation_5m
            || cache_creation_1h != &b.cache_creation_1h,
        status != &b.status || diagnostics != &b.diagnostics,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayClaims {
    pub basis: ReplayBasis,
    /// All observations in eligible groups, including their replay copies.
    pub eligible_observations: u64,
    pub representatives: u64,
    pub replay_observations: u64,
    pub unresolved_observations: u64,
    pub unresolved: UnresolvedCounts,
    pub conflicts: ConflictCounts,
    pub candidates: CandidateCounts,
    /// Sums of one retained observation per eligible group only. Unresolved claims are
    /// excluded here, remain in `raw`, and must never be interpreted as zero consumption.
    pub components: ClaudeComponents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Fixed-size inline result avoids another allocation.
pub enum ReplayEvaluation {
    Evaluated(ReplayClaims),
    IdentityBudgetExhausted { observations: u64, limit: usize },
    InvalidIdentityLimit,
    WrongHarness,
    AllocationFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeReplaySummary {
    /// The existing public summary, with its original basis, states and coverage.
    pub raw: ClaudeSummary,
    pub evaluation: ReplayEvaluation,
}

fn text(field: &Field<Box<str>>) -> Option<&str> {
    match field {
        Field::Valid(s) if !s.is_empty() && s.len() <= 256 => Some(s),
        _ => None,
    }
}

fn same_text(a: &Field<Box<str>>, b: &Field<Box<str>>) -> bool {
    text(a).is_some_and(|value| Some(value) == text(b))
}

fn identity_valid(o: &ClaudeObservation) -> bool {
    [
        &o.identity.record_type,
        &o.identity.record_uuid,
        &o.session_id,
        &o.request_id,
        &o.message_id,
        &o.model,
        &o.timestamp,
    ]
    .iter()
    .all(|f| text(f).is_some())
        && matches!(o.identity.is_sidechain, Field::Valid(_))
}

fn claims_valid(o: &ClaudeObservation) -> bool {
    o.status == Status::Accepted
        && o.diagnostics.bits() == 0
        && [&o.input, &o.output, &o.cache_read, &o.cache_creation()]
            .iter()
            .all(|f| matches!(f, Field::Valid(_)))
}

fn eligible(o: &ClaudeObservation) -> bool {
    identity_valid(o)
        && claims_valid(o)
        && text(&o.identity.record_type) == Some("assistant")
        && o.identity.is_sidechain == Field::Valid(false)
}

fn related(a: &ClaudeObservation, b: &ClaudeObservation) -> bool {
    same_text(&a.identity.record_uuid, &b.identity.record_uuid)
        || (same_text(&a.session_id, &b.session_id)
            // A changed request could be a sidechain replay or another unresolved
            // reuse. This relation can exclude a group, never prove equivalence.
            && same_text(&a.message_id, &b.message_id))
}

fn evaluate(batch: &ClaudeBatch, observations: u64, limit: usize) -> ReplayEvaluation {
    if batch.source.harness() != Harness::ClaudeCode {
        return ReplayEvaluation::WrongHarness;
    }
    if limit > MAX_RECORDS_CEILING {
        return ReplayEvaluation::InvalidIdentityLimit;
    }
    if observations > limit as u64 {
        return ReplayEvaluation::IdentityBudgetExhausted {
            observations,
            limit,
        };
    }
    let mut entries = Vec::new();
    if entries.try_reserve_exact(observations as usize).is_err() {
        return ReplayEvaluation::AllocationFailure;
    }
    for record in batch.observations() {
        entries.push(record.observation);
    }
    let mut sums = [
        Accumulator::new(),
        Accumulator::new(),
        Accumulator::new(),
        Accumulator::new(),
    ];
    let mut unresolved = UnresolvedCounts::default();
    let mut conflicts = ConflictCounts::default();
    let mut candidates = CandidateCounts::default();
    let (mut eligible_observations, mut representatives, mut replay_observations) = (0, 0, 0);
    for (index, o) in entries.iter().enumerate() {
        let mut conflict = false;
        let mut reasons = [false; 6];
        let mut candidate_reasons = [false; 4];
        for (peer_index, peer) in entries.iter().enumerate() {
            // Preserve the ABF140B predicate exactly; scan all peers for attribution.
            if peer_index != index && related(o, peer) && (!eligible(peer) || *o != *peer) {
                conflict = true;
                let [metadata, tokens, parser] = evidence_differences(o, peer);
                if metadata && !tokens && !parser {
                    candidate_reasons[candidate_category(o, peer)] = true;
                }
                let flags = [
                    !eligible(peer),
                    metadata,
                    tokens,
                    parser,
                    same_text(&o.identity.record_uuid, &peer.identity.record_uuid),
                    same_text(&o.session_id, &peer.session_id)
                        && same_text(&o.message_id, &peer.message_id),
                ];
                for (reason, flag) in reasons.iter_mut().zip(flags) {
                    *reason |= flag;
                }
            }
        }
        for (count, reason) in [
            &mut candidates.matching_request,
            &mut candidates.request_mismatch,
            &mut candidates.missing_identity,
            &mut candidates.other_metadata,
        ]
        .into_iter()
        .zip(candidate_reasons)
        {
            *count += u64::from(reason);
        }
        for (count, reason) in [
            &mut conflicts.ineligible_peer,
            &mut conflicts.identity_metadata,
            &mut conflicts.token_fields,
            &mut conflicts.parser_evidence,
            &mut conflicts.uuid_related,
            &mut conflicts.session_message_related,
        ]
        .into_iter()
        .zip(reasons)
        {
            *count += u64::from(reason);
        }
        unresolved.invalid_identity += u64::from(!identity_valid(o));
        unresolved.unsupported_record_type +=
            u64::from(text(&o.identity.record_type) != Some("assistant"));
        unresolved.sidechain += u64::from(o.identity.is_sidechain == Field::Valid(true));
        unresolved.invalid_claims += u64::from(!claims_valid(o));
        unresolved.conflicting_evidence += u64::from(conflict);
        if !eligible(o) || conflict {
            continue;
        }
        eligible_observations += 1;
        if entries[..index].contains(o) {
            replay_observations += 1;
            continue;
        }
        representatives += 1;
        for (sum, field) in
            sums.iter_mut()
                .zip([&o.input, &o.output, &o.cache_creation(), &o.cache_read])
        {
            sum.add(field);
        }
    }
    let [input, output, cache_creation, cache_read] = sums.map(|s| s.finish(representatives));
    ReplayEvaluation::Evaluated(ReplayClaims {
        basis: ReplayBasis::SourceScopedExactObservationClaimsV1,
        eligible_observations,
        representatives,
        replay_observations,
        unresolved_observations: observations - eligible_observations,
        unresolved,
        conflicts,
        candidates,
        components: ClaudeComponents {
            input,
            output,
            cache_creation,
            cache_read,
        },
    })
}

impl ClaudeBatch {
    /// Compare one source batch with a caller-lowered observation budget (0..=4096).
    /// Existing `summary()` and all records remain untouched. Inspect `raw.caveats` and
    /// unresolved counts even when evaluation succeeded. No completeness/uniqueness claim.
    pub fn exact_replay_summary(&self, max_identity_observations: usize) -> ClaudeReplaySummary {
        let raw = self.summary();
        let evaluation = evaluate(self, raw.records.observations, max_identity_observations);
        ClaudeReplaySummary { raw, evaluation }
    }
}
