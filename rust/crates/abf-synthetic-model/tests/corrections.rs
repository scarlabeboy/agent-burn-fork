//! ABF40 / M3B independent adversarial cases. Expected values derive from the
//! ABF36C contract text cited per test, not from the model implementation.
//! Tests prefixed `contract_` assert the contract outcome; a failure there is
//! evidence of a defect. Tests prefixed `probe_` pin observed conservative
//! behavior that the author's suite does not exercise.
use abf_synthetic_model::*;

fn positive() -> Policy {
    Policy {
        present: Disposition(Some(true)),
        reencounter: Disposition(Some(true)),
        unknown: Disposition(Some(true)),
        compatibility_proven: true,
        allow_unverified: true,
        allow_replacement: true,
        automatic: true,
        manual: true,
        retirement: true,
        ..Policy::default()
    }
}
fn caps() -> [u32; 12] {
    [20, 8, 8, 8, 8, 8, 80, 300, 4, 4, 2, 200]
}
fn lin(environment: u32) -> Lineage {
    Lineage {
        environment,
        principal: 7,
        harness: 3,
        directory: 9,
    }
}
fn spec(environment: u32, policy: Policy) -> ScopeSpec {
    ScopeSpec {
        lineage: lin(environment),
        policy,
        policy_version: 1,
        metadata: true,
        display: true,
        bytes: true,
        release: true,
        use_results: true,
        receiver: 11,
        display_receiver: 12,
        valid_until: 100,
    }
}
fn rng(start: u32) -> Range {
    Range {
        start,
        length: 4,
        domain_end: 64,
    }
}
fn obj<'id>(m: &mut Model<'id>, env: u32, hint: u32) -> Object<'id> {
    m.fixture_object(lin(env), hint, true, false).unwrap()
}
fn child<'id>(m: &mut Model<'id>, s: Scope<'id>, o: Object<'id>) -> Grant<'id> {
    let p = m.discover(s, o, true, false).unwrap();
    m.admit(p, false).unwrap()
}
fn capture<'id>(m: &mut Model<'id>, g: Grant<'id>, o: Object<'id>, start: u32) -> Observation<'id> {
    let op = m.begin(g, o, rng(start), 50).unwrap();
    m.return_read(op, o, 42).unwrap();
    let r = m.release(op, 11).unwrap();
    m.accept(r, 11).unwrap()
}

fn effects(m: &Model<'_>) -> (u32, u32, u32, u32) {
    let c = m.counters();
    (c.admissions, c.prompts, c.releases, c.uses)
}

