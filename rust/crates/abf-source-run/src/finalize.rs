use crate::reconcile::{Inputs, bump, capacity, narrow, reconcile, verdict};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

fn enumeration(raw: RawEnumeration) -> Result<EnumerationOutcome, FinalizeCode> {
    Ok(match raw {
        RawEnumeration::NotAttempted => EnumerationOutcome::NotAttempted,
        RawEnumeration::ScopeUnavailable(r) => EnumerationOutcome::ScopeUnavailable(r),
        RawEnumeration::RootUnresolved => EnumerationOutcome::RootUnresolved,
        RawEnumeration::RootAbsent => EnumerationOutcome::RootAbsent,
        RawEnumeration::LayoutUnrecognized => EnumerationOutcome::LayoutUnrecognized,
        RawEnumeration::Failed(r) => EnumerationOutcome::Failed(r),
        RawEnumeration::Partial(n, r) => EnumerationOutcome::Partial {
            listed: narrow(n)?,
            reason: r,
        },
        RawEnumeration::Closed(n) => EnumerationOutcome::Closed { listed: narrow(n)? },
    })
}
fn listing(
    payload: &Payload,
    scope: &CatalogScope,
    catalog: &CatalogSnapshot,
) -> Result<(EnumerationOutcome, Vec<SourceRef>), FinalizeCode> {
    let Payload::Enumeration {
        outcome, sources, ..
    } = payload
    else {
        unreachable!()
    };
    capacity(sources.len(), MAX_INSTANCES_PER_SCOPE as usize)?;
    let e = enumeration(*outcome)?;
    let mut refs = Vec::new();
    for s in sources {
        let r = SourceRef::new(s).map_err(|_| FinalizeCode::InvalidSourceRef)?;
        if r.scope_tag() != catalog.scope_tag(scope).expect("validated scope") {
            return Err(FinalizeCode::SourceScopeMismatch);
        }
        if refs.contains(&r) {
            return Err(FinalizeCode::DuplicateSource);
        }
        refs.push(r);
    }
    let expected = match e {
        EnumerationOutcome::Closed { listed } | EnumerationOutcome::Partial { listed, .. } => {
            listed as usize
        }
        _ => 0,
    };
    if expected != sources.len() {
        return Err(FinalizeCode::EnumerationCountMismatch);
    }
    Ok((e, refs))
}
fn owner(source: &str, members: &[FinalizedMember], catalog: &CatalogSnapshot) -> Option<usize> {
    let prefix = source.split_once('-')?.0;
    members
        .iter()
        .position(|m| catalog.scope_tag(&m.scope) == Some(prefix))
}
fn expected(source: &str, members: &[FinalizedMember]) -> Option<(usize, usize)> {
    members.iter().enumerate().find_map(|(m, v)| {
        v.sources
            .iter()
            .position(|s| s.0.as_str() == source)
            .map(|s| (m, s))
    })
}

/// Pure synthetic barrier computation. P-Q is fixed; no production reader or I/O.
pub fn finalize(
    raw: RawExecutionLog,
    catalog: &CatalogSnapshot,
) -> Result<FinalizedExecution, FinalizeCode> {
    finalize_inner(raw, catalog, 0, 0)
}

