use abf_windows_confinement_fixture::*;

fn binding() -> Binding {
    Binding {
        environment: Environment::Windows,
        descriptor: 7,
        revision: 2,
        scope: 3,
        attempt: 4,
        generation: 5,
    }
}
fn empty() -> (FakeWorld, Selection) {
    let w = FakeWorld::new();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    (w, s)
}
#[test]
fn missing_authority_is_not_empty() {
    let (w, _) = empty();
    let d = Driver::new(w, None, binding(), Limits::default(), true, false);
    assert_eq!(d.evidence().code(), Code::Unauthorized);
    assert_eq!(d.evidence().ledger().get(Resource::Operations), 0);
}
fn driver(w: FakeWorld, s: &Selection) -> Driver {
    Driver::new(w, Some(s), binding(), Limits::default(), true, false)
}
fn finish(d: &mut Driver) {
    for _ in 0..32 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
}
#[test]
fn clean_empty_has_independent_ledger() {
    let (w, s) = empty();
    let mut d = driver(w, &s);
    finish(&mut d);
    let e = d.evidence();
    assert_eq!(e.code(), Code::CompleteEmpty);
    assert_eq!(
        e.ledger().counts(),
        &[2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 3]
    );
    assert_eq!((e.opened(), e.closed(), e.live()), (1, 1, 0));
}
#[test]
fn no_work_never_claims_enumeration() {
    let (w, s) = empty();
    let d = Driver::new(w, Some(&s), binding(), Limits::default(), false, false);
    assert_eq!(d.evidence().code(), Code::NoWork);
    assert_eq!(d.evidence().ledger().counts(), &[0; 13]);
}
#[test]
fn every_binding_dimension_and_world_allocation_is_checked() {
    let (w, s) = empty();
    let b = binding();
    let variants = [
        Binding {
            environment: Environment::Wsl,
            ..b
        },
        Binding { descriptor: 8, ..b },
        Binding { revision: 3, ..b },
        Binding { scope: 4, ..b },
        Binding { attempt: 5, ..b },
        Binding { generation: 6, ..b },
    ];
    for b in variants {
        assert_eq!(
            Driver::new(w.clone(), Some(&s), b, Limits::default(), true, false)
                .evidence()
                .code(),
            Code::BindingMismatch
        );
    }
    assert_eq!(
        driver(FakeWorld::new(), &s).evidence().code(),
        Code::BindingMismatch
    );
    assert_eq!(
        FictionalAuthority::explicit_approval(
            &w,
            Binding {
                environment: Environment::Wsl,
                ..b
            }
        )
        .unwrap_err(),
        Code::Unauthorized
    );
}
#[test]
fn root_refusals_have_zero_operations() {
    for (fault, code) in [
        (Fault::Missing, Code::Missing),
        (Fault::Denied, Code::Denied),
        (Fault::Unsupported, Code::Unsupported),
        (Fault::Ambiguous, Code::Ambiguous),
        (Fault::Moved, Code::BindingMismatch),
        (Fault::Alias, Code::Alias),
        (Fault::Reparse, Code::Reparse),
        (Fault::MultiLink, Code::MultiLink),
        (Fault::UnknownLinks, Code::MultiLink),
        (Fault::WrongVolume, Code::WrongVolume),
    ] {
        let (mut w, s) = empty();
        w.fault(w.root(), fault).unwrap();
        let d = driver(w, &s);
        assert_eq!(d.evidence().code(), code);
        assert_eq!(d.evidence().ledger().get(Resource::Operations), 0);
    }
}
fn file_world() -> (FakeWorld, Selection, Object) {
    let mut w = FakeWorld::new();
    let f = w.file(w.root(), b"SECRET").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    (w, s, f)
}
#[test]
fn full_file_replay_and_exact_ledger() {
    let (w, s, _) = file_world();
    let mut a = driver(w.clone(), &s);
    let mut b = driver(w, &s);
    finish(&mut a);
    finish(&mut b);
    assert_eq!(a.evidence(), b.evidence());
    let e = a.evidence();
    assert_eq!(e.code(), Code::Complete);
    assert_eq!(e.bytes(), b"SECRET");
    assert_eq!(
        e.ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 6, 6, 2, 9]
    );
    assert_eq!((e.opened(), e.closed(), e.live()), (2, 2, 0));
}
#[test]
fn candidate_faults_preserve_refusal_and_never_touch_sentinel() {
    for (fault, code) in [
        (Fault::Moved, Code::BindingMismatch),
        (Fault::Alias, Code::Alias),
        (Fault::Reparse, Code::Reparse),
        (Fault::MultiLink, Code::MultiLink),
        (Fault::WrongVolume, Code::WrongVolume),
    ] {
        let (w, s, f) = file_world();
        let mut d = driver(w, &s);
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
        d.step(Step::Fault(f, fault));
        d.step(Step::Issue);
        assert_eq!(d.evidence().code(), code);
        assert_eq!(d.evidence().sentinel_accesses(), 0);
        assert_eq!(d.evidence().ledger().get(Resource::Operations), 1);
        assert_eq!(d.evidence().live(), 0);
    }
}
#[test]
fn replaced_root_and_substituted_candidate_cannot_adopt_outside_object() {
    let (w, s, _) = file_world();
    let root = w.root();
    let outside = w.sentinel();
    let mut d = driver(w, &s);
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    d.step(Step::Release);
    d.step(Step::Substitute(root, 0, outside));
    d.step(Step::Issue);
    assert_eq!(d.evidence().code(), Code::BindingMismatch);
    assert_eq!(d.evidence().sentinel_accesses(), 0);
    let (mut w, s) = empty();
    w.replace_root_with_sentinel();
    assert_eq!(driver(w, &s).evidence().code(), Code::BindingMismatch);
}
#[test]
fn moved_ancestor_and_return_time_swaps_are_refused() {
    let mut w = FakeWorld::new();
    let dir = w.directory(w.root()).unwrap();
    w.file(dir, b"X").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    d.step(Step::Release);
    d.step(Step::Issue);
    d.step(Step::Fault(dir, Fault::Moved));
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().code(), Code::BindingMismatch);
    assert_eq!(d.evidence().live(), 0);
    assert_eq!(d.evidence().sentinel_accesses(), 0);
}
#[test]
fn io_failures_and_partial_prefix_never_become_empty() {
    for (op_index, completion, expected) in [
        (0, Completion::Denied, Code::Denied),
        (0, Completion::Failed, Code::ListFailed),
        (1, Completion::Failed, Code::OpenFailed),
        (2, Completion::Failed, Code::ReadFailed),
    ] {
        let (w, s, _) = file_world();
        let mut d = driver(w, &s);
        for _ in 0..op_index {
            d.step(Step::Issue);
            d.step(Step::Return(Completion::Success));
            d.step(Step::Release);
        }
        d.step(Step::Issue);
        d.step(Step::Return(completion));
        assert_eq!(d.evidence().code(), expected);
        assert_eq!(d.evidence().live(), 0);
    }
    for prefix in [0, 1] {
        let (w, s, _) = file_world();
        let mut d = driver(w, &s);
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Partial(prefix)));
        d.step(Step::Release);
        assert_eq!(d.evidence().code(), Code::ListFailed);
        assert_eq!(d.evidence().admitted(), prefix as u64);
        assert_eq!(d.evidence().bytes(), b"");
    }
}
#[test]
fn rejection_after_admitted_prefix_is_explicit() {
    let mut w = FakeWorld::new();
    w.file(w.root(), b"X").unwrap();
    let bad = w.file(w.root(), b"Y").unwrap();
    w.fault(bad, Fault::Alias).unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    finish(&mut d);
    assert_eq!(d.evidence().code(), Code::Alias);
    assert_eq!(d.evidence().admitted(), 1);
    assert_eq!(d.evidence().ledger().get(Resource::Examined), 2);
    assert_eq!(d.evidence().ledger().get(Resource::Rejected), 1);
}
#[test]
fn cancellation_before_issue_and_deadline_are_not_empty() {
    for stop in [Step::Cancel, Step::Time(10)] {
        let (w, s) = empty();
        let mut d = driver(w, &s);
        d.step(stop);
        assert_eq!(d.evidence().cancellation(), Cancellation::Requested);
        d.step(Step::Issue);
        assert_eq!(d.evidence().code(), Code::Cancelled);
        assert_eq!(d.evidence().cancellation(), Cancellation::Acknowledged);
        assert_eq!(d.evidence().ledger().get(Resource::Operations), 0);
        assert_eq!(d.evidence().live(), 0);
    }
}
#[test]
fn pending_list_open_read_cancel_waits_for_return_and_cleans_once() {
    for op_index in 0..3 {
        let (w, s, _) = file_world();
        let mut d = driver(w, &s);
        for _ in 0..op_index {
            d.step(Step::Issue);
            d.step(Step::Return(Completion::Success));
            d.step(Step::Release);
        }
        d.step(Step::Issue);
        d.step(Step::Cancel);
        assert_eq!(d.evidence().cancellation(), Cancellation::Requested);
        d.step(Step::ObserveStop);
        assert_eq!(d.evidence().code(), Code::CancelPending);
        assert_eq!(d.evidence().cancellation(), Cancellation::Pending);
        assert!(d.evidence().live() > 0);
        d.step(Step::Issue);
        d.step(Step::Release);
        d.step(Step::Return(Completion::Success));
        assert_eq!(d.evidence().cancellation(), Cancellation::Acknowledged);
        assert_eq!(d.evidence().code(), Code::Cancelled);
        assert_eq!(d.evidence().bytes(), b"");
        assert_eq!(d.evidence().live(), 0);
        assert_eq!(d.evidence().opened(), d.evidence().closed());
        let frozen = d.evidence();
        finish(&mut d);
        assert_eq!(d.evidence(), frozen);
    }
}
#[test]
fn cancellation_after_return_prevents_release() {
    let (w, s, _) = file_world();
    let mut d = driver(w, &s);
    for _ in 0..2 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().ledger().get(Resource::Returned), 6);
    d.step(Step::Cancel);
    d.step(Step::Release);
    assert_eq!(d.evidence().bytes(), b"");
    assert_eq!(d.evidence().code(), Code::Cancelled);
    assert_eq!(d.evidence().live(), 0);
}
#[test]
fn allocation_refusal_and_graph_budget_are_preflight() {
    let (w, s) = empty();
    assert_eq!(
        Driver::new(
            w.clone(),
            Some(&s),
            binding(),
            Limits::default(),
            true,
            true
        )
        .evidence()
        .code(),
        Code::Allocation
    );
    let limits = Limits::default().with(Resource::Graph, 0).unwrap();
    let e = Driver::new(w, Some(&s), binding(), limits, true, false).evidence();
    assert_eq!(e.code(), Code::Budget);
    assert_eq!(e.ledger().get(Resource::Operations), 0);
}
#[test]
fn graph_storage_and_file_payload_have_hard_bounds() {
    let mut w = FakeWorld::new();
    let r = w.root();
    assert_eq!(w.file(r, b"123456789"), Err(Code::Budget));
    for _ in 0..4 {
        w.file(r, b"").unwrap();
    }
    assert_eq!(w.file(r, b""), Err(Code::Budget));
    let d = w.directory(w.sentinel()).unwrap();
    w.file(d, b"").unwrap();
    assert_eq!(w.file(d, b""), Err(Code::Budget));
}
#[test]
fn runtime_limits_refuse_before_excess_work() {
    for resource in [
        Resource::Depth,
        Resource::Examined,
        Resource::Sources,
        Resource::Requested,
        Resource::Returned,
        Resource::Evidence,
    ] {
        let (w, s, _) = file_world();
        let mut d = Driver::new(
            w,
            Some(&s),
            binding(),
            Limits::default().with(resource, 0).unwrap(),
            true,
            false,
        );
        finish(&mut d);
        assert_eq!(d.evidence().code(), Code::Budget, "{resource:?}");
        assert_eq!(d.evidence().ledger().get(resource), 0);
        assert_eq!(d.evidence().live(), 0);
    }
}
#[test]
fn privacy_is_redacted_even_with_sensitive_payload() {
    let (w, s, _) = file_world();
    let before = format!("{w:?}{s:?}");
    let mut d = driver(w, &s);
    finish(&mut d);
    let text = format!(
        "{before}{d:?}{:?}{:?}{}",
        d.evidence(),
        Code::ReadFailed,
        Code::ReadFailed
    );
    assert!(!text.contains("SECRET"));
    assert!(!text.contains("descriptor: 7"));
    assert_eq!(d.evidence().bytes(), b"SECRET");
}
#[test]
fn foreign_object_tokens_are_not_local_indices() {
    let other = FakeWorld::new();
    let mut w = FakeWorld::new();
    assert_eq!(w.file(other.root(), b"X"), Err(Code::BindingMismatch));
    assert_eq!(
        w.fault(other.root(), Fault::Alias),
        Err(Code::BindingMismatch)
    );
}
#[test]
fn byte_request_is_not_recorded_when_operation_budget_prevents_issue() {
    let (w, s, _) = file_world();
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Operations, 2).unwrap(),
        true,
        false,
    );
    for _ in 0..2 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    d.step(Step::Issue);
    assert_eq!(d.evidence().code(), Code::Budget);
    assert_eq!(d.evidence().ledger().get(Resource::Requested), 0);
    assert_eq!(
        d.evidence().operations(),
        &[Some(Operation::List), Some(Operation::Open)]
    );
    assert_eq!(d.evidence().live(), 0);
}
#[test]
fn retries_have_no_security_fallback_and_are_bounded() {
    let (w, s) = empty();
    let mut d = driver(w, &s);
    for _ in 0..2 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Transient));
        d.step(Step::Retry);
    }
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Transient));
    d.step(Step::Retry);
    assert_eq!(d.evidence().code(), Code::Budget);
    assert_eq!(d.evidence().ledger().get(Resource::Retries), 2);
    assert_eq!(d.evidence().ledger().get(Resource::Operations), 3);
    assert_eq!(d.evidence().live(), 0);
}
#[test]
fn pending_protocol_failure_retains_handles_until_completion() {
    let (w, s, _) = file_world();
    let mut d = driver(w, &s);
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    d.step(Step::Release);
    d.step(Step::Issue);
    d.step(Step::Issue);
    assert_eq!(d.evidence().code(), Code::InvalidStep);
    assert_eq!(d.evidence().live(), 1);
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().code(), Code::InvalidStep);
    assert_eq!(
        (
            d.evidence().opened(),
            d.evidence().closed(),
            d.evidence().live()
        ),
        (2, 2, 0)
    );
}
#[test]
fn handle_and_step_limits_preserve_cleanup_reserve() {
    let (w, s, _) = file_world();
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Handles, 1).unwrap(),
        true,
        false,
    );
    finish(&mut d);
    assert_eq!(d.evidence().code(), Code::Budget);
    assert_eq!((d.evidence().opened(), d.evidence().closed()), (1, 1));
    let (w, s) = empty();
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Steps, 1).unwrap(),
        true,
        false,
    );
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().code(), Code::Budget);
    assert_eq!(d.evidence().live(), 0);
}
#[test]
fn post_return_replacement_and_pre_release_alias_block_bytes() {
    let (w, s, f) = file_world();
    for (fault, expected) in [
        (Step::ReplaceRoot, Code::BindingMismatch),
        (Step::Time(10), Code::Cancelled),
        (Step::Fault(f, Fault::Alias), Code::Alias),
    ] {
        let mut d = driver(w.clone(), &s);
        for _ in 0..2 {
            d.step(Step::Issue);
            d.step(Step::Return(Completion::Success));
            d.step(Step::Release);
        }
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(fault);
        d.step(Step::Release);
        assert_eq!(d.evidence().code(), expected);
        assert_eq!(d.evidence().bytes(), b"");
        assert_eq!(d.evidence().live(), 0);
    }
}
#[test]
fn failure_retains_already_released_prefix() {
    let mut w = FakeWorld::new();
    w.file(w.root(), b"A").unwrap();
    w.file(w.root(), b"B").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    for _ in 0..3 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Failed));
    assert_eq!(d.evidence().code(), Code::OpenFailed);
    assert_eq!(d.evidence().bytes(), b"A");
    assert_eq!((d.evidence().opened(), d.evidence().closed()), (2, 2));
}
#[test]
fn structural_aliases_and_cycles_are_refused_without_redirect_access() {
    let mut w = FakeWorld::new();
    let a = w.file(w.root(), b"A").unwrap();
    w.file(w.root(), b"B").unwrap();
    w.substitute(w.root(), 1, a).unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    finish(&mut d);
    assert_eq!(d.evidence().code(), Code::Alias);
    assert_eq!(d.evidence().bytes(), b"");
    assert_eq!(d.evidence().sentinel_accesses(), 0);
    let mut w = FakeWorld::new();
    let dir = w.directory(w.root()).unwrap();
    w.file(dir, b"").unwrap();
    w.substitute(dir, 0, w.root()).unwrap();
    assert_eq!(
        FictionalAuthority::explicit_approval(&w, binding()).unwrap_err(),
        Code::Alias
    );
}
#[test]
fn accepted_directory_chain_and_depth_bound_are_explicit() {
    let mut w = FakeWorld::new();
    let a = w.directory(w.root()).unwrap();
    let b = w.directory(a).unwrap();
    w.file(b, b"X").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w.clone(), &s);
    finish(&mut d);
    assert_eq!(d.evidence().code(), Code::Complete);
    assert_eq!(d.evidence().bytes(), b"X");
    assert_eq!(d.evidence().ledger().get(Resource::Depth), 3);
    assert_eq!(
        (
            d.evidence().opened(),
            d.evidence().closed(),
            d.evidence().live()
        ),
        (4, 4, 0)
    );
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Depth, 2).unwrap(),
        true,
        false,
    );
    finish(&mut d);
    assert_eq!(d.evidence().code(), Code::Budget);
    assert_eq!(d.evidence().ledger().get(Resource::Depth), 2);
}
#[test]
fn cancellation_does_not_erase_an_already_released_prefix() {
    let mut w = FakeWorld::new();
    w.file(w.root(), b"A").unwrap();
    w.file(w.root(), b"B").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    for _ in 0..3 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    let snapshot = d.evidence();
    d.step(Step::Issue);
    d.step(Step::Cancel);
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().code(), Code::Cancelled);
    assert_eq!(d.evidence().bytes(), b"A");
    assert_eq!(snapshot.bytes(), b"A");
    assert_eq!(snapshot.code(), Code::Running);
    assert_eq!(
        (
            d.evidence().opened(),
            d.evidence().closed(),
            d.evidence().live()
        ),
        (3, 3, 0)
    );
}
#[test]
fn effective_runtime_budgets_are_checked_below_at_and_above_required_work() {
    for (resource, required) in [
        (Resource::Graph, 3),
        (Resource::Edges, 1),
        (Resource::Depth, 1),
        (Resource::Examined, 1),
        (Resource::Operations, 3),
        (Resource::Sources, 1),
        (Resource::Requested, 8),
        (Resource::Returned, 8),
        (Resource::Evidence, 6),
        (Resource::Handles, 2),
        (Resource::Steps, 9),
    ] {
        for (cap, expected) in [
            (required - 1, Code::Budget),
            (required, Code::Complete),
            (required + 1, Code::Complete),
        ] {
            let (w, s, _) = file_world();
            let mut d = Driver::new(
                w,
                Some(&s),
                binding(),
                Limits::default().with(resource, cap).unwrap(),
                true,
                false,
            );
            finish(&mut d);
            assert_eq!(d.evidence().code(), expected, "{resource:?}, {cap}");
            assert!(d.evidence().ledger().get(resource) <= cap);
            assert_eq!(d.evidence().live(), 0);
        }
    }
}
#[test]
fn rejected_entry_and_retry_budget_boundaries_are_enforced_by_driver() {
    for cap in [0, 1, 2] {
        let mut w = FakeWorld::new();
        let f = w.file(w.root(), b"X").unwrap();
        w.fault(f, Fault::Alias).unwrap();
        let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
        let mut d = Driver::new(
            w,
            Some(&s),
            binding(),
            Limits::default().with(Resource::Rejected, cap).unwrap(),
            true,
            false,
        );
        finish(&mut d);
        assert_eq!(d.evidence().ledger().get(Resource::Rejected), cap.min(1));
        assert_eq!(
            d.evidence().code(),
            [Code::Budget, Code::Alias, Code::Alias][cap as usize]
        );
    }
    for cap in [0, 1, 2] {
        let (w, s) = empty();
        let mut d = Driver::new(
            w,
            Some(&s),
            binding(),
            Limits::default().with(Resource::Retries, cap).unwrap(),
            true,
            false,
        );
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Transient));
        d.step(Step::Retry);
        finish(&mut d);
        assert_eq!(d.evidence().ledger().get(Resource::Retries), cap.min(1));
        assert_eq!(
            d.evidence().code(),
            [Code::Budget, Code::CompleteEmpty, Code::CompleteEmpty][cap as usize]
        );
    }
}
#[test]
fn logical_clock_reversal_and_incorrect_protocol_never_complete() {
    let (w, s) = empty();
    let mut d = driver(w, &s);
    d.step(Step::Time(4));
    d.step(Step::Time(3));
    assert_eq!(d.evidence().code(), Code::InvalidStep);
    assert_eq!(d.evidence().live(), 0);
    let (w, s) = empty();
    let mut d = driver(w, &s);
    d.step(Step::Release);
    assert_eq!(d.evidence().code(), Code::InvalidStep);
}
#[test]
fn late_read_has_an_independent_discard_and_handle_ledger() {
    let (w, s, _) = file_world();
    let mut d = driver(w, &s);
    for _ in 0..2 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    d.step(Step::Issue);
    d.step(Step::Cancel);
    d.step(Step::ObserveStop);
    assert_eq!(
        d.evidence().ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 0, 0, 2, 7]
    );
    d.step(Step::Return(Completion::Success));
    assert_eq!(
        d.evidence().ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 6, 0, 2, 7]
    );
    assert_eq!(
        (
            d.evidence().opened(),
            d.evidence().closed(),
            d.evidence().live()
        ),
        (2, 2, 0)
    );
    assert_eq!(d.evidence().bytes(), b"");
}
#[test]
fn immutable_evidence_retains_fictional_binding() {
    let (w, s) = empty();
    let mut d = driver(w, &s);
    finish(&mut d);
    assert_eq!(d.evidence().binding(), Some(binding()));
}
#[test]
fn pending_failure_survives_cancel_and_acknowledges_quiescence() {
    let (w, s) = empty();
    let mut d = driver(w, &s);
    d.step(Step::Issue);
    d.step(Step::Issue);
    d.step(Step::Cancel);
    d.step(Step::Return(Completion::Success));
    assert_eq!(d.evidence().code(), Code::InvalidStep);
    assert_eq!(d.evidence().cancellation(), Cancellation::Acknowledged);
    assert_eq!(d.evidence().live(), 0);
}

