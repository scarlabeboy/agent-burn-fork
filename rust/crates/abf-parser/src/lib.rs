//! Experimental synthetic-only OpenCode and Claude Code record parsing. No collector or I/O.
#![forbid(unsafe_code)]

mod claude;
mod claude_long;
mod opencode;
pub use claude::{ClaudeObservation, ClaudeParseResult, parse_claude};
pub use claude_long::{LongLimits, parse_claude_long};
pub use opencode::parse;

/// Callers may lower, but never raise, the experimental hard ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub frame_bytes: usize,
    pub depth: usize,
    pub entries: usize,
    pub string_bytes: usize,
    pub result_bytes: usize,
}
impl Limits {
    pub const MAX: Self = Self {
        frame_bytes: 64 * 1024,
        depth: 16,
        entries: 256,
        string_bytes: 256,
        result_bytes: 4096,
    };
    pub(crate) fn valid(self) -> bool {
        self.valid_with_result_floor(size_of::<ParseResult>())
    }
    pub(crate) fn valid_with_result_floor(self, floor: usize) -> bool {
        self.frame_bytes <= Self::MAX.frame_bytes
            && self.depth <= Self::MAX.depth
            && self.entries <= Self::MAX.entries
            && self.string_bytes <= Self::MAX.string_bytes
            && self.result_bytes <= Self::MAX.result_bytes
            && self.result_bytes >= floor
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field<T> {
    Absent,
    Null,
    WrongType,
    OutOfRange,
    /// Parent container is unavailable; no child-field claim can be extracted.
    Unsupported,
    Valid(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    InvalidLimits,
    FrameBudget,
    StringBudget,
    DepthBudget,
    EntryBudget,
    ResultBudget,
    InvalidUtf8,
    IncompleteJson,
    MalformedJson,
    TrailingData,
    RootShape,
    DuplicateKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Accepted,
    Qualified,
    Incomplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Diagnostic {
    InvalidField = 1,
    ExplicitNull = 2,
    UnsupportedComponent = 4,
    IncompleteTokens = 8,
}

/// Fixed-size code set: no user-controlled text, repetition or accumulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics(u8);
impl Diagnostics {
    pub fn contains(self, code: Diagnostic) -> bool {
        self.0 & code as u8 != 0
    }
    pub fn bits(self) -> u8 {
        self.0
    }
    pub(crate) fn insert(&mut self, code: Diagnostic) {
        self.0 |= code as u8;
    }
}

/// Claims only. Missing fields carry no default; a record establishes no source completeness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub correlation: Option<Box<str>>,
    pub message_id: Field<Box<str>>,
    pub session_id: Field<Box<str>>,
    pub model_id: Field<Box<str>>,
    pub provider_id: Field<Box<str>>,
    pub time: Field<()>,
    pub created: Field<i64>,
    pub tokens: Field<()>,
    pub cache: Field<()>,
    pub input: Field<u64>,
    pub output: Field<u64>,
    pub cache_read: Field<u64>,
    pub cache_write: Field<u64>,
    pub total: Field<u64>,
    pub status: Status,
    pub diagnostics: Diagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // Inline bounded result avoids an extra allocation.
pub enum ParseResult {
    Rejected(Code),
    Observation(Observation),
}
impl ParseResult {
    pub fn observation(&self) -> Option<&Observation> {
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
            for field in [&o.message_id, &o.session_id, &o.model_id, &o.provider_id] {
                if let Field::Valid(s) = field {
                    bytes = bytes.checked_add(s.len())?;
                }
            }
        }
        Some(bytes)
    }
}
