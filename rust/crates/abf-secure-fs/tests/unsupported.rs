//! Unsupported and unavailable paths: compiled capabilities, the fail-closed
//! backend for other targets, and injected `openat2` unavailability and
//! retry exhaustion (test harness: feature `fault-injection` + `--cfg
//! abf_secure_fs_harness`). None of these paths may fall back to a weaker
//! primitive or clear a flag.

use abf_secure_fs::{AccessError, Backend, BackendCapabilities, PathError, SecureRoot};

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
#[test]
fn supported_build_describes_itself_honestly() {
    let caps = BackendCapabilities::compiled();
    assert_eq!(caps.backend, Backend::LinuxX86_64Gnu);
    assert!(caps.metadata_pinning);
    // RESOLVE_BENEATH 8 | NO_SYMLINKS 4 | NO_MAGICLINKS 2 | NO_XDEV 1.
    assert_eq!(caps.required_resolve_mask, 0x0F);
    // M2A.2a: promotion is compiled on this backend; runtime procfs
    // availability is established only by `promote` itself.
    assert!(caps.readable_promotion);
    // M2A.2b: bounded single-directory enumeration is compiled.
    assert!(caps.directory_enumeration);
    assert!(!caps.environment_attested);
}

/// Built with `--cfg abf_secure_fs_force_unsupported` (or on any other
/// target): every authorization fails closed after lexical validation. No
/// pinned or readable file can exist, so promotion is unreachable and is
/// reported as not compiled.
#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
)))]
#[test]
fn unsupported_build_fails_closed() {
    let caps = BackendCapabilities::compiled();
    assert_eq!(caps.backend, Backend::Unavailable);
    assert!(!caps.metadata_pinning);
    assert_eq!(caps.required_resolve_mask, 0);
    assert!(!caps.readable_promotion && !caps.directory_enumeration);
    assert!(!caps.environment_attested);
    assert_eq!(
        SecureRoot::authorize("/home/user/fixtures").unwrap_err(),
        AccessError::SecurePrimitiveUnavailable
    );
    assert_eq!(
        SecureRoot::authorize("/").unwrap_err(),
        AccessError::Path(PathError::TooBroad)
    );
}

/// The M2A.2b facade is uninhabited without a backend: no root exists, so
/// no directory can be pinned, enumerated or listed. Budgets and
/// cancellation still validate (they touch no OS object).
#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
)))]
#[test]
fn unsupported_build_has_no_directory_capability() {
    use abf_secure_fs::{Cancellation, EnumerationBudget, EnumerationError};
    let caps = BackendCapabilities::compiled();
    assert!(!caps.directory_enumeration);
    // The only way to a directory is through an authorized root.
    assert_eq!(
        SecureRoot::authorize("/home/user/fixtures").unwrap_err(),
        AccessError::SecurePrimitiveUnavailable
    );
    assert!(EnumerationBudget::new(1, 1, 1, 1).is_ok());
    assert_eq!(
        EnumerationBudget::new(0, 1, 1, 1),
        Err(EnumerationError::InvalidBudget)
    );
    let cancel = Cancellation::new();
    cancel.request();
    assert!(cancel.is_requested());
}

#[test]
fn lexical_errors_precede_backend_availability() {
    assert_eq!(
        SecureRoot::authorize("C:\\Users\\x").unwrap_err(),
        AccessError::Path(PathError::CrossOsSeparator)
    );
}

#[cfg(all(
    feature = "fault-injection",
    abf_secure_fs_harness,
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
mod injected {
    use super::*;
    use abf_secure_fs::RelativePath;
    use abf_secure_fs::fault_injection::{self as fi, OpenAttempt, errno};
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    const FIXTURE_ENV: &str = "ABF_SECURE_FS_FIXTURE_DIR";
    const DIR_FLAGS: u64 = 0o10_000_000 | 0o200_000 | 0o400_000 | 0o2_000_000;
    const FILE_FLAGS: u64 = 0o10_000_000 | 0o400_000 | 0o2_000_000;
    const FULL_MASK: u64 = 0x0F;
    const ANCHOR_MASK: u64 = 0x07;

    /// A fresh case directory with `root/f.abf-fixture`, beneath the
    /// explicitly configured fixture directory only.
    struct Case {
        dir: PathBuf,
    }

    impl Case {
        fn new(name: &str) -> Self {
            let base = std::env::var_os(FIXTURE_ENV).unwrap_or_else(|| {
                panic!("{FIXTURE_ENV} must name the authorized fixture directory (no fallback)")
            });
            let base = PathBuf::from(base);
            assert!(base.is_absolute());
            assert!(fs::symlink_metadata(&base).unwrap().is_dir());
            let dir = base.join(format!("{name}-{}", std::process::id()));
            fs::create_dir(&dir).expect("case directory must be fresh");
            fs::create_dir(dir.join("root")).unwrap();
            fs::write(dir.join("root/f.abf-fixture"), "abf11-synthetic original\n").unwrap();
            Self { dir }
        }

