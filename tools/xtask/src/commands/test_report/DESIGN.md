# Local Evidence Receipts

Status: the v3 recorder, source input planner, immutable index and explicit coverage
verifier are implemented. Installed harness adapters and release-lock profile
composition remain CE-06 work.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| `veoveo.io/test-receipt/v1` | Repository-owned immutable JSON record of one command attempt, its admitted inputs, environment requirements and result |
| `veoveo.io/local-test-report/v3` | Repository-owned index of receipt references; source validity is evaluated independently for each check |
| `veoveo.io/test-check-catalog/v1` and `veoveo.io/test-coverage-profile/v1` | Owner-reviewed exact command/input declarations and required check identities with explicit freshness/bindings |
| Cargo metadata v1 | Local package graph and declared extra test inputs; conservative dependency traversal includes development and build dependencies |
| SHA-256 | Actual input bytes, modes, canonical command identity and receipt integrity; source provenance is a separate Git revision |
| Git | Tracked and non-ignored local source discovery, exact revision and dirty provenance; content identity does not depend on worktree location |
| JSON and RFC 3339 | Closed typed records and UTC observation timestamps |
| Native process execution | Commands run without an intervening shell; the recorder owns evidence publication, while the command owns its assertions |

## Ownership

The recorder executes checks and publishes receipts. The planner selects dependency
closures from command semantics and checked-in owner declarations. Harnesses own
assertions, service lifecycle and cleanup. A command-line option cannot remove an
input dependency or reclassify a failed check into qualifying evidence.

`cargo xtask test-report run --name NAME -- COMMAND` remains the command entrypoint.
Running a check always executes it; preserving an earlier valid result is an index
operation, not an implicit decision to skip a requested command. The display command
presents committed evidence. Release verification additionally evaluates required
coverage, environment bindings and explicit freshness limits.

## Input Planning

Every receipt includes its planner version, command, source manifest and toolchain
identity. Each check includes its owning catalog file; a declaration for an unrelated owner
does not enter that boundary. The manifest records actual bytes and file modes. A Git clean filter cannot
hide different materialized bytes from a check. Source additions, removals and changed
symlink targets participate in invalidation. A source link cannot admit bytes outside
its declared repository boundary.

Cargo checks include selected local packages, their transitive dependency sources,
workspace manifests, lockfile, Cargo configuration and pinned toolchain. Owner-reviewed exact commands in `testing/evidence-checks/` add fixtures and
generators outside the package. Existing package image-input declarations also enter
the test closure. The initial catalog is conservative and covers only admitted
source checks; an unknown invocation cannot invent a narrower input claim. The initial
lockfile boundary is conservative; dependency-specific lockfile pruning requires
separate qualification. Unknown commands or undeclared external dependencies retain
the whole-repository boundary and cannot supply narrower release coverage.

Console checks include their application, configuration, dependency lock and the
source/generator boundary for generated contracts. A service check does not inherit
Console source merely because both are present in the same Git checkout. A change to
shared contract sources invalidates both relevant closures.

The planner itself is an input. A changed declaration, matcher or dependency rule
invalidates results produced by the previous rule. The command identity is distinct
from its display name; renaming a check cannot conceal a failure of that command.
Changing test selection creates a different coverage claim.

## Environment And Freshness

Source compatibility and current runtime acceptance are separate facts. Toolchain
requirements identify the actual executable/version used. Runtime checks bind their
declared provider artifacts, topology and observation time. Installed coverage needs
exact runnable image and configuration identities. Visual coverage also needs the
owning harness's hardware evidence.

Secret values never enter receipt bodies or public hashes. A secret requirement uses
an installation-owned reference or revision. Unsupported command/environment shapes
remain explicitly unqualified for reuse; an opaque invocation must not acquire a
false identity by omitting an input. Arbitrary executable output is not a public
diagnostic record. Diagnostics references have a declared owner and retention path.

Admitted source execution clears `LD_LIBRARY_PATH`; Cargo constructs loader paths
for its own children. Unsupported compiler/environment overrides and external Cargo
configuration prevent reuse. Version observations identify the selected Rustup
toolchain without persisting arbitrary environment values. This is local source
evidence, not a hermetic build attestation.

Displaying an observed pass does not renew it. Release verification rejects expired
runtime evidence, changed installation artifacts, missing required checks and current
known failures. Local command evidence remains distinct from a release attestation.

One display or coverage verification reads and validates each immutable receipt once.
The selected results come from that validated read, including validation of superseded
and failed history. Cargo scopes share one freshly observed dependency graph within
that invocation. A later invocation observes the graph again. Before and after check
execution always use independent observations, preserving mutation detection. These
optimizations do not cache environment authority or extend receipt freshness.

## Publication And Selection

One run writes one new receipt with a unique run ID. Publication uses an atomic,
create-only operation. The index references a receipt by ID and content digest; it
does not rewrite the receipt when another check finishes or inputs later change.
The short index lock covers reload, merge and atomic replacement. Command execution
does not hold that lock. A crash may leave an unindexed complete receipt, which a
subsequent index rebuild can recover. It must not expose a partial qualifying result.

The latest completed attempt for an identity determines its current result. Failed
attempts remain immutable history. A late run whose inputs changed during execution
records that fact and cannot replace a qualifying run for newer inputs. Parallel
publication preserves every result. Input-equivalent checkouts can share receipts
while retaining each run's actual source revision.

V2 results remain historical in Git. They are not silently converted into scoped
receipts. The first v3 run creates the new index, and required checks earn new receipts. The
old aggregate implementation has been removed. Complete unindexed attempts prevent
presentation of an older index until publication recovers them.

Before/after fingerprints detect mutations across a run. They do not prevent an
adversarial change and restoration during execution; immutable execution snapshots
would be needed to claim that stronger property.

## Qualification

Owning Rust tests exercise unrelated Console edits, shared schema/toolchain changes,
new and deleted inputs, symlink confinement, cross-worktree identity, concurrent
publication, mid-run changes and failed-attempt history. Composition checks reject
missing coverage, stale runtime observations and changed installed configuration.
The repository's ordinary recorder commands then qualify the implementation itself.
