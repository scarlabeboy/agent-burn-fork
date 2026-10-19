use std::fmt;

use abf_parser::{Field, Limits, ParseResult};

use crate::{DeclaredContext, Inconsistency, SourceRunStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterCode {
    ParserStringBudget,
    ParserResultBudget,
}

/// One independent occurrence, including rejected records. Owns the original result.
/// Equality compares claims and status, never event identity. No sequence is retained.
#[derive(Clone, PartialEq, Eq)]
pub struct ObservationEnvelope {
    parser_result: ParseResult,
    context: DeclaredContext,
    source_status: SourceRunStatus,
}

impl ObservationEnvelope {
    /// Original bounded fields, statuses and diagnostic codes, without translation.
    /// The upstream type's Debug exposes metadata: do not log this reference.
    pub fn parser_result(&self) -> &ParseResult {
        &self.parser_result
    }
    pub fn context(&self) -> &DeclaredContext {
        &self.context
    }
    pub fn source_status(&self) -> SourceRunStatus {
        self.source_status
    }

    /// Explicit contradiction evidence; never rewrites source status or parser result.
    /// Absence does not certify complete usage, successful parsing or verified provenance.
    pub fn inconsistency(&self) -> Option<Inconsistency> {
        match self.source_status {
            SourceRunStatus::NotCollected | SourceRunStatus::Unavailable => {
                Some(Inconsistency::RecordWithoutReportedTraversal)
            }
            _ => None,
        }
    }
}

impl fmt::Debug for ObservationEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ObservationEnvelope([redacted])")
    }
}

/// Moves a parser result unchanged into declared synthetic context. Does not parse JSON.
/// Rechecks string/result ceilings because the parser's public structs are constructible.
/// Budget failure is an adapter error, never a replacement parser rejection or zero.
/// No semantic revalidation: even manually constructed statuses remain original claims.
pub fn from_opencode(
    parser_result: ParseResult,
    context: DeclaredContext,
    source_status: SourceRunStatus,
) -> Result<ObservationEnvelope, AdapterCode> {
    if let Some(o) = parser_result.observation() {
        if o.correlation
            .as_deref()
            .is_some_and(|s| s.len() > Limits::MAX.string_bytes)
        {
            return Err(AdapterCode::ParserStringBudget);
        }
        for field in [&o.message_id, &o.session_id, &o.model_id, &o.provider_id] {
            if let Field::Valid(s) = field
                && s.len() > Limits::MAX.string_bytes
            {
                return Err(AdapterCode::ParserStringBudget);
            }
        }
    }
    if parser_result
        .retained_bytes()
        .is_none_or(|n| n > Limits::MAX.result_bytes)
    {
        return Err(AdapterCode::ParserResultBudget);
    }
    Ok(ObservationEnvelope {
        parser_result,
        context,
        source_status,
    })
}
