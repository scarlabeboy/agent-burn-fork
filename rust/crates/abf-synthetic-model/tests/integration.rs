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
fn limits() -> Limits {
    Limits {
        caps: [20, 8, 8, 8, 8, 8, 80, 300, 4, 4, 2, 200],
    }
}
fn spec(environment: u32, policy: Policy) -> ScopeSpec {
    ScopeSpec {
        lineage: Lineage {
            environment,
            principal: 7,
            harness: 3,
            directory: 9,
        },
        policy,
        metadata: true,
        display: true,
        bytes: true,
        release: true,
        use_results: true,
        receiver: 11,
        display_receiver: 12,
        valid_until: 100,
        policy_version: 1,
    }
}
fn object<'id>(m: &mut Model<'id>, env: u32, hint: u32) -> Object<'id> {
    m.fixture_object(
        Lineage {
            environment: env,
            principal: 7,
            harness: 3,
            directory: 9,
        },
        hint,
        true,
        false,
    )
    .unwrap()
}
fn child<'id>(m: &mut Model<'id>, s: Scope<'id>, o: Object<'id>) -> Grant<'id> {
    let p = m.discover(s, o, true, false).unwrap();
    m.admit(p, false).unwrap()
}
fn capture<'id>(m: &mut Model<'id>, g: Grant<'id>, o: Object<'id>, start: u32) -> Observation<'id> {
    let op = m
        .begin(
            g,
            o,
            Range {
                start,
                length: 4,
                domain_end: 12,
            },
            50,
        )
        .unwrap();
    m.return_read(op, o, 42).unwrap();
    let released = m.release(op, 11).unwrap();
    m.accept(released, 11).unwrap()
}

