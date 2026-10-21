use crate::{EnvironmentKind, OpaqueRef};

pub const MAX_INSTANCES_PER_SCOPE: u32 = 256;
pub const MAX_EXECUTIONS_PER_ISSUER: u32 = 1024;
pub const MAX_MEMBERS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefCode {
    InvalidTag,
    DuplicateScopeTag,
    DuplicateScope,
    UnknownScope,
    DuplicateInstanceTag,
    ReservedCounterForm,
    DuplicateAttemptRef,
    CounterExhausted,
    CapacityExceeded,
}

fn tag(s: &str, max: usize, underscore: bool) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || (underscore && b == b'_'))
}
fn reserved(s: &str, prefix: char) -> bool {
    s.strip_prefix(prefix)
        .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
}

#[derive(Clone, PartialEq, Eq)]
pub struct CatalogScope {
    kind: EnvironmentKind,
    environment: OpaqueRef,
    family: OpaqueRef,
    layout: OpaqueRef,
}
impl CatalogScope {
    /// The accepted parser has one harness, OpenCode. Labels are synthetic claims.
    pub fn new(
        kind: EnvironmentKind,
        environment: &str,
        family: &str,
        layout: &str,
    ) -> Result<Self, RefCode> {
        let make = |s| OpaqueRef::new(s).map_err(|_| RefCode::InvalidTag);
        Ok(Self {
            kind,
            environment: make(environment)?,
            family: make(family)?,
            layout: make(layout)?,
        })
    }
    pub fn environment_kind(&self) -> EnvironmentKind {
        self.kind
    }
    pub fn environment_ref(&self) -> &OpaqueRef {
        &self.environment
    }
    pub fn family(&self) -> &OpaqueRef {
        &self.family
    }
    pub fn layout(&self) -> &OpaqueRef {
        &self.layout
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceRef(OpaqueRef);
impl SourceRef {
    /// Syntax only: finalization separately checks catalog binding, never issuer history.
    pub fn new(s: &str) -> Result<Self, RefCode> {
        let (scope, instance) = s.split_once('-').ok_or(RefCode::InvalidTag)?;
        if !tag(scope, 16, false) || !tag(instance, 47, true) {
            return Err(RefCode::InvalidTag);
        }
        if reserved(instance, 'i') {
            let digits = &instance[1..];
            let n = digits.parse::<u32>().map_err(|_| RefCode::InvalidTag)?;
            if digits.starts_with('0') || !(1..=MAX_INSTANCES_PER_SCOPE).contains(&n) {
                return Err(RefCode::InvalidTag);
            }
        }
        Ok(Self(OpaqueRef::new(s).map_err(|_| RefCode::InvalidTag)?))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub(crate) fn scope_tag(&self) -> &str {
        self.as_str().split_once('-').expect("validated").0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AttemptRef(OpaqueRef);
impl AttemptRef {
    pub fn new(s: &str) -> Result<Self, RefCode> {
        if !tag(s, 64, true) || !s.as_bytes()[0].is_ascii_alphabetic() || reserved(s, 'z') {
            return Err(RefCode::InvalidTag);
        }
        if reserved(s, 'a') {
            let n = s[1..].parse::<u32>().map_err(|_| RefCode::InvalidTag)?;
            if s[1..].starts_with('0') || !(1..=MAX_EXECUTIONS_PER_ISSUER).contains(&n) {
                return Err(RefCode::InvalidTag);
            }
        }
        Ok(Self(OpaqueRef::new(s).map_err(|_| RefCode::InvalidTag)?))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct FinalizationId(OpaqueRef);
impl FinalizationId {
    pub fn new(s: &str) -> Result<Self, RefCode> {
        let digits = s.strip_prefix('z').ok_or(RefCode::InvalidTag)?;
        let n = digits.parse::<u32>().map_err(|_| RefCode::InvalidTag)?;
        if digits.starts_with('0') || !(1..=MAX_EXECUTIONS_PER_ISSUER).contains(&n) {
            return Err(RefCode::InvalidTag);
        }
        Ok(Self(OpaqueRef::new(s).map_err(|_| RefCode::InvalidTag)?))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    entries: Vec<(CatalogScope, String)>,
}
impl CatalogSnapshot {
    pub fn scopes(&self) -> impl Iterator<Item = &CatalogScope> {
        self.entries.iter().map(|e| &e.0)
    }
    pub fn scope_tag(&self, scope: &CatalogScope) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| &e.0 == scope)
            .map(|e| e.1.as_str())
    }
}
#[derive(Default)]
pub struct CatalogBuilder {
    entries: Vec<(CatalogScope, String)>,
}
impl CatalogBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, scope: CatalogScope, scope_tag: &str) -> Result<(), RefCode> {
        if !tag(scope_tag, 16, false) {
            return Err(RefCode::InvalidTag);
        }
        if self.entries.iter().any(|e| e.1 == scope_tag) {
            return Err(RefCode::DuplicateScopeTag);
        }
        if self.entries.iter().any(|e| e.0 == scope) {
            return Err(RefCode::DuplicateScope);
        }
        if self.entries.len() >= MAX_MEMBERS {
            return Err(RefCode::CapacityExceeded);
        }
        self.entries.push((scope, scope_tag.into()));
        Ok(())
    }
    pub fn build(self) -> CatalogSnapshot {
        CatalogSnapshot {
            entries: self.entries,
        }
    }
}

struct Registry {
    scope: CatalogScope,
    tag: String,
    count: u32,
    labels: Vec<String>,
}
pub struct FixtureRefIssuer {
    registry: Vec<Registry>,
    attempts: Vec<String>,
    executions: u32,
    finalizations: u32,
}
impl FixtureRefIssuer {
    pub fn new(catalog: &CatalogSnapshot) -> Self {
        Self {
            registry: catalog
                .entries
                .iter()
                .map(|(s, t)| Registry {
                    scope: s.clone(),
                    tag: t.clone(),
                    count: 0,
                    labels: Vec::new(),
                })
                .collect(),
            attempts: Vec::new(),
            executions: 0,
            finalizations: 0,
        }
    }
    pub fn reset(&mut self) {
        for r in &mut self.registry {
            r.count = 0;
            r.labels.clear();
        }
        self.attempts.clear();
        self.executions = 0;
        self.finalizations = 0;
    }
    pub fn instance_counter(&self, scope: &CatalogScope) -> Result<u32, RefCode> {
        Ok(self
            .registry
            .iter()
            .find(|r| &r.scope == scope)
            .ok_or(RefCode::UnknownScope)?
            .count)
    }
    pub fn registry_size(&self, scope: &CatalogScope) -> Result<usize, RefCode> {
        let r = self
            .registry
            .iter()
            .find(|r| &r.scope == scope)
            .ok_or(RefCode::UnknownScope)?;
        Ok(r.count as usize + r.labels.len())
    }
    pub fn execution_counter(&self) -> u32 {
        self.executions
    }
    pub fn finalization_counter(&self) -> u32 {
        self.finalizations
    }
    pub fn create_instance(&mut self, scope: &CatalogScope) -> Result<SourceRef, RefCode> {
        let r = self
            .registry
            .iter_mut()
            .find(|r| &r.scope == scope)
            .ok_or(RefCode::UnknownScope)?;
        if r.count >= MAX_INSTANCES_PER_SCOPE
            || r.count as usize + r.labels.len() >= MAX_INSTANCES_PER_SCOPE as usize
        {
            return Err(RefCode::CounterExhausted);
        }
        let next = r.count.checked_add(1).ok_or(RefCode::CounterExhausted)?;
        let result = SourceRef::new(&format!("{}-i{next}", r.tag))?;
        r.count = next;
        Ok(result)
    }
    pub fn create_instance_labelled(
        &mut self,
        scope: &CatalogScope,
        label: &str,
    ) -> Result<SourceRef, RefCode> {
        if !tag(label, 47, true) {
            return Err(RefCode::InvalidTag);
        }
        if reserved(label, 'i') {
            return Err(RefCode::ReservedCounterForm);
        }
        let r = self
            .registry
            .iter_mut()
            .find(|r| &r.scope == scope)
            .ok_or(RefCode::UnknownScope)?;
        if r.labels.iter().any(|s| s == label) {
            return Err(RefCode::DuplicateInstanceTag);
        }
        if r.count as usize + r.labels.len() >= MAX_INSTANCES_PER_SCOPE as usize {
            return Err(RefCode::CounterExhausted);
        }
        let result = SourceRef::new(&format!("{}-{label}", r.tag))?;
        r.labels.push(label.into());
        Ok(result)
    }
    pub fn begin_execution(&mut self) -> Result<AttemptRef, RefCode> {
        if self.executions >= MAX_EXECUTIONS_PER_ISSUER {
            return Err(RefCode::CounterExhausted);
        }
        let next = self
            .executions
            .checked_add(1)
            .ok_or(RefCode::CounterExhausted)?;
        let result = AttemptRef::new(&format!("a{next}"))?;
        self.executions = next;
        Ok(result)
    }
    pub fn begin_execution_labelled(&mut self, label: &str) -> Result<AttemptRef, RefCode> {
        if reserved(label, 'a') || reserved(label, 'z') {
            return Err(RefCode::ReservedCounterForm);
        }
        let result = AttemptRef::new(label)?;
        if self.attempts.iter().any(|s| s == label) {
            return Err(RefCode::DuplicateAttemptRef);
        }
        if self.attempts.len() >= MAX_EXECUTIONS_PER_ISSUER as usize {
            return Err(RefCode::CounterExhausted);
        }
        self.attempts.push(label.into());
        Ok(result)
    }
    pub fn issue_finalization(&mut self) -> Result<FinalizationId, RefCode> {
        if self.finalizations >= MAX_EXECUTIONS_PER_ISSUER {
            return Err(RefCode::CounterExhausted);
        }
        let next = self
            .finalizations
            .checked_add(1)
            .ok_or(RefCode::CounterExhausted)?;
        let result = FinalizationId::new(&format!("z{next}"))?;
        self.finalizations = next;
        Ok(result)
    }
}
redacted!(
    CatalogScope,
    SourceRef,
    AttemptRef,
    FinalizationId,
    CatalogSnapshot,
    CatalogBuilder,
    FixtureRefIssuer
);
