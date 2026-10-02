# Installed Source Harnesses

## Standards And Protocols

| Contract | Supported profile |
|---|---|
| MCP `2026-07-28` | Official Rust SDK Discover and Streamable HTTP through the installation's public gateway |
| `ai.veoveo/knowledge-source` | Shared K01–K08 checks for one explicitly selected source; K09/K10 require owner review |
| `veoveo.ai/mcp-conformance-report/v1` | Source-only requirement results and the observed endpoint implementation |
| Installation target | Repository-owned typed public origin, Kubernetes coordinates and expected workload names from `veoveo-deploy-contract` |
| Kubernetes Deployment API | One requested rolling restart, native rollout/deletion watches and typed before/after readiness checks |

## Ownership

These helpers serve owner-local installed tests. `knowledge.rs` reads typed fixture
inputs, admits private token files, connects through the official SDK and writes the
source report. `restart.rs` controls one Deployment named by the installation target.
`tools.rs` sends typed owner requests through checked gateway tool names.
They do not choose domain data, grants, service names or expected search results.

Each server's integration test owns those choices and imports its library types.
Artifact owns the grant mutation driver that its metadata test and Reason's finding
test share. Native fixtures under `testing/fixtures` stay independent of installation
identities and credentials.

## Inputs And Commands

Set `VEOVEO_SOURCE_CONFORMANCE_INPUT` to an absolute JSON file path. Files are limited
to 64 KiB and deserialize through the owner's closed input type. Artifact accepts
`artifact` and `grantee` fields; Reason accepts `analysis` and `grantee`. These grant
cases also require an `administrator` object with `endpoint` and `tokenFile` fields.
Every owner requires this `installation` object:

| Field | Value |
|---|---|
| `installationTarget` | Absolute path to a valid installation-target JSON file |
| `endpoint` | Public HTTPS MCP endpoint for the ordinary caller |
| `callerTokenFile` | Absolute path to a private regular file containing that caller's bearer token |
| `deployment` | Expected Deployment name from the target |
| `output` | New absolute report path in an existing output directory |

Caller and administrator endpoints must match the target's public origin. Token files are limited to
64 KiB, must be nonempty and must have no group or other permissions on Unix. Tokens
never enter command arguments or reports. HTTP requests disable redirects and use
ten-second connection and 65-second request timeouts. Supply credentials that remain
valid through the run and cleanup.

The caller needs source read scopes and access to all fixture members. The
administrator needs Artifact grant and revoke permission. Choose a disposable
Artifact or completed Reason analysis, and a subject with no existing grant to its
selected Artifact. The harness refuses an existing grant before it mutates anything.
The selected Deployment must have positive replicas with every requested replica
updated and available. Its selector must name the component fixed by the owner test.
All declared source collections need populated members for read qualification.

Time accepts an `events` array containing two distinct event IDs. Both events must
start scheduled, remain in the future during qualification, belong to the caller
and have names beginning `Source conformance `. The fixture cancels one event before
restart and the other afterwards. Cleanup cancels both and verifies their final state.

Map accepts `layer`, `feature`, two `publications` containing `layer` and `publication`
IDs, and `releases` containing `dataset`, `original` and `candidate` IDs. The three
layers must differ and their titles must begin `Source conformance `. The first
layer contains the mutable feature. Archiving the other two tests publication
removal. Cleanup archives all three layers through the same version-checked tools.
The releases must form an active/staged pair from one source in a disposable
non-routing dataset, with version labels beginning `source-conformance-`. The probe
activates the candidate and restores the original after restart. Cleanup restores
the original pointer and refuses to overwrite an unrelated release selection.

Map also accepts a typed `search` request, `expected` geography members and a
`restrictedTokenFile`. The restricted caller must be authenticated at the same MCP
endpoint but lack the source's dataset-read authority. The shared checker requires
tool denial and full/conditional denial of every expected hit. Ordinary searches
must return the supplied geography members exactly.

Run one owner case at a time:

```sh
cargo test -p veoveo-artifact-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-reason-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-time-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-map-mcp --test gateway_source_conformance -- --ignored --nocapture
```

These tests require the `mcp` feature and are ignored by ordinary native test runs.
They need network access to the public gateway, `kubectl` access to the selected
context and namespace, and ready source dependencies. They perform no inference;
a reused GPU analysis keeps its original execution evidence.

## Restart And Cleanup

The restart helper verifies the target workload and records its current Pod names.
It dispatches `kubectl rollout restart` once, waits for rollout completion and for
every old Pod to disappear, then checks the same Deployment UID, a newer generation
and unchanged positive replica count. A restart has 75 seconds, including a
55-second rollout watch and a 15-second deletion watch. Cancellation kills the local
command; it does not repeat a mutation with an uncertain outcome. Command failures
report status without arbitrary process diagnostics that could expose credentials.

The source runner has a fifteen-minute deadline and each K07 probe has 90 seconds.
Cleanup runs after the source deadline expires; an outer test timer must not cancel
that reconciliation. SDK connection shutdown has a separate ten-second limit.
Artifact grant cleanup has
30 seconds: read the selected subject's current state, revoke it if present and
verify absence. This reconciliation also runs after a timed-out source run or
uncertain grant response. An externally killed test needs operator reconciliation
of the explicitly selected subject; no destructor claims asynchronous cleanup.
Time reconciles its two events concurrently within 30 seconds. Map allows two minutes
for cleanup, with 35 seconds for release restoration and 25 seconds for each layer.
It attempts every cleanup even if a previous operation failed.

The report uses a create-new, owner-private file and is synced before checking its
result. Failed requirements remain inspectable. Transport failures and cleanup
failures fail the test. A source report records K checks only; full hosted-server
certification, access-policy coverage, Knowledge retrieval and GPU acceptance each
have separate owning harnesses.
