//! Pure lexical validation. No filesystem access.

use abf_secure_fs::{
    AccessError, MAX_COMPONENT_BYTES, MAX_RELATIVE_BYTES, MAX_RELATIVE_COMPONENTS, MAX_ROOT_BYTES,
    MAX_ROOT_COMPONENTS, PathError, RelativePath, SecureRoot, check_root_path,
};

fn rel(text: &str) -> Result<(), PathError> {
    RelativePath::new(text).map(|_| ())
}

/// The documented bounds, written independently of the crate constants.
#[test]
fn bounds_match_documented_values() {
    assert_eq!(MAX_ROOT_BYTES, 1024);
    assert_eq!(MAX_ROOT_COMPONENTS, 32);
    assert_eq!(MAX_RELATIVE_BYTES, 1024);
    assert_eq!(MAX_RELATIVE_COMPONENTS, 32);
    assert_eq!(MAX_COMPONENT_BYTES, 255);
}

#[test]
fn ordinary_roots_are_accepted() {
    for root in [
        "/home/user/fixtures",
        "/a",
        "/srv/abf/r\u{e9}sum\u{e9}",
        "/mnt/wsl",
        "/mnt/cc",
        "/home/user/.hidden/x",
        "/home/user/name with spaces",
    ] {
        assert_eq!(check_root_path(root), Ok(()), "{root:?}");
    }
}

#[test]
fn root_rejections() {
    let cases: &[(&str, PathError)] = &[
        ("", PathError::Empty),
        ("relative/root", PathError::NotAbsolute),
        ("~/root", PathError::NotAbsolute),
        ("$HOME/root", PathError::NotAbsolute),
        ("/", PathError::TooBroad),
        ("//", PathError::EmptyComponent),
        ("/a//b", PathError::EmptyComponent),
        ("/a/b/", PathError::EmptyComponent),
        ("/a/./b", PathError::DotComponent),
        ("/.", PathError::DotComponent),
        ("/a/../b", PathError::ParentComponent),
        ("/..", PathError::ParentComponent),
        ("/a/b\0c", PathError::InvalidCharacter),
        ("/a/\u{7}bell", PathError::InvalidCharacter),
        ("/a/new\nline", PathError::InvalidCharacter),
        ("/a/\u{7f}", PathError::InvalidCharacter),
        ("/a/\u{85}next-line", PathError::InvalidCharacter),
        ("C:\\Users\\x", PathError::CrossOsSeparator),
        ("C:/Users/x", PathError::CrossOsSeparator),
        ("\\\\wsl$\\Ubuntu\\home", PathError::CrossOsSeparator),
        ("\\\\wsl.localhost\\Ubuntu", PathError::CrossOsSeparator),
        ("\\\\?\\C:\\x", PathError::CrossOsSeparator),
        ("\\\\server\\share", PathError::CrossOsSeparator),
        ("/a/b\\c", PathError::CrossOsSeparator),
        ("/a/file:stream", PathError::CrossOsSeparator),
        ("/mnt/c", PathError::CrossOsPath),
        ("/mnt/C/Users", PathError::CrossOsPath),
        ("/mnt/z/x", PathError::CrossOsPath),
        ("/proc/self/root", PathError::SystemNamespace),
        ("/proc", PathError::SystemNamespace),
        ("/sys/kernel", PathError::SystemNamespace),
        ("/dev/null", PathError::SystemNamespace),
    ];
    for (root, expected) in cases {
        assert_eq!(check_root_path(root), Err(*expected), "{root:?}");
    }
}

#[test]
fn root_length_boundaries() {
    // Exactly MAX_ROOT_BYTES: "/" + 4 components of 255 bytes, separated
    // by 3 slashes = 1 + 1020 + 3 = 1024.
    let component = "r".repeat(255);
    let exact = format!("/{0}/{0}/{0}/{0}", component);
    assert_eq!(exact.len(), 1024);
    assert_eq!(check_root_path(&exact), Ok(()));

    let over = format!("{exact}x");
    assert_eq!(over.len(), 1025);
    assert_eq!(check_root_path(&over), Err(PathError::TooLong));

    // Component length N and N+1.
    assert_eq!(check_root_path(&format!("/{}", "c".repeat(255))), Ok(()));
    assert_eq!(
        check_root_path(&format!("/{}", "c".repeat(256))),
        Err(PathError::ComponentTooLong)
    );

    // Multi-byte characters count in bytes: 128 two-byte chars = 256 bytes.
    assert_eq!(
        check_root_path(&format!("/{}", "\u{e9}".repeat(128))),
        Err(PathError::ComponentTooLong)
    );
}

#[test]
fn root_depth_boundaries() {
    let exact = "/d".repeat(32);
    assert_eq!(check_root_path(&exact), Ok(()));
    let over = "/d".repeat(33);
    assert_eq!(check_root_path(&over), Err(PathError::TooDeep));
}

#[test]
fn huge_inputs_are_rejected_by_length_without_panic() {
    let huge = "/a".repeat(1 << 20);
    assert_eq!(check_root_path(&huge), Err(PathError::TooLong));
    assert_eq!(rel(&"a/".repeat(1 << 20)).unwrap_err(), PathError::TooLong);
}

