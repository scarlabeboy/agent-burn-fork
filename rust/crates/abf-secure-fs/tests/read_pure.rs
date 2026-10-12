//! ABF28 research read surface: pure checks of the public two-key API (no
//! filesystem access). Compiled **only** in the deliberate two-key research
//! build on the supported target; every other build compiles this file to
//! nothing (the absence of the API there is checked by compiler-negative
//! controls, not by a runtime test).

#![cfg(all(
    feature = "fault-injection",
    abf_secure_fs_harness,
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]

use abf_secure_fs::AccessError;
use abf_secure_fs::fault_injection as fi;
use abf_secure_fs::research_read::{
    Coherence, HELD_DESCRIPTORS, MAX_CAPTURE_BYTES, MAX_CONSECUTIVE_EINTR_RETRIES,
    MAX_CONTROL_BYTES, MAX_FSTAT_ATTEMPTS, MAX_PREAD_ATTEMPTS, MAX_REQUEST_BYTES,
    MAX_REQUESTED_BYTES, MAX_RETAINED_BYTES, MAX_TOTAL_EINTR_RETRIES, ObservationEnd, ReadBudget,
    ReadBudgetKind, ResearchReadError, ResearchReadPermit, STAGE_CAPACITY_BYTES,
};

/// Independent expected values from the ABF25 numeric contract.
#[test]
fn hard_ceilings_are_the_abf25_contract() {
    assert_eq!(MAX_REQUEST_BYTES, 4096);
    assert_eq!(MAX_CAPTURE_BYTES, 65_536);
    assert_eq!(MAX_REQUESTED_BYTES, 262_144);
    assert_eq!(MAX_PREAD_ATTEMPTS, 64);
    assert_eq!(MAX_CONSECUTIVE_EINTR_RETRIES, 2);
    assert_eq!(MAX_TOTAL_EINTR_RETRIES, 8);
    assert_eq!(MAX_FSTAT_ATTEMPTS, 132);
    assert_eq!(STAGE_CAPACITY_BYTES, 65_536);
    assert_eq!(MAX_CONTROL_BYTES, 4096);
    assert_eq!(MAX_RETAINED_BYTES, 69_632);
    assert_eq!(HELD_DESCRIPTORS, 2);
    assert_eq!(fi::MAX_PREAD_PLAN, 128);
    assert_eq!(fi::MAX_FSTAT_PLAN, 256);
}

/// ABF30: the checked, precharged accounting keeps the exact ceiling
/// relations (independent arithmetic): requested bytes = attempts × request,
/// `fstat` = entry pair + one pair per attempt + the reserved final pair,
/// retained = stage + control; every budget field accepts its ceiling and
/// the value just below it.
#[test]
fn ceiling_relations_and_budget_edges_are_unchanged() {
    assert_eq!(MAX_REQUESTED_BYTES, 64 * 4096);
    assert_eq!(MAX_REQUESTED_BYTES, MAX_PREAD_ATTEMPTS * MAX_REQUEST_BYTES);
    assert_eq!(MAX_FSTAT_ATTEMPTS, 2 + 2 * MAX_PREAD_ATTEMPTS + 2);
    assert_eq!(MAX_RETAINED_BYTES, STAGE_CAPACITY_BYTES + MAX_CONTROL_BYTES);
    assert_eq!(STAGE_CAPACITY_BYTES, MAX_CAPTURE_BYTES);
    let ceilings = [4096, 65_536, 262_144, 64, 2, 8];
    for field in 0..ceilings.len() {
        for value in [ceilings[field], ceilings[field] - 1] {
            let mut args = ceilings;
            args[field] = value;
            assert!(
                ReadBudget::new(args[0], args[1], args[2], args[3], args[4], args[5]).is_ok(),
                "field {field} = {value}"
            );
        }
    }
}

#[test]
fn maximum_budget_is_the_ceiling_and_the_default() {
    let max = ReadBudget::maximum();
    assert_eq!(max, ReadBudget::default());
    assert_eq!(
        (
            max.request_bytes(),
            max.capture_bytes(),
            max.requested_bytes(),
            max.attempts(),
            max.consecutive_eintr_retries(),
            max.total_eintr_retries(),
        ),
        (4096, 65_536, 262_144, 64, 2, 8)
    );
    assert_eq!(ReadBudget::new(4096, 65_536, 262_144, 64, 2, 8), Ok(max));
}

#[test]
fn budgets_can_only_be_lowered_and_never_zero() {
    assert!(ReadBudget::new(1, 1, 1, 1, 1, 1).is_ok());
    let ceilings = [4096, 65_536, 262_144, 64, 2, 8];
    for field in 0..ceilings.len() {
        for value in [0, ceilings[field] + 1, usize::MAX] {
            let mut args = ceilings;
            args[field] = value;
            assert_eq!(
                ReadBudget::new(args[0], args[1], args[2], args[3], args[4], args[5]),
                Err(ResearchReadError::InvalidBudget),
                "field {field} = {value}"
            );
        }
    }
}

#[test]
fn statuses_are_conservative() {
    // The only coherence value: no snapshot is ever claimed.
    assert_eq!(
        format!("{:?}", Coherence::MutableObservation),
        "MutableObservation"
    );
    let ends = [
        ObservationEnd::ObservedEof,
        ObservationEnd::CaptureLimit,
        ObservationEnd::BudgetStop(ReadBudgetKind::Attempts),
        ObservationEnd::BudgetStop(ReadBudgetKind::RequestedBytes),
    ];
    for end in ends {
        let text = format!("{end:?}");
        for word in ["Complete", "Verified", "Snapshot", "Immutable", "Origin"] {
            assert!(!text.contains(word), "{text}");
        }
    }
}

#[test]
fn errors_are_fixed_codes_without_content() {
    let errors = [
        ResearchReadError::InvalidBudget,
        ResearchReadError::Cancelled,
        ResearchReadError::AllocationRefused,
        ResearchReadError::KernelContractViolation,
        ResearchReadError::ReadFailed(AccessError::UnexpectedOs(5)),
        ResearchReadError::WouldBlock,
        ResearchReadError::RetryExhausted,
        ResearchReadError::Metadata(AccessError::ObjectChanged),
        ResearchReadError::InvalidMetadata,
        ResearchReadError::ContentChanged,
    ];
    let displays: Vec<String> = errors.iter().map(ToString::to_string).collect();
    for text in &displays {
        assert!(text.starts_with("research read: "), "{text}");
        assert!(!text.contains('/'), "{text}");
    }
    let mut unique = displays.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), displays.len(), "distinct codes");
    // Errors are small Copy codes: nothing can carry bytes.
    assert!(std::mem::size_of::<ResearchReadError>() <= 16);
}

#[test]
fn permit_minting_is_single_outstanding_per_thread() {
    let permit = fi::mint_research_read_permit().expect("first permit");
    assert_eq!(format!("{permit:?}"), "ResearchReadPermit(..)");
    assert!(fi::mint_research_read_permit().is_none());
    drop(permit);
    let again = fi::mint_research_read_permit().expect("after release");
    drop(again);
    assert_eq!(std::mem::size_of::<ResearchReadPermit>(), 0);
}

// Compile-time proof that the permit is neither `Send` nor `Sync` (it stays
// on its minting thread): if it implemented either trait, both blanket
// impls would apply and the `_` below would be ambiguous.
trait AmbiguousIfSend<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

trait AmbiguousIfSync<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSync<()> for T {}
impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

#[test]
fn permit_is_thread_bound() {
    <ResearchReadPermit as AmbiguousIfSend<_>>::check();
    <ResearchReadPermit as AmbiguousIfSync<_>>::check();
}
