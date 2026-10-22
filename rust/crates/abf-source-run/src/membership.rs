use crate::{
    CatalogScope, CatalogSnapshot, EnvironmentKind, FinalizedMember, OpaqueRef, SecondEnumeration,
};
pub use abf_counting::Selection;
use std::collections::BTreeSet;

/// Selected synthetic environment identities. Each kind has at most one slot;
/// validation requires exactly the slots named by `Selection` (A1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedRefs {
    pub windows: Option<OpaqueRef>,
    pub selected_wsl: Option<OpaqueRef>,
}
impl SelectedRefs {
    fn get(&self, kind: EnvironmentKind) -> Option<&OpaqueRef> {
        match kind {
            EnvironmentKind::Windows => self.windows.as_ref(),
            EnvironmentKind::SelectedWsl => self.selected_wsl.as_ref(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdaptCode {
    SelectionRefMismatch,
    MemberKindNotSelected,
    EnvironmentRefNotSelected,
    DuplicateScopeMember,
    FamilyOverlap,
    /// ABF108D synthetic gate: supplied catalog is not the finalization authority.
    CatalogAuthorityMismatch,
    DeclaredCatalogOverlap,
    AttemptRefReused,
    MemberNotInManifest,
    RunKeyCollision,
    SourceRefNamespaceConflict,
    OccurrenceCeiling,
    DeclarationCeiling,
    Observation(abf_observation::AdapterCode),
    Occurrence(abf_identity_relations::Code),
    Projection(abf_counting::Code),
}

/// Admission only: immutable borrowed finalizations in canonical order.
/// No projection or numeric claim is constructed by this type.
///
/// Fields are private, with no default or unchecked constructor.
/// ```compile_fail,E0451
/// use abf_source_run::{ProjectionMembership, SelectedRefs, Selection};
/// let _ = ProjectionMembership {
///     selection: Selection::WindowsOnly,
///     selected_refs: SelectedRefs { windows: None, selected_wsl: None },
///     members: vec![],
/// };
/// ```
/// ```compile_fail,E0277
/// use abf_source_run::ProjectionMembership;
/// let _: ProjectionMembership<'static> = Default::default();
/// ```
/// ```compile_fail,E0277
/// use abf_source_run::{FinalizedMember, ProjectionMembership};
/// let members: Vec<&FinalizedMember> = vec![];
/// let _: ProjectionMembership<'_> = members.into();
/// ```
/// ```compile_fail,E0599
/// use abf_source_run::ProjectionMembership;
/// let _ = ProjectionMembership::new();
/// ```
/// ```compile_fail,E0616
/// use abf_source_run::FinalizedMember;
/// fn forge(member: &mut FinalizedMember) {
///     member.manifest.members.clear();
/// }
/// ```
/// ```compile_fail,E0594
/// use abf_source_run::{ProjectionMembership, FinalizedMember};
/// fn replace<'a>(membership: &ProjectionMembership<'a>, other: &'a FinalizedMember) {
///     membership.members()[0] = other;
/// }
/// ```
/// ```compile_fail,E0277
/// use abf_source_run::{ProjectionMembership, CoverageVerdict};
/// let _: ProjectionMembership<'_> = CoverageVerdict::NoAttempt.into();
/// ```
pub struct ProjectionMembership<'a> {
    selection: Selection,
    selected_refs: SelectedRefs,
    members: Vec<&'a FinalizedMember>,
}
redacted!(ProjectionMembership<'_>);

fn includes(selection: Selection, kind: EnvironmentKind) -> bool {
    matches!(
        (selection, kind),
        (Selection::Both, _)
            | (Selection::WindowsOnly, EnvironmentKind::Windows)
            | (Selection::SelectedWslOnly, EnvironmentKind::SelectedWsl)
    )
}
fn kind_key(kind: EnvironmentKind) -> u8 {
    match kind {
        EnvironmentKind::Windows => 0,
        EnvironmentKind::SelectedWsl => 1,
    }
}
fn scope_key(scope: &CatalogScope) -> (u8, &str, &str, &str) {
    // Harness is the single accepted OpenCode harness for every CatalogScope.
    (
        kind_key(scope.environment_kind()),
        scope.environment_ref().as_str(),
        scope.family().as_str(),
        scope.layout().as_str(),
    )
}
fn same_environment(a: &CatalogScope, b: &CatalogScope) -> bool {
    a.environment_kind() == b.environment_kind() && a.environment_ref() == b.environment_ref()
}

impl<'a> ProjectionMembership<'a> {
    /// V1.7 §9.2: all checks are whole-membership passes in normative order.
    /// Finalization already guarantees per-member source binding and uniqueness.
    /// ABF108D: after A5 and before A6, every member must share the supplied
    /// catalog's immutable authority (clones preserve it; rebuilds do not).
    pub fn validate(
        selection: Selection,
        selected_refs: SelectedRefs,
        members: &[&'a FinalizedMember],
        catalog: &CatalogSnapshot,
    ) -> Result<Self, AdaptCode> {
        // A1. Duplicate refs of a kind are unrepresentable in SelectedRefs.
        for kind in [EnvironmentKind::Windows, EnvironmentKind::SelectedWsl] {
            if selected_refs.get(kind).is_some() != includes(selection, kind) {
                return Err(AdaptCode::SelectionRefMismatch);
            }
        }
        // A2 and A3 must be separate passes: input order cannot set precedence.
        if members
            .iter()
            .any(|m| !includes(selection, m.scope.environment_kind()))
        {
            return Err(AdaptCode::MemberKindNotSelected);
        }
        if members.iter().any(|m| {
            selected_refs.get(m.scope.environment_kind()) != Some(m.scope.environment_ref())
        }) {
            return Err(AdaptCode::EnvironmentRefNotSelected);
        }
        // A4, A5, A6: earlier rule wins even if a later pair appears first.
        for (i, m) in members.iter().enumerate() {
            if members[..i].iter().any(|n| n.scope == m.scope) {
                return Err(AdaptCode::DuplicateScopeMember);
            }
        }
        for (i, m) in members.iter().enumerate() {
            if members[..i].iter().any(|n| {
                same_environment(&n.scope, &m.scope) && n.scope.family() == m.scope.family()
            }) {
                return Err(AdaptCode::FamilyOverlap);
            }
        }
        // ABF108D D1, synthetic only: A5 < authority precondition < A6.
        if members.iter().any(|m| !m.catalog.same_authority(catalog)) {
            return Err(AdaptCode::CatalogAuthorityMismatch);
        }
        for (i, m) in members.iter().enumerate() {
            if members[..i]
                .iter()
                .any(|n| catalog.possibly_overlapping(&n.scope, &m.scope))
            {
                return Err(AdaptCode::DeclaredCatalogOverlap);
            }
        }
        // A7. The execution manifest is an ordered list (§7.7), not a set.
        for (i, m) in members.iter().enumerate() {
            if members[..i].iter().any(|n| {
                n.attempt == m.attempt
                    && (n.finalization != m.finalization || n.manifest != m.manifest)
            }) {
                return Err(AdaptCode::AttemptRefReused);
            }
        }
        if members
            .iter()
            .any(|m| !m.manifest.members.contains(&m.scope))
        {
            return Err(AdaptCode::MemberNotInManifest);
        }
        // A8. run_ref is AttemptRef, never FinalizationId or record identity.
        let mut keys = BTreeSet::new();
        for m in members {
            for (source, _) in &m.sources {
                if !keys.insert((
                    kind_key(m.scope.environment_kind()),
                    m.scope.environment_ref().as_str(),
                    source.as_str(),
                    m.attempt.as_str(),
                )) {
                    return Err(AdaptCode::RunKeyCollision);
                }
            }
        }
        // A8b / ABF108D D2: both initial and performed-second listings bind
        // namespaces, independently of run_ref. Repetition within one member
        // is not cross-scope reuse. A8 and A9 keep their original populations.
        let mut sources = BTreeSet::new();
        for m in members {
            let additional = match &m.second {
                SecondEnumeration::Performed { new_sources, .. } => new_sources.as_slice(),
                _ => &[],
            };
            let listed: BTreeSet<_> = m
                .sources
                .iter()
                .map(|(s, _)| s.as_str())
                .chain(additional.iter().map(|s| s.as_str()))
                .collect();
            for source in listed {
                if !sources.insert((
                    kind_key(m.scope.environment_kind()),
                    m.scope.environment_ref().as_str(),
                    source,
                )) {
                    return Err(AdaptCode::SourceRefNamespaceConflict);
                }
            }
        }
        // A9: independent checked sums, no truncation or partial accepted value.
        let records = members
            .iter()
            .try_fold(0usize, |n, m| n.checked_add(m.records.len()));
        if records.is_none_or(|n| n > 128) {
            return Err(AdaptCode::OccurrenceCeiling);
        }
        let declarations = members
            .iter()
            .try_fold(0usize, |n, m| n.checked_add(m.sources.len()));
        if declarations.is_none_or(|n| n > 128) {
            return Err(AdaptCode::DeclarationCeiling);
        }
        // A10: only construct an accepted membership after every check succeeds.
        let mut members = members.to_vec();
        members.sort_by_key(|m| scope_key(&m.scope));
        Ok(Self {
            selection,
            selected_refs,
            members,
        })
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    pub fn selected_refs(&self) -> &SelectedRefs {
        &self.selected_refs
    }
    pub fn members(&self) -> &[&'a FinalizedMember] {
        &self.members
    }
}

#[cfg(test)]
mod tests;
