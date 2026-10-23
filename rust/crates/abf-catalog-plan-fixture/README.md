# Conditional catalog fixture (ABF115)

Experimental, fictional Windows claims only. A matching grant is not user consent;
a namespace is not verified environment identity; a selected locator is not a
filesystem capability, confinement proof, native layout recognition, enumeration,
physical source identity or production catalog authority. No production UD-3,
UD-4, UD-7 or UD-8 policy is selected. Every plan retains unmet native requirements.

## Contract

`plan_windows_fixture` takes one `WindowsRequest`, descriptor/candidate slices and
explicit fictional grants. All families use only the accepted synthetic OpenCode
semantics. Arbitrary bounded family labels identify fictional scopes, not other
producers. Schema 1 supports fictional `fixture-v1` and `fixture-v2` layouts;
these describe claims, not real installed layouts. Rule labels are local to a descriptor ID and revision. A descriptor carries an explicit
Windows CatalogScope, revision, rule, tag, approved classes and optional declared
overlap descriptor ID. No default descriptors or lookup exists.

The immutable result owns the selected namespace, one fresh CatalogSnapshot and canonical family-ordered
decisions. Conditional means internally consistent fictional claims ONLY.
Unresolved records missing candidate/grant/recognition evidence independently.
An explicit empty recognition list means Unrecognized. Missing grant with explicit
zero matches stays Unrecognized, with the missing grant visible in conditions.
No requests means NoSelectedWork. Otherwise RequestedWork is not execution or
completion, even when the catalog is empty. Only Conditional scopes are members.
The caller may inspect fictional root claims through explicit accessors; these
are sensitive access, not a logging interface. Debug is redacted.

All structural conflicts refuse the whole call; no partial plan is returned.
Missing grant/recognition is unresolved; foreign/stale/mismatched present claims
refuse. Multiple roots per requested family or multiple recognized matches refuse.
No fallback, content deduplication or first match. Duplicate identifiers, tags,
scopes, requests and grant targets refuse even if textually equal. Unselected
inputs still undergo bounds, schema, namespace, identifier, locator syntax,
class, binding and recognition-ambiguity checks. They never produce catalog membership.

## Fictional locator grammar

ASCII letter drive, colon, one separator, then 1–16 nonempty components.
Both slash `/` and backslash `\` are separators, including mixed separators.
Each component consists only of ASCII alphanumerics, underscore, hyphen, dot or
internal space; it cannot start with a space, end with dot/space or equal dot/dot-dot. Reserved device
stems (case insensitive, before the first dot) CON, PRN, AUX, NUL, COM1–9 and
LPT1–9 refuse, including extensions and spaces before an extension. UNC/WSL UNC/device/verbatim paths, relative,
drive-relative, root-relative and bare drives, empty/repeated components, streams,
control/non-ASCII characters and all other punctuation refuse. There is no
expansion, Unicode normalization, host path API or actual path access.

ASCII-lowercase and separator-folded comparison identifies suspected aliases only.
Selected candidates with equal keys refuse, even while unresolved. Component-wise
ancestor keys also refuse potential overlap; sibling prefixes do not. Neither
comparison establishes actual identity or confinement. Descriptor-declared overlap
between selected families refuses. Same-family multiple-layout acceptance is never
performed; unique recognition selects exactly one descriptor.

## Fixed validation precedence and bounds

1. Budget, then basic shape (bounded opaque labels, tags, nonempty class lists).
2. Schema, supported layout, selected Windows namespace, known requested families.
3. Duplicate descriptor/scope, request, tag, candidate, grant target; overlap IDs.
4. Locator syntax, approved class, complete fictional grant binding.
5. Recognition binding, ambiguity in any supplied claim set, multiple selected roots.
6. Lexical alias, lexical/declared overlap.
7. Allocate immutable decisions and build exactly one catalog.

Within each class the fixed code order listed by `PlanCode` is used, using whole
slice predicates, independent of input order. No invalid candidate wins by array
position. Bounds are checked before transformed output allocation. Input data is
borrowed; callers already own it. Checked accumulation never truncates.

Limits: 16 descriptors, requests, candidates and grants; 2 recognition claims per
candidate; 512 bytes per locator; 8192 total locator bytes; 16 components; 3 approved
classes per descriptor. Opaque strings use `[A-Za-z0-9_-]{1,64}`; scope tags use
`[A-Za-z0-9]{1,16}`. These are fixture limits, not production settings or a complete
process memory ceiling. Recoverable output allocations return AllocationFailure;
allocations inside accepted dependencies, snapshot Arc allocation, and Clone may
abort on exhaustion. No universal OOM recovery is promised. Aggregate 8193 bytes
cannot fit 16 locators of at most 512 bytes: its test deliberately exceeds both;
an internal checked-sum witness separately exercises aggregate overflow/refusal.

## Exclusions and authority

No I/O, environment, filesystem, process, clock, network, WSL, registry, Tauri,
callbacks, reader emission, payloads, serialization, storage, native handles or
executable plans. No SourceRef, AttemptRef or FinalizationId issuance. S1 remains
the reader emitter. Output clones preserve the original catalog allocation;
independent calls build fresh authority even with equal logical content. Downstream
finalization/admission must use the owned snapshot or clones, never a rebuild.
No production installation, histories or native APIs are accessed by tests.

## Contract-to-test acceptance matrix

| Contract | Executable coverage |
|---|---|
| A descriptors, requests, grants, deterministic refusal | planning: validation and permutation tests |
| B conditional/unresolved/unrecognized/no work | planning: state matrix and empty catalog |
| C lexical grammar, aliases, siblings, overlap | planning: locator tables and collision tests |
| D tags, order, authority, atomic refusal | planning + downstream: fresh/clone catalog admission |
| E c−1/c/c+1, checked totals and no truncation | planning: bounds; lib unit checked accumulation |
| F redaction, no capability surface | planning: Debug sentinels; gate source/dependency inspection |
| G unchanged S1/G4/G3B literal conservation | downstream: explicit test-only adapter and reader scripts |

Native Windows/WSL and real-source tests are prohibited and NOT_RUN. Fixture tests
cannot establish native security, actual consent, root identity or production readiness.