#[test]
fn ordinary_relative_paths_are_accepted() {
    for path in [
        "a.abf-fixture",
        "dir/sub/file.jsonl",
        ".hidden",
        "...",
        "a..b",
        "r\u{e9}sum\u{e9}.txt",
        "with space",
    ] {
        assert_eq!(rel(path), Ok(()), "{path:?}");
    }
}

#[test]
fn relative_rejections() {
    let cases: &[(&str, PathError)] = &[
        ("", PathError::Empty),
        ("/abs", PathError::Absolute),
        ("/", PathError::Absolute),
        ("a/", PathError::EmptyComponent),
        ("a//b", PathError::EmptyComponent),
        (".", PathError::DotComponent),
        ("./a", PathError::DotComponent),
        ("a/./b", PathError::DotComponent),
        ("..", PathError::ParentComponent),
        ("../outside", PathError::ParentComponent),
        ("a/../../outside", PathError::ParentComponent),
        ("a/..", PathError::ParentComponent),
        ("a\\b", PathError::CrossOsSeparator),
        ("..\\outside", PathError::CrossOsSeparator),
        ("C:/x", PathError::CrossOsSeparator),
        ("c:x", PathError::CrossOsSeparator),
        ("\\\\server\\share\\x", PathError::CrossOsSeparator),
        ("file:ads", PathError::CrossOsSeparator),
        ("a\0b", PathError::InvalidCharacter),
        ("a\tb", PathError::InvalidCharacter),
        ("a\rb", PathError::InvalidCharacter),
    ];
    for (path, expected) in cases {
        assert_eq!(rel(path), Err(*expected), "{path:?}");
    }
}

#[test]
fn relative_boundaries() {
    let component = "f".repeat(255);
    // 3 × 255 + 200 + 55 + 4 slashes = 1024, every component <= 255.
    let exact = format!(
        "{0}/{0}/{0}/{1}/{2}",
        component,
        "f".repeat(200),
        "f".repeat(55)
    );
    assert_eq!(exact.len(), 1024);
    assert_eq!(rel(&exact), Ok(()));
    assert_eq!(rel(&format!("{exact}f")), Err(PathError::TooLong));

    assert_eq!(rel(&"f".repeat(255)), Ok(()));
    assert_eq!(rel(&"f".repeat(256)), Err(PathError::ComponentTooLong));

    let deep_exact = vec!["d"; 32].join("/");
    assert_eq!(rel(&deep_exact), Ok(()));
    let deep_over = vec!["d"; 33].join("/");
    assert_eq!(rel(&deep_over), Err(PathError::TooDeep));
}

/// Exhaustive small-alphabet sweep: no input panics, and every accepted
/// relative path satisfies the documented invariants.
#[test]
fn exhaustive_small_alphabet_never_panics() {
    let alphabet = ['/', '.', 'a', '\\', ':', '\0', '\u{e9}'];
    let mut accepted = 0usize;
    let mut buf = String::new();
    fn walk(depth: usize, alphabet: &[char], buf: &mut String, accepted: &mut usize) {
        if let Ok(()) = RelativePath::new(buf).map(|_| ()) {
            *accepted += 1;
            assert!(!buf.is_empty());
            assert!(!buf.starts_with('/'));
            assert!(!buf.contains(['\\', ':', '\0']));
            for component in buf.split('/') {
                assert!(!component.is_empty());
                assert_ne!(component, ".");
                assert_ne!(component, "..");
            }
        }
        let root = format!("/{buf}");
        if check_root_path(&root).is_ok() {
            assert!(!root.contains("/../") && !root.ends_with("/.."));
            assert!(!root.contains("//"));
        }
        if depth == 0 {
            return;
        }
        for &c in alphabet {
            buf.push(c);
            walk(depth - 1, alphabet, buf, accepted);
            buf.pop();
        }
    }
    walk(6, &alphabet, &mut buf, &mut accepted);
    assert!(accepted > 0);
}

#[test]
fn diagnostics_never_echo_path_text() {
    let secret = "SECRET-CANARY-7f3a";
    let relative = RelativePath::new(&format!("dir/{secret}.jsonl")).unwrap();
    assert!(!format!("{relative:?}").contains(secret));

    for bad in [
        format!("/{secret}/../x"),
        secret.to_string(),
        format!("/{secret}/\0"),
        format!("C:\\{secret}"),
    ] {
        let error = check_root_path(&bad).unwrap_err();
        assert!(!format!("{error}").contains(secret));
        assert!(!format!("{error:?}").contains(secret));
        let access = SecureRoot::authorize(&bad).unwrap_err();
        assert!(!format!("{access}").contains(secret));
        assert!(!format!("{access:?}").contains(secret));
    }
}

#[test]
fn authorize_rejects_lexically_before_any_os_access() {
    // These never reach the OS: the error is the lexical code.
    for (root, expected) in [
        ("/", PathError::TooBroad),
        ("relative", PathError::NotAbsolute),
        ("/a/../b", PathError::ParentComponent),
        ("/mnt/c/x", PathError::CrossOsPath),
        ("/proc/self/fd", PathError::SystemNamespace),
    ] {
        assert_eq!(
            SecureRoot::authorize(root).unwrap_err(),
            AccessError::Path(expected),
            "{root:?}"
        );
    }
}
