use std::process::Command;

fn output() -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_abf-results-demo"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn section<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .split_once(name)
        .unwrap()
        .1
        .split("\n== ")
        .next()
        .unwrap()
}

#[test]
fn windows_only_is_fresh_without_a_wsl_result() {
    let output = output();
    let text = section(&output, "== WindowsOnly ==");
    assert!(text.contains("selection=WindowsOnly"));
    assert!(text.contains("Windows availability=Fresh freshness=Fresh"));
    assert!(text.contains("environment=fictionalWindows"));
    assert!(text.contains("coverage=CoveredWithinEnumeration { expected: 1 }"));
    assert!(text.contains("SelectedWSL: not selected; no WSL input required"));
    assert!(!text.contains("distro="));
}

#[test]
fn incomplete_selected_wsl_keeps_coverage_separate_from_projection() {
    let output = output();
    let text = section(&output, "== SelectedWSL interrupted ==");
    assert!(text.contains("selection=SelectedWslOnly"));
    assert!(text.contains("Windows: not selected"));
    assert!(text.contains(
        "SelectedWSL environment=fictionalWsl distro=Fictional-Ubuntu-Demo freshness=Unestablished"
    ));
    assert!(text.contains("delivery=Incomplete { after: 4 } cancelled=false"));
    assert!(text.contains("coverage=PartiallyCovered"));
    assert!(text.contains("process=TransportLost run_end=false"));
    assert!(text.contains("projection=admitted selection=SelectedWslOnly"));
}

#[test]
fn both_retains_stale_windows_and_last_good_beside_independent_wsl() {
    let output = output();
    let text = section(&output, "== Both independent ==");
    assert!(text.contains("selection=Both"));
    assert!(text.contains("Windows availability=Stale freshness=Stale"));
    assert!(text.contains("latest_code=Cancelled"));
    assert!(text.contains("last_good: retained separately"));
    assert!(text.contains("process=KilledCancel run_end=false"));
    assert!(text.contains("coverage=CoveredWithinEnumeration { expected: 1 }"));
    assert!(text.contains("projection=admitted selection=WindowsOnly"));
    assert!(text.contains("projection=admitted selection=SelectedWslOnly"));
    assert!(text.contains("freshness=Unestablished"));
}

#[test]
fn unavailable_and_pre_call_refusal_do_not_invent_evidence_or_zero() {
    let output = output();
    let text = section(&output, "== Both unavailable ==");
    assert!(text.contains("Windows availability=Unavailable freshness=Unavailable"));
    assert!(text.contains("latest: absent; no snapshot"));
    assert!(text.contains("last_good: absent"));
    assert!(text.contains("response_error=CancelledBeforeCall; no response or projection"));
    assert!(!text.contains("coverage="));
    assert!(!text.contains("attempt="));
    assert!(!text.contains("=0"));
}

#[test]
fn output_is_deterministic_and_omits_raw_evidence_and_combined_totals() {
    let first = output();
    assert_eq!(first, output());
    assert!(first.contains("Synthetic only; provenance=Unattested."));
    assert!(
        first
            .contains("No combined total, shared catalog authority or overall collection verdict.")
    );
    assert!(first.contains("qualifiers=[]"));
    assert!(first.contains("ceiling=Original"));
    assert!(!first.contains("\"tokens\""));
    assert!(!first.contains("PRIVATE-DEMO-SENTINEL"));
    assert!(!first.contains("C:\\"));
    assert!(first.is_ascii());
}

#[test]
fn collected_file_reaches_existing_results_with_reported_claim_sums() {
    let output = output();
    let text = section(&output, "== Fictional file ==");
    assert!(text.contains("collection=Complete"));
    assert!(text.contains("coverage=CoveredWithinEnumeration { expected: 1 }"));
    assert!(text.contains("input_tokens=ReportedClaimSum(37)"));
    assert!(text.contains("output_tokens=ReportedClaimSum(5)"));
    assert!(text.contains("total_tokens=NoValidClaims"));
    assert!(text.contains("usage=Unsupported"));
    assert!(text.contains("run=file-demo environment=fictionalWindows source=fictionalFiles"));
    assert!(text.contains("file=0 line=2"));
    assert!(text.contains("SyntheticFixture"));
}

#[test]
fn multiple_collected_files_keep_equal_claims_as_distinct_occurrences() {
    let output = output();
    let text = section(&output, "== Fictional multiple files ==");
    assert!(text.contains("coverage=CoveredWithinEnumeration { expected: 2 }"));
    assert!(text.contains("input_tokens=ReportedClaimSum(74)"));
    assert!(text.contains("output_tokens=ReportedClaimSum(10)"));
    assert!(text.contains("stored_occurrences=2"));
}

#[test]
fn unparseable_reasoning_reports_only_field_state_and_length() {
    let output = output();
    let text = section(&output, "== Fictional multiple files ==");
    let sentinel = "ABF130B-REASONING-RAW-SENTINEL";
    assert!(text.contains(&format!(
        "reasoning_tokens=Unparseable {{ byte_length: {} }}; reasoning unsupported by projector",
        sentinel.len()
    )));
    assert!(text.contains("parse=Partial"));
    assert!(text.contains("reasoning_tokens=Absent; reasoning unsupported by projector"));
    assert!(!output.contains(sentinel));
}

#[test]
fn rejected_file_has_codes_and_no_invented_numeric_claim() {
    let output = output();
    let text = section(&output, "== Fictional rejected file ==");
    assert!(text.contains("collection=Incomplete"));
    assert!(text.contains("file=0 status=Rejected(NotSyntheticFixture)"));
    assert!(text.contains("coverage=NotCovered"));
    assert!(text.contains("stored_occurrences=0"));
    assert!(!text.contains("ReportedClaimSum("));
    assert!(!text.contains("availability=Fresh"));
}

#[test]
fn incomplete_coverage_retains_known_claims_and_skipped_entry_code() {
    let output = output();
    let text = section(&output, "== Fictional incomplete coverage ==");
    assert!(text.contains("collection=Incomplete"));
    assert!(text.contains("skip=DepthLimit"));
    assert!(text.contains("coverage=InventoryIncomplete"));
    assert!(text.contains("input_tokens=ReportedClaimSum(37)"));
    assert!(!text.contains("availability=Fresh"));
}

#[test]
fn pending_tail_and_bad_records_never_become_complete_or_zero_usage() {
    let output = output();
    let text = section(&output, "== Fictional partial records ==");
    assert!(text.contains("collection=Complete; traversal/read completeness only"));
    assert!(text.contains("coverage=PartiallyCovered"));
    assert!(text.contains("parse=Partial"));
    assert!(text.contains("record_error=ProhibitedField(Content)"));
    assert!(text.contains("pending_tail_bytes=17"));
    assert!(text.contains("input_tokens=ReportedClaimSum(9)"));
    assert!(text.contains("output_tokens=NoValidClaims"));
    assert!(text.contains("total_tokens=ReportedClaimSum(12)"));
    assert!(text.contains("cache_read_tokens=ReportedClaimSum(3)"));
    assert!(text.contains("cache_write_tokens=ReportedClaimSum(0)"));
    assert!(text.contains("reasoning_tokens=Value(4); reasoning unsupported by projector"));
    assert!(text.contains("rejected: 1"));
    assert!(!text.contains("availability=Fresh"));
    assert!(!output.contains("FICTIONAL-CONTENT-SENTINEL"));
    assert!(!text.contains("ReportedClaimSum(9999)"));
}