        fn root_text(&self) -> String {
            self.dir.join("root").to_str().unwrap().to_owned()
        }
    }

    impl Drop for Case {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.dir) {
                eprintln!("cleanup failed for a case directory: {error}");
            }
        }
    }

    /// Checks that every descriptor wrapped on this thread was closed.
    fn assert_no_leak() {
        let counters = fi::fd_counters();
        assert_eq!(
            counters.opened, counters.closed,
            "descriptor leak: {counters:?}"
        );
    }

    fn masks(log: &[OpenAttempt]) -> Vec<(u64, u64, u64, Option<i32>)> {
        log.iter()
            .map(|a| (a.flags, a.mode, a.resolve, a.injected_errno))
            .collect()
    }

    const UNAVAILABLE: [i32; 4] = [errno::ENOSYS, errno::EPERM, errno::EINVAL, errno::E2BIG];

    #[test]
    fn unavailable_openat2_at_anchor_fails_closed_without_retry_or_fallback() {
        for code in UNAVAILABLE {
            fi::reset();
            fi::inject_openat2(&[Some(code)]);
            let error = SecureRoot::authorize("/home/abf11-never-touched").unwrap_err();
            assert_eq!(
                error,
                AccessError::SecurePrimitiveUnavailable,
                "errno {code}"
            );
            // Exactly one attempt, with the full anchor flags; nothing else.
            assert_eq!(
                masks(&fi::take_openat2_log()),
                vec![(DIR_FLAGS, 0, ANCHOR_MASK, Some(code))],
                "errno {code}"
            );
            assert_no_leak();
        }
    }

    #[test]
    fn unavailable_openat2_at_root_closes_the_anchor() {
        let case = Case::new("inj_root_unavailable");
        for code in UNAVAILABLE {
            fi::reset();
            fi::inject_openat2(&[None, Some(code)]);
            let error = SecureRoot::authorize(&case.root_text()).unwrap_err();
            assert_eq!(
                error,
                AccessError::SecurePrimitiveUnavailable,
                "errno {code}"
            );
            assert_eq!(
                masks(&fi::take_openat2_log()),
                vec![
                    (DIR_FLAGS, 0, ANCHOR_MASK, None),
                    (DIR_FLAGS, 0, FULL_MASK, Some(code)),
                ]
            );
            let counters = fi::fd_counters();
            assert_eq!((counters.opened, counters.closed), (1, 1));
        }
    }

    /// Injection at each of the four openat2 calls of a pin: the error is
    /// coded, no extra attempt is made, no flag differs, and every
    /// descriptor already acquired (root, anchor, held file, verifier) is
    /// closed.
    #[test]
    fn unavailable_openat2_at_each_pin_step() {
        let case = Case::new("inj_pin_unavailable");
        let path = RelativePath::new("f.abf-fixture").unwrap();
        let expected_calls = [
            (DIR_FLAGS, FULL_MASK),  // binding check
            (FILE_FLAGS, FULL_MASK), // acquisition
            (FILE_FLAGS, FULL_MASK), // verification
            (DIR_FLAGS, FULL_MASK),  // final binding check
        ];
        for step in 0..expected_calls.len() {
            for code in UNAVAILABLE {
                fi::reset();
                {
                    let root = SecureRoot::authorize(&case.root_text()).unwrap();
                    fi::take_openat2_log();
                    let mut plan = vec![None; step];
                    plan.push(Some(code));
                    fi::inject_openat2(&plan);
                    let error = root.pin_regular(&path).unwrap_err();
                    assert_eq!(error, AccessError::SecurePrimitiveUnavailable);
                    let log = fi::take_openat2_log();
                    assert_eq!(log.len(), step + 1, "step {step} errno {code}");
                    for (attempt, (flags, mask)) in log.iter().zip(expected_calls) {
                        assert_eq!(
                            (attempt.flags, attempt.mode, attempt.resolve),
                            (flags, 0, mask)
                        );
                    }
                    assert_eq!(fi::remaining_injections(), 0);
                }
                assert_no_leak();
            }
        }
    }

    #[test]
    fn retry_exhaustion_is_bounded_and_keeps_flags() {
        let case = Case::new("inj_retry");
        let path = RelativePath::new("f.abf-fixture").unwrap();
        for transient in [errno::EAGAIN, errno::EINTR] {
            fi::reset();
            // Anchor: three transient failures, then exhaustion. A fourth
            // planned entry must remain unconsumed.
            fi::inject_openat2(&[Some(transient), Some(transient), Some(transient), Some(0)]);
            assert_eq!(
                SecureRoot::authorize(&case.root_text()).unwrap_err(),
                AccessError::RetryExhausted
            );
            assert_eq!(fi::remaining_injections(), 1, "at most three attempts");
            let log = fi::take_openat2_log();
            assert_eq!(log.len(), 3);
            assert!(
                log.iter()
                    .all(|a| (a.flags, a.mode, a.resolve) == (DIR_FLAGS, 0, ANCHOR_MASK))
            );
            assert_no_leak();

            // Pin acquisition: exhaustion after the binding check.
            fi::reset();
            {
                let root = SecureRoot::authorize(&case.root_text()).unwrap();
                fi::take_openat2_log();
                fi::inject_openat2(&[None, Some(transient), Some(transient), Some(transient)]);
                assert_eq!(
                    root.pin_regular(&path).unwrap_err(),
                    AccessError::RetryExhausted
                );
                let log = fi::take_openat2_log();
                assert_eq!(log.len(), 4);
                assert!(
                    log[1..]
                        .iter()
                        .all(|a| (a.flags, a.mode, a.resolve) == (FILE_FLAGS, 0, FULL_MASK))
                );
            }
            assert_no_leak();

            // Binding check exhaustion.
            fi::reset();
            {
                let root = SecureRoot::authorize(&case.root_text()).unwrap();
                fi::inject_openat2(&[Some(transient), Some(transient), Some(transient)]);
                assert_eq!(
                    root.check_binding().unwrap_err(),
                    AccessError::RetryExhausted
                );
            }
            assert_no_leak();
        }
    }

    #[test]
    fn transient_errors_within_the_bound_recover_with_identical_flags() {
        let case = Case::new("inj_recover");
        let path = RelativePath::new("f.abf-fixture").unwrap();
        fi::reset();
        {
            fi::inject_openat2(&[Some(errno::EAGAIN), Some(errno::EINTR)]);
            let root = SecureRoot::authorize(&case.root_text()).unwrap();
            let log = fi::take_openat2_log();
            assert_eq!(
                masks(&log),
                vec![
                    (DIR_FLAGS, 0, ANCHOR_MASK, Some(errno::EAGAIN)),
                    (DIR_FLAGS, 0, ANCHOR_MASK, Some(errno::EINTR)),
                    (DIR_FLAGS, 0, ANCHOR_MASK, None),
                    (DIR_FLAGS, 0, FULL_MASK, None),
                ]
            );
            fi::inject_openat2(&[None, Some(errno::EINTR), Some(errno::EAGAIN)]);
            let pinned = root.pin_regular(&path).unwrap();
            assert_eq!(pinned.evidence().link_count(), 1);
            let log = fi::take_openat2_log();
            assert_eq!(log.len(), 6);
            assert!(
                log[1..4]
                    .iter()
                    .all(|a| (a.flags, a.resolve) == (FILE_FLAGS, FULL_MASK))
            );
        }
        assert_no_leak();
    }

    #[test]
    fn unexpected_errno_is_coded_not_retried() {
        let case = Case::new("inj_unexpected");
        fi::reset();
        fi::inject_openat2(&[None, Some(errno::ENOENT)]);
        assert_eq!(
            SecureRoot::authorize(&case.root_text()).unwrap_err(),
            AccessError::NotFound
        );
        fi::reset();
        fi::inject_openat2(&[None, Some(5)]); // EIO
        assert_eq!(
            SecureRoot::authorize(&case.root_text()).unwrap_err(),
            AccessError::UnexpectedOs(5)
        );
        assert_eq!(fi::take_openat2_log().len(), 2);
        assert_no_leak();
    }

    #[test]
    fn non_ext_filesystem_magic_is_refused() {
        let case = Case::new("inj_magic");
        // TMPFS_MAGIC 0x01021994, XFS 0x58465342, and 0.
        for magic in [0x0102_1994_i64, 0x5846_5342, 0] {
            fi::reset();
            fi::inject_filesystem_magic(Some(magic));
            assert_eq!(
                SecureRoot::authorize(&case.root_text()).unwrap_err(),
                AccessError::UnsupportedFilesystem,
                "magic {magic:#x}"
            );
            assert_no_leak();
        }
        fi::reset();
    }

    /// Inspection order: authorize = [anchor, root]; pin = [binding,
    /// candidate, verification, final binding, final candidate]. A zero
    /// device or inode is never accepted as a concrete identity, at any
    /// step.
    #[test]
    fn zero_identity_is_never_accepted() {
        let case = Case::new("inj_zero_identity");
        let path = RelativePath::new("f.abf-fixture").unwrap();
        let file = fs::symlink_metadata(case.dir.join("root/f.abf-fixture")).unwrap();
        let (dev, ino) = (file.dev(), file.ino());
        for zero in [(0, ino), (dev, 0), (0, 0)] {
            for step in 0..2 {
                fi::reset();
                let mut plan = vec![None; step];
                plan.push(Some(zero));
                fi::inject_identity(&plan);
                assert_eq!(
                    SecureRoot::authorize(&case.root_text()).unwrap_err(),
                    AccessError::IdentityUnavailable,
                    "authorize step {step} {zero:?}"
                );
                assert_no_leak();
            }
            for step in 0..5 {
                fi::reset();
                {
                    let root = SecureRoot::authorize(&case.root_text()).unwrap();
                    let mut plan = vec![None; step];
                    plan.push(Some(zero));
                    fi::inject_identity(&plan);
                    assert_eq!(
                        root.pin_regular(&path).unwrap_err(),
                        AccessError::IdentityUnavailable,
                        "pin step {step} {zero:?}"
                    );
                }
                assert_no_leak();
            }
        }
        fi::reset();
    }

    /// M2A.2b harness state is bounded: an over-long `getdents64` plan or a
    /// batch larger than the 8192-byte buffer is refused (panics) instead of
    /// being stored.
    #[test]
    fn getdents_injection_state_is_bounded() {
        use fi::DirentInjection;
        fi::reset();
        let too_long = vec![None; fi::MAX_GETDENTS_PLAN + 1];
        assert!(std::panic::catch_unwind(|| fi::inject_getdents(&too_long)).is_err());
        let too_big = vec![Some(DirentInjection::Batch(vec![0; 8193]))];
        assert!(std::panic::catch_unwind(|| fi::inject_getdents(&too_big)).is_err());
        assert_eq!(fi::remaining_getdents_injections(), 0);
        fi::inject_getdents(&vec![None; fi::MAX_GETDENTS_PLAN]);
        assert_eq!(fi::remaining_getdents_injections(), fi::MAX_GETDENTS_PLAN);
        fi::inject_getdents(&[Some(DirentInjection::Batch(vec![0; 8192]))]);
        assert_eq!(fi::remaining_getdents_injections(), 1);
        fi::reset();
        assert_eq!(fi::remaining_getdents_injections(), 0);
        assert_eq!(fi::getdents_attempts(), 0);
    }

    /// Defense in depth behind RESOLVE_NO_XDEV: a root on another device than
    /// "/", or a candidate on another device than the root, is refused.
    /// Reaching these checks natively would need a mount, which this gate
    /// does not create.
    #[test]
    fn foreign_device_is_refused() {
        let case = Case::new("inj_foreign_device");
        let path = RelativePath::new("f.abf-fixture").unwrap();
        let root_meta = fs::symlink_metadata(case.dir.join("root")).unwrap();
        let file = fs::symlink_metadata(case.dir.join("root/f.abf-fixture")).unwrap();

        fi::reset();
        fi::inject_identity(&[None, Some((root_meta.dev() + 1, root_meta.ino()))]);
        assert_eq!(
            SecureRoot::authorize(&case.root_text()).unwrap_err(),
            AccessError::UnsupportedFilesystem
        );
        assert_no_leak();

        // Candidate at acquisition (step 1), verification (step 2, the same
        // candidate policy) and final inspection (step 4).
        for step in [1usize, 2, 4] {
            fi::reset();
            {
                let root = SecureRoot::authorize(&case.root_text()).unwrap();
                let mut plan = vec![None; step];
                plan.push(Some((file.dev() + 1, file.ino())));
                fi::inject_identity(&plan);
                assert_eq!(
                    root.pin_regular(&path).unwrap_err(),
                    AccessError::ConfinementRefused,
                    "step {step}"
                );
            }
            assert_no_leak();
        }

        // Root binding (step 0 and 3) seen on another device: binding change.
        for step in [0usize, 3] {
            fi::reset();
            {
                let root = SecureRoot::authorize(&case.root_text()).unwrap();
                let mut plan = vec![None; step];
                plan.push(Some((root_meta.dev() + 1, root_meta.ino())));
                fi::inject_identity(&plan);
                assert_eq!(
                    root.pin_regular(&path).unwrap_err(),
                    AccessError::RootBindingChanged,
                    "step {step}"
                );
            }
            assert_no_leak();
        }
        fi::reset();
    }
}
