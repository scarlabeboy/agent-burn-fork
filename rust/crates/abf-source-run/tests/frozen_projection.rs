//! Reuse the unchanged G4B-1 fixture executions, including their literal snapshot
//! assertions and exhaustive arrival permutations. Every successful finalization
//! also crosses the real admission/adapter boundary here. This is supplementary
//! differential/conservation evidence; projection.rs owns independent G3B literals.
mod frozen {
    include!("core.rs");

    // A local function takes precedence over the fixture's glob import. It calls
    // the original finalizer once and never changes the input or returned value.
    fn finalize(
        raw: RawExecutionLog,
        catalog: &CatalogSnapshot,
    ) -> Result<FinalizedExecution, FinalizeCode> {
        let execution = abf_source_run::finalize(raw, catalog)?;
        let scope = &execution.manifest().members[0];
        let (selection, selected_refs) = match scope.environment_kind() {
            EnvironmentKind::Windows => (
                Selection::WindowsOnly,
                SelectedRefs {
                    windows: Some(scope.environment_ref().clone()),
                    selected_wsl: None,
                },
            ),
            EnvironmentKind::SelectedWsl => (
                Selection::SelectedWslOnly,
                SelectedRefs {
                    windows: None,
                    selected_wsl: Some(scope.environment_ref().clone()),
                },
            ),
        };
        let members: Vec<_> = execution.members().iter().collect();
        let retained: usize = members.iter().map(|m| m.records().len()).sum();
        let declarations: usize = members.iter().map(|m| m.sources().len()).sum();
        let admission = ProjectionMembership::validate(selection, selected_refs, &members, catalog);
        if retained > 128 {
            assert_eq!(admission.unwrap_err(), AdaptCode::OccurrenceCeiling);
            return Ok(execution);
        }
        if declarations > 128 {
            assert_eq!(admission.unwrap_err(), AdaptCode::DeclarationCeiling);
            return Ok(execution);
        }
        let membership = admission.unwrap();
        let adapted = project_with_companion(&membership);
        if let Err(AdaptCode::Observation(code)) = &adapted {
            // The accepted G4B-1 public-struct budget witness bounds aggregate
            // retained bytes only. G2A independently refuses an overlong string;
            // projection is fallible, never a substituted rejection or number.
            assert_eq!(*code, abf_observation::AdapterCode::ParserStringBudget);
            assert!(members.iter().flat_map(|m| m.records()).any(|r| {
                let context = abf_observation::DeclaredContext::new(
                    scope.environment_kind(),
                    scope.environment_ref().clone(),
                    OpaqueRef::new(r.source().as_str()).unwrap(),
                    OpaqueRef::new(execution.manifest().attempt.as_str()).unwrap(),
                    None,
                    None,
                );
                matches!(
                    abf_observation::from_opencode(
                        r.parse_result().clone(),
                        context,
                        SourceRunStatus::Complete
                    ),
                    Err(abf_observation::AdapterCode::ParserStringBudget)
                )
            }));
            return Ok(execution);
        }
        let paired = adapted.unwrap();
        let p = paired.projection();
        assert_eq!(p.computer.stored_occurrences.0 as usize, retained);
        let environment = match selection {
            Selection::WindowsOnly => p.windows.selected().unwrap(),
            Selection::SelectedWslOnly => p.selected_wsl.selected().unwrap(),
            Selection::Both => unreachable!(),
        };
        assert_eq!(environment.runs.run_keys as usize, declarations);
        assert_eq!(environment.runs.undeclared_record_keys, 0);
        assert_eq!(environment.runs.conflicting_keys, 0);
        assert_eq!(
            environment.runs.inventory,
            abf_counting::InventoryBasis::Declared
        );

        // Primary literal classes shared by the frozen row families. These are
        // fixed expected numbers, not a recomputation of a sum or the adapter's
        // own generated inputs. Independent scenario-specific literals live in
        // projection.rs. Empty inventories always have the exact nonnumeric class.
        use abf_counting::{ComponentValue as V, CoverageState as C, IndeterminateReason as I};
        if declarations == 0 {
            assert_eq!(environment.runs.coverage, C::NoEvidence);
            assert_eq!(
                environment.components.input.value,
                V::Indeterminate(I::NoRunEvidence)
            );
            assert!(
                environment
                    .components
                    .input
                    .evidence
                    .qualifications
                    .is_empty()
            );
        }
        let mut input_claims: Vec<_> = members
            .iter()
            .flat_map(|m| m.records())
            .filter_map(|r| {
                let o = r.parse_result().observation()?;
                match o.input {
                    abf_parser::Field::Valid(v) => Some(v),
                    _ => None,
                }
            })
            .collect();
        input_claims.sort_unstable();
        let literal = match input_claims.as_slice() {
            [0] | [0, 0] => Some(0),
            [7] => Some(7),
            [10] => Some(10),
            [40] => Some(40),
            [50] => Some(50),
            [100] => Some(100),
            [10, 20] => Some(30),
            [40, 100] => Some(140),
            [50, 100] => Some(150),
            [100, 100] => Some(200),
            [90, 100] => Some(190),
            [] => None,
            _ => panic!("frozen fixture needs an independently specified literal class"),
        };
        if let Some(value) = literal {
            assert_eq!(
                environment.components.input.value,
                V::ReportedClaimSum(value)
            );
        }

        // Independent construction from the read-only evidence API. No adapter
        // input builder, internal context/declaration, or local handle is reused.
        let mut set =
            abf_identity_relations::SyntheticSet::new(abf_identity_relations::Limits::MAX).unwrap();
        let mut reported = Vec::new();
        for member in &members {
            let scope = member.scope();
            for (source, reconciliation) in member.sources() {
                reported.push(abf_counting::RunDeclaration {
                    environment_kind: scope.environment_kind(),
                    environment_ref: scope.environment_ref().clone(),
                    source_ref: OpaqueRef::new(source.as_str()).unwrap(),
                    run_ref: OpaqueRef::new(member.identity().0.as_str()).unwrap(),
                    status: reconciliation.status(),
                });
            }
            for record in member.records() {
                let status = member
                    .sources()
                    .iter()
                    .find(|(s, _)| s == record.source())
                    .unwrap()
                    .1
                    .status();
                let context = abf_observation::DeclaredContext::new(
                    scope.environment_kind(),
                    scope.environment_ref().clone(),
                    OpaqueRef::new(record.source().as_str()).unwrap(),
                    OpaqueRef::new(member.identity().0.as_str()).unwrap(),
                    None,
                    None,
                );
                let envelope =
                    abf_observation::from_opencode(record.parse_result().clone(), context, status)
                        .unwrap();
                assert_eq!(
                    envelope.inconsistency().is_some(),
                    matches!(
                        status,
                        SourceRunStatus::NotCollected | SourceRunStatus::Unavailable
                    )
                );
                let handle =
                    OpaqueRef::new(&format!("witness{}", set.occurrences().len())).unwrap();
                set.insert(abf_identity_relations::Occurrence::new(handle, envelope))
                    .unwrap();
            }
        }
        assert_eq!(
            *p,
            abf_counting::project(
                &set,
                abf_counting::RunInventory::Declared(&reported),
                selection
            )
            .unwrap()
        );
        Ok(execution)
    }
}
