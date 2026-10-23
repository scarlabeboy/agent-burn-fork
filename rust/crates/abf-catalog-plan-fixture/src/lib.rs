//! Pure conditional planning of explicitly fictional Windows claims.
//! No native authority, inventory, reader events or persistent identities.
#![forbid(unsafe_code)]

use abf_source_run::{CatalogBuilder, CatalogScope, CatalogSnapshot, EnvironmentKind};

macro_rules! redacted {
    ($($t:ty),+ $(,)?) => {$(impl std::fmt::Debug for $t {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(concat!(stringify!($t), "([redacted])"))
        }
    })+};
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootClass {
    Default,
    Configured,
    Relocated,
}
#[derive(Clone)]
pub struct WindowsRequest<'a> {
    pub namespace: &'a str,
    pub families: &'a [&'a str],
}
#[derive(Clone)]
pub struct FixtureDescriptor<'a> {
    pub id: &'a str,
    pub schema: u32,
    pub revision: &'a str,
    pub scope: CatalogScope,
    pub rule: &'a str,
    pub scope_tag: &'a str,
    pub approved_classes: &'a [RootClass],
    /// Optional fictional potential-overlap claim; target is another descriptor ID.
    pub overlaps: Option<&'a str>,
}
#[derive(Clone)]
pub struct FictionalCandidate<'a> {
    pub id: &'a str,
    pub family: &'a str,
    pub namespace: &'a str,
    pub revision: &'a str,
    pub class: RootClass,
    pub locator: &'a str,
    /// None = absent evidence, Some(empty) = explicit zero matches.
    pub recognition: Option<&'a [RecognitionClaim<'a>]>,
}
#[derive(Clone)]
pub struct FictionalGrant<'a> {
    pub candidate: &'a str,
    pub family: &'a str,
    pub namespace: &'a str,
    pub candidate_revision: &'a str,
    pub class: RootClass,
    pub descriptor: &'a str,
    pub descriptor_revision: &'a str,
}
#[derive(Clone)]
pub struct RecognitionClaim<'a> {
    pub descriptor: &'a str,
    pub descriptor_revision: &'a str,
    pub rule: &'a str,
}