#[test]
fn pending_invalid_step_remains_visible_during_cancellation() {
    let (w, s, _) = file_world();
    let mut d = driver(w, &s);
    d.step(Step::Issue);
    d.step(Step::Return(Completion::Success));
    d.step(Step::Release);
    d.step(Step::Issue); // Open remains pending; root handle is still owned.
    d.step(Step::Issue); // Deferred protocol failure, no second Open.
    assert_eq!(d.evidence().code(), Code::InvalidStep);
    d.step(Step::Cancel);
    assert_eq!(d.evidence().cancellation(), Cancellation::Requested);
    d.step(Step::ObserveStop);
    let pending = d.evidence();
    assert_eq!(pending.code(), Code::InvalidStep);
    assert_eq!(pending.cancellation(), Cancellation::Pending);
    assert_eq!(
        pending.ledger().counts(),
        &[3, 1, 1, 1, 0, 2, 0, 1, 0, 0, 0, 2, 5]
    );
    assert_eq!(
        (pending.opened(), pending.closed(), pending.live()),
        (1, 0, 1)
    );
    assert_eq!(pending.bytes(), b"");
    d.step(Step::Return(Completion::Success)); // Adopt and close the late handle.
    let terminal = d.evidence();
    assert_eq!(terminal.code(), Code::InvalidStep);
    assert_eq!(terminal.cancellation(), Cancellation::Acknowledged);
    assert_eq!(
        terminal.ledger().counts(),
        &[3, 1, 1, 1, 0, 2, 0, 1, 0, 0, 0, 2, 5]
    );
    assert_eq!(
        (terminal.opened(), terminal.closed(), terminal.live()),
        (2, 2, 0)
    );
    assert_eq!(terminal.bytes(), b"");
    finish(&mut d);
    assert_eq!(d.evidence(), terminal);
}

