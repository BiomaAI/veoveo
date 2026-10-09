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
Owner tests outside source conformance can select their own fixture environment
variable through `input_from`; the same path and size checks apply.
`knowledge::bearer_header` admits a private token file for owner HTTP requests and
returns a sensitive Authorization header. Malformed header diagnostics omit the token.

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

Map accepts `layer`, `feature`, two `publicationLayers` IDs, and `releases` containing
`dataset`, `original` and `candidate` IDs. The three
layers must differ and their titles must begin `Source conformance `. The first
layer contains the mutable feature. The publication driver publishes one of the other
layers before restart and the second afterwards. It dispatches each publication once
and returns its typed knowledge URI. The shared checker verifies both immutable
members. Cleanup archives all three layers through the version-checked tools;
Map preserves the publications of archived layers.
The releases must form an active/staged pair from one source in a disposable
non-routing dataset. Its registered source must be an Authority Vector with
`synthetic_test` authority and a name beginning `Source conformance `. Acquisition
supplies the normal digest-based release labels. The probe
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

## Time Consumers

`servers/time-mcp/tests/gateway_consumers.rs` selects a separate closed input with
`VEOVEO_TIME_CONSUMERS_INPUT`. Its `installation` uses the fields above and selects
`time-mcp`; this read-only case does not inspect or restart that Deployment. Supply
the expected `authority`, existing versioned `calendar` and `epoch`, two to sixteen
`resolutions` with typed `request`, `expected` instant and `expectedUtc`, and distinct
`zoneIds` and `scales`. Resolution vectors include both subsecond endpoints. The
calendar contains recurrence windows and the epoch and expectations bind the selected
current authority. Keep that authority unchanged throughout qualification.
The `source` is an existing typed HTTPS `TimeSource`. Supply `administrator` with
the installed admin `profile` and private `tokenFile`; the owner derives its read-only
source route under the same public Gateway origin.

Run local admission controls with `cargo test -p veoveo-time-mcp --test gateway_consumers`.
The installed case is ignored and runs through `cargo xtask smoke time-installed-consumers`
only when installation access is authorized. Its create-new private receipt uses
`veoveo.ai/time-consumers-acceptance/v1` and records each completed check, failure
category and separate connection cleanup. Typed request traces retain complete-response
and error digests, available MCP codes and HTTP statuses without response bodies or tokens.
Interrupted body reads record status without a complete-response digest.
MCP response digests cover re-encoded admitted typed values; admin HTTP digests cover
the complete received body bytes.
Task delivery, restart recovery, authority
activation and conflict rollback require additional owning cases.

## Restart And Cleanup

The readiness-only restart helper verifies the target workload and records its current Pod names.
It dispatches `kubectl rollout restart` once, waits for rollout completion and for
every old Pod to disappear, then checks the same Deployment UID, a newer generation
and unchanged positive replica count. It then reads the owner's contract through the
public Gateway connection. An explicit MCP request bypasses the SDK resource cache.
Service routing can lag Pod readiness, so transport and internal errors permit at
most forty read attempts, 250 milliseconds apart, within ten seconds. Other protocol
errors fail immediately. The read must return the selected URI's nonempty text body.
K07 checks retained state and dispatches its second mutation only after this read.
A restart has a total deadline of 75 seconds, including the
55-second rollout watch, 15-second deletion watch and public-read readiness deadline.
Cancellation kills the local
command; it does not repeat a mutation with an uncertain outcome. Command failures
report status without arbitrary process diagnostics that could expose credentials.

The optional selected-container API in
[testing support](../support/DESIGN.md#installed-process-drain) additionally admits
namespace, Deployment and Pod identities through the ReplicaSet owner chain. It
establishes the old Pod watch before a UID/resourceVersion-tested JSON Patch,
requires actual successful container termination inside configured grace, and
returns a typed drain receipt. Existing readiness-only consumers cannot claim
process exit from Pod deletion. The receipt qualifies server-process drain; GPU
throughput and visual acceptance belong to their owning scenario.

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
result. Each owner writes the available report before cleanup and defers any report
error until cleanup and connection shutdown have both been attempted. Failed
requirements remain inspectable when reconciliation fails. Transport failures and cleanup
failures fail the test. A source report records K checks only; full hosted-server
certification, access-policy coverage, Knowledge retrieval and GPU acceptance each
have separate owning harnesses.
