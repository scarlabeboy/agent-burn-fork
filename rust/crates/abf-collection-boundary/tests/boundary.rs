//! Synthetic boundary tests. Every input is created here under a fresh
//! directory inside `std::env::temp_dir()`; run with an isolated `TMPDIR`.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use abf_collection_boundary::*;

const CANARY: &str = "ABF8-CANARY-7f3a";
const HEADER_LEN: u64 = FIXTURE_HEADER.len() as u64 + 1;
const RECORD_A: &str = "message_id=msg_001\trequest_id=req_001\tsession_id=sess-1\t\
model_provider=anthropic\tmodel=synthetic-model-1\tevent_time=2026-05-01T10:00:00.000Z\t\
project_label=demo\tinput_tokens=100\toutput_tokens=20\tcache_read_tokens=5";

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "abf8-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).expect("create fresh test directory");
        Self {
            path: fs::canonicalize(&path).expect("canonicalize test directory"),
        }
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // `remove_dir_all` does not follow symbolic links.
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn fixture(records: &[&str]) -> String {
    let mut text = format!("{FIXTURE_HEADER}\n");
    for record in records {
        text.push_str(record);
        text.push('\n');
    }
    text
}

fn id(value: &str) -> OpaqueId {
    OpaqueId::new(value).expect("valid opaque id")
}

fn wsl_env() -> EnvironmentIdentity {
    EnvironmentIdentity::wsl(id("env-wsl"), WslDistroName::new("Ubuntu").unwrap())
}

fn windows_env() -> EnvironmentIdentity {
    EnvironmentIdentity::windows(id("env-win"))
}

fn claude_source() -> SourceInstance {
    SourceInstance::new(id("src-claude"), Harness::ClaudeCode)
}

fn run(name: &str) -> ScanRun {
    ScanRun::new(id(name), CaptureTime::UnixMicros(1_780_000_000_000_000))
}

fn scope(environment: EnvironmentIdentity, source: SourceInstance, root: &Path) -> CollectionScope {
    let root = AuthorizedRoot::new(root).expect("authorized test root");
    CollectionScope::new(environment, source, root, HostPolicy::AllowSimulated)
        .expect("scope with simulation allowed")
}

fn collect(scope: &CollectionScope, run_id: &str) -> CollectionBatch {
    collect_synthetic_root(scope, &run(run_id)).expect("root collection")
}

fn reasons(report: &FileReport) -> Vec<Option<RecordRejection>> {
    report
        .records
        .iter()
        .map(|record| match record {
            RecordOutcome::Accepted(_) => None,
            RecordOutcome::Rejected(rejected) => Some(rejected.reason),
        })
        .collect()
}

fn accepted(report: &FileReport, index: usize) -> &Observation {
    match &report.records[index] {
        RecordOutcome::Accepted(observation) => observation,
        RecordOutcome::Rejected(rejected) => {
            panic!("record {index} rejected: {:?}", rejected.reason)
        }
    }
}

#[test]
fn environment_identities_are_distinct() {
    let windows = EnvironmentIdentity::windows(id("env-1"));
    let wsl = EnvironmentIdentity::wsl(id("env-1"), WslDistroName::new("Ubuntu").unwrap());
    let other_distro =
        EnvironmentIdentity::wsl(id("env-1"), WslDistroName::new("Ubuntu-24.04").unwrap());

    assert_eq!(windows.kind(), EnvironmentKind::WindowsNative);
    assert_eq!(wsl.kind(), EnvironmentKind::Wsl2);
    assert_ne!(
        windows, wsl,
        "same opaque id must not make Windows and WSL equal"
    );
    assert_ne!(wsl, other_distro);
    assert_eq!(windows.wsl_distro(), None);
    assert_eq!(wsl.wsl_distro().map(WslDistroName::as_str), Some("Ubuntu"));

    for bad in ["", "a b", "../x", "é", &"x".repeat(65)] {
        assert_eq!(
            OpaqueId::new(bad),
            Err(IdentityError::InvalidOpaqueId),
            "{bad:?}"
        );
    }
    for bad in ["", "Ubuntu 22", "a/b", "a\\b"] {
        assert_eq!(
            WslDistroName::new(bad),
            Err(IdentityError::InvalidDistroName),
            "{bad:?}"
        );
    }
}

#[test]
fn same_fixture_in_windows_and_wsl_scopes_stays_separately_attributable() {
    let root = TempDir::new("cross-env");
    root.write("sessions/a.abf-fixture", fixture(&[RECORD_A]));

    let windows = collect(&scope(windows_env(), claude_source(), &root.path), "run-1");
    let wsl = collect(&scope(wsl_env(), claude_source(), &root.path), "run-1");

    let windows_obs: Vec<_> = windows.observations().collect();
    let wsl_obs: Vec<_> = wsl.observations().collect();
    assert_eq!(windows_obs.len(), 1);
    assert_eq!(wsl_obs.len(), 1);

    // Identical bytes, path, claimed ids and source id: still two observations.
    assert_eq!(windows_obs[0].claimed_ids, wsl_obs[0].claimed_ids);
    assert_eq!(windows_obs[0].usage, wsl_obs[0].usage);
    assert_ne!(windows_obs[0].occurrence, wsl_obs[0].occurrence);
    assert_eq!(
        windows_obs[0].occurrence.environment.kind(),
        EnvironmentKind::WindowsNative
    );
    assert_eq!(
        wsl_obs[0].occurrence.environment.kind(),
        EnvironmentKind::Wsl2
    );

    let all: Vec<&Observation> = windows_obs.into_iter().chain(wsl_obs).collect();
    let distinct: HashSet<_> = all.iter().map(|obs| &obs.occurrence).collect();
    assert_eq!(distinct.len(), 2);

    if cfg!(target_os = "linux") {
        assert_eq!(windows.host_match, HostMatch::Simulated);
        assert_eq!(wsl.host_match, HostMatch::Native);
        assert_eq!(
            all[0].origin,
            ObservationOrigin::SyntheticFixture {
                format_version: 1,
                host_match: HostMatch::Simulated
            }
        );
    }
}

