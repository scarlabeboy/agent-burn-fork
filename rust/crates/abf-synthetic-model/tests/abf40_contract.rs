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

#[rustfmt::skip]
#[test]
fn contract_deadline_alone_does_not_release_worker_occupancy() {
    let mut c = caps();
    c[1] = 1; // one worker
    simulate(Limits { caps: c }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let _hung = m.begin(g, a, rng(0), 5).unwrap();
        m.step(5).unwrap(); // deadline passes; the worker never returned
        // D13: "Quota release requires observed accounted cleanup, not elapsed time."
        assert_eq!(m.resources().held[1], 1, "worker credit freed by elapsed time");
    });
}

#[rustfmt::skip]
#[test]
fn contract_dispute_is_not_lost_when_work_is_exhausted() {
    let mut c = caps();
    // approve 1 + discover 2 + admit 1 + begin 4 + release 1 + accept 1 + derive 1 = 11
    c[7] = 11;
    simulate(Limits { caps: c }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        assert_eq!(m.resources().held[7], 11);
        let downgrade = m.downgrade(x, true, true);
        let shown = m.view(d).unwrap().qualification;
        // C03a: a later dispute invalidates dependents before they may be served;
        // C07: exhaustion never causes correctness downgrades.
        assert!(
            downgrade.is_ok() || (shown.disputed && shown.stale),
            "dispute rejected ({downgrade:?}) and view still serves {shown:?}"
        );
    });
}

#[rustfmt::skip]
#[test]
fn contract_computer_counts_each_contributing_observation_once() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s1 = m.approve(spec(1, positive())).unwrap();
        let s2 = m.approve(spec(2, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let b = obj(&mut m, 2, 1);
        let ga = child(&mut m, s1, a);
        let gb = child(&mut m, s2, b);
        let x = capture(&mut m, ga, a, 0);
        let y = capture(&mut m, gb, b, 0);
        let windows = m.derive(&[x]).unwrap();
        let both = m.derive(&[x, y]).unwrap();
        let computer = m.computer(&[windows, both]).unwrap();
        let v = m.view(computer).unwrap();
        // C03: each admitted interval counted once; x is one accepted observation.
        assert_eq!(v.observations, 2, "observation x counted {} times", v.observations - 1);
        assert_eq!(v.ranges.len(), 2);
    });
}

#[rustfmt::skip]
#[test]
fn contract_derive_counts_repeated_handle_once() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x, x, x]).unwrap();
        assert_eq!(m.view(d).unwrap().observations, 1);
    });
}

#[rustfmt::skip]
#[test]
fn contract_computer_requires_current_contributor_rights() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let d = m.derive(&[x]).unwrap();
        m.retire(g).unwrap();
        assert_eq!(m.derive(&[x]), Err(Reason::Retired)); // derive gates
        let before = m.resources().spent[7];
        let c = m.computer(&[d]); // computer does not
        assert!(c.is_err(), "new Computer projection minted after R; work {} -> {}",
            before, m.resources().spent[7]);
    });
}

#[rustfmt::skip]
#[test]
fn contract_each_scope_right_is_independently_required() {
    simulate(Limits { caps: caps() }, |mut m| {
        let a = obj(&mut m, 1, 1);
        let mut no_bytes = spec(1, positive());
        no_bytes.bytes = false;
        let s = m.approve(no_bytes).unwrap();
        let p = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.admit(p, false), Err(Reason::Unauthorized));
        m.retire_scope(s).unwrap();

        let mut no_release = spec(1, positive());
        no_release.release = false;
        let s = m.approve(no_release).unwrap();
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 50).unwrap();
        m.return_read(op, a, 1).unwrap();
        assert_eq!(m.release(op, 11), Err(Reason::Unauthorized));
        m.retire_scope(s).unwrap();

        let mut no_use = spec(1, positive());
        no_use.use_results = false;
        let s = m.approve(no_use).unwrap();
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 50).unwrap();
        m.return_read(op, a, 1).unwrap();
        let r = m.release(op, 11).unwrap();
        assert_eq!(m.accept(r, 11), Err(Reason::Unauthorized));
        assert_eq!(m.counters().uses, 0);
        m.retire_scope(s).unwrap();

        let mut no_meta = spec(1, positive());
        no_meta.metadata = false;
        assert_eq!(m.approve(no_meta), Err(Reason::Unauthorized));
    });
}

#[rustfmt::skip]
#[test]
fn contract_receivers_are_checked_at_accept_use_and_display() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.request_prompt(p, 11, true), Err(Reason::Unauthorized)); // content rx != display rx
        m.discard(p).unwrap();
        let g = child(&mut m, s, a);
        let op = m.begin(g, a, rng(0), 50).unwrap();
        m.return_read(op, a, 1).unwrap();
        let r = m.release(op, 11).unwrap();
        assert_eq!(m.accept(r, 12), Err(Reason::Unauthorized));
        let x = m.accept(r, 11).unwrap();
        let d = m.derive(&[x]).unwrap();
        assert_eq!(m.use_result(d, 12), Err(Reason::Unauthorized));
        assert_eq!(m.counters().uses, 1);
    });
}