// The seed is private and used only by the unit boundary witness below.
fn finalize_inner(
    raw: RawExecutionLog,
    catalog: &CatalogSnapshot,
    unexpected_seed: u32,
    unbound_seed: u32,
) -> Result<FinalizedExecution, FinalizeCode> {
    let scopes = &raw.manifest.members;
    capacity(scopes.len(), MAX_MEMBERS)?;
    if scopes.is_empty()
        || scopes.iter().enumerate().any(|(i, s)| {
            catalog.scope_tag(s).is_none()
                || scopes[..i].contains(s)
                || s.environment_kind() != scopes[0].environment_kind()
                || s.environment_ref() != scopes[0].environment_ref()
        })
    {
        return Err(FinalizeCode::InvalidManifest);
    }
    if raw.manifest.retry_of.as_ref() == Some(&raw.manifest.attempt) {
        return Err(FinalizeCode::InvalidManifest);
    }
    if raw.barrier == Barrier::NotDrained
        || (raw.barrier == Barrier::AbandonedWithoutReader && !raw.pre_barrier.is_empty())
    {
        return Err(FinalizeCode::InvalidBarrier);
    }
    capacity(raw.pre_barrier.len(), MAX_FRAMES)?;
    // Representation checks apply even to quarantined deliveries, never truncate.
    for f in &raw.pre_barrier {
        if f.attempt != raw.manifest.attempt {
            return Err(FinalizeCode::ForeignFrame);
        }
        narrow(f.frame_seq)?;
        match &f.payload {
            Payload::Enumeration {
                member,
                outcome,
                sources,
                ..
            } => {
                if *member >= scopes.len() {
                    return Err(FinalizeCode::InvalidManifest);
                }
                enumeration(*outcome)?;
                capacity(sources.len(), MAX_INSTANCES_PER_SCOPE as usize)?;
            }
            Payload::Source {
                source,
                source_seq,
                event,
            } => {
                SourceRef::new(source).map_err(|_| FinalizeCode::InvalidSourceRef)?;
                narrow(*source_seq)?;
                match event {
                    SourceEvent::End { records, .. } => {
                        narrow(*records)?;
                    }
                    SourceEvent::Record { bytes, result } => {
                        capacity(bytes.len(), MAX_RECORD_BYTES)?;
                        if result
                            .retained_bytes()
                            .is_none_or(|n| n > abf_parser::Limits::MAX.result_bytes)
                        {
                            return Err(FinalizeCode::InvalidParserResult);
                        }
                    }
                    _ => {}
                }
            }
            Payload::RunEnd => {}
        }
    }
    let frames = &raw.pre_barrier;
    let mut dispositions = vec![Disposition::Admitted; frames.len()];
    let mut members: Vec<_> = scopes
        .iter()
        .map(|scope| FinalizedMember {
            scope: scope.clone(),
            enumeration: EnumerationOutcome::NotAttempted,
            sources: Vec::new(),
            records: Vec::new(),
            delivery: DeliveryIntegrity::default(),
            second: SecondEnumeration::NotPerformed,
            verdict: CoverageVerdict::NoAttempt,
            cause: VerdictRule::NoAttempt,
            run_end: false,
            attempt: raw.manifest.attempt.clone(),
            finalization: raw.manifest.finalization.clone(),
            process: raw.process,
            retry_of: raw.manifest.retry_of.clone(),
            subset: raw.manifest.subset,
        })
        .collect();
    for m in &mut members {
        m.delivery.unexpected_source = unexpected_seed;
    }
    let mut unbound_unexpected_source_deliveries = unbound_seed;
    if !raw.distribution_matches {
        dispositions.fill(Disposition::Quarantined(
            QuarantineCode::DistributionMismatch,
        ));
        for m in &mut members {
            m.enumeration =
                EnumerationOutcome::ScopeUnavailable(UnavailableReason::DistributionMismatch);
        }
        return Ok(snapshot(raw, members, dispositions, catalog, 0));
    }
    let mut groups: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, f) in frames.iter().enumerate() {
        groups.entry(f.frame_seq as u32).or_default().push(i);
    }
    let conflicts: Vec<_> = groups
        .values()
        .filter(|g| g.iter().any(|&i| frames[i].payload != frames[g[0]].payload))
        .collect();
    let mut initial_conflict = vec![false; members.len()];
    // Establish control admission first; no source authority has yet been granted.
    for g in groups.values() {
        if conflicts.contains(&g) {
            for &i in g {
                dispositions[i] = Disposition::Quarantined(QuarantineCode::SequenceConflict);
                if let Payload::Enumeration { member, second, .. } = &frames[i].payload {
                    if *second {
                        members[*member].delivery.second_enum_conflict = true;
                    } else {
                        initial_conflict[*member] = true;
                    }
                }
            }
        } else if frames[g[0]].payload.source().is_none() {
            for &i in &g[1..] {
                dispositions[i] = Disposition::Quarantined(QuarantineCode::ExactReplay);
            }
            for m in &mut members {
                for _ in &g[1..] {
                    bump(&mut m.delivery.global_replays)?;
                }
            }
        }
    }
    let mut seconds: Vec<Vec<(EnumerationOutcome, Vec<SourceRef>)>> =
        vec![Vec::new(); members.len()];
    let mut initials: Vec<Vec<(EnumerationOutcome, Vec<SourceRef>)>> =
        vec![Vec::new(); members.len()];
    for g in groups.values() {
        let i = g[0];
        if dispositions[i] != Disposition::Admitted {
            continue;
        }
        if let Payload::Enumeration { member, second, .. } = &frames[i].payload {
            let value = listing(&frames[i].payload, &members[*member].scope, catalog)?;
            if *second {
                bump(&mut members[*member].delivery.second_enum_identities)?;
                seconds[*member].push(value);
            } else {
                bump(&mut members[*member].delivery.initial_enum_identities)?;
                initials[*member].push(value);
            }
        }
    }
    for (i, m) in members.iter_mut().enumerate() {
        let (e, refs) = if initial_conflict[i] {
            (
                EnumerationOutcome::Failed(EnumFailure::DeliveryConflict),
                Vec::new(),
            )
        } else if initials[i].len() > 1 {
            (
                EnumerationOutcome::Failed(EnumFailure::AmbiguousInitial),
                Vec::new(),
            )
        } else {
            initials[i]
                .pop()
                .unwrap_or((EnumerationOutcome::NotAttempted, Vec::new()))
        };
        m.enumeration = e;
        m.sources = refs
            .into_iter()
            .map(|s| {
                (
                    s,
                    reconcile(Inputs {
                        claims: Vec::new(),
                        q: BTreeSet::new(),
                        records: 0,
                        act: false,
                        opening: false,
                        conflict: false,
                        reuse: false,
                        run_end: false,
                        process: raw.process,
                    }),
                )
            })
            .collect();
    }
    let mut marked: BTreeSet<String> = BTreeSet::new();
    // D2 source attribution uses the final frozen population, independently of admission.
    for g in &conflicts {
        let first = frames[g[0]].payload.source();
        let attributable = first
            .and_then(|s| expected(s, &members))
            .filter(|_| g.iter().all(|&i| frames[i].payload.source() == first));
        if let Some((m, _)) = attributable {
            bump(&mut members[m].delivery.seq_conflicts)?;
        } else {
            for m in &mut members {
                bump(&mut m.delivery.global_seq_conflicts)?;
            }
        }
        for &i in *g {
            if let Some(s) = frames[i].payload.source()
                && expected(s, &members).is_some()
            {
                marked.insert(s.into());
            }
        }
    }
    // Final source dispositions. D2 retains precedence over unexpected-source quarantine.
    for g in groups.values() {
        if conflicts.contains(&g) {
            continue;
        }
        if let Some(s) = frames[g[0]].payload.source() {
            if let Some((m, _)) = expected(s, &members) {
                for &i in &g[1..] {
                    dispositions[i] = Disposition::Quarantined(QuarantineCode::ExactReplay);
                    bump(&mut members[m].delivery.replays_quarantined)?;
                }
            } else {
                for &i in g {
                    dispositions[i] = Disposition::Quarantined(QuarantineCode::UnexpectedSource);
                }
                if let Some(m) = owner(s, &members, catalog) {
                    for _ in g {
                        bump(&mut members[m].delivery.unexpected_source)?;
                    }
                } else {
                    for _ in g {
                        bump(&mut unbound_unexpected_source_deliveries)?;
                    }
                }
            }
        }
    }
    capacity(
        dispositions
            .iter()
            .filter(|d| matches!(d, Disposition::Quarantined(_)))
            .count(),
        MAX_QUARANTINE,
    )?;
    let run_end = frames
        .iter()
        .zip(&dispositions)
        .any(|(f, d)| *d == Disposition::Admitted && f.payload == Payload::RunEnd);
    for m in &mut members {
        m.run_end = run_end;
        for (source, reconciliation) in &mut m.sources {
            let mut input = Inputs {
                claims: Vec::new(),
                q: BTreeSet::new(),
                records: 0,
                act: false,
                opening: false,
                conflict: marked.contains(source.as_str()),
                reuse: false,
                run_end,
                process: raw.process,
            };
            for (&seq, g) in &groups {
                let i = g[0];
                if dispositions[i] != Disposition::Admitted {
                    continue;
                }
                let Payload::Source {
                    source: s,
                    source_seq,
                    event,
                } = &frames[i].payload
                else {
                    continue;
                };
                if s != source.as_str() {
                    continue;
                }
                let source_seq = *source_seq as u32;
                let claim = match event {
                    SourceEvent::Record { result, .. } => {
                        input.act = true;
                        input.reuse |= !input.q.insert(source_seq);
                        bump(&mut input.records)?;
                        m.records.push(RetainedRecord {
                            source: source.clone(),
                            source_seq,
                            frame_seq: seq,
                            delivery: DeliveryIndex(i as u32),
                            result: result.clone(),
                        });
                        None
                    }
                    SourceEvent::OpenAttempt => {
                        input.opening = true;
                        None
                    }
                    SourceEvent::OpenSuccess => {
                        input.act = true;
                        None
                    }
                    SourceEvent::Skipped(r) => Some(TerminalKind::Skipped(*r)),
                    SourceEvent::OpenFailed(r) => Some(TerminalKind::OpenFailed(*r)),
                    SourceEvent::Vanished => Some(TerminalKind::Vanished),
                    SourceEvent::End { records, change } => Some(TerminalKind::End {
                        records: *records as u32,
                        change: *change,
                    }),
                    SourceEvent::ReadError(r) => Some(TerminalKind::ReadError(*r)),
                };
                if let Some(kind) = claim {
                    input.claims.push(TerminalClaimView {
                        kind,
                        frame_seq: seq,
                        source_seq,
                    });
                }
            }
            *reconciliation = reconcile(input);
            if reconciliation.matched.contains(&8) && !reconciliation.conflict {
                m.delivery.attributable_gaps = m
                    .delivery
                    .attributable_gaps
                    .checked_add(reconciliation.missing)
                    .ok_or(FinalizeCode::CountOverflow)?;
            }
        }
        m.records.sort_by(|a, b| {
            (a.source.as_str(), a.source_seq, a.frame_seq).cmp(&(
                b.source.as_str(),
                b.source_seq,
                b.frame_seq,
            ))
        });
    }
    // Only gaps between observed identities are facts: no invented trailing/leading frame.
    let keys: Vec<_> = groups.keys().copied().collect();
    let mut gaps = 0u32;
    for pair in keys.windows(2) {
        let missing = pair[1].checked_sub(pair[0]).expect("sorted") - 1;
        gaps = gaps
            .checked_add(missing)
            .ok_or(FinalizeCode::CountOverflow)?;
    }
    let explained = members.iter().try_fold(0u32, |n, m| {
        n.checked_add(m.delivery.attributable_gaps)
            .ok_or(FinalizeCode::CountOverflow)
    })?;
    let unexplained = gaps
        .checked_sub(gaps.min(explained))
        .expect("min bounded by gaps");
    for (i, m) in members.iter_mut().enumerate() {
        m.delivery.unattributable_gaps = unexplained;
        let d = m.delivery;
        m.second = if seconds[i].is_empty() && !d.second_enum_conflict {
            SecondEnumeration::NotPerformed
        } else if !m.enumeration.has_set() {
            SecondEnumeration::Unusable(SecondUnusable::NoInitialSet)
        } else if d.second_enum_conflict {
            SecondEnumeration::Unusable(SecondUnusable::DeliveryConflict)
        } else if seconds[i].len() > 1 {
            SecondEnumeration::Unusable(SecondUnusable::Ambiguous)
        } else {
            let (outcome, refs) = seconds[i].pop().expect("one second");
            let listing = match outcome {
                EnumerationOutcome::Closed { listed } => Some(SecondListing::Closed { listed }),
                EnumerationOutcome::Partial { listed, .. } => {
                    Some(SecondListing::Partial { listed })
                }
                _ => None,
            };
            match listing {
                Some(listing) => SecondEnumeration::Performed {
                    listing,
                    new_sources: refs
                        .into_iter()
                        .filter(|s| !m.sources.iter().any(|e| &e.0 == s))
                        .collect(),
                },
                None => SecondEnumeration::Unusable(SecondUnusable::NotListed),
            }
        };
    }
    Ok(snapshot(
        raw,
        members,
        dispositions,
        catalog,
        unbound_unexpected_source_deliveries,
    ))
}

