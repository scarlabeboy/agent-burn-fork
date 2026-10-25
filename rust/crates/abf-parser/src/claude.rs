use crate::opencode::{Node, child_integer, decode_frame, diagnose, metadata};
use crate::{Code, Diagnostic, Diagnostics, Field, Limits, Status};
use std::fmt;

// Fixed allowlist of usage keys with a token-count meaning; any other `*_tokens` key
// qualifies the record because a component could be missing from the claims.
const KNOWN_USAGE_KEYS: [&str; 4] = [
    "input_tokens",
    "output_tokens",
    "cache_creation_input_tokens",
    "cache_read_input_tokens",
];
const KNOWN_BREAKDOWN_KEYS: [&str; 2] = ["ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"];

/// Claims from one Claude Code JSONL record. Missing fields carry no default; a record
/// establishes no source completeness, physical event identity or authenticity.
///
/// `Debug` is redacted: it prints field states, never identifiers, timestamps or counts.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeObservation {
    pub correlation: Option<Box<str>>,
    pub session_id: Field<Box<str>>,
    pub request_id: Field<Box<str>>,
    /// Raw claim of `timestamp`; not parsed, normalized or bucketed here.
    pub timestamp: Field<Box<str>>,
    pub message: Field<()>,
    pub message_id: Field<Box<str>>,
    pub model: Field<Box<str>>,
    pub usage: Field<()>,
    pub input: Field<u64>,
    pub output: Field<u64>,
    pub cache_read: Field<u64>,
    /// Flat `cache_creation_input_tokens` claim, kept apart from the breakdown.
    pub cache_creation_flat: Field<u64>,
    /// State of the `cache_creation` breakdown object.
    pub cache_creation_detail: Field<()>,
    pub cache_creation_5m: Field<u64>,
    pub cache_creation_1h: Field<u64>,
    pub status: Status,
    pub diagnostics: Diagnostics,
}

impl ClaudeObservation {
    /// Cache-creation tokens without double counting. When the breakdown object exists
    /// it alone is used: the sum of its present children (an overflow is `OutOfRange`),
    /// or the first invalid child state. A breakdown with no children falls back to the
    /// flat claim. A null or wrong-type breakdown is reported as such; the flat claim is
    /// never added to, or substituted for, an invalid breakdown. Absent breakdown uses
    /// the flat claim.
    pub fn cache_creation(&self) -> Field<u64> {
        match &self.cache_creation_detail {
            Field::Absent => self.cache_creation_flat.clone(),
            Field::Valid(()) => {
                let children = [&self.cache_creation_5m, &self.cache_creation_1h];
                let mut sum: Option<u64> = None;
                for child in children {
                    match child {
                        Field::Valid(n) => match sum.unwrap_or(0).checked_add(*n) {
                            Some(total) => sum = Some(total),
                            None => return Field::OutOfRange,
                        },
                        Field::Absent => {}
                        other => return other.clone(),
                    }
                }
                sum.map_or_else(|| self.cache_creation_flat.clone(), Field::Valid)
            }
            Field::Null => Field::Null,
            Field::OutOfRange => Field::OutOfRange,
            Field::WrongType | Field::Unsupported => Field::WrongType,
        }
    }
}

fn state<T>(field: &Field<T>) -> &'static str {
    match field {
        Field::Absent => "Absent",
        Field::Null => "Null",
        Field::WrongType => "WrongType",
        Field::OutOfRange => "OutOfRange",
        Field::Unsupported => "Unsupported",
        Field::Valid(_) => "Valid",
    }
}

impl fmt::Debug for ClaudeObservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeObservation")
            .field(
                "correlation",
                &self.correlation.as_ref().map(|_| "<redacted>"),
            )
            .field("session_id", &state(&self.session_id))
            .field("request_id", &state(&self.request_id))
            .field("timestamp", &state(&self.timestamp))
            .field("message", &state(&self.message))
            .field("message_id", &state(&self.message_id))
            .field("model", &state(&self.model))
            .field("usage", &state(&self.usage))
            .field("input", &state(&self.input))
            .field("output", &state(&self.output))
            .field("cache_read", &state(&self.cache_read))
            .field("cache_creation_flat", &state(&self.cache_creation_flat))
            .field("cache_creation_detail", &state(&self.cache_creation_detail))
            .field("cache_creation_5m", &state(&self.cache_creation_5m))
            .field("cache_creation_1h", &state(&self.cache_creation_1h))
            .field("status", &self.status)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Inline bounded result avoids an extra allocation.
pub enum ClaudeParseResult {
    Rejected(Code),
    Observation(ClaudeObservation),
}
impl ClaudeParseResult {
    pub fn observation(&self) -> Option<&ClaudeObservation> {
        match self {
            Self::Observation(o) => Some(o),
            Self::Rejected(_) => None,
        }
    }
    /// Inline result size plus exact owned string payload bytes; excludes allocator overhead.
    pub fn retained_bytes(&self) -> Option<usize> {
        let mut bytes = size_of::<Self>();
        if let Some(o) = self.observation() {
            bytes = bytes.checked_add(o.correlation.as_deref().map_or(0, str::len))?;
            for field in [
                &o.session_id,
                &o.request_id,
                &o.timestamp,
                &o.message_id,
                &o.model,
            ] {
                if let Field::Valid(s) = field {
                    bytes = bytes.checked_add(s.len())?;
                }
            }
        }
        Some(bytes)
    }
}

fn unknown_keys(node: Option<&Node<'_>>, known: &[&str], only_token_keys: bool) -> bool {
    match node {
        Some(Node::Object(members)) => members.iter().any(|(key, _)| {
            !known.contains(&key.as_str()) && (!only_token_keys || key.ends_with("_tokens"))
        }),
        _ => false,
    }
}

