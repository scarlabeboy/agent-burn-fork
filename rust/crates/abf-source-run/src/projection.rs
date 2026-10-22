use crate::{AdaptCode, FinalizedMember, OpaqueRef, ProjectionMembership, SelectedRefs, Selection};
use abf_counting::{Projection, RunDeclaration, RunInventory};
use abf_identity_relations::{Limits, Occurrence, SyntheticSet};
use abf_observation::{DeclaredContext, from_opencode};

/// Verbatim numeric projection paired with the admitted immutable companion evidence.
/// Only validated membership can construct this value. References remain synthetic
/// labels, with no cross-membership or physical-event identity guarantee.
/// ```compile_fail,E0451
/// use abf_source_run::{PairedProjection, Selection, SelectedRefs};
/// fn forge(projection: abf_counting::Projection, selected_refs: SelectedRefs) {
///     let _ = PairedProjection { projection, members: vec![],
///         selection: Selection::WindowsOnly, selected_refs };
/// }
/// ```
/// ```compile_fail,E0277
/// use abf_source_run::PairedProjection;
/// let _: PairedProjection<'static> = Default::default();
/// ```
/// ```compile_fail,E0308
/// use abf_source_run::{CoverageVerdict, project_with_companion};
/// let _ = project_with_companion(&CoverageVerdict::CoveredWithinEnumeration { expected: 1 });
/// ```
/// ```compile_fail,E0599
/// use abf_source_run::PairedProjection;
/// let _ = PairedProjection::new();
/// ```
/// ```compile_fail,E0277
/// use abf_source_run::PairedProjection;
/// fn forge(raw: abf_counting::Projection) {
///     let _: PairedProjection<'static> = raw.into();
/// }
/// ```
/// ```compile_fail,E0616
/// use abf_source_run::PairedProjection;
/// fn replace(p: &mut PairedProjection<'_>, raw: abf_counting::Projection) {
///     p.projection = raw;
/// }
/// ```
/// ```compile_fail,E0594
/// use abf_source_run::PairedProjection;
/// fn mutate(p: &PairedProjection<'_>) {
///     p.projection().computer.stored_occurrences.0 = 999;
/// }
/// ```
pub struct PairedProjection<'a> {
    projection: Projection,
    members: Vec<&'a FinalizedMember>,
    selection: Selection,
    selected_refs: SelectedRefs,
}
redacted!(PairedProjection<'_>);
impl<'a> PairedProjection<'a> {
    pub fn projection(&self) -> &Projection {
        &self.projection
    }
    pub fn members(&self) -> impl Iterator<Item = &'a FinalizedMember> + '_ {
        self.members.iter().copied()
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    /// One immutable assessment per admitted member, in `members()` order. One
    /// diagnostics snapshot is required per represented execution; it can serve
    /// several members of that execution. Missing, duplicate or unrelated
    /// snapshots fail closed. There is no environment or Computer verdict roll-up.
    /// ```compile_fail,E0308
    /// use abf_source_run::{PairedProjection, CurrentAssessment};
    /// fn forge(p: &PairedProjection<'_>, view: CurrentAssessment) {
    ///     let _ = p.assess(&[view]);
    /// }
    /// ```
    pub fn assess(
        &self,
        diags: &[crate::DiagnosticsSnapshot],
    ) -> Result<PairedAssessment, crate::AssessCode> {
        let mut assessments = Vec::new();
        let mut used = vec![false; diags.len()];
        for member in &self.members {
            let mut matching = diags
                .iter()
                .enumerate()
                .filter_map(|(i, d)| crate::assess(member, d).ok().map(|a| (i, a)));
            let (i, assessment) = matching.next().ok_or(crate::AssessCode::IdentityMismatch)?;
            if matching.next().is_some() {
                return Err(crate::AssessCode::IdentityMismatch);
            }
            used[i] = true;
            assessments.push(assessment);
        }
        if used.iter().any(|u| !u) {
            return Err(crate::AssessCode::IdentityMismatch);
        }
        Ok(PairedAssessment {
            members: assessments,
        })
    }
    pub fn selected_refs(&self) -> &SelectedRefs {
        &self.selected_refs
    }
}

// Only ProjectionMembership can enter. Each expected source yields one declared
// key, including zero-record/untraversed sources. Second discoveries are absent.
fn inputs(
    membership: &ProjectionMembership<'_>,
) -> Result<(SyntheticSet, Vec<RunDeclaration>), AdaptCode> {
    let mut set = SyntheticSet::new(Limits::MAX).map_err(AdaptCode::Occurrence)?;
    let mut declarations = Vec::new();
    for member in membership.members() {
        let scope = member.scope();
        let run_ref = OpaqueRef::new(member.identity().0.as_str()).expect("validated AttemptRef");
        let mut sources: Vec<_> = member.sources().iter().collect();
        sources.sort_by_key(|(source, _)| source.as_str());
        for (source, reconciliation) in sources {
            declarations.push(RunDeclaration {
                environment_kind: scope.environment_kind(),
                environment_ref: scope.environment_ref().clone(),
                source_ref: OpaqueRef::new(source.as_str()).expect("validated SourceRef"),
                run_ref: run_ref.clone(),
                // §7.2.1 already reconciled the full set of terminal claims.
                status: reconciliation.status(),
            });
        }
        let mut records: Vec<_> = member.records().iter().collect();
        records.sort_by_key(|r| (r.source().as_str(), r.source_seq(), r.frame_seq()));
        for record in records {
            let declaration = declarations
                .iter()
                .rev()
                .find(|d| {
                    d.source_ref.as_str() == record.source().as_str()
                        && d.run_ref == run_ref
                        && d.environment_kind == scope.environment_kind()
                        && &d.environment_ref == scope.environment_ref()
                })
                .expect("finalization binds every retained record to an expected source");
            let context = DeclaredContext::new(
                declaration.environment_kind,
                declaration.environment_ref.clone(),
                declaration.source_ref.clone(),
                declaration.run_ref.clone(),
                None,
                None,
            );
            #[cfg(test)]
            CALLS.with(|v| {
                let (e, o, p) = v.get();
                v.set((e + 1, o, p));
            });
            let envelope =
                from_opencode(record.parse_result().clone(), context, declaration.status)
                    .map_err(AdaptCode::Observation)?;
            // Canonical local storage handle only, independent of claims or event identity.
            let handle = OpaqueRef::new(&format!("o{}", set.occurrences().len()))
                .expect("bounded local handle");
            #[cfg(test)]
            CALLS.with(|v| {
                let (e, o, p) = v.get();
                v.set((e, o + 1, p));
            });
            set.insert(Occurrence::new(handle, envelope))
                .map_err(AdaptCode::Occurrence)?;
        }
    }
    Ok((set, declarations))
}

/// Adapt finalized admitted deliveries and declared source inventory, then call
/// the unchanged G3B projector exactly once. Admission refusal cannot reach here.
pub fn project_with_companion<'a>(
    membership: &ProjectionMembership<'a>,
) -> Result<PairedProjection<'a>, AdaptCode> {
    let (set, declarations) = inputs(membership)?;
    #[cfg(test)]
    CALLS.with(|v| {
        let (e, o, p) = v.get();
        v.set((e, o, p + 1));
    });
    let projection = abf_counting::project(
        &set,
        RunInventory::Declared(&declarations),
        membership.selection(),
    )
    .map_err(AdaptCode::Projection)?;
    Ok(PairedProjection {
        projection,
        members: membership.members().to_vec(),
        selection: membership.selection(),
        selected_refs: membership.selected_refs().clone(),
    })
}

/// Read-only assessment views aligned with the paired member list. This type is
/// never accepted as evidence and cannot change any finalized or numeric result.
pub struct PairedAssessment {
    members: Vec<crate::CurrentAssessment>,
}
impl PairedAssessment {
    pub fn members(&self) -> &[crate::CurrentAssessment] {
        &self.members
    }
}
redacted!(PairedAssessment);

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;

#[cfg(test)]
std::thread_local! {
    // Test-only invocation counts: envelope constructions, occurrence insertions,
    // G3B calls. No public API and no production storage or side effects.
    static CALLS: std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) };
}