#[rustfmt::skip]
#[test]
fn contract_manual_needs_prompt_and_replacement_defaults_deny() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let p = m.discover(s, a, true, false).unwrap();
        assert_eq!(m.admit(p, true), Err(Reason::Unauthorized)); // no user-requested prompt
        m.request_prompt(p, 12, true).unwrap();
        assert_eq!(m.request_prompt(p, 12, true), Err(Reason::Replay));
        m.discard(p).unwrap();
        let mut strict = positive();
        strict.allow_replacement = false;
        let s2 = m.approve(spec(1, strict)).unwrap();
        let r = m.discover(s2, a, true, true).unwrap();
        assert_eq!(m.admit(r, false), Err(Reason::ReplacementDenied));
        let mut auto_off = positive();
        auto_off.automatic = false;
        let s3 = m.approve(spec(1, auto_off)).unwrap();
        let q = m.discover(s3, a, true, false).unwrap();
        assert_eq!(m.admit(q, false), Err(Reason::Unauthorized));
        assert_eq!(m.counters().admissions, 0);
    });
}

#[rustfmt::skip]
#[test]
fn contract_scope_expiry_retires_and_drains() {
    simulate(Limits { caps: caps() }, |mut m| {
        let mut sp = spec(1, positive());
        sp.valid_until = 10;
        let s = m.approve(sp).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        let x = capture(&mut m, g, a, 0);
        let second = obj(&mut m, 1, 2);
        let pending = m.discover(s, second, true, false).unwrap();
        m.step(10).unwrap();
        assert_eq!(m.admit(pending, false), Err(Reason::Retired));
        assert_eq!(m.begin(g, a, rng(0), 50), Err(Reason::Retired));
        assert!(m.observation(x).unwrap().revoked); // history kept, status explicit
        assert_eq!(m.resources().held[0], 0);
        assert_eq!(m.resources().held[3], 0);
        let mut late = spec(1, positive());
        late.valid_until = 10;
        assert_eq!(m.approve(late), Err(Reason::Unauthorized));
    });
}

#[rustfmt::skip]
#[test]
fn contract_binding_mismatch_is_irreversible_at_begin_and_return() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let b = obj(&mut m, 1, 2);
        let g = child(&mut m, s, a);
        assert_eq!(m.begin(g, b, rng(0), 50), Err(Reason::BindingMismatch));
        assert_eq!(m.begin(g, a, rng(0), 50), Err(Reason::Retired));
        let gb = child(&mut m, s, b);
        let op = m.begin(gb, b, rng(0), 50).unwrap();
        assert_eq!(m.return_read(op, a, 9), Err(Reason::BindingMismatch));
        assert_eq!(m.release(op, 11), Err(Reason::Retired));
        assert_eq!(m.begin(gb, b, rng(0), 50), Err(Reason::Retired));
        assert_eq!(m.counters().post_samples, 2); // mandatory pair still sampled
        assert_eq!(m.counters().releases, 0);
        assert_eq!((m.resources().held[1], m.resources().held[2]), (0, 0));
    });
}

#[rustfmt::skip]
#[test]
fn contract_range_worker_and_recovery_preconditions() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s, a);
        for r in [
            Range { start: 0, length: 0, domain_end: 8 },
            Range { start: 0, length: 65_537, domain_end: u32::MAX },
            Range { start: 6, length: 4, domain_end: 8 },
        ] {
            assert_eq!(m.begin(g, a, r, 50), Err(Reason::RangeInvalid));
        }
        m.step(3).unwrap();
        assert_eq!(m.begin(g, a, rng(0), 3), Err(Reason::Fenced));
        assert_eq!(
            m.restart(IndependentEvidence::CompleteDrain, true),
            Err(Reason::RecoveryUnavailable) // not fenced
        );
        let mut no_pressure = positive();
        no_pressure.retirement = false;
        let s2 = m.approve(spec(2, no_pressure)).unwrap();
        let b = obj(&mut m, 2, 1);
        let gb = child(&mut m, s2, b);
        assert_eq!(m.pressure_retire(gb), Err(Reason::Unauthorized));
        assert_eq!(m.counters().reads, 0);
    });
}

#[rustfmt::skip]
#[test]
fn contract_duplicate_live_grant_across_scopes_and_after_retirement() {
    simulate(Limits { caps: caps() }, |mut m| {
        let s1 = m.approve(spec(1, positive())).unwrap();
        let s2 = m.approve(spec(1, positive())).unwrap();
        let a = obj(&mut m, 1, 1);
        let g = child(&mut m, s1, a);
        let p = m.discover(s2, a, true, false).unwrap();
        assert_eq!(m.admit(p, false), Err(Reason::DuplicateLiveObjectGrant));
        m.retire(g).unwrap();
        let fresh = m.admit(p, false).unwrap(); // new immutable epoch, not a revival
        assert_ne!(fresh, g);
        assert_eq!(m.transient(g), Err(Reason::Retired));
    });
}