#[test]
fn deny_all_default_has_no_admission_or_unsolicited_prompt() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, Policy::default())).unwrap();
        let o = object(&mut m, 1, 1);
        let p = m.discover(s, o, true, false).unwrap();
        assert!(m.admit(p, false).is_err());
        assert!(m.request_prompt(p, 12, true).is_err());
        assert_eq!(
            (
                m.counters().admissions,
                m.counters().prompts,
                m.counters().releases
            ),
            (0, 0, 0)
        );
    });
}
#[test]
fn precision_positive_and_same_held_duplicate_denial() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let o = object(&mut m, 1, 1);
        let g = child(&mut m, s, o);
        assert_eq!(m.reencounter(s, o).unwrap(), g);
        let p = m.discover(s, o, true, false).unwrap();
        assert_eq!(m.admit(p, false), Err(Reason::DuplicateLiveObjectGrant));
        let ob = capture(&mut m, g, o, 0);
        let q = m.observation(ob).unwrap();
        assert!(
            q.producer_unverified
                && q.version_unproven
                && q.identity_unresolved
                && q.inventory_partial
        );
        assert_eq!(
            (
                m.counters().admissions,
                m.counters().prompts,
                m.counters().releases,
                m.counters().uses
            ),
            (1, 0, 1, 1)
        );
    });
}
#[test]
fn hints_are_not_held_identity_and_equal_observations_are_not_events() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 88);
        let b = object(&mut m, 1, 88); // same name, distinct held objects / ground-truth events
        let ga = child(&mut m, s, a);
        let gb = child(&mut m, s, b);
        assert_ne!(ga, gb);
        let x = capture(&mut m, ga, a, 0);
        let y = capture(&mut m, gb, b, 0);
        let z = capture(&mut m, ga, a, 0); // independent recapture of first ground-truth event
        let d = m.derive(&[x, y, z]).unwrap();
        let view = m.view(d).unwrap();
        assert_eq!(view.observations, 3);
        assert!(view.qualification.identity_unresolved);
        assert_eq!(view.sampled_bytes, 8); // union per held object, not event dedup
    });
}
#[test]
fn decline_continuity_and_explicit_budgeted_clear() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let o = object(&mut m, 1, 5);
        let p = m.discover(s, o, true, false).unwrap();
        m.request_prompt(p, 12, true).unwrap();
        m.decline(p).unwrap();
        for version in 2..5 {
            m.retire_scope(s).unwrap();
            let mut terms = spec(1, positive());
            terms.policy_version = version;
            let fresh = m.approve(terms).unwrap();
            let p = m.discover(fresh, o, true, false).unwrap();
            assert_eq!(
                m.request_prompt(p, 12, true),
                Err(Reason::DeclinedSuppressed)
            );
            assert_eq!(m.admit(p, false), Err(Reason::DeclinedSuppressed));
            m.discard(p).unwrap();
        }
        assert_eq!(m.counters().prompts, 1);
        assert_eq!(
            m.clear_decline(spec(1, positive()).lineage, 5, false),
            Err(Reason::Unauthorized)
        );
        m.clear_decline(spec(1, positive()).lineage, 5, true)
            .unwrap();
        let fresh = m.approve(spec(1, positive())).unwrap();
        let p = m.discover(fresh, o, true, false).unwrap();
        m.request_prompt(p, 12, true).unwrap();
        assert_eq!(m.counters().prompts, 2);
    });
}
#[test]
fn restart_carries_declines_and_never_reuses_old_objects_or_grants() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let old = object(&mut m, 1, 5);
        let g = child(&mut m, s, old);
        let other = object(&mut m, 1, 6);
        let p = m.discover(s, other, true, false).unwrap();
        m.decline(p).unwrap();
        m.session_loss();
        assert!(
            m.begin(
                g,
                old,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8
                },
                50
            )
            .is_err()
        );
        assert_eq!(
            m.restart(IndependentEvidence::SelfReport, true),
            Err(Reason::RecoveryUnavailable)
        );
        assert_eq!(
            m.restart(IndependentEvidence::OwnersOnly, false),
            Err(Reason::Unauthorized)
        );
        m.restart(IndependentEvidence::OwnersOnly, true).unwrap();
        assert!(!m.quiescence_confirmed());
        let fresh = m.approve(spec(1, positive())).unwrap();
        assert!(m.discover(fresh, old, true, false).is_err());
        let o = object(&mut m, 1, 6);
        let p = m.discover(fresh, o, true, false).unwrap();
        assert_eq!(
            m.request_prompt(p, 12, true),
            Err(Reason::DeclinedSuppressed)
        );
    });
}
#[test]
fn limited_manual_credit_overflow_and_lost_continuity() {
    let mut l = limits();
    l.caps[8] = 1;
    l.caps[5] = 1;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.request_prompt(p, 12, false), Err(Reason::Unauthorized));
        m.request_prompt(p, 12, true).unwrap();
        m.decline(p).unwrap();
        let b = object(&mut m, 1, 2);
        let p = m.discover(s, b, true, false).unwrap();
        assert_eq!(m.request_prompt(p, 12, true), Err(Reason::Exhausted));
        assert_eq!(m.decline(p), Err(Reason::ManualSuppressionOverflow));
        let new = m.renew(s, spec(1, positive())).unwrap();
        let p = m.discover(new, b, true, false).unwrap();
        assert_eq!(
            m.request_prompt(p, 12, true),
            Err(Reason::ManualSuppressionOverflow)
        );
        assert_eq!(m.counters().prompts, 1);
        m.lose_consent_continuity();
        assert_eq!(
            m.clear_decline(spec(1, positive()).lineage, 1, true),
            Err(Reason::ContinuityLost)
        );
    });
}
#[test]
fn renewal_and_new_grants_do_not_refill_work_or_prompt_credit() {
    let mut l = limits();
    l.caps[8] = 1;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        m.request_prompt(p, 12, true).unwrap();
        m.discard(p).unwrap();
        let new = m.renew(s, spec(1, positive())).unwrap();
        let b = object(&mut m, 1, 2);
        let p = m.discover(new, b, true, false).unwrap();
        assert_eq!(m.request_prompt(p, 12, true), Err(Reason::Exhausted));
        assert_eq!(m.resources().spent[8], 1);
    });
}
#[test]
fn revoke_before_release_or_use_preserves_history_and_drains_copies() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let o = object(&mut m, 1, 1);
        let g = child(&mut m, s, o);
        let earlier = capture(&mut m, g, o, 0);
        let op = m
            .begin(
                g,
                o,
                Range {
                    start: 4,
                    length: 4,
                    domain_end: 12,
                },
                50,
            )
            .unwrap();
        m.duplicate_worker_copy(op).unwrap();
        m.retire(g).unwrap();
        assert_eq!(m.resources().held[0], 6); // scope 2 + F 1 + worker pair 2 + copy 1
        assert_eq!(m.release(op, 11), Err(Reason::Retired));
        m.return_read(op, o, 42).unwrap(); // A<R may finish privately with reserved postchecks
        assert_eq!(m.release(op, 11), Err(Reason::Retired));
        assert_eq!(m.counters().post_samples, 4);
        assert_eq!(m.resources().held[0], 2);
        assert!(m.observation(earlier).unwrap().revoked);
    });
}
#[test]
fn orphan_worker_fences_without_retiring_independent_holder() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let o = object(&mut m, 1, 1);
        let g = child(&mut m, s, o);
        let op = m
            .begin(
                g,
                o,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        m.fence_worker(op).unwrap();
        m.return_read(op, o, 42).unwrap();
        assert_eq!(m.release(op, 11), Err(Reason::Fenced));
        assert_eq!(m.resources().held[1], 0);
        capture(&mut m, g, o, 0);
    });
}
#[test]
fn pressure_requires_r_then_accounted_drain_before_credit() {
    let mut l = limits();
    l.caps[0] = 5;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        let b = object(&mut m, 1, 2);
        assert_eq!(m.discover(s, b, true, false), Err(Reason::Exhausted));
        m.pressure_retire(g).unwrap();
        assert_eq!(m.resources().held[0], 5);
        m.return_read(op, a, 42).unwrap();
        assert_eq!(m.resources().held[0], 2);
        let fresh = child(&mut m, s, b);
        assert_ne!(fresh, g);
    });
}
#[test]
fn scope_mismatch_irreversibly_retires_all_dependents_but_transient_does_not() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        m.transient(g).unwrap();
        capture(&mut m, g, a, 0);
        m.scope_mismatch(s).unwrap();
        assert!(m.reencounter(s, a).is_err());
        assert!(
            m.begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8
                },
                50
            )
            .is_err()
        );
    });
}
#[test]
fn hint_eviction_expiry_and_unknown_history_only_restrict() {
    simulate(limits(), |mut m| {
        let mut p = positive();
        p.unknown = Disposition(Some(false));
        let s = m.approve(spec(1, p)).unwrap();
        let a = object(&mut m, 1, 1);
        let x = m.discover(s, a, true, false).unwrap();
        assert!(m.admit(x, false).is_err()); // genuine first child still R+U
        m.discard(x).unwrap();
        m.fixture_retained_hint(s, 1).unwrap();
        let g = child(&mut m, s, a);
        m.retire(g).unwrap();
        m.evict_hint(s, 1);
        let x = m.discover(s, a, true, false).unwrap();
        assert!(m.admit(x, false).is_err());
    });
}
#[test]
fn multiple_ranges_late_downgrade_and_computer_have_no_third_source() {
    simulate(limits(), |mut m| {
        let s1 = m.approve(spec(1, positive())).unwrap();
        let s2 = m.approve(spec(2, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let b = object(&mut m, 2, 1);
        let ga = child(&mut m, s1, a);
        let gb = child(&mut m, s2, b);
        let x = capture(&mut m, ga, a, 0);
        let y = capture(&mut m, ga, a, 4);
        let z = capture(&mut m, gb, b, 0);
        let d1 = m.derive(&[x, y]).unwrap();
        let d2 = m.derive(&[z]).unwrap();
        let computer = m.computer(&[d1, d2]).unwrap();
        let before = m.use_result(computer, 11).unwrap();
        assert_eq!(before.environments, vec![1, 2]);
        assert_eq!(before.sampled_bytes, 12);
        assert!(
            before.qualification.producer_unverified
                && before.qualification.inventory_partial
                && before.qualification.byte_partial
        );
        assert!(before.qualification.version_unproven && before.qualification.identity_unresolved);
        m.downgrade(z, true, true).unwrap();
        let after = m.use_result(computer, 11).unwrap();
        assert!(after.qualification.stale && after.qualification.disputed);
        assert_eq!(after.observations, 3);
        m.retire(gb).unwrap();
        assert!(m.view(computer).unwrap().qualification.revoked);
        assert_eq!(m.use_result(computer, 11), Err(Reason::Retired));
    });
}
#[test]
fn ral_p_permutations_and_replay_are_receiver_decisions() {
    // Fixed expected receiver counters, not generated from model transitions.
    const ORACLE: &[(usize, u32, u32)] = &[(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 1, 0), (4, 1, 1)];
    for &(r_at, releases, uses) in ORACLE {
        simulate(limits(), |mut m| {
            let s = m.approve(spec(1, positive())).unwrap();
            let o = object(&mut m, 1, 1);
            let g = child(&mut m, s, o);
            if r_at == 0 {
                m.retire(g).unwrap();
            }
            let op = m.begin(
                g,
                o,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            );
            if r_at == 1 {
                m.retire(g).unwrap();
            }
            if let Ok(op) = op {
                m.return_read(op, o, 42).unwrap();
                if r_at == 2 {
                    m.retire(g).unwrap();
                }
                let released = m.release(op, 11);
                if r_at == 3 {
                    m.retire(g).unwrap();
                }
                if let Ok(released) = released {
                    let accepted = m.accept(released, 11);
                    if r_at == 4 {
                        m.retire(g).unwrap();
                    }
                    assert!(m.accept(released, 11).is_err());
                    assert_eq!(accepted.is_ok(), uses == 1);
                }
                assert!(m.release(op, 11).is_err());
            }
            assert_eq!((m.counters().releases, m.counters().uses), (releases, uses));
        });
    }
}
#[test]
fn wrong_environment_origin_receiver_and_invalid_ranges_deny() {
    simulate(limits(), |mut m| {
        let mut p = positive();
        p.allow_unverified = false;
        let s = m.approve(spec(1, p)).unwrap();
        let a = object(&mut m, 1, 1);
        let b = object(&mut m, 2, 1);
        assert_eq!(m.discover(s, b, true, false), Err(Reason::Ineligible));
        let x = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.admit(x, true), Err(Reason::ProducerUnverified));
        m.discard(x).unwrap();
        let s = m.approve(spec(1, positive())).unwrap();
        let g = child(&mut m, s, a);
        assert_eq!(
            m.begin(
                g,
                a,
                Range {
                    start: u32::MAX,
                    length: 4,
                    domain_end: u32::MAX
                },
                50
            ),
            Err(Reason::RangeInvalid)
        );
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        m.return_read(op, a, 42).unwrap();
        assert_eq!(m.release(op, 99), Err(Reason::Unauthorized));
    });
}
#[test]
fn binding_swap_after_admission_denies_prebyte_and_newness_is_not_inferred() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let b = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        assert_eq!(
            m.begin(
                g,
                b,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8
                },
                50
            ),
            Err(Reason::BindingMismatch)
        );
        assert_eq!(m.counters().reads, 0);
        let mut p = positive();
        p.require_newness = true;
        let s = m.approve(spec(1, p)).unwrap();
        let x = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.admit(x, false), Err(Reason::TemporalNewnessUnproven));
    });
}
#[test]
fn fake_clock_loss_and_missing_lineage_prevent_current_use() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        m.lose_lineage(d).unwrap();
        assert_eq!(m.use_result(d, 11), Err(Reason::LineageUnknown));
        m.step(10).unwrap();
        assert_eq!(m.step(9), Err(Reason::Fenced));
        assert!(m.reencounter(s, a).is_err());
    });
}