#[test]
fn pending_budget_remains_visible_during_cancellation() {
    let (w, s, _) = file_world();
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Steps, 7).unwrap(),
        true,
        false,
    );
    for _ in 0..2 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    d.step(Step::Issue); // Read pending; all seven work steps consumed.
    d.step(Step::Issue); // Deferred Budget, not an issued operation.
    assert_eq!(d.evidence().code(), Code::Budget);
    d.step(Step::Cancel);
    assert_eq!(d.evidence().cancellation(), Cancellation::Requested);
    d.step(Step::ObserveStop);
    let pending = d.evidence();
    assert_eq!(pending.code(), Code::Budget);
    assert_eq!(pending.cancellation(), Cancellation::Pending);
    assert_eq!(
        pending.ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 0, 0, 2, 7]
    );
    assert_eq!(
        (pending.opened(), pending.closed(), pending.live()),
        (2, 0, 2)
    );
    assert_eq!(pending.bytes(), b"");
    d.step(Step::Return(Completion::Success)); // Returned bytes counted, never released.
    let terminal = d.evidence();
    assert_eq!(terminal.code(), Code::Budget);
    assert_eq!(terminal.cancellation(), Cancellation::Acknowledged);
    assert_eq!(
        terminal.ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 6, 0, 2, 7]
    );
    assert_eq!(
        (terminal.opened(), terminal.closed(), terminal.live()),
        (2, 2, 0)
    );
    assert_eq!(terminal.bytes(), b"");
    finish(&mut d);
    assert_eq!(d.evidence(), terminal);
}

