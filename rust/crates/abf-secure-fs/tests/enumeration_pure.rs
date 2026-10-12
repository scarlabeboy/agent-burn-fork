//! M2A.2b public API checks that touch no filesystem object: hard limits,
//! budget validation, cancellation, fixed error strings and privacy of
//! diagnostics. They run on every backend (including the fail-closed one).
//! The safe record parser, status and storage engine are tested by the unit
//! tests in `src/enumeration.rs` (byte models, no OS access).

use abf_secure_fs::{
    AccessError, BudgetKind, Cancellation, Coverage, EnumerationBudget, EnumerationError,
    MAX_CANDIDATES, MAX_CONSECUTIVE_INTERRUPTS, MAX_INSPECTED_RECORDS, MAX_READ_ATTEMPTS,
    MAX_RETAINED_NAME_BYTES, MAX_RETAINED_OUTPUT_BYTES, MAX_SCAN_STEPS,
    MAX_TEMPORARY_PAYLOAD_BYTES, PathError, READ_BUFFER_BYTES, StreamEnd,
};

#[test]
fn hard_limits_are_the_reviewed_values() {
    assert_eq!(READ_BUFFER_BYTES, 8192);
    assert_eq!(MAX_INSPECTED_RECORDS, 4096);
    assert_eq!(MAX_CANDIDATES, 512);
    assert_eq!(MAX_RETAINED_NAME_BYTES, 65_536);
    assert_eq!(MAX_READ_ATTEMPTS, 64);
    assert_eq!(MAX_CONSECUTIVE_INTERRUPTS, 2);
    assert_eq!(MAX_SCAN_STEPS, 4160);
    assert_eq!(MAX_RETAINED_OUTPUT_BYTES, 98_304);
    assert_eq!(MAX_TEMPORARY_PAYLOAD_BYTES, 16_384);
}

#[test]
fn budgets_are_validated_and_cannot_exceed_the_maximums() {
    let max = EnumerationBudget::maximum();
    assert_eq!(
        (
            max.inspected_records(),
            max.candidates(),
            max.retained_name_bytes(),
            max.read_attempts()
        ),
        (4096, 512, 65_536, 64)
    );
    assert_eq!(EnumerationBudget::default(), max);
    assert_eq!(EnumerationBudget::new(4096, 512, 65_536, 64), Ok(max));
    let small = EnumerationBudget::new(1, 1, 1, 1).unwrap();
    assert_eq!(small.candidates(), 1);
    for (records, candidates, bytes, attempts) in [
        (0, 0, 0, 0),
        (0, 1, 1, 1),
        (1, 0, 1, 1),
        (1, 1, 0, 1),
        (1, 1, 1, 0),
        (4097, 1, 1, 1),
        (4096, 513, 1, 1),
        (4096, 1, 65_537, 1),
        (4096, 1, 1, 65),
        (10, 11, 1, 1),
        (usize::MAX, usize::MAX, usize::MAX, usize::MAX),
    ] {
        assert_eq!(
            EnumerationBudget::new(records, candidates, bytes, attempts),
            Err(EnumerationError::InvalidBudget),
            "{records} {candidates} {bytes} {attempts}"
        );
    }
}

#[test]
fn cancellation_is_a_shared_irreversible_flag() {
    let cancel = Cancellation::new();
    assert!(!cancel.is_requested());
    std::thread::scope(|scope| {
        scope.spawn(|| cancel.request());
    });
    assert!(cancel.is_requested());
    cancel.request();
    assert!(cancel.is_requested());
}

#[test]
fn errors_display_fixed_codes_only() {
    let cases = [
        (EnumerationError::Cancelled, "enumeration cancelled"),
        (
            EnumerationError::InvalidBudget,
            "invalid enumeration budget",
        ),
        (
            EnumerationError::AllocationRefused,
            "enumeration storage refused",
        ),
        (
            EnumerationError::MalformedDirectoryData,
            "malformed directory data",
        ),
        (
            EnumerationError::DirectoryBindingChanged,
            "directory binding changed",
        ),
        (EnumerationError::OrdinalOutOfRange, "ordinal out of range"),
        (
            EnumerationError::Access(AccessError::RootBindingChanged),
            "root binding changed",
        ),
        (
            EnumerationError::Access(AccessError::Path(PathError::TooLong)),
            "path: too long",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
    assert_eq!(
        EnumerationError::from(AccessError::NotFound),
        EnumerationError::Access(AccessError::NotFound)
    );
}

#[test]
fn status_vocabulary_has_no_complete_state() {
    // The only coverage values; a listing's status is built by the crate.
    for coverage in [Coverage::Exhausted, Coverage::Partial] {
        assert!(!format!("{coverage:?}").contains("Complete"));
    }
    let ends = [
        StreamEnd::Eof,
        StreamEnd::Cancelled,
        StreamEnd::Budget(BudgetKind::InspectedRecords),
        StreamEnd::Budget(BudgetKind::Candidates),
        StreamEnd::Budget(BudgetKind::NameBytes),
        StreamEnd::Budget(BudgetKind::ReadAttempts),
    ];
    assert_eq!(ends.len(), 6);
}

#[test]
fn diagnostics_of_public_values_are_bounded() {
    let text = format!(
        "{:?} {:?}",
        EnumerationBudget::maximum(),
        Cancellation::new()
    );
    assert!(text.len() < 200, "{text}");
    assert!(!text.contains('/'));
}