#[test]
fn deadline_drain_and_independent_witness_cover_worker_copies_and_stages() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                5,
            )
            .unwrap();
        m.duplicate_worker_copy(op).unwrap();
        m.session_loss();
        assert_eq!(
            m.restart(IndependentEvidence::CompleteDrain, true),
            Err(Reason::RecoveryUnavailable)
        );
        assert_eq!(
            (
                m.resources().held[0],
                m.resources().held[1],
                m.resources().held[2]
            ),
            (6, 1, 1)
        );
        m.step(5).unwrap(); // timeout fences; it is not a completion witness
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
        m.return_read(op, a, 42).unwrap(); // actual late return accounts both held-owner checks
        assert_eq!(
            (
                m.resources().held[0],
                m.resources().held[1],
                m.resources().held[2]
            ),
            (0, 0, 0)
        );
        assert_eq!(m.counters().post_samples, 2);
        assert_eq!(
            m.restart(IndependentEvidence::PartialInventory, true),
            Err(Reason::RecoveryUnavailable)
        );
        m.restart(IndependentEvidence::CompleteDrain, true).unwrap();
        assert!(m.quiescence_confirmed());
        assert!(
            m.begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8
                },
                50
            )
            .is_err()
        );
    });
}
#[test]
fn global_work_exhaustion_cannot_be_reset_by_new_scope() {
    let mut l = limits();
    l.caps[7] = 3;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let bad = m
            .fixture_object(spec(1, positive()).lineage, 1, false, false)
            .unwrap();
        assert_eq!(m.discover(s, bad, true, false), Err(Reason::Ineligible));
        assert_eq!(m.discover(s, bad, true, false), Err(Reason::Ineligible));
        assert_eq!(m.discover(s, bad, true, false), Err(Reason::Exhausted));
        assert_eq!(m.renew(s, spec(1, positive())), Err(Reason::Exhausted));
        assert_eq!(m.resources().spent[7], 3);
    });
}
#[test]
fn finite_hint_overflow_and_expiry_keep_literal_unknown_class() {
    let mut l = limits();
    l.caps[4] = 1;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let b = object(&mut m, 1, 2);
        let ga = child(&mut m, s, a);
        let gb = child(&mut m, s, b);
        assert_eq!(m.resources().held[4], 1);
        m.retire(ga).unwrap();
        m.retire(gb).unwrap();
        let mut p = positive();
        p.unknown = Disposition(Some(false));
        let fresh = m.renew(s, spec(1, p)).unwrap();
        let proposal = m.discover(fresh, b, true, false).unwrap();
        assert!(m.admit(proposal, false).is_err());
        m.discard(proposal).unwrap();
        m.evict_hint(fresh, 1); // explicit retention expiry uses same conservative transition
        let proposal = m.discover(fresh, a, true, false).unwrap();
        assert!(m.admit(proposal, false).is_err());
    });
}
#[test]
fn prompt_and_release_receivers_count_visible_results_with_positive_canary() {
    simulate(limits(), |mut m| {
        // Separate receiver/UI stub observes actual return values, not broker counters.
        let mut ui = Vec::new();
        let mut content = Vec::new();
        let mut accepted = 0;
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 0xCAFE);
        let p = m.discover(s, a, true, false).unwrap();
        let prompt = m.request_prompt(p, 12, true).unwrap();
        ui.push((prompt.name_hint, prompt.producer_unverified));
        let g = m.admit(p, true).unwrap();
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        m.return_read(op, a, 0xBEEF).unwrap();
        let r = m.release(op, 11).unwrap();
        content.push(r.payload());
        m.accept(r, 11).unwrap();
        accepted += 1;
        assert_eq!(ui, vec![(0xCAFE, true)]);
        assert_eq!(content, vec![0xBEEF]);
        assert_eq!(accepted, 1);
        assert_eq!(format!("{r:?}"), "Released(opaque)");
        assert_eq!(format!("{prompt:?}"), "Prompt(untrusted hint redacted)");
        assert_eq!(
            (
                m.counters().prompts,
                m.counters().releases,
                m.counters().uses
            ),
            (1, 1, 1)
        );
    });
}