fn listed_subdirectory() -> (Driver, Object, Object, Object) {
    let mut w = FakeWorld::new();
    let dir = w.directory(w.root()).unwrap();
    let a = w.file(dir, b"AA").unwrap();
    let b = w.file(dir, b"BB").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    // List root, Open directory, List directory: both candidates are now queued.
    for _ in 0..3 {
        d.step(Step::Issue);
        d.step(Step::Return(Completion::Success));
        d.step(Step::Release);
    }
    (d, dir, a, b)
}

#[test]
fn c1_candidate_slot_exchange_without_alias_is_refused() {
    let (mut d, dir, a, b) = listed_subdirectory();
    // Exchange both slots before the next operation: each object has exactly
    // one incoming edge, so an alias refusal cannot mask a missing slot check.
    d.step(Step::Substitute(dir, 0, b));
    d.step(Step::Substitute(dir, 1, a));
    finish(&mut d);
    let e = d.evidence();
    assert_eq!(e.code(), Code::BindingMismatch);
    assert_eq!(
        e.ledger().counts(),
        &[5, 3, 2, 3, 0, 3, 0, 2, 0, 0, 0, 2, 12]
    );
    assert_eq!(e.bytes(), b"");
    assert_eq!((e.opened(), e.closed(), e.live()), (2, 2, 0));
    assert_eq!(e.sentinel_accesses(), 0);
}

