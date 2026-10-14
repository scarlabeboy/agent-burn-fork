use abf_synthetic_model::{Disposition, Policy, Reason};

// Ground truth from ABF36C D13.3 / ABF38B §2, independent of hint storage.
type OracleRow = (Option<bool>, Option<bool>, Option<bool>, bool, bool, bool);
const ROWS: &[OracleRow] = &[
    (None, None, None, true, false, false),
    (Some(true), Some(true), Some(true), true, false, true),
    (Some(true), Some(false), Some(false), true, true, false),
    (Some(true), Some(false), Some(false), true, false, false),
    (Some(false), Some(true), Some(true), true, true, false),
    (Some(true), Some(false), Some(true), true, false, false),
    (Some(true), Some(true), Some(false), true, true, true),
    (Some(true), Some(true), Some(false), true, false, false),
    (None, Some(false), Some(false), true, true, false),
];

// Applicable code sets derived from D13.3, with no total precedence.
const CODES: &[&[Reason]] = &[
    &[Reason::DispositionUnconfigured],
    &[],
    &[Reason::DispositionConflictDenied],
    &[Reason::DispositionConflictDenied],
    &[Reason::DispositionConflictDenied],
    &[
        Reason::DispositionPolicyIncompatible,
        Reason::DispositionConflictDenied,
    ],
    &[],
    &[Reason::DispositionConflictDenied],
    &[Reason::DispositionUnconfigured, Reason::DispositionDenied],
];

#[test]
fn sealed_disposition_table() {
    for (index, &(p, r, u, present, retained, allow)) in ROWS.iter().enumerate() {
        let policy = Policy {
            present: Disposition(p),
            reencounter: Disposition(r),
            unknown: Disposition(u),
            compatibility_proven: true,
            ..Policy::default()
        };
        let result = policy.dispositions(present, retained);
        assert_eq!(result.is_ok(), allow, "row {p:?}/{r:?}/{u:?}");
        if let Err(codes) = result {
            assert!(!codes.is_empty());
            assert!(codes.iter().all(|code| CODES[index].contains(code)));
        }
    }
}

#[test]
fn r02_accepts_applicable_code_set_without_precedence() {
    let p = Policy {
        present: Disposition(None),
        reencounter: Disposition(Some(false)),
        unknown: Disposition(Some(false)),
        compatibility_proven: true,
        ..Policy::default()
    };
    let codes = p.dispositions(true, true).unwrap_err();
    assert!(
        codes
            .iter()
            .any(|c| [Reason::DispositionUnconfigured, Reason::DispositionDenied].contains(c))
    );
}

#[test]
fn incompatible_and_unproven_configuration_has_fixed_validation_codes() {
    let p = Policy {
        present: Disposition(Some(true)),
        reencounter: Disposition(Some(false)),
        unknown: Disposition(Some(true)),
        compatibility_proven: true,
        ..Policy::default()
    };
    assert!(
        p.dispositions(true, false)
            .unwrap_err()
            .contains(&Reason::DispositionPolicyIncompatible)
    );
    let p = Policy {
        compatibility_proven: false,
        present: Disposition(Some(true)),
        reencounter: Disposition(Some(true)),
        unknown: Disposition(Some(true)),
        ..Policy::default()
    };
    assert!(
        p.dispositions(true, false)
            .unwrap_err()
            .contains(&Reason::DispositionCompatibilityUnproven)
    );
}