#[test]
fn overdue_read_keeps_copy_stage_and_credit_until_actual_late_return() {
    let mut c = caps();
    c[1] = 1;
    simulate(Limits { caps: c }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 5).unwrap();
        m.duplicate_worker_copy(op).unwrap();
        let before = m.resources();
        m.step(5).unwrap();
        assert_eq!(m.resources().held, before.held);
        assert_eq!(m.counters().post_samples, 0);
        assert_eq!(m.begin(g, a, rng(4), 50), Err(Reason::Exhausted));
        let e = effects(&m);
        assert_eq!(m.release(op, 11), Err(Reason::Fenced));
        assert_eq!(effects(&m), e);
        assert_eq!(m.duplicate_worker_copy(op), Err(Reason::Fenced));
        m.return_read(op, a, 42).unwrap();
        assert_eq!(m.counters().post_samples, 2);
        assert_eq!(
            (
                m.resources().held[0],
                m.resources().held[1],
                m.resources().held[2]
            ),
            (3, 0, 0)
        );
        assert_eq!(m.return_read(op, a, 42), Err(Reason::Replay));
        assert_eq!(m.counters().post_samples, 2);
        assert_eq!(m.release(op, 11), Err(Reason::Fenced));
        assert_eq!(effects(&m), e);
    });
}
#[test]
fn timeout_only_cannot_unlock_restart_or_quiescence() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 5).unwrap();
        m.duplicate_worker_copy(op).unwrap();
        m.session_loss();
        m.step(1000).unwrap();
        assert_eq!(
            (
                m.resources().held[0],
                m.resources().held[1],
                m.resources().held[2]
            ),
            (6, 1, 1)
        );
        assert_eq!(m.counters().post_samples, 0);
        assert_eq!(
            m.restart(IndependentEvidence::CompleteDrain, true),
            Err(Reason::RecoveryUnavailable)
        );
        assert!(!m.quiescence_confirmed());
        m.return_read(op, a, 42).unwrap();
        assert_eq!(m.counters().post_samples, 2);
        assert_eq!(m.resources().held[0], 0);
        m.restart(IndependentEvidence::CompleteDrain, true).unwrap();
        assert_eq!(m.begin(g, a, rng(0), 1100), Err(Reason::Retired));
    });
}
#[test]
fn restrictive_changes_survive_work_and_retained_lineage_exhaustion() {
    for exhausted in [6, 7] {
        let mut c = caps();
        c[6] = if exhausted == 6 { 3 } else { 80 };
        c[7] = if exhausted == 7 { 12 } else { 300 };
        simulate(Limits { caps: c }, |mut m| {
            let s = m.approve(spec(1, positive())).unwrap();
            let a = obj(&mut m, 1, 1);
            let g = child(&mut m, s, a);
            let x = capture(&mut m, g, a, 0);
            let d = m.derive(&[x]).unwrap();
            let computer = m.computer(&[d]).unwrap();
            assert_eq!(m.resources().held[exhausted], c[exhausted]);
            let resources = m.resources();
            m.downgrade(x, true, true).unwrap();
            assert_eq!(m.resources().held, resources.held);
            for result in [d, computer] {
                let q = m.view(result).unwrap().qualification;
                assert!(q.stale && q.disputed && !q.revoked);
            }
            if exhausted == 6 {
                let current = m.use_result(computer, 11).unwrap();
                assert!(current.qualification.stale && current.qualification.disputed);
            } else {
                let e = effects(&m);
                assert_eq!(m.use_result(computer, 11), Err(Reason::Exhausted));
                assert_eq!(effects(&m), e);
            }
            m.retire(g).unwrap();
            assert!(m.view(computer).unwrap().qualification.revoked);
            let e = effects(&m);
            assert_eq!(m.use_result(computer, 11), Err(Reason::Retired));
            assert_eq!(effects(&m), e);
        });
    }
}
#[test]
fn identity_only_union_charges_unique_retained_lineage() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let y = capture(&mut m, g, a, 0); // same payload/time/range; distinct observations
        let before = m.resources().held[6];
        let d = m.derive(&[x, x, x]).unwrap();
        assert_eq!(m.resources().held[6] - before, 1);
        assert_eq!(m.view(d).unwrap().observations, 1);
        let both = m.derive(&[x, y]).unwrap();
        let before = m.resources().held[6];
        let c = m.computer(&[d, both, d]).unwrap();
        assert_eq!(m.resources().held[6] - before, 2);
        let view = m.view(c).unwrap();
        assert_eq!(view.observations, 2);
        assert_eq!(view.ranges.len(), 2);
        assert!(view.qualification.identity_unresolved);
        assert_eq!(view.sampled_bytes, 4);
    });
}
#[test]
fn restart_requires_fence_even_without_owners() {
    simulate(Limits { caps: caps() }, |mut m| {
        assert_eq!(
            m.restart(IndependentEvidence::CompleteDrain, true),
            Err(Reason::RecoveryUnavailable)
        );
        assert!(!m.quiescence_confirmed());
        assert_eq!(effects(&m), (0, 0, 0, 0));
    });
}
#[test]
fn orphan_copy_and_grant_fences_refuse_new_work() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 50).unwrap();
        m.duplicate_worker_copy(op).unwrap();
        m.fence_worker(op).unwrap();
        let r = m.resources();
        let e = effects(&m);
        assert_eq!(m.duplicate_worker_copy(op), Err(Reason::Fenced));
        assert_eq!(m.resources().held, r.held);
        assert_eq!(m.release(op, 11), Err(Reason::Fenced));
        assert_eq!(effects(&m), e);
        m.return_read(op, a, 42).unwrap();
        assert_eq!(m.counters().post_samples, 2);
        let second = m.begin(g, a, rng(0), 50).unwrap();
        m.retire(g).unwrap();
        assert_eq!(m.duplicate_worker_copy(second), Err(Reason::Retired));
        assert_eq!(m.begin(g, a, rng(0), 50), Err(Reason::Retired));
        assert_eq!(effects(&m), e);
    });
}
#[test]
fn derive_and_computer_refuse_revoked_lineage_without_minting_or_effects() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        m.retire(g).unwrap();
        let r = m.resources();
        let e = effects(&m);
        assert_eq!(m.derive(&[x]), Err(Reason::Retired));
        assert_eq!(m.computer(&[d]), Err(Reason::Retired));
        assert_eq!(m.resources().held, r.held);
        assert_eq!(effects(&m), e);
        assert!(m.view(d).unwrap().qualification.revoked);
    });
}
#[test]
fn display_permission_and_manual_policy_are_individual_guards() {
    for kind in 0..2 {
        simulate(Limits { caps: caps() }, |mut m| {
            let mut sp = spec(1, positive());
            if kind == 0 {
                sp.display = false;
            } else {
                sp.policy.manual = false;
            }
            let s = m.approve(sp).unwrap();
            let a = obj(&mut m, 1, 1);
            let p = m.discover(s, a, true, false).unwrap();
            let e = effects(&m);
            assert_eq!(m.request_prompt(p, 12, true), Err(Reason::Unauthorized));
            assert_eq!(effects(&m), e);
            assert_eq!(m.resources().spent[8], 0);
            assert_eq!(m.admit(p, true), Err(Reason::Unauthorized));
            assert_eq!(effects(&m), e);
        });
    }
}
#[test]
fn invalid_policy_cannot_present_or_spend_prompt_credit() {
    for unproved in [false, true] {
        simulate(Limits { caps: caps() }, |mut m| {
            let mut policy = positive();
            let expected = if unproved {
                policy.compatibility_proven = false;
                Reason::DispositionCompatibilityUnproven
            } else {
                policy.reencounter = Disposition(Some(false));
                Reason::DispositionPolicyIncompatible
            };
            let s = m.approve(spec(1, policy)).unwrap();
            let a = obj(&mut m, 1, 1);
            let p = m.discover(s, a, true, false).unwrap();
            assert_eq!(m.request_prompt(p, 12, true), Err(expected));
            assert_eq!(effects(&m), (0, 0, 0, 0));
            assert_eq!(m.resources().spent[8], 0);
        });
    }
}
#[test]
fn invalid_renewal_preserves_old_scope_but_exhausted_renewal_is_fail_closed() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let mut bad = spec(1, positive());
        bad.metadata = false;
        assert_eq!(m.renew(s, bad), Err(Reason::Unauthorized));
        assert_eq!(m.transient(g), Ok(()));
        bad.metadata = true;
        bad.valid_until = 0;
        assert_eq!(m.renew(s, bad), Err(Reason::Unauthorized));
        assert_eq!(m.transient(g), Ok(()));
    });
    let mut c = caps();
    c[7] = 4;
    simulate(Limits { caps: c }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        assert_eq!(m.renew(s, spec(1, positive())), Err(Reason::Exhausted));
        assert_eq!(m.transient(g), Err(Reason::Retired));
        assert_eq!(m.resources().held[0], 0);
    });
}
#[test]
fn sampled_extent_and_scope_ceiling_cannot_be_enlarged_by_range_arguments() {
    for (size, ceiling) in [(4, 64), (64, 4)] {
        simulate(Limits { caps: caps() }, |mut m| {
            let s = m.approve_with_range(spec(1, positive()), ceiling).unwrap();
            let a = m
                .fixture_object_with_size(lin(1), 1, true, false, size)
                .unwrap();
            let g = child(&mut m, s, a);
            assert_eq!(
                m.begin(
                    g,
                    a,
                    Range {
                        start: 0,
                        length: 4,
                        domain_end: 8
                    },
                    50
                ),
                Err(Reason::RangeInvalid)
            );
            assert_eq!(m.counters().reads, 0);
            assert_eq!(effects(&m), (1, 0, 0, 0));
            let op = m
                .begin(
                    g,
                    a,
                    Range {
                        start: 0,
                        length: 4,
                        domain_end: 4,
                    },
                    50,
                )
                .unwrap();
            m.return_read(op, a, 42).unwrap();
        });
    }
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve_with_range(spec(1, positive()), u32::MAX).unwrap();
        let a = m
            .fixture_object_with_size(lin(1), 1, true, false, u32::MAX)
            .unwrap();
        let g = child(&mut m, s, a);
        for range in [
            Range {
                start: 0,
                length: 65537,
                domain_end: u32::MAX,
            },
            Range {
                start: u32::MAX,
                length: 4,
                domain_end: u32::MAX,
            },
        ] {
            assert_eq!(m.begin(g, a, range, 50), Err(Reason::RangeInvalid));
        }
        assert_eq!(m.counters().reads, 0);
    });
}
#[test]
fn prompt_scope_epoch_and_receiver_effects_remain_exact() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        m.request_prompt(p, 12, true).unwrap();
        let fresh = m.renew(s, spec(1, positive())).unwrap();
        let e = effects(&m);
        assert_eq!(m.admit(p, true), Err(Reason::Retired));
        assert_eq!(effects(&m), e);
        let p = m.discover(fresh, a, true, false).unwrap();
        assert_eq!(m.admit(p, true), Err(Reason::Unauthorized));
        assert_eq!(effects(&m), e);
    });
}