#[test]
fn c2_candidate_aba_restoration_is_refused_by_revision() {
    let (mut d, dir, a, b) = listed_subdirectory();
    // Restore the original slot and unique incoming edges. Only the directory
    // revision records the intervening substitution; root revision is unchanged.
    d.step(Step::Substitute(dir, 0, b));
    d.step(Step::Substitute(dir, 0, a));
    finish(&mut d);
    let e = d.evidence();
    assert_eq!(e.code(), Code::BindingMismatch);
    assert_eq!(
        e.ledger().counts(),
        &[5, 3, 2, 3, 0, 3, 0, 2, 0, 0, 0, 2, 12]
    );
    assert_eq!(e.bytes(), b"");
    assert_eq!((e.opened(), e.closed(), e.live()), (2, 2, 0));
    assert_eq!(e.sentinel_accesses(), 0);
}

#[test]
fn c7_structural_cross_directory_exchange_is_refused() {
    let mut w = FakeWorld::new();
    let d1 = w.directory(w.root()).unwrap();
    let d2 = w.directory(w.root()).unwrap();
    let f1 = w.file(d1, b"ONE").unwrap();
    let f2 = w.file(d2, b"TWO").unwrap();
    // Both objects stay inside, with unique incoming edges and no Fault marker.
    // Recorded parent containment disagrees with the exchanged directory edges.
    w.substitute(d1, 0, f2).unwrap();
    w.substitute(d2, 0, f1).unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    let mut d = driver(w, &s);
    finish(&mut d);
    let e = d.evidence();
    assert_eq!(e.code(), Code::BindingMismatch);
    assert_eq!(
        e.ledger().counts(),
        &[6, 4, 1, 3, 1, 3, 0, 0, 0, 0, 0, 2, 9]
    );
    assert_eq!(e.bytes(), b"");
    assert_eq!((e.opened(), e.closed(), e.live()), (2, 2, 0));
    assert_eq!(e.sentinel_accesses(), 0);
}

