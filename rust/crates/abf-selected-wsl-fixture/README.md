# ABF125 - B1/S5 synthetic SelectedWSL transport

ABF125C aligns the cancellation indicator with effective fictional termination.

Standalone, unpublished, synchronous, in-memory research fixture. No discovery,
process launcher, filesystem reader, WSL adapter, network, clocks, Windows APIs,
UNC access, credentials, persistence or production bridge. It does not establish
native authorization or physical provenance. ABF120-F01/F02/F03 remain unchanged.

## Minimal API and existing contracts

- `Transport::new(selected, catalog, scope)` requires exactly one explicitly
  supplied S0 `EnvironmentIdentity::wsl`, including its fictional distribution
  name. Empty, multiple (even identical), and Windows selections refuse. The
  scope must be G4 `SelectedWsl`, match the supplied environment ID, and belong
  to the supplied catalog. No name, environment ID or source is discovered or
  minted from payloads. S0 identity equality includes the distribution name;
  this is a caller claim, not registry/GUID verification.
- `exchange(selection, request, cancel_before_call)` performs at most one virtual
  exchange. `WindowsOnly` refuses first, before identity issuance, script
  inspection or emission. Pre-cancellation and mismatched request/response
  environment identities also make no calls. `calls()` counts virtual exchanges,
  never OS operations. There are no callbacks, background tasks or retries.
- `Request` borrows caller-owned S1 steps and explicit fictional request target,
  response environment, delivery, process, barrier and opaque time facts.
  Both identities must equal the selected identity, including its distribution.
  A mismatch refuses before emission; it does not bind foreign observations to
  the selected environment. This is an in-memory fixture API, not a wire format
  or authenticated handshake.
- S1 `emit` alone owns parsing, delivery identity, replay, grammar, cancellation
  suppression and full-script resource preflight. There is no parallel reader,
  parser, collection batch or finalization model. Resource limits apply even to
  transport-discarded and S1-cancelled suffixes. A cutoff is a count of emitted
  frames, not a script index. Out-of-range cutoffs refuse explicitly.
- `Delivery::Complete` means all S1-emitted frames are delivered, including when
  the script itself ends early or cancels. It does not certify collection
  completion. `Failed`, `Incomplete` and `Cancelled` retain exactly the requested
  prefix; the withheld suffix is discarded, not late evidence. These are
  explicit fictional outcomes, not a production drain protocol. Failure and
  incomplete delivery model G4 `TransportLost`; cancellation models `KilledCancel`.
  S1-emitted cancellation also models `KilledCancel`, superseding the supplied
  process or transport-loss fact, as in S4. The original delivery fact remains
  separately inspectable. This precedence is a model choice, not OS evidence.
  With `Complete`, the supplied process remains effective unless S1 cancels:
  supplied `KilledCancel` sets `cancelled()` to true, while supplied `TransportLost`
  leaves it false. Both remain representable with all frames delivered. Even
  with End and RunEnd, G4 reports `CompletionUnconfirmed` for these terminations;
  successful projection preserves that qualifier and does not certify completion.
- G4 `finalize` receives the exact retained frames, one original scope and
  catalog authority, fresh issuer attempt/finalization labels, and the supplied
  barrier/times. No End, RunEnd, enumeration or drain evidence is invented.
  `NotDrained` still refuses. Failure with no frames preserves G4 `NoAttempt`,
  rather than becoming successful empty enumeration. Missing completion and
  failed inventory retain their existing G4 meanings.
- `Response` owns the fictional environment (including distribution), issued
  identities, delivery/cancellation facts and either immutable G4 evidence or
  the exact reader/finalizer refusal. Issued labels survive these refusals.
  Pre-call refusals issue no identity. `execution()` explicitly exposes sensitive
  evidence; retained records preserve source, sequence, frame and delivery
  provenance through G4. G4 attestation remains `Unattested`.
  `cancelled()` reports accepted fictional cancellation from S1, a validated
  `Delivery::Cancelled` cutoff, or effective `KilledCancel`. Reader refusal accepts
  none of these facts and leaves the indicator false, even if the refused inputs
  request cancellation. Once accepted, the indicator survives finalizer refusal
  (including an undrained barrier), while the exact error remains inspectable.
  `Failed`/`Incomplete` still replace the supplied process with `TransportLost`;
  supplied `KilledCancel` alone does not override that existing precedence.
- `Response::project` delegates whole admission and paired projection to G4 with
  the original catalog allocation. It projects only the SelectedWSL leg, even
  when the exchange selection is `Both`. No Windows/WSL composition occurs here.
  Projection refusal retains the entire execution and cancellation flag; it
  cannot filter evidence, alter G4 verdicts or turn failed transport into a
  completion claim. A rebuilt equal-content catalog is not the same authority.

S2 plans and S4 supervisor construction are deliberately Windows-specific. They
remain unchanged. S5 uses the existing explicit G4 catalog/scope for its separate
SelectedWSL leg, without pretending an S2 Windows plan authorizes WSL. S3 has no
accepted record-framing conversion or native confinement proof and is not used as
transport authorization. This fixture does not choose any open production schema,
CLI bridge, environment identity policy or native reader contract.

## Bounds and privacy

S1 enforces 512 steps, 128 record deliveries, 65536 bytes per record, 131072
cumulative record bytes (including replays), and 256 sources per listing. Its
finite enumeration-copy ceiling is 8 MiB of source-name characters, plus headers,
frames, parser storage and allocation overhead. G4 independently enforces its
admission ceilings; excess declarations refuse whole projection.

One distribution and scope are retained per transport. The existing fixture issuer
permits at most 1024 exchanges, including reader/finalizer refusals. Exhaustion
prevents further calls without reusing identities; there is no reset API. Each
call creates at most one S1 emission and one G4 candidate. The transport retains
no response history. Caller-owned inputs and accumulated returned responses are
outside its per-call bound. Dependency allocations can abort on OOM; no portable
process-memory or native cancellation-latency guarantee is claimed.

Debug output for `Transport`, `Request` and `Response` is redacted. Explicit
identity, execution and projection access is evidence access, not a logging or
serialization policy. Linking S0 for its pure identity types does not invoke its
filesystem collection APIs. No native authority conversion or extension port
exists in this crate.

## Validation

From the repository root:

```sh
cargo fmt --manifest-path rust/crates/abf-selected-wsl-fixture/Cargo.toml --check
cargo test --offline --locked --manifest-path rust/crates/abf-selected-wsl-fixture/Cargo.toml
cargo clippy --offline --locked --manifest-path rust/crates/abf-selected-wsl-fixture/Cargo.toml --all-targets -- -D warnings
```

The tests cover explicit selection and distribution isolation, WindowsOnly and
pre-cancel no-call behavior, original catalog authority and retained provenance,
whole projection refusal alongside cancellation, exact interrupted prefixes,
missing completion, undrained refusal, deterministic replay, full-script bounds,
issuer exhaustion and debug redaction. Existing S1/G4/S4 tests provide focused
regressions without modifying those contracts.
ABF125C adds permanent regressions for Complete with supplied KilledCancel and
TransportLost (process, indicator, coverage and paired projection), accepted
cancellation on finalizer refusal, unaccepted cancellation on reader refusal,
and transport-loss precedence over a supplied cancellation fact. Unaffected
ABF125 dependency regression evidence is reused without rerunning those suites.

NOT_RUN: actual Windows/WSL execution, distribution inspection/startup, native
confinement, real transport/drain/cancellation, provider data and full product
builds. Synthetic results grant no production access or release authority.
