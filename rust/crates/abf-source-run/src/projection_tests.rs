use super::*;
use crate::*;

fn fixture(records: usize, sources: usize) -> (CatalogSnapshot, FinalizedExecution) {
    let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v1").unwrap();
    let mut b = CatalogBuilder::new();
    b.add(scope.clone(), "tag").unwrap();
    let c = b.build();
    let attempt = AttemptRef::new("att1").unwrap();
    let mut payloads = vec![Payload::Enumeration {
        member: 0,
        second: false,
        outcome: RawEnumeration::Closed(sources as u64),
        sources: (1..=sources).map(|n| format!("tag-i{n}")).collect(),
        observed_at: None,
    }];
    payloads.extend((1..=records).map(|seq| {
        let bytes = br#"{"id":"fictional_secret","tokens":{"input":1}}"#.to_vec();
        Payload::Source {
            source: "tag-i1".into(),
            source_seq: seq as u64,
            event: SourceEvent::Record {
                result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
                bytes,
            },
        }
    }));
    let pre_barrier = payloads
        .into_iter()
        .enumerate()
        .map(|(i, payload)| ReaderFrame {
            attempt: attempt.clone(),
            frame_seq: i as u64 + 1,
            payload,
        })
        .collect();
    let x = finalize(
        RawExecutionLog {
            manifest: ExecutionManifest {
                attempt,
                finalization: FinalizationId::new("z1").unwrap(),
                members: vec![scope],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier,
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(0, 0),
        },
        &c,
    )
    .unwrap();
    (c, x)
}
fn admit<'a>(
    members: &[&'a FinalizedMember],
    c: &CatalogSnapshot,
) -> Result<ProjectionMembership<'a>, AdaptCode> {
    ProjectionMembership::validate(
        Selection::WindowsOnly,
        SelectedRefs {
            windows: Some(OpaqueRef::new("w1").unwrap()),
            selected_wsl: None,
        },
        members,
        c,
    )
}

#[test]
fn exact_contexts_declarations_envelopes_occurrences_and_private_pair_boundary() {
    let (c, x) = fixture(2, 2);
    let m = admit(&[&x.members()[0]], &c).unwrap();
    CALLS.with(|v| v.set((0, 0, 0)));
    let p = project_with_companion(&m).unwrap();
    assert_eq!(CALLS.with(|v| v.get()), (2, 2, 1));
    assert_eq!(p.members().count(), 1);
    assert!(std::ptr::eq(p.members().next().unwrap(), &x.members()[0]));
    let (set, declarations) = inputs(&m).unwrap();
    assert_eq!(set.occurrences().len(), 2);
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].source_ref.as_str(), "tag-i1");
    assert_eq!(declarations[1].source_ref.as_str(), "tag-i2");
    assert_eq!(declarations[0].status, SourceRunStatus::Failed);
    assert_eq!(declarations[1].status, SourceRunStatus::NotCollected);
    for o in set.occurrences() {
        let e = o.envelope();
        let context = e.context();
        assert_eq!(context.environment_kind(), EnvironmentKind::Windows);
        assert_eq!(context.environment_ref().as_str(), "w1");
        assert_eq!(context.source_ref().as_str(), "tag-i1");
        assert_eq!(context.run_ref().as_str(), "att1");
        assert_eq!(e.source_status(), SourceRunStatus::Failed);
        assert_eq!(e.inconsistency(), None);
        assert_eq!(
            e.parser_result(),
            x.members()[0].records()[0].parse_result()
        );
    }
    assert!(set.relations().is_empty());
    assert_eq!(format!("{p:?}"), "PairedProjection<'_>([redacted])");
    let assessed = p
        .assess(&[PostFinalizationLog::bind(&x).snapshot()])
        .unwrap();
    assert_eq!(format!("{assessed:?}"), "PairedAssessment([redacted])");
}

#[test]
fn refusal_constructs_no_envelope_occurrence_or_g3b_projection() {
    let (c, x) = fixture(129, 1);
    CALLS.with(|v| v.set((0, 0, 0)));
    assert_eq!(
        admit(&[&x.members()[0]], &c).unwrap_err(),
        AdaptCode::OccurrenceCeiling
    );
    assert_eq!(CALLS.with(|v| v.get()), (0, 0, 0));
    assert_eq!(x.members()[0].records().len(), 129);
    let (c, x) = fixture(1, 1);
    CALLS.with(|v| v.set((0, 0, 0)));
    assert_eq!(
        admit(&[&x.members()[0], &x.members()[0]], &c).unwrap_err(),
        AdaptCode::DuplicateScopeMember
    );
    assert_eq!(CALLS.with(|v| v.get()), (0, 0, 0));
    assert_eq!(x.members()[0].records().len(), 1);
}

#[test]
fn inherited_public_parser_string_budget_refusal_produces_no_occurrence_or_g3b_result() {
    let (c, x) = fixture(1, 1);
    // Match G4B-1's accepted manually constructed public parser-result witness.
    // This internal injection is test-only; the external wrapper also exercises
    // the unchanged real finalize() fixture at 4095/4096 retained bytes.
    let mut member = x.members()[0].clone();
    let abf_parser::ParseResult::Observation(o) = &mut member.records[0].result else {
        unreachable!()
    };
    o.message_id =
        abf_parser::Field::Valid("x".repeat(abf_parser::Limits::MAX.string_bytes + 1).into());
    let m = admit(&[&member], &c).unwrap();
    CALLS.with(|v| v.set((0, 0, 0)));
    assert_eq!(
        project_with_companion(&m).unwrap_err(),
        AdaptCode::Observation(abf_observation::AdapterCode::ParserStringBudget)
    );
    assert_eq!(CALLS.with(|v| v.get()), (1, 0, 0));
    assert_eq!(member.records().len(), 1);
}
