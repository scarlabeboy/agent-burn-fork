//! Payload encoding of every value outcome, through the private serializer. Fictional
//! in-memory input only. These tests live in the module because the serializer is private.

use super::*;
use crate::claude_jsonl::{ClaudeBatch, collect_claude_jsonl_memory};

fn batch(files: &[&[u8]]) -> ClaudeBatch {
    let id = |v: &str| OpaqueId::new(v).unwrap();
    collect_claude_jsonl_memory(
        EnvironmentIdentity::wsl(id("e"), WslDistroName::new("Fictional").unwrap()),
        SourceInstance::new(id("s"), Harness::ClaudeCode),
        &ScanRun::new(id("r"), CaptureTime::NotRecorded),
        files,
        ClaudeReadLimits::default(),
    )
    .unwrap()
}

fn payload(files: &[&[u8]]) -> String {
    payload_json(&batch(files).summary())
}

#[test]
fn demo_payload_is_the_fixed_summary_through_the_private_serializer() {
    assert_eq!(
        fictional_demo_payload_json(),
        payload_json(&fictional_demo_summary())
    );
}

#[test]
fn valid_zero_is_a_sum_of_zero_not_unavailable() {
    let json = payload(&[b"{\"message\":{\"usage\":{\"input_tokens\":0}}}\n"]);
    assert!(
        json.contains(
            r#""input":{"value":{"kind":"sum","amount":"0"},"states":{"valid":1,"validZero":1,"#
        ),
        "{json}"
    );
    assert!(
        json.contains(r#""output":{"value":{"kind":"no-valid-claims"}"#),
        "{json}"
    );
}

#[test]
fn overflow_no_valid_claims_and_full_range_are_encoded_distinctly() {
    let max = u64::MAX;
    let overflow = format!(
        "{{\"message\":{{\"usage\":{{\"input_tokens\":{max}}}}}}}\n{{\"message\":{{\"usage\":{{\"input_tokens\":1}}}}}}\n"
    );
    let json = payload(&[overflow.as_bytes()]);
    assert!(
        json.contains(r#""input":{"value":{"kind":"overflow"}"#),
        "{json}"
    );
    assert!(json.contains(r#""output":{"value":{"kind":"no-valid-claims"}"#));
    // The full u64 range survives as a decimal string.
    let single = format!("{{\"message\":{{\"usage\":{{\"input_tokens\":{max}}}}}}}\n");
    assert!(payload(&[single.as_bytes()]).contains(&format!(r#""amount":"{max}""#)));
}

#[test]
fn no_observations_is_indeterminate_and_carries_no_amount() {
    let json = payload(&[]);
    assert!(json.contains(r#""value":{"kind":"indeterminate","reason":"no-observations"}"#));
    assert!(!json.contains(r#""amount""#));
    assert!(json.contains(r#""provenance":"fictional-fixture""#));
}