fn observe(
    root: &Node<'_>,
    correlation: Option<&str>,
    limits: Limits,
) -> Result<ClaudeObservation, Code> {
    let message = root.get("message");
    let usage = message.and_then(|m| m.get("usage"));
    let message_state = match message {
        None => Field::Absent,
        Some(Node::Null) => Field::Null,
        Some(Node::Object(_)) => Field::Valid(()),
        _ => Field::WrongType,
    };
    let container = |node: Option<&Node<'_>>, parent_ok: bool| {
        if !parent_ok {
            return Field::Unsupported;
        }
        match node {
            None => Field::Absent,
            Some(Node::Null) => Field::Null,
            Some(Node::Object(_)) => Field::Valid(()),
            _ => Field::WrongType,
        }
    };
    let message_ok = matches!(message, Some(Node::Object(_)));
    let usage_ok = matches!(usage, Some(Node::Object(_)));
    let detail = usage.and_then(|u| u.get("cache_creation"));
    let detail_ok = matches!(detail, Some(Node::Object(_)));
    let child_meta = |key: &str| -> Result<Field<Box<str>>, Code> {
        if message_ok {
            metadata(message.and_then(|m| m.get(key)), limits)
        } else {
            Ok(Field::Unsupported)
        }
    };
    let mut o = ClaudeObservation {
        correlation: correlation.map(Box::from),
        session_id: metadata(root.get("sessionId"), limits)?,
        request_id: metadata(root.get("requestId"), limits)?,
        timestamp: metadata(root.get("timestamp"), limits)?,
        message: message_state,
        message_id: child_meta("id")?,
        model: child_meta("model")?,
        usage: container(message.and_then(|m| m.get("usage")), message_ok),
        input: child_integer(usage, "input_tokens"),
        output: child_integer(usage, "output_tokens"),
        cache_read: child_integer(usage, "cache_read_input_tokens"),
        cache_creation_flat: child_integer(usage, "cache_creation_input_tokens"),
        cache_creation_detail: container(detail, usage_ok),
        cache_creation_5m: child_integer(
            if detail_ok { detail } else { None },
            "ephemeral_5m_input_tokens",
        ),
        cache_creation_1h: child_integer(
            if detail_ok { detail } else { None },
            "ephemeral_1h_input_tokens",
        ),
        status: Status::Accepted,
        diagnostics: Diagnostics::default(),
    };
    // A non-object parent cannot supply children: mark them unsupported, not absent.
    if !usage_ok {
        o.input = Field::Unsupported;
        o.output = Field::Unsupported;
        o.cache_read = Field::Unsupported;
        o.cache_creation_flat = Field::Unsupported;
    }
    if !detail_ok {
        o.cache_creation_5m = Field::Unsupported;
        o.cache_creation_1h = Field::Unsupported;
    }
    for field in [
        &o.session_id,
        &o.request_id,
        &o.timestamp,
        &o.message_id,
        &o.model,
    ] {
        diagnose(field, &mut o.diagnostics);
    }
    for field in [&o.message, &o.usage, &o.cache_creation_detail] {
        diagnose(field, &mut o.diagnostics);
    }
    for field in [
        &o.input,
        &o.output,
        &o.cache_read,
        &o.cache_creation_flat,
        &o.cache_creation_5m,
        &o.cache_creation_1h,
    ] {
        diagnose(field, &mut o.diagnostics);
    }
    if unknown_keys(usage, &KNOWN_USAGE_KEYS, true)
        || unknown_keys(detail, &KNOWN_BREAKDOWN_KEYS, false)
    {
        o.diagnostics.insert(Diagnostic::UnsupportedComponent);
    }
    let creation = o.cache_creation();
    if matches!(creation, Field::OutOfRange) {
        o.diagnostics.insert(Diagnostic::InvalidField);
    }
    let any_valid = [&o.input, &o.output, &o.cache_read, &creation]
        .iter()
        .any(|f| matches!(f, Field::Valid(_)));
    if !any_valid {
        o.diagnostics.insert(Diagnostic::IncompleteTokens);
        o.status = Status::Incomplete;
    } else if o.diagnostics.bits() != 0 {
        o.status = Status::Qualified;
    }
    Ok(o)
}

/// Parse one complete caller-owned Claude Code JSONL record (no trailing newline required).
/// Line splitting, oversized-line detection and file I/O belong to a future collector:
/// a frame above `limits.frame_bytes` (or with more than `limits.entries` JSON members and
/// array elements, content arrays included) is rejected, never truncated or counted as zero.
/// Message content, tool data, paths and unknown fields are validated for grammar and
/// discarded; only the allowlisted claims above are retained.
pub fn parse_claude(frame: &[u8], correlation: Option<&str>, limits: Limits) -> ClaudeParseResult {
    fn inner(
        frame: &[u8],
        correlation: Option<&str>,
        limits: Limits,
    ) -> Result<ClaudeParseResult, Code> {
        if !limits.valid_with_result_floor(size_of::<ClaudeParseResult>()) {
            return Err(Code::InvalidLimits);
        }
        let root = decode_frame(frame, correlation, limits)?;
        let result = ClaudeParseResult::Observation(observe(&root, correlation, limits)?);
        if result.retained_bytes().ok_or(Code::ResultBudget)? > limits.result_bytes {
            return Err(Code::ResultBudget);
        }
        Ok(result)
    }
    inner(frame, correlation, limits).unwrap_or_else(ClaudeParseResult::Rejected)
}