#[test]
fn r8_rejection_reservation_refuses_an_otherwise_valid_entry() {
    let mut w = FakeWorld::new();
    w.file(w.root(), b"OK").unwrap();
    let s = FictionalAuthority::explicit_approval(&w, binding()).unwrap();
    // Control: the entry is valid and completes when rejection capacity exists.
    let mut control = driver(w.clone(), &s);
    finish(&mut control);
    let admitted = control.evidence();
    assert_eq!(admitted.code(), Code::Complete);
    assert_eq!(
        admitted.ledger().counts(),
        &[3, 1, 1, 1, 0, 3, 0, 1, 8, 2, 2, 2, 9]
    );
    assert_eq!(admitted.bytes(), b"OK");
    let mut d = Driver::new(
        w,
        Some(&s),
        binding(),
        Limits::default().with(Resource::Rejected, 0).unwrap(),
        true,
        false,
    );
    finish(&mut d);
    let e = d.evidence();
    assert_eq!(e.code(), Code::Budget);
    assert_eq!(
        e.ledger().counts(),
        &[3, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 3]
    );
    assert_eq!(e.admitted(), 0);
    assert_eq!(e.bytes(), b"");
    assert_eq!((e.opened(), e.closed(), e.live()), (1, 1, 0));
    assert_eq!(e.sentinel_accesses(), 0);
}