fn snapshot(
    raw: RawExecutionLog,
    members: Vec<FinalizedMember>,
    dispositions: Vec<Disposition>,
    catalog: &CatalogSnapshot,
    unbound_unexpected_source_deliveries: u32,
) -> FinalizedExecution {
    let deliveries = raw
        .pre_barrier
        .iter()
        .map(|f| {
            let retain_source = f
                .payload
                .source()
                .is_none_or(|s| owner(s, &members, catalog).is_some());
            DeliveryEvidence::from_frame(f, retain_source)
        })
        .collect();
    let identities = raw
        .pre_barrier
        .into_iter()
        .zip(&dispositions)
        .filter(|(_, d)| **d == Disposition::Admitted)
        .map(|(f, _)| {
            (
                f.frame_seq as u32,
                crate::evidence::IdentityPayload::from_payload(f.payload),
            )
        })
        .collect();
    let mut execution = FinalizedExecution {
        unbound_unexpected_source_deliveries,
        manifest: raw.manifest,
        members,
        dispositions,
        identities,
        deliveries,
        times: raw.times,
    };
    // Derive once from the final execution-owned authority, after all counters settle.
    let unbound = execution.unbound_unexpected_source_deliveries;
    for m in &mut execution.members {
        (m.verdict, m.cause) = verdict(m, unbound);
    }
    execution
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r101b_seeded_finalization_overflow_returns_no_snapshot() {
        let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v").unwrap();
        let mut builder = CatalogBuilder::new();
        builder.add(scope.clone(), "f1v1").unwrap();
        let catalog = builder.build();
        let raw = RawExecutionLog {
            manifest: ExecutionManifest {
                attempt: AttemptRef::new("a1").unwrap(),
                finalization: FinalizationId::new("z1").unwrap(),
                members: vec![scope],
                retry_of: None,
                subset: RetrySubset::AllExpected,
            },
            pre_barrier: vec![ReaderFrame {
                attempt: AttemptRef::new("a1").unwrap(),
                frame_seq: 1,
                payload: Payload::Source {
                    source: "f1v1-unexpected".into(),
                    source_seq: 1,
                    event: SourceEvent::OpenAttempt,
                },
            }],
            process: ProcessEnd::Crashed,
            barrier: Barrier::Drained,
            distribution_matches: true,
            times: TimeObservations::new(0, 0),
        };
        assert_eq!(
            finalize_inner(raw, &catalog, 4294967295, 0).unwrap_err(),
            FinalizeCode::CountOverflow
        );
    }
    #[test]
    fn r101a_r101b_r101c_r101e_checked_helpers_never_wrap_or_silently_saturate() {
        let mut count = 4294967294;
        assert_eq!(bump(&mut count), Ok(()));
        assert_eq!(count, 4294967295);
        assert_eq!(bump(&mut count), Err(FinalizeCode::CountOverflow));
        assert_eq!(count, 4294967295);
        assert_eq!(narrow(4294967295), Ok(4294967295));
        assert_eq!(narrow(4294967296), Err(FinalizeCode::CountOverflow));
        assert_eq!(4294967295u32.checked_sub(4294967294), Some(1));
        assert_eq!(4294967295u32.checked_add(1), None);
        for limit in [MAX_FRAMES, MAX_QUARANTINE, MAX_INSTANCES_PER_SCOPE as usize] {
            assert_eq!(capacity(limit - 1, limit), Ok(()));
            assert_eq!(capacity(limit, limit), Ok(()));
            assert_eq!(
                capacity(limit + 1, limit),
                Err(FinalizeCode::CapacityExceeded)
            );
        }
    }
    fn ua7_seed_fixture(source: Option<&str>) -> (RawExecutionLog, CatalogSnapshot) {
        let scope = CatalogScope::new(EnvironmentKind::Windows, "w1", "f", "v").unwrap();
        let mut builder = CatalogBuilder::new();
        builder.add(scope.clone(), "f1v1").unwrap();
        let attempt = AttemptRef::new("a1").unwrap();
        let mut frames = vec![ReaderFrame {
            attempt: attempt.clone(),
            frame_seq: 1,
            payload: Payload::Enumeration {
                member: 0,
                second: false,
                outcome: RawEnumeration::Closed(0),
                sources: vec![],
                observed_at: None,
            },
        }];
        if let Some(source) = source {
            frames.push(ReaderFrame {
                attempt: attempt.clone(),
                frame_seq: 2,
                payload: Payload::Source {
                    source: source.into(),
                    source_seq: 1,
                    event: SourceEvent::OpenAttempt,
                },
            });
        }
        (
            RawExecutionLog {
                manifest: ExecutionManifest {
                    attempt,
                    finalization: FinalizationId::new("z1").unwrap(),
                    members: vec![scope],
                    retry_of: None,
                    subset: RetrySubset::AllExpected,
                },
                pre_barrier: frames,
                process: ProcessEnd::ExitedNormally,
                barrier: Barrier::Drained,
                distribution_matches: true,
                times: TimeObservations::new(0, 0),
            },
            builder.build(),
        )
    }
    #[test]
    fn r102m_seeded_unbound_maximum_overflow_and_local_independence() {
        // Seeded arithmetic witnesses are not G4-I24 delivery-conservation witnesses.
        let (r, c) = ua7_seed_fixture(Some("unk9-i1"));
        let x = finalize_inner(r.clone(), &c, 0, 4294967294).unwrap();
        assert_eq!(x.unbound_unexpected_source_deliveries(), 4294967295);
        assert_eq!(x.members()[0].cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(
            finalize_inner(r.clone(), &c, 0, 4294967295).unwrap_err(),
            FinalizeCode::CountOverflow
        );
        let x = finalize_inner(r, &c, 4294967295, 0).unwrap();
        assert_eq!(x.unbound_unexpected_source_deliveries(), 1);
        assert_eq!(x.members()[0].delivery().unexpected_source, 4294967295);
        let (mut r, c) = ua7_seed_fixture(None);
        r.pre_barrier[0].payload = Payload::Enumeration {
            member: 0,
            second: false,
            outcome: RawEnumeration::Closed(1),
            sources: vec!["f1v1-i1".into()],
            observed_at: None,
        };
        let bytes = br#"{"tokens":{"input":100}}"#.to_vec();
        r.pre_barrier.push(ReaderFrame {
            attempt: r.manifest.attempt.clone(),
            frame_seq: 2,
            payload: Payload::Source {
                source: "f1v1-i1".into(),
                source_seq: 1,
                event: SourceEvent::Record {
                    result: abf_parser::parse(&bytes, None, abf_parser::Limits::MAX),
                    bytes,
                },
            },
        });
        r.pre_barrier.push(ReaderFrame {
            attempt: r.manifest.attempt.clone(),
            frame_seq: 3,
            payload: Payload::Source {
                source: "f1v1-i1".into(),
                source_seq: 2,
                event: SourceEvent::End {
                    records: 1,
                    change: ChangeObservation::NoChangeObserved,
                },
            },
        });
        r.pre_barrier.push(ReaderFrame {
            attempt: r.manifest.attempt.clone(),
            frame_seq: 4,
            payload: Payload::RunEnd,
        });
        let x = finalize_inner(r, &c, 0, 4294967295).unwrap();
        assert_eq!(x.unbound_unexpected_source_deliveries(), 4294967295);
        assert_eq!(x.members()[0].cause(), VerdictRule::ExecutionGlobalAnomaly);
        assert_eq!(x.members()[0].records().len(), 1);
        assert_eq!(x.members()[0].sources()[0].1.matched(), &[10]);
        let (r, c) = ua7_seed_fixture(Some("f1v1-i9"));
        let x = finalize_inner(r, &c, 0, 4294967295).unwrap();
        assert_eq!(x.unbound_unexpected_source_deliveries(), 4294967295);
        assert_eq!(x.members()[0].delivery().unexpected_source, 1);
    }
}