#[test]
fn similar_records_are_never_merged() {
    let root = TempDir::new("similar");
    root.write("one.abf-fixture", fixture(&[RECORD_A, RECORD_A]));
    root.write("two.abf-fixture", fixture(&[RECORD_A]));

    let claude = collect(&scope(wsl_env(), claude_source(), &root.path), "run-1");
    let observations: Vec<_> = claude.observations().collect();
    assert_eq!(
        observations.len(),
        3,
        "equal records are three physical occurrences"
    );
    let occurrences: HashSet<_> = observations.iter().map(|obs| &obs.occurrence).collect();
    assert_eq!(occurrences.len(), 3);

    let one = &claude.files[0];
    assert_eq!(one.relative_path, "one.abf-fixture");
    let first = &accepted(one, 0).occurrence;
    let second = &accepted(one, 1).occurrence;
    assert_eq!((first.line_ordinal, first.byte_offset), (2, HEADER_LEN));
    assert_eq!(
        (second.line_ordinal, second.byte_offset),
        (3, HEADER_LEN + RECORD_A.len() as u64 + 1)
    );
    assert_eq!(first.byte_length, RECORD_A.len() as u64);

    // A different source instance over the same bytes is a different source.
    let codex_source = SourceInstance::new(id("src-codex"), Harness::Codex);
    let codex = collect(&scope(wsl_env(), codex_source, &root.path), "run-1");
    let codex_obs: Vec<_> = codex.observations().collect();
    assert_eq!(codex_obs.len(), 3);
    assert_eq!(codex_obs[0].harness(), Harness::Codex);
    assert_ne!(codex_obs[0].occurrence, observations[0].occurrence);

    // A later run does not claim continuity with an earlier one.
    let rerun = collect(&scope(wsl_env(), claude_source(), &root.path), "run-2");
    let rerun_first = rerun.observations().next().unwrap();
    assert_ne!(rerun_first.occurrence, observations[0].occurrence);
    assert_eq!(
        rerun_first.occurrence.byte_offset,
        observations[0].occurrence.byte_offset
    );
}

#[test]
fn root_must_be_explicit_absolute_canonical_directory() {
    let base = TempDir::new("roots");
    let file = base.write("plain-file", "x");
    fs::create_dir(base.join("real")).unwrap();
    fs::create_dir(base.join("real/sub")).unwrap();

    let reject = |path: &Path| AuthorizedRoot::new(path).unwrap_err();
    assert_eq!(reject(Path::new("")), RootRejection::Empty);
    assert_eq!(
        reject(Path::new("relative/dir")),
        RootRejection::NotAbsolute
    );
    assert_eq!(reject(Path::new("~")), RootRejection::NotAbsolute);
    assert_eq!(reject(Path::new("~/.claude")), RootRejection::NotAbsolute);
    assert_eq!(reject(Path::new("$HOME/x")), RootRejection::NotAbsolute);
    assert_eq!(reject(Path::new("/")), RootRejection::TooBroad);
    assert_eq!(
        reject(Path::new("/mnt/c/Users")),
        RootRejection::CrossOsPath
    );
    assert_eq!(
        reject(&base.join("real/../real")),
        RootRejection::RelativeComponent
    );
    assert_eq!(
        reject(Path::new(&format!("{}/", base.path.display()))),
        RootRejection::EmptyComponent
    );
    assert_eq!(
        reject(&base.join("missing")),
        RootRejection::Io(std::io::ErrorKind::NotFound)
    );
    assert_eq!(reject(&file), RootRejection::NotADirectory);
    assert!(AuthorizedRoot::new(&base.join("real")).is_ok());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
        assert_eq!(reject(&base.join("link")), RootRejection::Symlink);
        assert_eq!(reject(&base.join("link/sub")), RootRejection::NotCanonical);
    }
}

