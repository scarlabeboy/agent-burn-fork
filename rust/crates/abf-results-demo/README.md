# ABF130 - fictional collection-to-results demonstration

Standalone, unpublished example of the existing ABF126 `present(Inputs)` API.
The original four scenarios use fixed fictional in-memory inputs. Five additional
scenarios collect disposable fictional files through `collect_synthetic_root`.
There are no command-line inputs, clocks, history discovery, native Windows reads,
WSL launches, network calls, credentials or production wiring.
Dependencies are unchanged existing local path crates only. The
empty workspace table isolates this crate without changing a workspace manifest.

From the repository root, using already installed Rust tools:

```sh
cargo run --offline --locked --quiet --manifest-path rust/crates/abf-results-demo/Cargo.toml
```

The executable first prints the four original scenarios in fixed order:

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
identity or overall collection success is reported. The original renderer does not
read numeric totals or recompute them. A normal program exit only means the fixed
demonstration ran. It does not certify collection success. Output explicitly
selects safe identity/status fields and fixed codes; raw records, parser results,
source locators and opaque times are omitted. A fictional private-field sentinel
in the script makes accidental raw payload output detectable in focused tests.

## ABF130 file collection flow

Each invocation exclusively creates a new directory below this crate's
`target/abf130/`, writes fixed fictional fixtures, and removes its directory on
normal return or unwinding. Existing directories are never reused or deleted.
The directory name uses the process id and an available numeric suffix; output
does not expose either. Abrupt termination can leave disposable files behind.
The root is canonicalized and explicitly passed to `AuthorizedRoot`; no home,
environment-variable or provider-root discovery occurs. Collection limits are
4096 bytes per file, 16 entries and depth 4, except the depth-limit scenario
which uses depth 1. These remain small trusted fixtures, not hostile trees.

The demo-local adapter translates accepted numeric fields into the existing
fictional JSON parser input, then uses S1 emission, S4 supervision and ABF126
`present(Inputs::WindowsOnly)` without changing their contracts or dependencies.
Each sorted file has a distinct synthetic `windows-fileN` source reference.
Records retain their order; equal claims are never deduplicated. Original batch
provenance, file/record codes, pending tails and completeness are displayed beside
the results. The file ordinal connects each displayed occurrence to its script
source; the original batch retains paths and file-object evidence in memory.
Paths, file-object ids, claimed metadata strings and raw payloads are not printed.

| Scenario | Observed results |
| --- | --- |
| Fictional file | Input claim sum 37, output 5; CoveredWithinEnumeration, expected 1 |
| Fictional multiple files | Input 74, output 10; two retained occurrences, expected 2 |
| Fictional rejected file | NotSyntheticFixture; collection Incomplete, NotCovered; no numeric claim sums |
| Fictional incomplete coverage | DepthLimit; collection Incomplete, InventoryIncomplete; known input 37 and output 5 remain visible |
| Fictional partial records | Partial field, prohibited-content rejection and 17 pending bytes; collection Complete, PartiallyCovered; input 9, reported total 12, cache read 3 and explicit cache write 0 |

All numeric results are unchanged projector `ReportedClaimSum` values, not
deduplicated usage. Components are independent: input plus output never invents
a reported total. Missing and invalid fields do not become zeros. Reasoning is
unsupported by the projector and is shown separately in its original field
state. Unparseable numeric values become fixed invalid field markers; rejected
records become a fixed parser rejection while their exact original boundary
code remains visible. This is a lossy demo translation, not a parser-equivalence,
real OpenCode origin or production bridge claim. Harness/environment labels are
fictional; original SyntheticFixture/host-match and Unattested evidence remain
explicit. The fixed capture time is NotRecorded; supervisor times are synthetic.

DepthLimit maps to the existing partial enumeration with Budget reason. A
rejected file gets no successful traversal; a pending tail gets no terminal
source-end claim. RunEnd is withheld when any file lacks that end. Read files
use ChangeNotObservable, avoiding a coherent-snapshot claim. Complete describes
boundary traversal/read completeness only and can coexist with rejected records,
partial fields and pending tails. The independent original batch status remains
visible. InventoryIncomplete can coexist with a ReportedComplete run summary for
the files that were enumerated; that summary does not cover the hidden file.
Fresh is logical S4 availability, never proof of complete usage. Any root
collection error prints its code without constructing results or numeric totals.

The existing path-based confinement, read-coherence and cumulative memory limits
documented by abf-collection-boundary still apply. This adapter is private and
used only with these fixed small scenarios. It does not admit caller roots,
implement production schemas, preserve cross-run identity or authorize native
collection. The first four virtual WSL scenarios still perform no WSL operations.

ABF130 validation: all 10 demo output tests passed offline and locked, including
repeated deterministic output, all five file scenarios and payload omission.
New tests failed before their scenarios were implemented. Results-fixture passed
16 integration tests and 3 compile-fail doc tests. Collection-boundary passed
6 unit tests and 27 integration tests, with one existing Unix socket test
excluded because its canonical crate-local fixture path exceeds the Unix socket
pathname limit (the same ABF129 limitation). Fmt checks and all-target clippy with
`-D warnings` passed for all three crates. Build/test outputs stay in the three
authorized crates' target directories. No manifests or lockfiles change.

Run the demo checks using the commands in the historical validation section.
The collector command used for ABF130, after creating its disposable TMPDIR:

```sh
mkdir -p rust/crates/abf-collection-boundary/target/abf130-tmp
TMPDIR="$PWD/rust/crates/abf-collection-boundary/target/abf130-tmp" \
cargo test --offline --locked \
  --manifest-path rust/crates/abf-collection-boundary/Cargo.toml \
  -- --skip hard_links_and_non_regular_files_are_refused
```

NOT_RUN: native Windows/WSL execution or compilation, real histories/credentials,
network, production app/profile, coherent snapshot/confinement validation,
persistence, restart, full product builds, publication and release. Synthetic
filesystem tests on this Linux environment establish no native Windows or WSL
integration evidence. Root-error output is implemented but not exercised by the
fixed demo scenarios. The existing Unix socket/hard-link combined test is NOT_RUN
in ABF130; no shortened external fixture root was used. No production provenance
is established.

Recommended next action, not executed: read-only review of the demo translation
and displayed coverage against the existing collector and projector contracts.

## Historical ABF127 validation and scope evidence

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
publication and release. ABF120 findings are dispositioned by Contract A V2 in
AGENTS.md (documentation only; no access granted). Synthetic results establish
no production provenance or native confinement under Contract A or B.

Recommended next action, not executed: read-only review of this demonstration's
output and scope against the existing ABF126 presentation contract.