#[test]
fn grant_rights_and_correct_receivers_precede_every_visible_effect() {
    // Each denial returns no receiver capability and leaves the effect counters fixed.
    for right in 0..3 {
        simulate(Limits { caps: caps() }, |mut m| {
            let mut sp = spec(1, positive());
            match right {
                0 => sp.bytes = false,
                1 => sp.release = false,
                _ => sp.use_results = false,
            }
            let s = m.approve(sp).unwrap();
            let a = obj(&mut m, 1, 1);
            let p = m.discover(s, a, true, false).unwrap();
            if right == 0 {
                assert_eq!(m.admit(p, false), Err(Reason::Unauthorized));
                assert_eq!(effects(&m), (0, 0, 0, 0));
            } else {
                let g = m.admit(p, false).unwrap();
                let op = m.begin(g, a, rng(0), 50).unwrap();
                m.return_read(op, a, 42).unwrap();
                let e = effects(&m);
                if right == 1 {
                    assert_eq!(m.release(op, 11), Err(Reason::Unauthorized));
                    assert_eq!(effects(&m), e);
                } else {
                    let r = m.release(op, 11).unwrap();
                    let e = effects(&m);
                    assert_eq!(m.accept(r, 11), Err(Reason::Unauthorized));
                    assert_eq!(effects(&m), e);
                }
            }
        });
    }
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        let e = effects(&m);
        assert_eq!(m.request_prompt(p, 11, true), Err(Reason::Unauthorized));
        assert_eq!(effects(&m), e);
        let g = m.admit(p, false).unwrap();
        let op = m.begin(g, a, rng(0), 50).unwrap();
        m.return_read(op, a, 42).unwrap();
        let e = effects(&m);
        assert_eq!(m.release(op, 12), Err(Reason::Unauthorized));
        assert_eq!(effects(&m), e);
        let r = m.release(op, 11).unwrap();
        let e = effects(&m);
        assert_eq!(m.accept(r, 12), Err(Reason::Unauthorized));
        assert_eq!(effects(&m), e);
        let x = m.accept(r, 11).unwrap();
        let d = m.derive(&[x]).unwrap();
        let e = effects(&m);
        assert_eq!(m.use_result(d, 12), Err(Reason::Unauthorized));
        assert_eq!(effects(&m), e);
        assert_eq!(m.counters().uses, 1);
    });
}
#[test]
fn completed_private_stage_deadline_disposes_without_inventing_postchecks() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 5).unwrap();
        m.return_read(op, a, 42).unwrap();
        assert_eq!((m.resources().held[2], m.counters().post_samples), (1, 2));
        m.step(5).unwrap();
        assert_eq!((m.resources().held[2], m.counters().post_samples), (0, 2));
        assert_eq!(m.release(op, 11), Err(Reason::Fenced));
        assert_eq!(effects(&m), (1, 0, 0, 0));
    });
}
#[test]
fn released_receipt_cannot_be_used_after_worker_fence() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 5).unwrap();
        m.return_read(op, a, 42).unwrap();
        let r = m.release(op, 11).unwrap();
        m.step(5).unwrap();
        let e = effects(&m);
        assert_eq!(m.accept(r, 11), Err(Reason::Fenced));
        assert_eq!(effects(&m), e);
    });
}

#[test]
fn restart_never_reactivates_old_unexpired_grant() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 5).unwrap();
        m.session_loss();
        m.step(5).unwrap(); // old consent expiry is 100: no expiry guard can mask resurrection
        m.return_read(op, a, 42).unwrap();
        m.restart(IndependentEvidence::CompleteDrain, true).unwrap();
        let e = effects(&m);
        assert_eq!(m.begin(g, a, rng(0), 50), Err(Reason::Retired));
        assert_eq!(m.reencounter(s, a), Err(Reason::Retired));
        assert_eq!(effects(&m), e);
        let fresh = m.approve(spec(1, positive())).unwrap();
        assert_eq!(m.discover(fresh, a, true, false), Err(Reason::Ineligible));
    });
}

#[test]
fn b_computer_cannot_relaunder_lineage_unknown_inputs() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        m.lose_lineage(d).unwrap();
        let before = (m.resources().held, m.counters().uses);
        assert_eq!(m.computer(&[d]), Err(Reason::LineageUnknown)); // C03a: withhold possibly dependent
        assert_eq!((m.resources().held, m.counters().uses), before);
    });
}