#[test]
fn scope_r_between_private_proposal_and_admission_never_creates_authority() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        m.retire_scope(s).unwrap();
        assert!(m.admit(p, false).is_err());
        assert_eq!(m.counters().admissions, 0);
    });
}
#[test]
fn late_parent_lineage_loss_withholds_existing_computer() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        let c = m.computer(&[d]).unwrap();
        m.lose_lineage(d).unwrap();
        assert_eq!(m.use_result(c, 11), Err(Reason::LineageUnknown));
    });
}

#[test]
fn independent_owner_work_and_receiver_counter_table() {
    // Contract-owned arithmetic, declared independently of Resource::reserve.
    // (owners, workers, stages, proposals, hints, lineage, work, records, post, L, P)
    const EXPECTED: &[[u32; 11]] = &[
        [2, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0],  // scope R/D metadata owners
        [2, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0],  // abstract fixture object
        [3, 0, 0, 1, 0, 0, 3, 3, 0, 0, 0],  // held proposal + disposal reservation
        [3, 0, 0, 0, 1, 0, 4, 4, 0, 0, 0],  // canonical F transfer, individual child epoch
        [5, 1, 1, 0, 1, 0, 8, 5, 0, 0, 0],  // worker pair/stage + 2 post/cleanup credits
        [3, 0, 1, 0, 1, 0, 8, 5, 2, 0, 0],  // returned read pair closed
        [3, 0, 0, 0, 1, 0, 9, 5, 2, 1, 0],  // actual L
        [3, 0, 0, 0, 1, 1, 10, 6, 2, 1, 1], // actual P
    ];
    fn row(m: &Model<'_>) -> [u32; 11] {
        let r = m.resources();
        let c = m.counters();
        [
            r.held[0],
            r.held[1],
            r.held[2],
            r.held[3],
            r.held[4],
            r.held[6],
            r.spent[7],
            r.spent[11],
            c.post_samples,
            c.releases,
            c.uses,
        ]
    }
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        assert_eq!(row(&m), EXPECTED[0]);
        let a = object(&mut m, 1, 1);
        assert_eq!(row(&m), EXPECTED[1]);
        let p = m.discover(s, a, true, false).unwrap();
        assert_eq!(row(&m), EXPECTED[2]);
        let g = m.admit(p, false).unwrap();
        assert_eq!(row(&m), EXPECTED[3]);
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        assert_eq!(row(&m), EXPECTED[4]);
        m.return_read(op, a, 42).unwrap();
        assert_eq!(row(&m), EXPECTED[5]);
        let r = m.release(op, 11).unwrap();
        assert_eq!(row(&m), EXPECTED[6]);
        m.accept(r, 11).unwrap();
        assert_eq!(row(&m), EXPECTED[7]);
        assert_eq!(m.resources().peak[0], 5);
    });
}

