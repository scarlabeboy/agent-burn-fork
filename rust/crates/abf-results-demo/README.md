# ABF127 - synthetic B1 executable demonstration

Standalone, unpublished example of the existing ABF126 `present(Inputs)` API.
All scenarios are fixed fictional in-memory inputs. There are no command-line
inputs, clocks, discovery, native reads, WSL launches, network calls, credentials
or production wiring. Dependencies are existing local path crates only. The
empty workspace table isolates this crate without changing a workspace manifest.

From the repository root, using already installed Rust tools:

```sh
cargo run --offline --locked --quiet --manifest-path rust/crates/abf-results-demo/Cargo.toml
```

The executable prints four scenarios in fixed order:

1. `WindowsOnly`: S4 covered fictional evidence with logical Fresh availability.
   This result is constructed and displayed before creating any WSL identity or
   virtual transport; it requires no WSL input or installed distribution.
2. `SelectedWSL interrupted`: one explicitly fictional distribution,
   `Fictional-Ubuntu-Demo`. S5 delivery stops after four frames. Its projection
   is admitted while collection coverage remains PartiallyCovered.
3. `Both independent`: a subsequent S4 script cancellation makes Windows Stale,
   preserving the covered last-good snapshot. A separate complete S5 response
   remains independent, with Unestablished freshness.
4. `Both unavailable`: a new S4 supervisor has no issued attempt or snapshot.
   An actual synthetic S5 pre-call cancellation returns CancelledBeforeCall,
   without a response, projection, fabricated timestamp or numeric zero.

Representative output excerpts (other evidence lines omitted here):

```text
== WindowsOnly ==
selection=WindowsOnly
Windows availability=Fresh freshness=Fresh
  latest_code=Completed

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

== Both unavailable ==
selection=Both
Windows availability=Unavailable freshness=Unavailable
  latest_report: absent; no attempt issued
  latest: absent; no snapshot
  last_good: absent; no snapshot
SelectedWSL environment=fictionalWsl distro=Fictional-Ubuntu-Demo freshness=Unestablished
  response_error=CancelledBeforeCall; no response or projection
```

## Interpretation and limits

The program constructs S4 via its existing S2 fictional plan and S1 script,
and S5 via its existing single-distribution in-memory transport. Only ABF126
assembles the presentation envelope. Formatting reads its borrowed views; no
new result model, counting, finalization, recovery or aggregation rule is added.
The two Windows slots can reference the same snapshot when Fresh. Last-good
retention is exclusively S4 policy. S5 supplies no last-good or late-diagnostics
assessment; the demonstration invents neither.

Output includes each available environment reference, exact coverage verdict,
qualifiers, process/RunEnd facts, independent projection admission or refusal,
Windows assessment ceiling and relevant fixed error codes. An unavailable
Windows supervisor supplies no snapshot environment identity, so none is
invented. The labels are fictional claims and provenance remains Unattested.
Fresh/Stale are logical S4 states, not measured ages. Complete delivery is not
a collection verdict. CoveredWithinEnumeration is qualified by the fictional
enumeration and original assessment ceiling.

No combined total, Computer projection, shared catalog authority, cross-environment
identity or overall collection success is reported. The renderer does not read
numeric totals or recompute them. A normal program exit only means the fixed
demonstration ran. It does not certify collection success. Output explicitly
selects safe identity/status fields and fixed codes; raw records, parser results,
source locators and opaque times are omitted. A fictional private-field sentinel
in the script makes accidental raw payload output detectable in focused tests.

## Validation and scope evidence

Baseline: `abf/main`, HEAD `1c1de7d8a8824ea78d9947127eba5f467e61f332`,
with clean index/worktree/untracked state before changes. Only this crate is
added; existing contracts, crates and workspace manifests are unchanged.
Applicable instructions: root `AGENTS.md`; no nested instructions apply here.
Inspected APIs and evidence: ABF126 results source/tests/README/manifest/lockfile,
B1/S4 supervisor and B1/S5 SelectedWSL source/README, S1 script validation, G4
source-run types/projection and G3 counting result types.

RUN: the executable and five focused output tests, including deterministic
repeated runs and raw-payload omission. The initial empty executable failed
the tests before implementation. Final validation commands:

```sh
cargo fmt --manifest-path rust/crates/abf-results-demo/Cargo.toml --check
cargo test --offline --locked --manifest-path rust/crates/abf-results-demo/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-results-demo/Cargo.toml --all-targets -- -D warnings
```

Cargo build outputs stay in this crate's ignored `target/` directory. No package
downloads or installation are needed. Unaffected ABF126C, S4 and S5 regression
evidence documented by their existing READMEs is reused; those suites are not
rerun and this demonstration does not independently reprove their contracts.

NOT_RUN: actual Windows/WSL operations, distribution discovery/startup, native
filesystem collection or confinement, real transport/cancellation, provider
histories, credentials, network, persistence/restart, production CLI/app builds,
publication and release. ABF120-F01/F02 blockers and F03 prerequisite remain
unchanged. Synthetic results establish no production provenance or native
confinement under Contract A or B.

Recommended next action, not executed: read-only review of this demonstration's
output and scope against the existing ABF126 presentation contract.
