use std::fmt;

/// Research-only opaque reference ceiling, matching the earlier boundary's 64 bytes.
const MAX_REF_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextCode {
    Empty,
    TooLong,
    Malformed,
}

/// Declared synthetic claim, never proof of identity. ASCII [A-Za-z0-9_-]{1,64}.
/// Syntax excludes path separators but does not prove absence of sensitive values.
#[derive(Clone, PartialEq, Eq)]
pub struct OpaqueRef(Box<str>);

impl OpaqueRef {
    pub fn new(value: &str) -> Result<Self, ContextCode> {
        if value.is_empty() {
            return Err(ContextCode::Empty);
        }
        if value.len() > MAX_REF_BYTES {
            return Err(ContextCode::TooLong);
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(ContextCode::Malformed);
        }
        Ok(Self(value.into()))
    }

    /// Explicit access to a claim; not sanitized for logging or export.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OpaqueRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpaqueRef([redacted])")
    }
}

/// Computer is a future projection, never a source environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentKind {
    Windows,
    SelectedWsl,
}

/// Harness is independent of record model/provider claims. Only this adapter exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    OpenCode,
}

/// Parser implementation identification, not a deployed OpenCode schema version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParserId {
    AbfOpenCodeV0_1_0,
}

/// Caller-declared synthetic context. No paths, distro names or account fields.
/// Equal claims establish neither physical uniqueness nor cross-source equivalence.
#[derive(Clone, PartialEq, Eq)]
pub struct DeclaredContext {
    environment_kind: EnvironmentKind,
    environment_ref: OpaqueRef,
    source_ref: OpaqueRef,
    run_ref: OpaqueRef,
    correlation: Option<OpaqueRef>,
    ordinal: Option<u64>,
}

impl DeclaredContext {
    /// Ordinal is an optional zero-based caller claim, not a physical locator.
    pub fn new(
        environment_kind: EnvironmentKind,
        environment_ref: OpaqueRef,
        source_ref: OpaqueRef,
        run_ref: OpaqueRef,
        correlation: Option<OpaqueRef>,
        ordinal: Option<u64>,
    ) -> Self {
        Self {
            environment_kind,
            environment_ref,
            source_ref,
            run_ref,
            correlation,
            ordinal,
        }
    }

    pub fn environment_kind(&self) -> EnvironmentKind {
        self.environment_kind
    }
    pub fn environment_ref(&self) -> &OpaqueRef {
        &self.environment_ref
    }
    pub fn source_ref(&self) -> &OpaqueRef {
        &self.source_ref
    }
    pub fn run_ref(&self) -> &OpaqueRef {
        &self.run_ref
    }
    pub fn correlation(&self) -> Option<&OpaqueRef> {
        self.correlation.as_ref()
    }
    pub fn ordinal(&self) -> Option<u64> {
        self.ordinal
    }
    pub fn harness(&self) -> Harness {
        Harness::OpenCode
    }
    pub fn parser(&self) -> ParserId {
        ParserId::AbfOpenCodeV0_1_0
    }
}

impl fmt::Debug for DeclaredContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeclaredContext([redacted])")
    }
}