#[test]
fn new_grant_cannot_refill_reserved_read_and_cleanup_work() {
    let mut l = limits();
    l.caps[7] = 7;
    simulate(l, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        // Scope + proposal/disposal + child admission already consumed 4 credits.
        // A read needs 4 more: admission, 2 postchecks and cleanup. Only 3 remain.
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
            Err(Reason::Exhausted)
        );
        assert_eq!(m.resources().held[7], 4);
        assert_eq!(
            (
                m.resources().held[0],
                m.resources().held[1],
                m.resources().held[2]
            ),
            (3, 0, 0)
        );
        assert_eq!(m.reencounter(s, a).unwrap(), g);
    });
}

#[test]
fn exact_environment_principal_harness_and_directory_isolation() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        for lineage in [
            Lineage {
                environment: 2,
                ..spec(1, positive()).lineage
            },
            Lineage {
                principal: 8,
                ..spec(1, positive()).lineage
            },
            Lineage {
                harness: 4,
                ..spec(1, positive()).lineage
            },
            Lineage {
                directory: 10,
                ..spec(1, positive()).lineage
            },
        ] {
            let a = m.fixture_object(lineage, 1, true, false).unwrap();
            assert_eq!(m.discover(s, a, true, false), Err(Reason::Ineligible));
        }
        assert_eq!(m.counters().admissions, 0);
    });
}
#[test]
fn live_release_and_acceptance_replay_deliver_once() {
    simulate(limits(), |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = object(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let op = m
            .begin(
                g,
                a,
                Range {
                    start: 0,
                    length: 4,
                    domain_end: 8,
                },
                50,
            )
            .unwrap();
        m.return_read(op, a, 42).unwrap();
        let released = m.release(op, 11).unwrap();
        assert_eq!(m.release(op, 11), Err(Reason::Replay));
        m.accept(released, 11).unwrap();
        assert_eq!(m.accept(released, 11), Err(Reason::Replay));
        assert_eq!((m.counters().releases, m.counters().uses), (1, 1));
    });
}