/// Fixed diagnostic codes. Declaration order documents precedence within phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanCode {
    Budget,
    InvalidShape,
    UnsupportedSchema,
    UnsupportedLayout,
    NamespaceMismatch,
    UnknownFamily,
    DuplicateDescriptor,
    DuplicateRequest,
    DuplicateTag,
    DuplicateCandidate,
    DuplicateGrant,
    IdentifierBinding,
    InvalidLocator,
    ClassDenied,
    GrantBinding,
    RecognitionBinding,
    AmbiguousRecognition,
    MultipleRoots,
    AliasCollision,
    OverlapConflict,
    AllocationFailure,
    CatalogConstruction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionState {
    Conditional,
    Unresolved,
    Unrecognized,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanOutcome {
    NoSelectedWork,
    RequestedWork,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissingEvidence {
    pub candidate: bool,
    pub grant: bool,
    pub recognition: bool,
}
/// All remain unmet for every plan, including a plan with no selected work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRequirement {
    ActualConsent,
    VerifiedWindowsIdentity,
    FilesystemAccess,
    RootConfinement,
    NativeLayoutRecognition,
    CompletedEnumeration,
    PhysicalSourceIdentity,
    ProductionCatalogAuthority,
}
const REQUIREMENTS: [NativeRequirement; 8] = [
    NativeRequirement::ActualConsent,
    NativeRequirement::VerifiedWindowsIdentity,
    NativeRequirement::FilesystemAccess,
    NativeRequirement::RootConfinement,
    NativeRequirement::NativeLayoutRecognition,
    NativeRequirement::CompletedEnumeration,
    NativeRequirement::PhysicalSourceIdentity,
    NativeRequirement::ProductionCatalogAuthority,
];
#[derive(Clone, PartialEq, Eq)]
pub struct Decision {
    family: String,
    state: DecisionState,
    missing: MissingEvidence,
    root: Option<FictionalRootClaim>,
    scope: Option<CatalogScope>,
}
/// Owned claim, never a path capability. Explicit access can expose sensitive text.
#[derive(Clone, PartialEq, Eq)]
pub struct FictionalRootClaim {
    candidate: String,
    locator: String,
    revision: String,
    class: RootClass,
    descriptor: Option<String>,
    descriptor_revision: Option<String>,
    recognition_rule: Option<String>,
    grant_present: bool,
}
impl FictionalRootClaim {
    pub fn candidate(&self) -> &str {
        &self.candidate
    }
    pub fn locator(&self) -> &str {
        &self.locator
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn class(&self) -> RootClass {
        self.class
    }
    pub fn descriptor(&self) -> Option<&str> {
        self.descriptor.as_deref()
    }
    pub fn descriptor_revision(&self) -> Option<&str> {
        self.descriptor_revision.as_deref()
    }
    pub fn recognition_rule(&self) -> Option<&str> {
        self.recognition_rule.as_deref()
    }
    pub fn grant_present(&self) -> bool {
        self.grant_present
    }
}
impl Decision {
    pub fn family(&self) -> &str {
        &self.family
    }
    pub fn state(&self) -> DecisionState {
        self.state
    }
    pub fn missing(&self) -> MissingEvidence {
        self.missing
    }
    pub fn root_claim(&self) -> Option<&FictionalRootClaim> {
        self.root.as_ref()
    }
    pub fn conditional_scope(&self) -> Option<&CatalogScope> {
        self.scope.as_ref()
    }
}
/// Private validated construction; cloning preserves the snapshot's allocation authority.
/// ```compile_fail,E0451
/// use abf_catalog_plan_fixture::{FixturePlan, PlanOutcome};
/// use abf_source_run::CatalogBuilder;
/// let forged = FixturePlan { namespace: String::new(), catalog: CatalogBuilder::new().build(),
///     decisions: vec![], outcome: PlanOutcome::RequestedWork };
/// ```
/// ```compile_fail,E0599
/// use abf_catalog_plan_fixture::FixturePlan;
/// fn authorize(plan: &FixturePlan) { plan.into_authorized_root(); }
/// ```
/// ```compile_fail,E0596
/// use abf_catalog_plan_fixture::FixturePlan;
/// fn mutate(plan: &FixturePlan) { plan.decisions().clear(); }
/// ```
#[derive(Clone)]
pub struct FixturePlan {
    namespace: String,
    catalog: CatalogSnapshot,
    decisions: Vec<Decision>,
    outcome: PlanOutcome,
}
impl FixturePlan {
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn catalog(&self) -> &CatalogSnapshot {
        &self.catalog
    }
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }
    pub fn outcome(&self) -> PlanOutcome {
        self.outcome
    }
    pub fn requirements(&self) -> &[NativeRequirement] {
        &REQUIREMENTS
    }
}
redacted!(
    WindowsRequest<'_>,
    FixtureDescriptor<'_>,
    FictionalCandidate<'_>,
    FictionalGrant<'_>,
    RecognitionClaim<'_>,
    Decision,
    FictionalRootClaim,
    FixturePlan
);

fn opaque(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn tag(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn checked_total(mut lengths: impl Iterator<Item = usize>) -> Result<usize, PlanCode> {
    let n = lengths.try_fold(0usize, |a, b| a.checked_add(b).ok_or(PlanCode::Budget))?;
    if n > 8192 {
        return Err(PlanCode::Budget);
    }
    Ok(n)
}
fn separator(b: u8) -> bool {
    b == b'\\' || b == b'/'
}
fn valid_locator(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !separator(bytes[2])
    {
        return false;
    }
    s[3..].split(['\\', '/']).all(|part| {
        if part.is_empty()
            || part.starts_with(' ')
            || part.ends_with(['.', ' '])
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b' '))
        {
            return false;
        }
        let stem = part
            .split('.')
            .next()
            .expect("nonempty component")
            .trim_end_matches(' ');
        !["CON", "PRN", "AUX", "NUL"]
            .iter()
            .any(|r| stem.eq_ignore_ascii_case(r))
            && !(stem.len() == 4
                && (stem[..3].eq_ignore_ascii_case("COM") || stem[..3].eq_ignore_ascii_case("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    })
}
fn key(s: &str) -> impl Iterator<Item = u8> + '_ {
    s.bytes().map(|b| {
        if separator(b) {
            b'\\'
        } else {
            b.to_ascii_lowercase()
        }
    })
}
fn alias(a: &str, b: &str) -> bool {
    key(a).eq(key(b))
}
fn ancestor(a: &str, b: &str) -> bool {
    b.len() > a.len() && key(a).eq(key(b).take(a.len())) && separator(b.as_bytes()[a.len()])
}
fn duplicate<T>(s: &[T], same: impl Fn(&T, &T) -> bool) -> bool {
    s.iter()
        .enumerate()
        .any(|(i, a)| s[..i].iter().any(|b| same(a, b)))
}
fn copy(s: &str) -> Result<String, PlanCode> {
    let mut result = String::new();
    result
        .try_reserve_exact(s.len())
        .map_err(|_| PlanCode::AllocationFailure)?;
    result.push_str(s);
    Ok(result)
}

/// Validate fictional claims without any probing, emission or native authorization.
/// Every refusal is atomic: no usable partial catalog escapes.
pub fn plan_windows_fixture(
    request: &WindowsRequest<'_>,
    descriptors: &[FixtureDescriptor<'_>],
    candidates: &[FictionalCandidate<'_>],
    grants: &[FictionalGrant<'_>],
) -> Result<FixturePlan, PlanCode> {
    // 1. Budgets before shape and before any transformed allocation.
    if [
        request.families.len(),
        descriptors.len(),
        candidates.len(),
        grants.len(),
    ]
    .into_iter()
    .any(|n| n > 16)
        || descriptors.iter().any(|d| d.approved_classes.len() > 3)
        || candidates
            .iter()
            .any(|c| c.locator.len() > 512 || c.recognition.is_some_and(|r| r.len() > 2))
    {
        return Err(PlanCode::Budget);
    }
    checked_total(candidates.iter().map(|c| c.locator.len()))?;
    if candidates
        .iter()
        .any(|c| c.locator.bytes().skip(3).filter(|b| separator(*b)).count() >= 16)
    {
        return Err(PlanCode::Budget);
    }
    if !opaque(request.namespace)
        || request.families.iter().any(|f| !opaque(f))
        || descriptors.iter().any(|d| {
            !opaque(d.id)
                || !opaque(d.revision)
                || !opaque(d.rule)
                || !tag(d.scope_tag)
                || d.overlaps.is_some_and(|v| !opaque(v))
                || d.approved_classes.is_empty()
                || duplicate(d.approved_classes, |a, b| a == b)
        })
        || candidates.iter().any(|c| {
            [c.id, c.family, c.namespace, c.revision]
                .into_iter()
                .any(|v| !opaque(v))
                || c.recognition.is_some_and(|rs| {
                    rs.iter().any(|r| {
                        [r.descriptor, r.descriptor_revision, r.rule]
                            .into_iter()
                            .any(|v| !opaque(v))
                    })
                })
        })
        || grants.iter().any(|g| {
            [
                g.candidate,
                g.family,
                g.namespace,
                g.candidate_revision,
                g.descriptor,
                g.descriptor_revision,
            ]
            .into_iter()
            .any(|v| !opaque(v))
        })
    {
        return Err(PlanCode::InvalidShape);
    }
    // 2. Schema, layout and namespace are distinct fixed refusals.
    if descriptors.iter().any(|d| d.schema != 1) {
        return Err(PlanCode::UnsupportedSchema);
    }
    if descriptors
        .iter()
        .any(|d| !matches!(d.scope.layout().as_str(), "fixture-v1" | "fixture-v2"))
    {
        return Err(PlanCode::UnsupportedLayout);
    }
    if descriptors.iter().any(|d| {
        d.scope.environment_kind() != EnvironmentKind::Windows
            || d.scope.environment_ref().as_str() != request.namespace
    }) || candidates.iter().any(|c| c.namespace != request.namespace)
    {
        return Err(PlanCode::NamespaceMismatch);
    }
    if request
        .families
        .iter()
        .any(|f| !descriptors.iter().any(|d| d.scope.family().as_str() == *f))
    {
        return Err(PlanCode::UnknownFamily);
    }
    // 3. Uniqueness and structural bindings across ALL supplied claims.
    if duplicate(descriptors, |a, b| a.id == b.id || a.scope == b.scope) {
        return Err(PlanCode::DuplicateDescriptor);
    }
    if duplicate(request.families, |a, b| a == b) {
        return Err(PlanCode::DuplicateRequest);
    }
    if duplicate(descriptors, |a, b| a.scope_tag == b.scope_tag) {
        return Err(PlanCode::DuplicateTag);
    }
    if duplicate(candidates, |a, b| a.id == b.id) {
        return Err(PlanCode::DuplicateCandidate);
    }
    if duplicate(grants, |a, b| a.candidate == b.candidate) {
        return Err(PlanCode::DuplicateGrant);
    }
    if descriptors.iter().any(|d| {
        d.overlaps
            .is_some_and(|id| id == d.id || !descriptors.iter().any(|other| other.id == id))
    }) || candidates.iter().any(|c| {
        !descriptors
            .iter()
            .any(|d| d.scope.family().as_str() == c.family)
    }) {
        return Err(PlanCode::IdentifierBinding);
    }
    // 4. Lexical grammar and fictional class/grant consistency. No OS path API.
    if candidates.iter().any(|c| !valid_locator(c.locator)) {
        return Err(PlanCode::InvalidLocator);
    }
    if candidates.iter().any(|c| {
        !descriptors
            .iter()
            .any(|d| d.scope.family().as_str() == c.family && d.approved_classes.contains(&c.class))
    }) {
        return Err(PlanCode::ClassDenied);
    }
    if grants.iter().any(|g| {
        let c = candidates.iter().find(|c| c.id == g.candidate);
        let d = descriptors.iter().find(|d| d.id == g.descriptor);
        match (c, d) {
            (Some(c), Some(d)) => {
                g.namespace != request.namespace
                    || g.namespace != c.namespace
                    || g.family != c.family
                    || g.family != d.scope.family().as_str()
                    || g.candidate_revision != c.revision
                    || g.descriptor_revision != d.revision
                    || g.class != c.class
                    || !d.approved_classes.contains(&g.class)
                    || c.recognition
                        .is_some_and(|r| r.len() == 1 && r[0].descriptor != g.descriptor)
            }
            _ => true,
        }
    }) {
        return Err(PlanCode::GrantBinding);
    }
    // 5. Validate every recognition binding, then refuse ambiguity and selected roots.
    if candidates.iter().any(|c| {
        c.recognition.is_some_and(|rs| {
            rs.iter().any(|r| {
                !descriptors.iter().any(|d| {
                    d.id == r.descriptor
                        && d.revision == r.descriptor_revision
                        && d.rule == r.rule
                        && d.scope.family().as_str() == c.family
                        && d.approved_classes.contains(&c.class)
                })
            })
        })
    }) {
        return Err(PlanCode::RecognitionBinding);
    }
    let selected = |c: &FictionalCandidate<'_>| request.families.contains(&c.family);
    if candidates
        .iter()
        .any(|c| c.recognition.is_some_and(|r| r.len() > 1))
    {
        return Err(PlanCode::AmbiguousRecognition);
    }
    if request
        .families
        .iter()
        .any(|f| candidates.iter().filter(|c| c.family == *f).count() > 1)
    {
        return Err(PlanCode::MultipleRoots);
    }
    // 6. Suspected aliases and overlap: claims only, never actual object identity.
    if duplicate(candidates, |a, b| {
        selected(a) && selected(b) && alias(a.locator, b.locator)
    }) {
        return Err(PlanCode::AliasCollision);
    }
    if duplicate(candidates, |a, b| {
        selected(a)
            && selected(b)
            && (ancestor(a.locator, b.locator) || ancestor(b.locator, a.locator))
    }) || descriptors.iter().any(|d| {
        request.families.contains(&d.scope.family().as_str())
            && d.overlaps.is_some_and(|id| {
                descriptors.iter().any(|other| {
                    other.id == id && request.families.contains(&other.scope.family().as_str())
                })
            })
    }) {
        return Err(PlanCode::OverlapConflict);
    }
    // 7. One catalog allocation only, after complete replay-independent preflight.
    let mut families = Vec::new();
    families
        .try_reserve_exact(request.families.len())
        .map_err(|_| PlanCode::AllocationFailure)?;
    families.extend_from_slice(request.families);
    families.sort_unstable();
    let mut decisions = Vec::new();
    decisions
        .try_reserve_exact(families.len())
        .map_err(|_| PlanCode::AllocationFailure)?;
    let mut catalog = CatalogBuilder::new();
    for family in families {
        let candidate = candidates.iter().find(|c| c.family == family);
        let grant = candidate.and_then(|c| grants.iter().find(|g| g.candidate == c.id));
        let recognized = candidate.and_then(|c| c.recognition);
        let missing = MissingEvidence {
            candidate: candidate.is_none(),
            grant: grant.is_none(),
            recognition: recognized.is_none(),
        };
        let matched = recognized
            .and_then(|rs| rs.first())
            .and_then(|r| descriptors.iter().find(|d| d.id == r.descriptor));
        let state = match recognized {
            Some([]) => DecisionState::Unrecognized,
            Some([_]) if grant.is_some() => DecisionState::Conditional,
            _ => DecisionState::Unresolved,
        };
        let bound_descriptor = matched
            .or_else(|| grant.and_then(|g| descriptors.iter().find(|d| d.id == g.descriptor)));
        let root = candidate
            .map(|c| -> Result<_, PlanCode> {
                Ok(FictionalRootClaim {
                    candidate: copy(c.id)?,
                    locator: copy(c.locator)?,
                    revision: copy(c.revision)?,
                    class: c.class,
                    descriptor: bound_descriptor.map(|d| copy(d.id)).transpose()?,
                    descriptor_revision: bound_descriptor.map(|d| copy(d.revision)).transpose()?,
                    recognition_rule: matched.map(|d| copy(d.rule)).transpose()?,
                    grant_present: grant.is_some(),
                })
            })
            .transpose()?;
        let scope = if state == DecisionState::Conditional {
            let d = matched.expect("preflight unique recognition");
            catalog
                .add(d.scope.clone(), d.scope_tag)
                .map_err(|_| PlanCode::CatalogConstruction)?;
            Some(d.scope.clone())
        } else {
            None
        };
        decisions.push(Decision {
            family: copy(family)?,
            state,
            missing,
            root,
            scope,
        });
    }
    Ok(FixturePlan {
        namespace: copy(request.namespace)?,
        catalog: catalog.build(),
        decisions,
        outcome: if request.families.is_empty() {
            PlanOutcome::NoSelectedWork
        } else {
            PlanOutcome::RequestedWork
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_accumulation_refuses_total_excess_and_arithmetic_overflow() {
        assert_eq!(checked_total([8191].into_iter()), Ok(8191));
        assert_eq!(checked_total([4096, 4096].into_iter()), Ok(8192));
        assert_eq!(
            checked_total([4096, 4097].into_iter()),
            Err(PlanCode::Budget)
        );
        assert_eq!(
            checked_total([usize::MAX, 1].into_iter()),
            Err(PlanCode::Budget)
        );
    }
}
