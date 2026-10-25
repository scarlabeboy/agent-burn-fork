//! ABF130: demo-local translation, not a production reader or bridge contract.
use std::{fs, io, path::PathBuf};

use abf_collection_boundary::{
    identity::{EnvironmentIdentity, Harness, OpaqueId, SourceInstance},
    observation::{CaptureTime, CollectionBatch, Field, FileStatus, RecordOutcome, ScanRun},
    scope::{AuthorizedRoot, CollectionLimits, CollectionScope, HostPolicy},
    synthetic::{FIXTURE_HEADER, collect_synthetic_root},
};
use abf_reader_fixture::{Action, Event, Step};
use abf_results_fixture::{Inputs, present};
use abf_source_run::*;

// No arbitrary input roots. Each invocation owns a new crate-local directory.
struct Fixtures(PathBuf);
impl Fixtures {
    fn new() -> io::Result<Self> {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/abf130");
        fs::create_dir_all(&parent)?;
        for index in 0..1000 {
            let path = parent.join(format!("demo-{}-{index}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }
}
impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Serialize only numeric field states into the existing fictional JSON parser.
// Missing remains absent; unparseable remains invalid, with no raw value copied.
fn tokens(record: &RecordOutcome) -> Vec<u8> {
    let RecordOutcome::Accepted(observation) = record else {
        return b"!".to_vec(); // Fixed parser rejection; original code stays in the batch.
    };
    let usage = &observation.usage;
    let mut fields = Vec::new();
    for (key, field) in [
        ("input", &usage.input),
        ("output", &usage.output),
        ("total", &usage.total_reported),
    ] {
        match field {
            Field::Value(n) => fields.push(format!("\"{key}\":{n}")),
            Field::Unparseable { .. } => fields.push(format!("\"{key}\":\"invalid\"")),
            Field::Absent | Field::NotCollected => {}
        }
    }
    let mut cache = Vec::new();
    for (key, field) in [("read", &usage.cache_read), ("write", &usage.cache_write)] {
        match field {
            Field::Value(n) => cache.push(format!("\"{key}\":{n}")),
            Field::Unparseable { .. } => cache.push(format!("\"{key}\":\"invalid\"")),
            Field::Absent | Field::NotCollected => {}
        }
    }
    if !cache.is_empty() {
        fields.push(format!("\"cache\":{{{}}}", cache.join(",")));
    }
    format!("{{\"tokens\":{{{}}}}}", fields.join(",")).into_bytes()
}

fn display(title: &str, batch: &CollectionBatch) {
    let mut supervisor = super::windows();
    let sources: Vec<_> = (0..batch.files.len())
        .map(|i| SourceRef::new(&format!("windows-file{i}")).unwrap())
        .collect();
    let payloads: Vec<Vec<_>> = batch
        .files
        .iter()
        .map(|file| file.records.iter().map(tokens).collect())
        .collect();
    let mut events = vec![Event::Enumeration {
        // These fixed scenarios have only DepthLimit as an incomplete skip.
        outcome: if batch.skipped.iter().any(|s| s.reason.makes_incomplete()) {
            RawEnumeration::Partial(sources.len() as u64, StopReason::Budget)
        } else {
            RawEnumeration::Closed(sources.len() as u64)
        },
        sources: &sources,
    }];
    let mut ended = true;
    for ((file, source), records) in batch.files.iter().zip(&sources).zip(&payloads) {
        events.push(Event::Source {
            source,
            source_seq: 1,
            action: Action::OpenAttempt,
        });
        if file.status != FileStatus::Read {
            // Do not claim successful source traversal for a rejected file.
            ended = false;
            continue;
        }
        events.push(Event::Source {
            source,
            source_seq: 1,
            action: Action::OpenSuccess,
        });
        for (i, bytes) in records.iter().enumerate() {
            events.push(Event::Record {
                source,
                source_seq: i as u64 + 1,
                bytes,
            });
        }
        if file.pending_tail_bytes != 0 {
            ended = false;
            continue;
        }
        events.push(Event::Source {
            source,
            source_seq: records.len() as u64 + 1,
            action: Action::End(ChangeObservation::NotObservable),
        });
    }
    if ended {
        events.push(Event::RunEnd);
    }
    let steps: Vec<_> = events
        .into_iter()
        .enumerate()
        .map(|(i, event)| Step {
            frame_seq: i as u64 + 1,
            event,
        })
        .collect();
    supervisor.run(&steps, None, super::facts(), false);
    let results = present(Inputs::WindowsOnly(&supervisor)).unwrap();
    super::display(title, &results);
    println!(
        "  collection={:?}; traversal/read completeness only",
        batch.completeness
    );
    println!(
        "  run={} environment={} source={} harness={:?} host_match={:?}",
        batch.scan_run.id.as_str(),
        batch.environment.id().as_str(),
        batch.source.id().as_str(),
        batch.source.harness(),
        batch.host_match
    );
    for (i, file) in batch.files.iter().enumerate() {
        println!(
            "  file={i} status={:?} pending_tail_bytes={}",
            file.status, file.pending_tail_bytes
        );
        for record in &file.records {
            println!(
                "    file={i} line={} offset={} length={}",
                record.occurrence().line_ordinal,
                record.occurrence().byte_offset,
                record.occurrence().byte_length
            );
            match record {
                RecordOutcome::Accepted(o) => println!(
                    "    origin={:?} parse={:?} reasoning_tokens={:?}; reasoning unsupported by projector",
                    o.origin, o.parse_status, o.usage.reasoning
                ),
                RecordOutcome::Rejected(r) => println!("    record_error={:?}", r.reason),
            }
        }
    }
    for skip in &batch.skipped {
        println!("  skip={:?}", skip.reason);
    }
    if let Some(view) = results.windows().unwrap().latest() {
        match view.projection() {
            Ok(p) => {
                let result = p.projection().windows.selected().unwrap();
                println!(
                    "  stored_occurrences={} input_tokens={:?} output_tokens={:?} total_tokens={:?}",
                    result.stored_occurrences.0,
                    result.components.input.value,
                    result.components.output.value,
                    result.components.total.value
                );
                println!(
                    "  cache_read_tokens={:?} cache_write_tokens={:?} usage={:?}",
                    result.components.cache_read.value,
                    result.components.cache_write.value,
                    result.usage
                );
                println!(
                    "  outcomes={:?} reported_run_coverage={:?}",
                    result.outcomes.0, result.runs.coverage
                );
                println!("  input_evidence={:?}", result.components.input.evidence);
            }
            Err(code) => println!("  totals_error={code:?}"),
        }
    }
}

pub(super) fn run() -> io::Result<()> {
    let fixtures = Fixtures::new()?;
    let valid = format!("{FIXTURE_HEADER}\ninput_tokens=37\toutput_tokens=5\n");
    let invalid_reasoning = format!(
        "{FIXTURE_HEADER}\ninput_tokens=37\toutput_tokens=5\treasoning_tokens=ABF130B-REASONING-RAW-SENTINEL\n"
    );
    let partial = format!(
        "{FIXTURE_HEADER}\ninput_tokens=9\toutput_tokens=invalid\ttotal_tokens_reported=12\tcache_read_tokens=3\tcache_write_tokens=0\treasoning_tokens=4\ncontent=FICTIONAL-CONTENT-SENTINEL\tinput_tokens=9999\ninput_tokens=9999"
    );
    for (name, title, files, depth) in [
        (
            "single",
            "Fictional file",
            vec![("a.abf-fixture", valid.as_str())],
            4,
        ),
        (
            "multiple",
            "Fictional multiple files",
            vec![
                ("a.abf-fixture", valid.as_str()),
                ("b.abf-fixture", invalid_reasoning.as_str()),
            ],
            4,
        ),
        (
            "rejected",
            "Fictional rejected file",
            vec![("bad.abf-fixture", "fictional invalid header\n")],
            4,
        ),
        (
            "incomplete",
            "Fictional incomplete coverage",
            vec![
                ("a.abf-fixture", valid.as_str()),
                ("hidden/b.abf-fixture", valid.as_str()),
            ],
            1,
        ),
        (
            "partial",
            "Fictional partial records",
            vec![("a.abf-fixture", partial.as_str())],
            4,
        ),
    ] {
        let root = fixtures.0.join(name);
        fs::create_dir(&root)?;
        for (relative, contents) in files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, contents)?;
        }
        let scope = CollectionScope::new(
            EnvironmentIdentity::windows(OpaqueId::new("fictionalWindows").unwrap()),
            SourceInstance::new(OpaqueId::new("fictionalFiles").unwrap(), Harness::OpenCode),
            AuthorizedRoot::new(&fs::canonicalize(root)?).unwrap(),
            HostPolicy::AllowSimulated,
        )
        .unwrap()
        .with_limits(CollectionLimits {
            max_file_bytes: 4096,
            max_entries: 16,
            max_depth: depth,
        })
        .unwrap();
        let run = ScanRun::new(
            OpaqueId::new("file-demo").unwrap(),
            CaptureTime::NotRecorded,
        );
        match collect_synthetic_root(&scope, &run) {
            Ok(batch) => display(title, &batch),
            Err(code) => {
                println!("\n== {title} ==\ncollection_error={code:?}; no results or totals")
            }
        }
    }
    Ok(())
}
