// ABF131E - fixed ABF130 stdout, captured from fictional crate-local fixtures only.
// Generating Git HEAD: 47c85fbcdd665ed5d6dc961b40c5dbb4d42f9739
// Command (repository root): cargo run --offline --locked --quiet --manifest-path rust/crates/abf-results-demo/Cargo.toml
// Environment: CARGO_TARGET_DIR=rust/crates/abf-results-demo/target (absolute);
// TMPDIR was a disposable abf131e-* directory inside that target, removed after capture.
// SHA-256 of the UTF-8 string bytes (not this module): 11d70e3068f694e66a1aa35a49ffb50fb1c2b87aa8a0a986da39e5b8b39901e0
// Preserve the captured text verbatim; builds and startup never run the generator.
export const fictionalResultsSnapshot = `ABF130 - fictional file and in-memory demonstration
Synthetic only; provenance=Unattested.
Coverage is limited to fictional enumeration; freshness is logical, not measured age.
No combined total, shared catalog authority or overall collection verdict.

== WindowsOnly ==
selection=WindowsOnly
Windows availability=Fresh freshness=Fresh
  latest_code=Completed
  latest: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 1 } ceiling=Original post=NoneObserved
  last_good: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 1 } ceiling=Original post=NoneObserved
SelectedWSL: not selected; no WSL input required

== SelectedWSL interrupted ==
selection=SelectedWslOnly
Windows: not selected
SelectedWSL environment=fictionalWsl distro=Fictional-Ubuntu-Demo freshness=Unestablished
  delivery=Incomplete { after: 4 } cancelled=false
    environment=fictionalWsl coverage=PartiallyCovered qualifiers=[]
    process=TransportLost run_end=false provenance=Unattested
    projection=admitted selection=SelectedWslOnly; admission is not collection success

== Both independent ==
selection=Both
Windows availability=Stale freshness=Stale
  latest_code=Cancelled
  latest: retained separately
    environment=fictionalWindows coverage=PartiallyCovered qualifiers=[]
    process=KilledCancel run_end=false provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=PartiallyCovered ceiling=Original post=NoneObserved
  last_good: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 1 } ceiling=Original post=NoneObserved
SelectedWSL environment=fictionalWsl distro=Fictional-Ubuntu-Demo freshness=Unestablished
  delivery=Complete cancelled=false
    environment=fictionalWsl coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=SelectedWslOnly; admission is not collection success

== Both unavailable ==
selection=Both
Windows availability=Unavailable freshness=Unavailable
  latest_report: absent; no attempt issued
  latest: absent; no snapshot
  last_good: absent; no snapshot
SelectedWSL environment=fictionalWsl distro=Fictional-Ubuntu-Demo freshness=Unestablished
  response_error=CancelledBeforeCall; no response or projection

== Fictional file ==
selection=WindowsOnly
Windows availability=Fresh freshness=Fresh
  latest_code=Completed
  latest: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[ChangeNotObservable]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 1 } ceiling=Original post=NoneObserved
  last_good: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 1 } qualifiers=[ChangeNotObservable]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 1 } ceiling=Original post=NoneObserved
SelectedWSL: not selected; no WSL input required
  collection=Complete; traversal/read completeness only
  run=file-demo environment=fictionalWindows source=fictionalFiles harness=OpenCode host_match=Simulated
  file=0 status=Read pending_tail_bytes=0
    file=0 line=2 offset=24 length=31
    origin=SyntheticFixture { format_version: 1, host_match: Simulated } parse=Complete reasoning_tokens=Absent; reasoning unsupported by projector
  stored_occurrences=1 input_tokens=ReportedClaimSum(37) output_tokens=ReportedClaimSum(5) total_tokens=NoValidClaims
  cache_read_tokens=NoValidClaims cache_write_tokens=NoValidClaims usage=Unsupported
  outcomes=OutcomeCounts { accepted: 1, qualified: 0, incomplete: 0, rejected: 0 } reported_run_coverage=ReportedComplete
  input_evidence=ComponentEvidence { eligible_records: 1, contributing: 1, zero_valued: 0, field_states: FieldStateCounts { valid_zero: 0, valid_positive: 1, absent: 0, null: 0, wrong_type: 0, out_of_range: 0, unsupported: 0 }, qualifications: Qualifications(0), identity: Unresolved, traversal: CallerReported }

== Fictional multiple files ==
selection=WindowsOnly
Windows availability=Fresh freshness=Fresh
  latest_code=Completed
  latest: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 2 } qualifiers=[ChangeNotObservable]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 2 } ceiling=Original post=NoneObserved
  last_good: retained separately
    environment=fictionalWindows coverage=CoveredWithinEnumeration { expected: 2 } qualifiers=[ChangeNotObservable]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=CoveredWithinEnumeration { expected: 2 } ceiling=Original post=NoneObserved
SelectedWSL: not selected; no WSL input required
  collection=Complete; traversal/read completeness only
  run=file-demo environment=fictionalWindows source=fictionalFiles harness=OpenCode host_match=Simulated
  file=0 status=Read pending_tail_bytes=0
    file=0 line=2 offset=24 length=31
    origin=SyntheticFixture { format_version: 1, host_match: Simulated } parse=Complete reasoning_tokens=Absent; reasoning unsupported by projector
  file=1 status=Read pending_tail_bytes=0
    file=1 line=2 offset=24 length=79
    origin=SyntheticFixture { format_version: 1, host_match: Simulated } parse=Partial reasoning_tokens=Unparseable { byte_length: 30 }; reasoning unsupported by projector
  stored_occurrences=2 input_tokens=ReportedClaimSum(74) output_tokens=ReportedClaimSum(10) total_tokens=NoValidClaims
  cache_read_tokens=NoValidClaims cache_write_tokens=NoValidClaims usage=Unsupported
  outcomes=OutcomeCounts { accepted: 2, qualified: 0, incomplete: 0, rejected: 0 } reported_run_coverage=ReportedComplete
  input_evidence=ComponentEvidence { eligible_records: 2, contributing: 2, zero_valued: 0, field_states: FieldStateCounts { valid_zero: 0, valid_positive: 2, absent: 0, null: 0, wrong_type: 0, out_of_range: 0, unsupported: 0 }, qualifications: Qualifications(0), identity: Unresolved, traversal: CallerReported }

== Fictional rejected file ==
selection=WindowsOnly
Windows availability=Unavailable freshness=Unavailable
  latest_code=Degraded(NotCovered)
  latest: retained separately
    environment=fictionalWindows coverage=NotCovered qualifiers=[]
    process=ExitedNormally run_end=false provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=NotCovered ceiling=Original post=NoneObserved
  last_good: absent; no snapshot
SelectedWSL: not selected; no WSL input required
  collection=Incomplete; traversal/read completeness only
  run=file-demo environment=fictionalWindows source=fictionalFiles harness=OpenCode host_match=Simulated
  file=0 status=Rejected(NotSyntheticFixture) pending_tail_bytes=0
  stored_occurrences=0 input_tokens=NoValidClaims output_tokens=NoValidClaims total_tokens=NoValidClaims
  cache_read_tokens=NoValidClaims cache_write_tokens=NoValidClaims usage=Unsupported
  outcomes=OutcomeCounts { accepted: 0, qualified: 0, incomplete: 0, rejected: 0 } reported_run_coverage=ReportedIncomplete
  input_evidence=ComponentEvidence { eligible_records: 0, contributing: 0, zero_valued: 0, field_states: FieldStateCounts { valid_zero: 0, valid_positive: 0, absent: 0, null: 0, wrong_type: 0, out_of_range: 0, unsupported: 0 }, qualifications: Qualifications(256), identity: Unresolved, traversal: CallerReported }

== Fictional incomplete coverage ==
selection=WindowsOnly
Windows availability=Unavailable freshness=Unavailable
  latest_code=Degraded(InventoryIncomplete)
  latest: retained separately
    environment=fictionalWindows coverage=InventoryIncomplete qualifiers=[ChangeNotObservable]
    process=ExitedNormally run_end=true provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=InventoryIncomplete ceiling=Original post=NoneObserved
  last_good: absent; no snapshot
SelectedWSL: not selected; no WSL input required
  collection=Incomplete; traversal/read completeness only
  run=file-demo environment=fictionalWindows source=fictionalFiles harness=OpenCode host_match=Simulated
  file=0 status=Read pending_tail_bytes=0
    file=0 line=2 offset=24 length=31
    origin=SyntheticFixture { format_version: 1, host_match: Simulated } parse=Complete reasoning_tokens=Absent; reasoning unsupported by projector
  skip=DepthLimit
  stored_occurrences=1 input_tokens=ReportedClaimSum(37) output_tokens=ReportedClaimSum(5) total_tokens=NoValidClaims
  cache_read_tokens=NoValidClaims cache_write_tokens=NoValidClaims usage=Unsupported
  outcomes=OutcomeCounts { accepted: 1, qualified: 0, incomplete: 0, rejected: 0 } reported_run_coverage=ReportedComplete
  input_evidence=ComponentEvidence { eligible_records: 1, contributing: 1, zero_valued: 0, field_states: FieldStateCounts { valid_zero: 0, valid_positive: 1, absent: 0, null: 0, wrong_type: 0, out_of_range: 0, unsupported: 0 }, qualifications: Qualifications(0), identity: Unresolved, traversal: CallerReported }

== Fictional partial records ==
selection=WindowsOnly
Windows availability=Unavailable freshness=Unavailable
  latest_code=Degraded(PartiallyCovered)
  latest: retained separately
    environment=fictionalWindows coverage=PartiallyCovered qualifiers=[]
    process=ExitedNormally run_end=false provenance=Unattested
    projection=admitted selection=WindowsOnly; admission is not collection success
    assessment=PartiallyCovered ceiling=Original post=NoneObserved
  last_good: absent; no snapshot
SelectedWSL: not selected; no WSL input required
  collection=Complete; traversal/read completeness only
  run=file-demo environment=fictionalWindows source=fictionalFiles harness=OpenCode host_match=Simulated
  file=0 status=Read pending_tail_bytes=17
    file=0 line=2 offset=24 length=121
    origin=SyntheticFixture { format_version: 1, host_match: Simulated } parse=Partial reasoning_tokens=Value(4); reasoning unsupported by projector
    file=0 line=3 offset=146 length=52
    record_error=ProhibitedField(Content)
  stored_occurrences=2 input_tokens=ReportedClaimSum(9) output_tokens=NoValidClaims total_tokens=ReportedClaimSum(12)
  cache_read_tokens=ReportedClaimSum(3) cache_write_tokens=ReportedClaimSum(0) usage=Unsupported
  outcomes=OutcomeCounts { accepted: 0, qualified: 1, incomplete: 0, rejected: 1 } reported_run_coverage=ReportedIncomplete
  input_evidence=ComponentEvidence { eligible_records: 1, contributing: 1, zero_valued: 0, field_states: FieldStateCounts { valid_zero: 0, valid_positive: 1, absent: 0, null: 0, wrong_type: 0, out_of_range: 0, unsupported: 0 }, qualifications: Qualifications(274), identity: Unresolved, traversal: CallerReported }
`;