#[test]
fn root_text_rules_are_conservative_for_both_hosts() {
    let windows = |text: &str| check_root_text(text, HostFamily::Windows);
    assert_eq!(windows(r"C:\Users\dev\abf-fixtures"), Ok(()));
    assert_eq!(windows("D:/abf/fixtures"), Ok(()));
    for cross_os in [
        r"\\wsl$\Ubuntu\home\dev",
        r"\\wsl.localhost\Ubuntu\home",
        "//wsl$/Ubuntu/x",
        r"\\WSL$\Ubuntu",
    ] {
        assert_eq!(
            windows(cross_os),
            Err(RootRejection::CrossOsPath),
            "{cross_os}"
        );
    }
    for unc in [r"\\server\share\x", r"\\?\C:\x", r"\\.\PhysicalDrive0"] {
        assert_eq!(windows(unc), Err(RootRejection::UncOrDevicePath), "{unc}");
    }
    assert_eq!(windows(r"C:\"), Err(RootRejection::TooBroad));
    for relative in ["C:", "C:foo", r"foo\bar", r"%USERPROFILE%\x", "~"] {
        assert_eq!(
            windows(relative),
            Err(RootRejection::NotAbsolute),
            "{relative}"
        );
    }
    assert_eq!(windows(r"C:\a\..\b"), Err(RootRejection::RelativeComponent));
    assert_eq!(windows(r"C:\a\\b"), Err(RootRejection::EmptyComponent));
    assert_eq!(
        windows(r"C:\a\b:stream"),
        Err(RootRejection::InvalidCharacter)
    );
    assert_eq!(
        windows(r"C:\a\trailing."),
        Err(RootRejection::InvalidCharacter)
    );
    for reserved in [
        r"C:\a\CON\b",
        r"C:\a\nul.txt",
        r"C:\a\COM1",
        r"C:\a\lpt9.log",
    ] {
        assert_eq!(
            windows(reserved),
            Err(RootRejection::ReservedName),
            "{reserved}"
        );
    }
    assert_eq!(windows(r"C:\a\COM0"), Ok(()));

    let unix = |text: &str| check_root_text(text, HostFamily::Unix);
    assert_eq!(unix("/home/dev/abf-fixtures"), Ok(()));
    assert_eq!(unix("/mnt/wsl/shared"), Ok(()));
    assert_eq!(unix("/mnt/cdrom"), Ok(()));
    assert_eq!(unix("/mnt/c"), Err(RootRejection::CrossOsPath));
    assert_eq!(unix("/mnt/D/data"), Err(RootRejection::CrossOsPath));
    assert_eq!(unix(r"/home/a\b"), Err(RootRejection::InvalidCharacter));
    assert_eq!(unix("/home/./x"), Err(RootRejection::RelativeComponent));
}

#[test]
fn relative_paths_cannot_escape_the_root() {
    let base = TempDir::new("escape");
    base.write("outside.abf-fixture", fixture(&[RECORD_A]));
    base.write("root/a/b.abf-fixture", fixture(&[RECORD_A]));
    let scope = scope(wsl_env(), claude_source(), &base.join("root"));
    let run = run("run-1");
    let attempt = |relative: &str| collect_synthetic_file(&scope, &run, relative).map(|_| ());

    assert_eq!(attempt("a/b.abf-fixture"), Ok(()));
    let cases = [
        ("", PathRejection::Empty),
        ("/etc/passwd", PathRejection::Absolute),
        (r"\x.abf-fixture", PathRejection::Absolute),
        ("C:/x.abf-fixture", PathRejection::Absolute),
        ("../outside.abf-fixture", PathRejection::Traversal),
        ("a/../../outside.abf-fixture", PathRejection::Traversal),
        ("./a/b.abf-fixture", PathRejection::Traversal),
        ("a//b.abf-fixture", PathRejection::EmptyComponent),
        (r"a\..\b.abf-fixture", PathRejection::InvalidCharacter),
        ("a/b.abf-fixture/", PathRejection::EmptyComponent),
        ("a/b.jsonl", PathRejection::NotFixtureExtension),
        (".abf-fixture", PathRejection::NotFixtureExtension),
        (
            "missing.abf-fixture",
            PathRejection::Io(std::io::ErrorKind::NotFound),
        ),
        (
            "a/b.abf-fixture/c.abf-fixture",
            PathRejection::NotADirectoryComponent,
        ),
    ];
    for (relative, expected) in cases {
        assert_eq!(attempt(relative), Err(expected), "{relative:?}");
    }
    let deep = format!("{}x.abf-fixture", "d/".repeat(16));
    assert_eq!(attempt(&deep), Err(PathRejection::TooDeep));
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    let base = TempDir::new("symlink");
    base.write(
        "outside/secret.abf-fixture",
        fixture(&["session_id=outside-marker\tinput_tokens=999"]),
    );
    base.write("root/real/ok.abf-fixture", fixture(&[RECORD_A]));
    let root = base.join("root");
    std::os::unix::fs::symlink(
        base.join("outside/secret.abf-fixture"),
        root.join("link.abf-fixture"),
    )
    .unwrap();
    std::os::unix::fs::symlink(base.join("outside"), root.join("linkdir")).unwrap();
    std::os::unix::fs::symlink(base.join("outside"), root.join("real/up")).unwrap();

    let scope = scope(wsl_env(), claude_source(), &root);
    let run = run("run-1");
    assert_eq!(
        collect_synthetic_file(&scope, &run, "link.abf-fixture").map(|_| ()),
        Err(PathRejection::Symlink)
    );
    assert_eq!(
        collect_synthetic_file(&scope, &run, "linkdir/secret.abf-fixture").map(|_| ()),
        Err(PathRejection::SymlinkComponent)
    );

    let batch = collect_synthetic_root(&scope, &run).unwrap();
    let observations: Vec<_> = batch.observations().collect();
    assert_eq!(observations.len(), 1);
    assert_eq!(
        observations[0].occurrence.relative_path,
        "real/ok.abf-fixture"
    );
    assert!(!format!("{batch:?}").contains("outside-marker"));
    let symlinks: Vec<_> = batch
        .skipped
        .iter()
        .filter(|entry| entry.reason == SkipReason::Symlink)
        .filter_map(|entry| entry.relative_path.as_deref())
        .collect();
    assert_eq!(symlinks, ["link.abf-fixture", "linkdir", "real/up"]);
    assert_eq!(
        batch.completeness,
        Completeness::Incomplete,
        "skipped links may hide data"
    );
}

#[cfg(unix)]
#[test]
fn hard_links_and_non_regular_files_are_refused() {
    let base = TempDir::new("special");
    let outside = base.write("outside.abf-fixture", fixture(&[RECORD_A]));
    let root = base.join("root");
    fs::create_dir(&root).unwrap();
    fs::hard_link(&outside, root.join("hl.abf-fixture")).unwrap();
    fs::create_dir(root.join("dir.abf-fixture")).unwrap();
    let _socket = std::os::unix::net::UnixListener::bind(root.join("sock.abf-fixture")).unwrap();

    let scope = scope(wsl_env(), claude_source(), &root);
    let run = run("run-1");
    let attempt = |relative: &str| collect_synthetic_file(&scope, &run, relative).map(|_| ());
    assert_eq!(attempt("hl.abf-fixture"), Err(PathRejection::HardLinked));
    assert_eq!(
        attempt("dir.abf-fixture"),
        Err(PathRejection::NotRegularFile)
    );
    assert_eq!(
        attempt("sock.abf-fixture"),
        Err(PathRejection::NotRegularFile)
    );

    let batch = collect_synthetic_root(&scope, &run).unwrap();
    assert_eq!(batch.observations().count(), 0);
    assert!(was_skipped(
        &batch,
        "hl.abf-fixture",
        SkipReason::Path(PathRejection::HardLinked)
    ));
    assert!(was_skipped(
        &batch,
        "sock.abf-fixture",
        SkipReason::NotRegularFile
    ));
    assert_eq!(batch.completeness, Completeness::Incomplete);
}

#[cfg(unix)]
fn was_skipped(batch: &CollectionBatch, relative: &str, reason: SkipReason) -> bool {
    batch
        .skipped
        .iter()
        .any(|entry| entry.relative_path.as_deref() == Some(relative) && entry.reason == reason)
}

#[test]
fn non_fixture_files_are_not_parsed() {
    let root = TempDir::new("non-fixture");
    root.write("real-looking.jsonl", fixture(&[RECORD_A]));
    root.write(
        "harness-like.abf-fixture",
        format!("{{\"type\":\"assistant\",\"message\":{{\"content\":\"{CANARY}\"}}}}\n"),
    );
    root.write("empty.abf-fixture", "");
    root.write("header-only.abf-fixture", FIXTURE_HEADER);
    root.write(
        "crlf.abf-fixture",
        format!("{FIXTURE_HEADER}\r\n{RECORD_A}\r\n"),
    );

    let batch = collect(&scope(wsl_env(), claude_source(), &root.path), "run-1");
    assert_eq!(batch.observations().count(), 0);
    for file in &batch.files {
        assert_eq!(
            file.status,
            FileStatus::Rejected(FileRejection::NotSyntheticFixture),
            "{}",
            file.relative_path
        );
        assert!(file.records.is_empty());
    }
    assert_eq!(batch.files.len(), 4);
    assert_eq!(batch.skipped.len(), 1);
    assert_eq!(batch.skipped[0].reason, SkipReason::NotFixtureExtension);
    assert_eq!(batch.completeness, Completeness::Incomplete);
    assert!(!format!("{batch:?}").contains(CANARY));
}

#[test]
fn invalid_or_missing_metadata_is_explicit() {
    let root = TempDir::new("metadata");
    let mut text = fixture(&[
        "input_tokens=5",
        "model=bad model\tinput_tokens=1",
        "input_tokens=-1\toutput_tokens=+1\tcache_read_tokens=99999999999999999999999",
        "event_time=yesterday\tinput_tokens=1",
        "",
        "input_tokens",
        "input_tokens=1\tinput_tokens=2",
        "input_tokens=",
        "Input_Tokens=1",
        "provider=x\tinput_tokens=1",
        "model=synthetic-model-1",
        "input_tokens=1\r",
    ])
    .into_bytes();
    text.extend_from_slice(b"input_tokens=1\tmodel=\xff\xfe\n");
    text.extend_from_slice(b"input_tokens=7");
    root.write("m.abf-fixture", &text);

    let batch = collect(&scope(wsl_env(), claude_source(), &root.path), "run-1");
    let report = &batch.files[0];
    assert_eq!(report.status, FileStatus::Read);
    assert_eq!(report.pending_tail_bytes, "input_tokens=7".len() as u64);
    assert_eq!(
        reasons(report),
        [
            None,
            None,
            None,
            None,
            Some(RecordRejection::EmptyRecord),
            Some(RecordRejection::MalformedPair),
            Some(RecordRejection::DuplicateField("input_tokens")),
            Some(RecordRejection::EmptyValue),
            Some(RecordRejection::InvalidKey),
            Some(RecordRejection::UnknownField),
            Some(RecordRejection::NoUsageComponents),
            Some(RecordRejection::ControlCharacter),
            Some(RecordRejection::InvalidUtf8),
        ]
    );
    let ordinals: Vec<u64> = report
        .records
        .iter()
        .map(|r| r.occurrence().line_ordinal)
        .collect();
    assert_eq!(
        ordinals,
        (2..=14).collect::<Vec<_>>(),
        "every physical line is accounted for"
    );

    let minimal = accepted(report, 0);
    assert_eq!(minimal.parse_status, ParseStatus::Complete);
    assert_eq!(minimal.usage.input, Field::Value(5));
    assert_eq!(minimal.usage.output, Field::Absent);
    assert_eq!(minimal.usage.total_reported, Field::Absent);
    assert_eq!(minimal.claimed_ids.message_id, Field::Absent);
    assert_eq!(minimal.model, Field::Absent);
    assert_eq!(minimal.model_provider, Field::Absent);
    assert_eq!(minimal.event_time_raw, Field::Absent);
    assert_eq!(minimal.account_scope, AccountScope::NotCollected);

    let bad_model = accepted(report, 1);
    assert_eq!(bad_model.parse_status, ParseStatus::Partial);
    assert_eq!(bad_model.model, Field::Unparseable { byte_length: 9 });

    let bad_counts = accepted(report, 2);
    assert_eq!(
        bad_counts.usage.input,
        Field::Unparseable { byte_length: 2 }
    );
    assert_eq!(
        bad_counts.usage.output,
        Field::Unparseable { byte_length: 2 }
    );
    assert_eq!(
        bad_counts.usage.cache_read,
        Field::Unparseable { byte_length: 23 }
    );

    assert_eq!(
        accepted(report, 3).event_time_raw,
        Field::Unparseable { byte_length: 9 }
    );
    assert_eq!(
        batch.completeness,
        Completeness::Complete,
        "rejected records are explicit, not missing"
    );
}

#[test]
fn prohibited_content_and_credentials_never_reach_output() {
    let root = TempDir::new("privacy");
    let records = [
        (
            format!("prompt={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("content={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("tool_output={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("message_text={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("reasoning_text={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("PROMPT={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("input_tokens=1\tcwd={CANARY}"),
            Some(ProhibitedClass::Content),
        ),
        (
            format!("access_token={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Credential),
        ),
        (
            format!("refresh_token={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Credential),
        ),
        (
            format!("api_key={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Credential),
        ),
        (
            format!("oauth_state={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Credential),
        ),
        (
            format!("session_token={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::Credential),
        ),
        (
            format!("account_uuid={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::AccountIdentity),
        ),
        (
            format!("email={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::AccountIdentity),
        ),
        (
            format!("cost_usd={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::PricingOrQuota),
        ),
        (
            format!("quota_remaining={CANARY}\tinput_tokens=1"),
            Some(ProhibitedClass::PricingOrQuota),
        ),
        (
            format!("Bad-Key=1\tprompt={CANARY}"),
            Some(ProhibitedClass::Content),
        ),
        (format!("x_custom={CANARY}\tinput_tokens=1"), None),
        (format!("model={CANARY} with space\tinput_tokens=1"), None),
        (format!("event_time={CANARY}\tinput_tokens=1"), None),
        (
            format!("project_label={CANARY}/../{{x}}\tinput_tokens=1"),
            None,
        ),
    ];
    let lines: Vec<&str> = records.iter().map(|(line, _)| line.as_str()).collect();
    root.write("p.abf-fixture", fixture(&lines));

    let batch = collect(&scope(wsl_env(), claude_source(), &root.path), "run-1");
    let report = &batch.files[0];
    for (index, (_, class)) in records.iter().enumerate() {
        if let Some(class) = class {
            assert_eq!(
                reasons(report)[index],
                Some(RecordRejection::ProhibitedField(*class)),
                "record {index}"
            );
        }
    }
    assert_eq!(reasons(report)[17], Some(RecordRejection::UnknownField));
    assert!(accepted(report, 18).model.is_unparseable());
    assert!(accepted(report, 19).event_time_raw.is_unparseable());
    assert!(accepted(report, 20).project_label.is_unparseable());

    let rendered = format!("{batch:?}");
    assert!(
        !rendered.contains(CANARY),
        "canary leaked into boundary output"
    );
    assert!(
        !rendered
            .to_ascii_lowercase()
            .contains(&CANARY.to_ascii_lowercase())
    );
}

#[test]
fn no_implicit_user_directory_or_process_access_in_source() {
    let sources = [
        ("lib.rs", include_str!("../src/lib.rs")),
        ("claude_jsonl.rs", include_str!("../src/claude_jsonl.rs")),
        ("identity.rs", include_str!("../src/identity.rs")),
        ("observation.rs", include_str!("../src/observation.rs")),
        ("scope.rs", include_str!("../src/scope.rs")),
        ("synthetic.rs", include_str!("../src/synthetic.rs")),
    ];
    let forbidden = [
        "env::var",
        "var_os",
        "home_dir",
        "temp_dir",
        "current_dir",
        "\"HOME\"",
        "USERPROFILE",
        "APPDATA",
        "XDG_",
        ".claude",
        ".codex",
        ".config/opencode",
        "agent-burn",
        "std::process",
        "Command::new",
        "std::net",
        "TcpStream",
        "UdpSocket",
        "wsl.exe",
        "unsafe ",
        "extern \"C\"",
    ];
    for (name, source) in sources {
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{name} contains forbidden pattern {pattern:?}"
            );
        }
    }
}

#[test]
fn repeated_collection_is_deterministic_with_provenance() {
    let root = TempDir::new("determinism");
    root.write("b/z.abf-fixture", fixture(&[RECORD_A]));
    root.write("a/y.abf-fixture", fixture(&["input_tokens=1", RECORD_A]));
    root.write("a.abf-fixture", fixture(&[RECORD_A]));
    let scope = scope(wsl_env(), claude_source(), &root.path);

    let first = collect(&scope, "run-1");
    let second = collect(&scope, "run-1");
    assert_eq!(first, second);
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(first.completeness, Completeness::Complete);
    let paths: Vec<_> = first
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["a.abf-fixture", "a/y.abf-fixture", "b/z.abf-fixture"]
    );

    let observation = accepted(&first.files[1], 1);
    let occurrence = &observation.occurrence;
    assert_eq!(occurrence.scan_run.as_str(), "run-1");
    assert_eq!(occurrence.environment, wsl_env());
    assert_eq!(occurrence.source.id().as_str(), "src-claude");
    assert_eq!(observation.harness(), Harness::ClaudeCode);
    assert_eq!(occurrence.relative_path, "a/y.abf-fixture");
    assert_eq!(occurrence.line_ordinal, 3);
    assert_eq!(
        occurrence.byte_offset,
        HEADER_LEN + "input_tokens=1\n".len() as u64
    );
    assert_eq!(occurrence.byte_length, RECORD_A.len() as u64);
    assert_eq!(
        observation.capture_time,
        CaptureTime::UnixMicros(1_780_000_000_000_000)
    );
    assert_eq!(
        observation.claimed_ids.message_id,
        Field::Value("msg_001".to_owned())
    );
    assert_eq!(
        observation.claimed_ids.request_id,
        Field::Value("req_001".to_owned())
    );
    assert_eq!(
        observation.claimed_ids.session_id,
        Field::Value("sess-1".to_owned())
    );
    assert_eq!(
        observation.model_provider,
        Field::Value("anthropic".to_owned())
    );
    assert_eq!(
        observation.model,
        Field::Value("synthetic-model-1".to_owned())
    );
    assert_eq!(
        observation.event_time_raw,
        Field::Value("2026-05-01T10:00:00.000Z".to_owned())
    );
    assert_eq!(observation.project_label, Field::Value("demo".to_owned()));
    assert_eq!(observation.usage.input, Field::Value(100));
    assert_eq!(observation.usage.output, Field::Value(20));
    assert_eq!(observation.usage.cache_read, Field::Value(5));
    assert_eq!(observation.usage.cache_write, Field::Absent);
    assert_eq!(observation.usage.reasoning, Field::Absent);
    #[cfg(unix)]
    assert!(matches!(occurrence.file_object, FileObjectRef::Unix { ino, .. } if ino != 0));
}

#[cfg(target_os = "linux")]
#[test]
fn host_policy_is_explicit() {
    let root = TempDir::new("host");
    let authorized = || AuthorizedRoot::new(&root.path).unwrap();
    assert_eq!(
        CollectionScope::new(
            windows_env(),
            claude_source(),
            authorized(),
            HostPolicy::RequireNative
        )
        .unwrap_err(),
        ScopeError::EnvironmentHostMismatch
    );
    let native = CollectionScope::new(
        wsl_env(),
        claude_source(),
        authorized(),
        HostPolicy::RequireNative,
    )
    .unwrap();
    assert_eq!(native.host_match(), HostMatch::Native);
    let simulated = CollectionScope::new(
        windows_env(),
        claude_source(),
        authorized(),
        HostPolicy::AllowSimulated,
    )
    .unwrap();
    assert_eq!(simulated.host_match(), HostMatch::Simulated);
}

fn limited(root: &Path, limits: CollectionLimits) -> CollectionScope {
    scope(wsl_env(), claude_source(), root)
        .with_limits(limits)
        .expect("valid limits")
}

#[test]
fn limits_bound_reads_and_mark_batches_incomplete() {
    let root = TempDir::new("limits");
    root.write("big.abf-fixture", fixture(&[RECORD_A]));
    let small = limited(
        &root.path,
        CollectionLimits {
            max_file_bytes: 64,
            ..CollectionLimits::default()
        },
    );
    let batch = collect(&small, "run-1");
    assert_eq!(
        batch.files[0].status,
        FileStatus::Rejected(FileRejection::TooLarge)
    );
    assert!(batch.files[0].records.is_empty());
    assert_eq!(batch.completeness, Completeness::Incomplete);

    let deep = TempDir::new("depth");
    deep.write("a/b/c.abf-fixture", fixture(&["input_tokens=1"]));
    deep.write("a/ok.abf-fixture", fixture(&["input_tokens=1"]));
    let shallow = limited(
        &deep.path,
        CollectionLimits {
            max_depth: 2,
            ..CollectionLimits::default()
        },
    );
    assert_eq!(
        collect_synthetic_file(&shallow, &run("run-1"), "a/b/c.abf-fixture").map(|_| ()),
        Err(PathRejection::TooDeep)
    );
    let batch = collect(&shallow, "run-1");
    assert_eq!(batch.files.len(), 1);
    assert_eq!(batch.skipped[0].reason, SkipReason::DepthLimit);
    assert_eq!(batch.skipped[0].relative_path.as_deref(), Some("a/b"));
    assert_eq!(batch.completeness, Completeness::Incomplete);
}

/// Finding 2: the entry budget covers every entry of the whole tree
/// (files, directories, non-fixtures). Exceeding it fails the collection;
/// it never yields an order-dependent subset.
#[test]
fn entry_limit_is_total_and_fails_closed() {
    let tree = TempDir::new("entries");
    // 7 entries: a.abf-fixture, b.jsonl, sub, sub/c.abf-fixture,
    // sub/d.abf-fixture, sub/inner, sub/inner/e.abf-fixture.
    for relative in [
        "a.abf-fixture",
        "sub/c.abf-fixture",
        "sub/d.abf-fixture",
        "sub/inner/e.abf-fixture",
    ] {
        tree.write(relative, fixture(&["input_tokens=1"]));
    }
    tree.write("b.jsonl", "not a fixture");
    let with_entries = |max_entries| {
        limited(
            &tree.path,
            CollectionLimits {
                max_entries,
                ..CollectionLimits::default()
            },
        )
    };

    let exact = collect(&with_entries(7), "run-1");
    assert_eq!(exact.files.len(), 4);
    assert_eq!(exact.completeness, Completeness::Complete);
    assert_eq!(exact, collect(&with_entries(MAX_ENTRIES_CEILING), "run-1"));

    for max_entries in [1, 2, 4, 6] {
        assert_eq!(
            collect_synthetic_root(&with_entries(max_entries), &run("run-1")),
            Err(CollectError::EntryLimitExceeded),
            "max_entries = {max_entries}"
        );
    }
}

#[test]
fn collection_limits_are_range_checked() {
    let root = TempDir::new("limit-ranges");
    root.write("a.abf-fixture", fixture(&["input_tokens=1"]));
    let base = || scope(wsl_env(), claude_source(), &root.path);
    let default = CollectionLimits::default();
    let invalid = [
        CollectionLimits {
            max_file_bytes: 0,
            ..default
        },
        CollectionLimits {
            max_file_bytes: MAX_FILE_BYTES_CEILING + 1,
            ..default
        },
        CollectionLimits {
            max_file_bytes: u64::MAX,
            ..default
        },
        CollectionLimits {
            max_entries: 0,
            ..default
        },
        CollectionLimits {
            max_entries: MAX_ENTRIES_CEILING + 1,
            ..default
        },
        CollectionLimits {
            max_entries: usize::MAX,
            ..default
        },
        CollectionLimits {
            max_depth: 0,
            ..default
        },
        CollectionLimits {
            max_depth: MAX_DEPTH_CEILING + 1,
            ..default
        },
        CollectionLimits {
            max_depth: usize::MAX,
            ..default
        },
    ];
    for limits in invalid {
        assert_eq!(
            base().with_limits(limits),
            Err(ScopeError::InvalidLimits),
            "{limits:?}"
        );
    }

    let ceilings = CollectionLimits {
        max_file_bytes: MAX_FILE_BYTES_CEILING,
        max_entries: MAX_ENTRIES_CEILING,
        max_depth: MAX_DEPTH_CEILING,
    };
    let batch = collect(&base().with_limits(ceilings).unwrap(), "run-1");
    assert_eq!(batch.files[0].status, FileStatus::Read);
    let minimal = CollectionLimits {
        max_file_bytes: 1,
        max_entries: 1,
        max_depth: 1,
    };
    let batch = collect(&base().with_limits(minimal).unwrap(), "run-1");
    assert_eq!(
        batch.files[0].status,
        FileStatus::Rejected(FileRejection::TooLarge)
    );
}

#[test]
fn vanished_root_is_an_error_not_an_empty_batch() {
    let base = TempDir::new("vanishing");
    fs::create_dir(base.join("root")).unwrap();
    let scope = scope(wsl_env(), claude_source(), &base.join("root"));
    fs::remove_dir(base.join("root")).unwrap();
    let not_found = RootRejection::Io(std::io::ErrorKind::NotFound);
    assert_eq!(
        collect_synthetic_root(&scope, &run("run-1")),
        Err(CollectError::RootChanged(not_found))
    );
    assert_eq!(
        collect_synthetic_file(&scope, &run("run-1"), "x.abf-fixture").map(|_| ()),
        Err(PathRejection::RootChanged(not_found))
    );
}

/// Finding 1: the authorized root path is replaced after the scope was
/// built (synthetic, deterministic, sequential — not a concurrent race).
#[cfg(unix)]
fn collect_after_root_replacement(
    replace_with_symlink: bool,
) -> (
    Result<CollectionBatch, CollectError>,
    Result<FileReport, PathRejection>,
) {
    let base = TempDir::new("root-swap");
    base.write(
        "allowed/inside.abf-fixture",
        fixture(&["session_id=inside-marker\tinput_tokens=1"]),
    );
    base.write(
        "outside/inside.abf-fixture",
        fixture(&["session_id=outside-marker\tinput_tokens=999"]),
    );
    let scope = scope(wsl_env(), claude_source(), &base.join("allowed"));

    fs::rename(base.join("allowed"), base.join("allowed-old")).unwrap();
    if replace_with_symlink {
        std::os::unix::fs::symlink(base.join("outside"), base.join("allowed")).unwrap();
    } else {
        fs::rename(base.join("outside"), base.join("allowed")).unwrap();
    }

    let run = run("run-1");
    let root = collect_synthetic_root(&scope, &run);
    let file = collect_synthetic_file(&scope, &run, "inside.abf-fixture");
    assert!(!format!("{root:?}{file:?}").contains("outside-marker"));
    (root, file)
}

#[cfg(unix)]
#[test]
fn replaced_root_symlink_does_not_leak_outside_data() {
    let (root, file) = collect_after_root_replacement(true);
    assert_eq!(root, Err(CollectError::RootChanged(RootRejection::Symlink)));
    assert_eq!(
        file.map(|_| ()),
        Err(PathRejection::RootChanged(RootRejection::Symlink))
    );
}

#[cfg(unix)]
#[test]
fn replaced_root_directory_does_not_leak_outside_data() {
    let (root, file) = collect_after_root_replacement(false);
    assert_eq!(
        root,
        Err(CollectError::RootChanged(RootRejection::Replaced))
    );
    assert_eq!(
        file.map(|_| ()),
        Err(PathRejection::RootChanged(RootRejection::Replaced))
    );
}

/// Control: restoring the original directory object makes the same scope
/// usable again, so revalidation keys on identity rather than failing
/// unconditionally after a rename.
#[cfg(unix)]
#[test]
fn restored_root_object_is_accepted_again() {
    let base = TempDir::new("root-restore");
    base.write("allowed/a.abf-fixture", fixture(&["input_tokens=1"]));
    let scope = scope(wsl_env(), claude_source(), &base.join("allowed"));
    fs::rename(base.join("allowed"), base.join("allowed-old")).unwrap();
    assert!(collect_synthetic_root(&scope, &run("run-1")).is_err());
    fs::rename(base.join("allowed-old"), base.join("allowed")).unwrap();
    let batch = collect(&scope, "run-1");
    assert_eq!(batch.observations().count(), 1);
    assert_eq!(batch.completeness, Completeness::Complete);
}

#[test]
fn record_output_is_bounded_across_files_without_successful_truncation() {
    let root = TempDir::new("output");
    root.write("a.abf-fixture", fixture(&vec!["input_tokens=1"; 4096]));
    root.write("b.abf-fixture", fixture(&["input_tokens=2"]));
    root.write("c.abf-fixture", fixture(&[]));
    let batch = collect(&scope(wsl_env(), claude_source(), &root.path), "run-budget");
    assert_eq!(batch.files[0].status, FileStatus::Read);
    assert_eq!(batch.files[0].records.len(), 4096);
    assert_eq!(
        batch.files[1].status,
        FileStatus::Rejected(FileRejection::OutputLimit)
    );
    assert!(batch.files[1].records.is_empty());
    assert_eq!(batch.files[2].status, FileStatus::Read);
    assert!(batch.files[2].records.is_empty());
    assert_eq!(batch.completeness, Completeness::Incomplete);
}

#[test]
fn oversized_record_output_rejects_whole_file() {
    let root = TempDir::new("output-single");
    root.write("a.abf-fixture", fixture(&vec![""; 4097]));
    let report = collect_synthetic_file(
        &scope(wsl_env(), claude_source(), &root.path),
        &run("run-budget"),
        "a.abf-fixture",
    )
    .unwrap();
    assert_eq!(
        report.status,
        FileStatus::Rejected(FileRejection::OutputLimit)
    );
    assert!(report.records.is_empty());
}

#[test]
fn missing_file_is_not_an_empty_success() {
    let root = TempDir::new("missing");
    let result = collect_synthetic_file(
        &scope(wsl_env(), claude_source(), &root.path),
        &run("run-missing"),
        "missing.abf-fixture",
    );
    assert_eq!(result, Err(PathRejection::Io(std::io::ErrorKind::NotFound)));
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_a_failure_alongside_readable_results() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new("unreadable");
    root.write("a.abf-fixture", fixture(&["input_tokens=7"]));
    let unreadable = root.write("b.abf-fixture", fixture(&["input_tokens=99"]));
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
    let batch = collect(
        &scope(wsl_env(), claude_source(), &root.path),
        "run-unreadable",
    );
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(batch.files[0].status, FileStatus::Read);
    assert_eq!(
        batch.files[1].status,
        FileStatus::Rejected(FileRejection::Io(std::io::ErrorKind::PermissionDenied))
    );
    assert!(batch.files[1].records.is_empty());
    assert_eq!(batch.observations().count(), 1);
    assert_eq!(batch.completeness, Completeness::Incomplete);
}

#[test]
fn empty_short_and_truncated_headers_are_coded_rejections() {
    let root = TempDir::new("truncated-header");
    let scope = scope(wsl_env(), claude_source(), &root.path);
    let run = run("run-truncated");
    for length in 0..FIXTURE_HEADER.len() {
        root.write(
            "truncated.abf-fixture",
            &FIXTURE_HEADER.as_bytes()[..length],
        );
        let report = collect_synthetic_file(&scope, &run, "truncated.abf-fixture").unwrap();
        assert_eq!(
            report.status,
            FileStatus::Rejected(FileRejection::NotSyntheticFixture),
            "prefix length {length}"
        );
        assert!(report.records.is_empty());
        assert_eq!(report.pending_tail_bytes, 0);
    }
    let batch = collect_synthetic_root(&scope, &run).unwrap();
    assert_eq!(batch.completeness, Completeness::Incomplete);
    assert_eq!(batch.observations().count(), 0);
}

#[test]
fn exact_header_without_separator_is_a_coded_rejection() {
    let root = TempDir::new("header-boundary");
    root.write("header.abf-fixture", FIXTURE_HEADER);
    let report = collect_synthetic_file(
        &scope(wsl_env(), claude_source(), &root.path),
        &run("run-header"),
        "header.abf-fixture",
    )
    .unwrap();
    assert_eq!(
        report.status,
        FileStatus::Rejected(FileRejection::NotSyntheticFixture)
    );
    assert!(report.records.is_empty());
    assert_eq!(report.pending_tail_bytes, 0);
}

#[test]
fn header_plus_separator_requires_a_newline_and_emits_no_records() {
    let root = TempDir::new("separator-boundary");
    let scope = scope(wsl_env(), claude_source(), &root.path);
    let run = run("run-separator");
    for (separator, status) in [
        ('\n', FileStatus::Read),
        (
            'x',
            FileStatus::Rejected(FileRejection::NotSyntheticFixture),
        ),
        (
            '\r',
            FileStatus::Rejected(FileRejection::NotSyntheticFixture),
        ),
    ] {
        root.write("header.abf-fixture", format!("{FIXTURE_HEADER}{separator}"));
        let report = collect_synthetic_file(&scope, &run, "header.abf-fixture").unwrap();
        assert_eq!(report.status, status);
        assert!(report.records.is_empty());
        assert_eq!(report.pending_tail_bytes, 0);
    }
}
