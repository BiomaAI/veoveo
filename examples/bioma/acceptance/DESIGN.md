# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.3.0 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |
| Native MCP clients | Official Rust SDK MCP 2026-07-28 Discover, Tasks, resource reads and request-scoped subscriptions over Streamable HTTP; bearer issuance uses the runtime-owned fixture adapter |
| Installed DuckDB | Existing `installation-verify --scope duckdb` consumer, owner contract CSV/schema/catalog/usage and Artifact provenance over normal Gateway OAuth; private `veoveo.ai/installed-duckdb/v1` receipt |
| Installed Stream | Owner-library run identities, completion products and analysis results; cross-replica DeepStream replay requires NVIDIA hardware |
| Public Stream and Reason callers | Existing GPU scenarios use normal OAuth at the installation's operator MCP endpoint; private `veoveo.ai/stream-public-caller/v1` and `veoveo.ai/reason-public-caller/v1` inputs, with corresponding `public-consumer/v1` JSONL observations |
| Candidate process ownership | Python standard-library Linux pidfd APIs; private `veoveo.ai/candidate-launch/v1` JSON with camelCase invocation, process group and start ticks; no public protocol extension |
| Installation input | Required `--installation` file using `veoveo.ai/installation-target/v1`; identities and endpoints validated against its control-plane document |
| MCP and authentication | Repository conformance CLI over public HTTPS, the hosted MCP 2026-07-28 profile, OAuth token exchange, exact Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.ai/uav-sim-acceptance/v12` JSON with bounded typed mission, world, video and observation parameters |
| Recording analysis | Stream and Reason contract features, Recording catalog and checked live-part snapshot types; `veoveo.ai/uav-recording-acceptance/v2` JSON result |
| Stream live sessions | Server-owned live-session types imported through the Stream library's isolated `contract` feature |
| Stream notifications | Official workspace Rust MCP SDK; public resource subscriptions, uncached typed reads, cancellation and reconnected baselines; `veoveo.ai/stream-notification-acceptance/v1` result |
| UAV control grants | UAV-owned grant, permission and collection types through its isolated contract feature; a 60-second and 100-page traversal limit; Map owns mobility-profile references |
| Route admission | Map-owned route request, position, policy and result types through its isolated contract feature; a current aviation route Task runs before flight commands |
| World readiness | UAV-owned simulation, tile and camera state types; the scenario timeout includes reads and warmup waits; invalid bindings, failed resources and unsupported encoders fail immediately |
| Browser automation | Headed Chrome DevTools Protocol, hardware-backed WebGPU or WebGL, shared browser assertions owned by `testing/browser-smoke` |
| GPU workload evidence | Existing NVIDIA resource identity, NVENC source and concurrent workload assertions; software rendering is rejected |
| Installed View fixture | `veoveo.ai/view-installed-fixture/v1` closed JSON; nonzero Kubernetes UIDs, opaque resourceVersions, immutable ConfigMap bytes and qualified image release; relative release paths resolve beside the declaration |
| View fixture preparation | `veoveo.ai/view-fixture-preparation/v1` JSON utility receipt; preparation produces no smoke acceptance outcome |
| Installed Timeseries evidence | `veoveo.ai/installed-timeseries/v2` private JSON with typed catalog intents and expected/actual members, response digests or redacted SDK failures, one forecast dispatch intent, gateway/native Task and Artifact identities, delivered completion, RRD/provenance/usage observations and separate cleanup results |
| Installed Frames reference evidence | `veoveo.ai/frames-installed-reference/v1` private JSON with fixture-owned synthetic stream references, Create/Publish intents, retained world/revision readback, current consumer refusals and awaited client cleanup |
| Installed Frames evidence | `veoveo.ai/frames-installed-evidence/v3` camelCase JSON with typed dispatch intents and observations, owner world/revision/native usage identities, gateway Task routes and separate cleanup outcomes |
| Frames crash fixture | `veoveo.ai/frames-crash-fixture/v1` closed private JSON binds the installation/control-plane digest and selected Deployment/ReplicaSet/Pod/container runtime identities |
| Frames crash marker and receipt | `veoveo.ai/frames-ready-for-crash/v1` and `veoveo.ai/frames-recovery-evidence/v1`; private settled typed snapshots, caller-owned watch progress, operation outcome and awaited cleanup |
| Installed CPU protocol fixture | `veoveo.ai/installed-protocol-fixture/v1` closed JSON binds exactly DuckDB, Timeseries, Frames and Media loopback origins, upstream Host authorities, workload UIDs/images and the selected control-plane SHA-256 |
| Installed CPU protocol receipt | `veoveo.ai/installed-protocol/v2` private JSON records all four owners’ expected/observed tool identities before validation, requests, HTTP status/body digest, MCP code/message digest, administrator health/audit correlation and separate operation, cleanup and qualification results |
| Installed transport | Maintained curl HTTP/1.1 with explicit empty Host, no redirects or proxy, 15-second requests and 64 KiB responses; RFC 9112 missing-Host 400 and installation Host admission 421 |
| Installed CPU Host fixture | Closed owner JSON with `deployment`, `pod` and `container`; private absolute regular-file input capped at 64 KiB; database identity enters through the DuckDB owner type |
| Installed CPU Host evidence | `veoveo.ai/installed-cpu-host/v1` JSON with gateway Task identity, admitted process/drain identities and separate completion, retained-payload and connection-cleanup results |
| Installed View evidence | `veoveo.ai/view-installed-evidence/v1` private JSON; official OAuth/MCP Tasks and the shared selected-container Kubernetes drain profile |
| View process interruption | Docker Engine HTTP API 1.44 subset over the fixture's active local Unix socket; version and owned-container inspection precede one STOP signal, followed by durable claim admission and the existing restart checks |
| World publication | Frames-owned immutable revisions and typed frame URIs; UAV-owned validated installation binding; `veoveo.ai/uav-world-publication/v2` JSON receipt with the output file SHA-256 |
| Evidence | `veoveo.ai/uav-showcase-acceptance-evidence/v6` JSON and revision-qualified captures; `veoveo.ai/uav-showcase-phase-outcomes/v3` records domain and visual outcomes and preserves successful visual measurements even when the domain phase fails; both mark Reason `not_run` because its acceptance runs separately |
| Focused restart stages | `veoveo.ai/uav-live-view-restart-stage/v1` records each accepted container restart and its headed hardware browser observations before the next restart begins |

Browser upload, UX and resume reports use their current `/v2` tags and share one
closed report declaration in `src/reports/artifact_upload.rs`. It embeds the full
Artifact owner upload receipt. Receivers check occurrence URI equality and distinct
selected upload/occurrence identities before browser or SDK effects. Workspace
Markdown and Artifact consumer reports also use `/v2`, because their copied receipt
and observation shapes changed. The Python consumer uses an explicit camelCase
stdin peer model and the SDK's Artifact identity/address admission; its embedded
GatewayInternalIdentity keeps the frozen signed identity profile. These decoder
controls establish receiving behavior, while headed hardware acceptance and
installed SDK transfer measurements require their actual declared environments.

The recording-analysis operator report emits camelCase fields under its v2 schema. It is
a one-way JSON output; this repository has no report decoder. Operators must replace
stored v1 reports and update external readers when coordinating the writer cut.

## Ownership

This crate validates the Bioma reference installation. Generic agent authoring and
runtime lifecycle remain in the gateway, platform store and agent manager. No Bioma
identity or migration procedure enters those components.

`knowledge_config` checks that the installation's indexer can discover its approved
sources, that every registered tool call is denied, and that its membership ends at
the Operations viewer role. User profiles expose Knowledge reads independently of
the machine's indexing profile. Collection approvals belong to this installation;
server protocol declarations are compared with the generic development catalog.

`record_restore` qualifies bound single-record insertion and transactional batch
restore in disposable database fixtures. It compares complete native record values,
rejects a conflicting final record, and checks that the failed transaction leaves
no partial writes. Its SQL lives under `tests/queries/record_restore`.

## Qualification

Run `cargo test -p veoveo-bioma-acceptance` for composition and native fixture checks.
The database harness provides isolated credentials, timeouts and owned cleanup.
No private installation exports or running pilot workloads are prerequisites for
these checks.

## Installation Assertion Delivery

The `artifact-upload-consumers` case checks public OAuth upload/MCP interoperability
and separately runs the Python SDK inside the selected installation Pod with a
delegated signed identity. The latter verifies typed metadata, resolution, one-member
catalog pages within 60 seconds and 256 pages, isolated-tenant refusal and selected
bytes. Its request context must pass Artifact service
audit admission. The private `veoveo.ai/artifact-upload-consumer-acceptance/v3`
receipt preserves both consumer profiles. Installed execution requires its own
qualification.

`--service-recovery` additionally selects `artifactConsumer.serviceRecovery` and an
isolated, stable fixture WorkContext. The case admits the Artifact service's
namespace, Deployment, Pod and container before retaining a tiny Task-bound write
capability across one service replacement. It redeems the original request and
replays its idempotency key, requiring the same occurrence, metadata and bytes.
The public Artifact index probes the service after replacement. Artifact MCP
documents alone cannot establish service readiness. Capability secrets stay in
the child process; receipts record identities and expiry. The API provides no
capability revoke or Artifact deletion operation, so the fixture uses short expiry
and retention and records those retained effects for reconciliation. The scenario
declares Kubernetes mutation because this explicitly selected mode restarts a
service; its default consumer profile performs no restart.
The installation must identify the service behind both its private SDK origin and
the public Artifact index. The selected container is `artifact-service`. The
synthetic delegated caller keeps empty clearance, so this profile admits only a
WorkContext whose output policy has no inherited classification or labels. Public
OAuth capability issuance and interrupted-write recovery require separate cases.
Recovery work has a 300-second local limit within the scenario's execution budget.
The actual MCP client's consuming close future is registered before acquisition
and retained across cancellation. The existing process-group owner drains the SDK
child. Both use the owner's shared cleanup deadline. Cleanup records preserve
interrupted work and original close outcomes; an empty slot cannot establish a
successful close. The private JSONL journal records those facts even when the
outer scenario drops the recovery operation.

The nondefault `smoke` feature supplies `installation-smoke` and `installation-browser-smoke`. Their single assertion sources own the multi-domain Store, Artifact, Gateway and installation relationships they exercise. The default `native` feature selects the composition and Store dependencies used by the existing native controls. `smoke` includes that profile. The separate `reports` feature exports the same upload report declarations through only Artifact contracts, shared types and browser hardware contracts; it excludes composition, Store, Tasks, service implementations and CDP execution. Production domain packages do not depend on this composition.

Tracked `smoke/scenarios.json` entries name actual Cargo targets and preparations. Dispatch preserves native command arguments, hardware/service prerequisites and owner cleanup. Browser assertions use the shared headed CDP and hardware admission mechanics; pure browser sampler controls establish behavioral coverage only. Interrupted remote operations keep the owning reconciliation requirement.

## Compiler Candidate Ownership

The candidate probe records the selected Pod UID and a generated private executable,
App and cache path before publishing remote effects. It keeps the remote process
leader and Linux start ticks once launch is observed. Cleanup compares the current
Pod identity and holds Linux pidfds for the leader and each admitted group member
before signalling. A pidfd must still name the inspected process on both sides of
its `/proc` admission. Python and kernel pidfd support is probed before launch.
The launching process publishes a private `veoveo.ai/candidate-launch/v1` receipt
with its invocation, group and start ticks before executing the candidate; cleanup
never adopts a process by executable name. A changed
identity refuses cleanup and keeps the dispatch intent for reconciliation.

The owning cleanup action and the normal completion path use the same removal
implementation. Local `kubectl exec` groups drain separately from remote process
settlement. Missing process identity, expired cleanup or an unconfirmed removal
cannot authorize another candidate launch. Installation GPU checks still use the
existing NVIDIA runtime and require hardware acceptance.

## Stream Cross-Replica Acceptance

The installed `stream-gpu` smoke accepts `--replica-pods WRITER OBSERVER` to pin
dispatch and observation to different ready Pods of the same ReplicaSet and image.
The operator supplies both Pods; the harness does not scale the Deployment. Each Pod
keeps the installation's required GPU resources. The probe subscribes through the
official SDK to the Task, run and result on the observer while the Task is working.
It requires a completion notification and subsequent run/result invalidations, then
compares completed Task envelopes and typed result reads across both Pods. A fresh
subscription must recover the same completed state, and both subscriptions must cancel.
Pod identities, image IDs and restart counts must stay unchanged throughout the case.

The probe writes `stream-replicas.json` in the requested work directory, including the
created run ID on failure. It dispatches once and waits at most 300 seconds. A Task
that finishes before observer admission fails qualification. Reuse of the report path
is rejected so an uncertain dispatch cannot trigger an automatic replay. Port forwards
and SDK connections belong to the harness and close when it ends. The ordinary replay
checks still require processed GPU frames, detections and Artifact outputs. Live-session
delivery belongs to its GPU owner and requires its separate installed case.

Run against two already-ready Pods with a fresh output directory:

```sh
cargo xtask smoke stream-gpu \
  --installation examples/bioma/installation-target.json \
  --pipeline-id <installed-object-detection-pipeline> \
  --producer-key-secret <installed-recording-producer-secret> \
  --replica-pods <writer-pod> <observer-pod> \
  --work-dir <new-output-directory>
```

The ordinary scenario also requires an environment file containing the installation's
internal assertion signer and Store credentials, plus an installed producer Secret.
Tokens stay out of arguments and receipts. Local port 8797 addresses the writer and
18797 the observer; both must be unused before the run. The sample generator, recording
producer and Artifact checks use the same prerequisites as ordinary `stream-gpu`.
This direct-server check does not qualify public Gateway load balancing or live-session
routing. It complements the installed public MCP and composed-flight cases.

Stream and Reason's direct GPU fixtures sign a synthetic automated service identity
with the current Work Context policy revision and the selected profile's audience.
Their assertion includes a checked `GatewayRequestContext` with fresh audit correlation,
matching actor, client, scopes and expiration. Artifact admission requires that context
before issuing read or write capabilities. These fixtures do not establish public OAuth
authentication; public acceptance uses the installation's registered client.

## Public Stream And Reason Consumers

The existing GPU scenarios select their public caller profile through a private input
file. They dispatch once through the Gateway with a normal OAuth token and retain the
existing NVIDIA workload and domain-result assertions.

| Scenario | Input selector | Input schema |
|---|---|---|
| `stream-gpu` | `VEOVEO_STREAM_PUBLIC_CALLER_INPUT` | `veoveo.ai/stream-public-caller/v1` |
| `reason-gpu` | `VEOVEO_REASON_PUBLIC_CALLER_INPUT` | `veoveo.ai/reason-public-caller/v1` |

Each input declares `schema`, `installationTarget`, `endpoint`, `profile`,
`callerTokenFile` and `output`. Admission requires the same installation as the
scenario, its operator profile and its public `/mcp/{profile}` endpoint. The input,
token and new output journal use absolute private file paths. Candidate mode and
Stream's direct-replica mode cannot combine with this profile. Admission precedes
fixture effects. Recording production and Store fixture checks keep their declared
credentials; the public MCP consumer does not issue an internal assertion.

The official SDK creates the Task and listens to that acknowledged Task ID. The
private journal syncs dispatch intent and the acknowledged identity before waiting
for delivery. Completion requires the matching subscription identity, a delivered
Completed payload and agreement with `tasks/get`. The owner decodes the tool output,
constructs its typed result address and checks the domain result. A separate exact
resource subscription must deliver its initial current snapshot. The journal records
that observation separately from Task completion. Interrupted runs preserve intent
and known identities for reconciliation; reusing the journal path is refused.

SDK connections and subscriptions retain their consuming cleanup futures under the
existing lifecycle owner. Closing a subscription proves that listener closed. It
does not establish Task cancellation. Initial resource delivery does not establish
post-mutation invalidation, unfinished process recovery or cross-replica delivery;
those checks use their separate owner profiles.

## Composed Flight Acceptance

The focused [Flight client](../flight/DESIGN.md) owns the installed UAV flight, live Stream, Recording replay and hardware browser acceptance obligations. It compiles the single shared browser assertion source in this component, while its package graph excludes service implementations.

## Current Copied Reports And Map Bridge

Compiler acceptance receipts use `veoveo.ai/compiler-acceptance/v4` and
retain the complete checked Stream or Reason result alongside the selected service.
Component-scope reports use version 2 around the complete current installation
receipt; their receiver compares selected locked content and unselected state.

The browser Map wrapper and capture use version 3. `browser/map_workspace.rs`
constructs resources through Map builders and admits copied query requests through
the same contract types. The existing browser harness owns HTTP and browser cleanup.
Map source-query digests come from the Map contract's cursor-independent producer.
The hosted App requires network access to the credential-free OpenFreeMap Positron
and Dark HTTPS styles at `tiles.openfreemap.org`. Its CSP admits that declared
origin. Pure request controls do not qualify headed rendering or hardware graphics.

## Installed View Lifecycle

The existing `view-mcp` scenario retains its local default. Installed mode uses
`--installation`, `--installed-fixture` and `--evidence-output`. Ops stages one
immutable ConfigMap from the maintained local triangle, tileset and catalog, mounts
it read-only at `/fixtures` and `/etc/veoveo/view`, and supplies the selected
namespace, Deployment and Pod UIDs/resourceVersions and qualified image release.
The declaration uses `veoveo.ai/view-installed-fixture/v1`. A Google-only catalog
fails private admission before public calls. The scenario never replaces a catalog.
Fixture names use lowercase DNS labels and resourceVersions have no ordering. The
selected caller WorkContext must contain no existing Views before fixture creation.
Admission inspects both Pod command and arguments. It accepts the qualified image
entrypoint or its explicit `/usr/local/bin/view-mcp` executable and rejects catalog
overrides and wrapper commands.

`installation-smoke view-fixture-export --output <directory>` writes the same fixture
files and a `veoveo.ai/view-fixture-preparation/v1` receipt. It performs preparation
only and has no registered smoke scenario or acceptance outcome.

Normal Gateway OAuth creates independent official MCP clients for the same caller,
profile, tenant and WorkContext. A completed GPU capture must deliver a Task baseline
and unchanged JPEG bytes to that independent context and to a fresh context after
Pod replacement. The declared comparison WorkContext and distinct administrator
principal must receive the exact unknown-Task response for get/cancel. Their Task
listeners may acknowledge requested handles; a permitted Views collection baseline
anchors processing before a one-second interval excludes owner Task notifications.

The selected server uses the shared
[process-drain profile](../../../testing/support/DESIGN.md#installed-process-drain).
Its receipt records the actual old-container exit within declared grace and checks
replacement readiness independently. Evidence uses
`veoveo.ai/view-installed-evidence/v1`, preserves dispatch intent before restart,
and reports cleanup separately. Each client admission permits 15 seconds; lifecycle
assertions permit 240 seconds, followed by 30 seconds for owned View closure and
10 seconds for client shutdown. The first failure is redacted before receipt and
CLI output. Ops owns removal of its installed catalog and fixture WorkContext;
completed Task/frame products stay available for the declared recovery assertions.
View creation records dispatch intent before sending its request. A lost or invalid
reply does not permit another Create. Cleanup reads the same caller collection and
closes at most one View bound to the admitted composition, digest and local layer.
An empty collection before admitted replacement leaves cleanup unqualified. After
successful old-process drain and replacement, that collection must instead prove
the old in-process View absent; completed Tasks and frame products remain readable.
These owner cases require installed NVIDIA hardware;
CPU fixture and observer controls establish admission and lifecycle mechanics only.

## Installed DuckDB Consumers

The existing installation verifier has a focused CPU selection:

```sh
cargo xtask smoke installation-verify --scope duckdb \
  --installation /private/installation.json \
  --evidence-output /private/new-duckdb-receipt.json
```

The default `--scope full` keeps the complete installation's deployment and GPU
checks. The DuckDB selection reports only its owner scope and requires a new
absolute private receipt. It uses the normal operator OAuth client and a distinct
administrator principal/profile for foreign reads. The checked control plane must
expose DuckDB Tasks and its execute, ingest and export tools. Actual negotiated
Tasks and complete tool/template catalogs are admitted before mutation; missing
Artifact metadata or occurrence templates stop the case.

The case traverses complete typed database and usage pages before choosing its
fixture size. It dispatches at most 101 fresh database-create execute Tasks, one
four-row inline-CSV ingest and one small CSV export. Existing visible databases and
usage records reduce the number of seed Tasks; they are never modified. The selected
operator/profile must have exclusive DuckDB mutation use during the case: another
caller session creating Tasks would invalidate its new usage ledger. Ops must keep
Artifact/Time setup from dispatching DuckDB work under that identity until cleanup
finishes. Dispatch is serial. Each fresh database has an unpredictable owner-admitted name and a fixed
fixture table. The final catalog and usage traversals must both deliver multiple
pages, preserve ordering and include the selected fixtures. Schema readback checks
the source columns; export bytes must contain the submitted rows in order.

The export's DuckDB-presented Artifact metadata and current neutral metadata must
agree through the owner's presentation adapter, excluding only the transient
download URL. DuckDB's
public origin supplies the native Task identity, database, operation and row count.
The consumer compares bytes read through DuckDB and Artifact, records their digest,
and checks the export usage parent and fields. Every new corpus usage record must
match a fixture execute or ingest. Gateway Task routes stay opaque; the consumer
never derives a native Task ID from them. Foreign database and export-usage reads
must return the current owner's InvalidParams denial.

The receipt is created exclusively with mode 0600 and is limited to four MiB.
Catalog/resource intents and observed response classifications are persisted before
assertions. Every mutation intent is persisted before dispatch; Task admission and
completed output are retained before later awaits. A journal failure stops new
dispatch. The 600-second operation budget includes admission and observations;
requests use 15-second limits, catalogs and Tasks use 30 seconds, and each page
traversal stops after 32 pages. Existing corpora above 3,000 database or usage
members fail before writes. After operation cancellation, the caller still owns
its listener, clients and journal. Cleanup allows five seconds for the listener
and ten seconds per client outside the operation budget. Cleanup failure fails the
aggregate result. Unknown dispatched outcomes stay unresolved, and cleanup neither
replays mutations nor cancels domain Tasks.

Fixture database names, mutation outcomes, Task IDs and the Artifact are retained
in the private receipt. The case leaves its databases, Tasks, usage and Artifact
for operator reconciliation; it deletes no unrelated data. It checks public owner
isolation and schema/catalog behavior, without inspecting physical owner-directory
or storage metadata formats. The accepted `installed-host` graceful-restart case
is separate. Preterminal cancellation, destructive recovery, headed Workbench
acceptance and full-installation qualification remain separate gates.

## Installed Shared MCP Host

`installed-host` selects the CPU Task and server-process lifecycle case in the
existing `installation-smoke` harness:

```sh
cargo xtask smoke installed-host \
  --installation examples/bioma/installation-target-initial.json \
  --database <existing-owned-database> \
  --installed-fixture /private/duckdb-fixture.json \
  --evidence-output /private/new-host-receipt.json
```

The installation supplies the registered operator client's
`VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE` and `VEOVEO_SERVICE_CLIENT_KEY_ID`.
Normal gateway `private_key_jwt` OAuth resolves the selected profile, scopes and
Work Context. The database must already belong to that caller. The fixture declares
the existing `duckdb-mcp` Deployment, current Pod and `duckdb-mcp` container.
The selected-process helper admits their live UID and resourceVersion identities
before effects. The fixture is a closed private JSON input; Pod names are operator
inputs rather than defaults.

The case reads the owned database resource, dispatches one read-only query and
requires delivered completion for its gateway Task identity. It then drains that
selected server process once and requires the same completed Task and payload after
restart. The private receipt separates dispatch or restart uncertainty, observed
completion, retained payload and connection cleanup. Lost responses do not trigger
another query or restart. Receipt creation refuses occupied destinations before
dispatch. The Task and database data stay with their owner.

The owner case has a 180-second deadline from admission. OAuth and MCP connection
each allow 15 seconds; Task delivery allows 30 seconds. The ordinary server drain
uses its fixed 30-second profile and the shared 75-second observation budget.
Connection cleanup runs outside the operation timer. Listener cleanup allows five
seconds and client cleanup ten seconds. The smoke
descriptor includes declared compilation in its separate execution budget.
This case requires a ready CPU target and cluster process-mutation authority.
Whole-installation deployment and GPU checks belong to `installation-verify`;
domain cancellation, concurrency and additional lifecycle cases keep their owning
acceptance gates. Task delivery here establishes completed state without requiring
an observable Working transition.

## Installed CPU Protocol

`installed-protocol` invokes the existing `installation-smoke` harness with required
`--installation`, `--installed-fixture` and `--evidence-output` inputs. Preparation
selects only the assertion executable and `gateway-smoke-support`. Ops supplies four
existing loopback port-forwards; the case creates no domain fixtures and changes no
cluster configuration. The supplied installation still undergoes its full target and
control-plane admission. Results cover the four selected CPU servers rather than every
server registered in that installation. Recording gRPC and Media generation/completion
are separate qualification cases; this case does not claim them.

Before authenticated discovery, an isolated HTTP client sends the four SDK-encoded
list requests without bearer, cookie or session headers to the checked public MCP
endpoint. Every request must return 401. The client disables proxies and redirects,
uses a three-second connection budget and a fifteen-second request budget, and bounds
each response to 64 KiB. The receipt records the response status and body digest
before the assertion; a transport or receipt-write failure cannot establish denial.

The private fixture names each owner, server slug, resource scheme, mount, loopback
origin, allowed Host authority and Deployment/Pod/image identities. It binds the gateway
workload and raw control-plane SHA-256. Admission requires exactly DuckDB, Timeseries,
Frames and Media with distinct origins and nonzero UIDs. Checked catalog manifests and
profile exposure supply tool and prompt expectations; fixture input cannot choose them.
The operator registration must use FullMcp. Tool expectations exclude only the manifest’s
declared compatibility helpers, which that client surface does not expose.
The fixture is a regular file at most 64 KiB with no group or world permissions.
Ops verifies the live workload UIDs, images and port-forwards before dispatch; decoding
these declarations performs no Kubernetes lookup. The control-plane digest identifies the
local checked input; Ops separately admits the live selected profiles and four-server
projection. This case does not prove byte equality with the full live ConfigMap or final
source/image/configuration closure. Both distinct operator and administrator
identities must be admitted before either OAuth token exchange.

Maintained curl sends HTTP/1.1 directly to each mounted health route with an explicitly
empty Host, a wrong Host and the admitted service Host. Missing Host must return 400,
wrong Host 421, and mounted health/readiness 200. One unsigned Media callback carries a
fresh syntactically valid Task identity and must return 401 with `invalid signature`.
The request cannot create a Task because the owner authenticates signatures first.
Curl clears inherited environment credentials and proxy settings, disables redirects,
and permits only HTTP loopback URLs admitted before launch.

Normal operator OAuth reads all four complete discovery catalogs and selected owner
documents, contracts and completions. A direct invalid-resource request must return the
owner's current -32602 and message digest. Normal administrator OAuth reads selected
server docs and the complete health inventory. The same health route under the operator's
own profile must deny it with 403. One bounded public Audit export proves the actual
administrator actor, client, profile, Work Context, target and Allowed health admission
inside the run interval. Its healthy sealer and complete export framing are required.
The administrator prompt catalog qualifies Isolate only when an actually attempted
unavailable upstream appears in typed degradation metadata and healthy prompts survive.
Absent such a failure, the receipt explicitly marks that required observation unqualified.

The aggregate operation receives 600 seconds. Catalog traversal has 30 seconds; OAuth,
connections and individual requests have 15 seconds. Request identity is persisted before
waiting, and observed status or protocol failure before assertion. Transport failures
cannot satisfy denial checks. HTTP bodies and MCP messages enter the receipt only as
SHA-256 digests. The exclusive output file has mode 0600. SDK clients and any pending curl
process stay owned outside the aggregate timer and receive five seconds each for awaited
cleanup. Operation success, cleanup and overall qualification are distinct receipt fields.
The descriptor's 3600-second preparation-inclusive and 180-second cleanup ceilings preserve
room for compiler prerequisites; they do not extend the owner operation deadline.

## Installed Timeseries

`timeseries-installed` selects `installation-smoke timeseries-installed` with
required `--installation` and `--evidence-output` inputs. The existing installed
harness resolves the declared operator and distinct administrator through normal
Gateway OAuth. Preparation selects `installation-smoke` and its sole prerequisite,
`gateway-smoke-support`; installed Timeseries and Artifact services supply the domain
processes. This entrypoint launches no local server or provider.

Before dispatch, the case requires official Task support, `timeseries__forecast`,
and both Artifact occurrence and metadata templates. The installation must permit
Artifact template discovery and the corresponding resource reads; selecting the
Artifact scheme in a profile does not supply those policy permissions. The receipt
records each catalog request before awaiting its complete collector. It persists
the returned tool names or template URIs and the declared required members before
asserting coverage. Missing-member diagnostics name the required members. SDK MCP
errors record code and message digest; available HTTP failures record status.
Transport errors, catalog-validation failures and actual deadlines stay distinct.
An interrupted request has no invented response status. These observations contain
no bearer, headers or response bodies.

The case dispatches one inline-CSV forecast using the owner’s NaiveTrend model. It
requires delivered Task completion, then admits the forecast output and RRD Artifact
through their owner contracts. It reads both the Timeseries-presented RRD and neutral
Artifact occurrence, validates RRD Scalar values and timelines, and compares current
Artifact metadata. The four observations follow source rows 0–3 with values 1–4;
the two forecast steps have means 5 and 6 and must agree with the typed preview.
These are data checks and establish no playback or rendering qualification. Provenance and caller-scoped usage must correlate
with that same forecast. The native Task ID comes from the produced metadata and
provenance, independently of the opaque Gateway Task route. Exactly one usage record
must name that native Task, model `timeseries/naive-trend` and quantity 4 in
`source_row` units, with no charge, currency or provider identifiers. Owner-admitted
usage metadata must describe one series, horizon 2 and `rerun_rrd` format. Its
recording timestamp is observed rather than fixed by the fixture. A distinct administrator
uses ordinary OAuth to verify the native Task-usage read refusal. Artifact metadata
is read through the Artifact MCP contract-only library’s `metadata_uri` builder and
admitted as the owner’s bare metadata resource response. This contract edge adds no
Artifact runtime integration. The Task, Artifact and usage are retained domain writes.
The scenario performs no Kubernetes mutation.

`--lifecycle-input` selects a second typed forecast workload for cancellation or
connection replacement. The original four-row consumer assertions still run.
Cancellation requires an observed unfinished Task, dispatches once and requires
current cancelled state; a completed Task cannot qualify it. Connection replacement
requires completion of the original Task with matching identity, creation time,
owner output and Artifact provenance. That Task may finish during reconnection;
the receipt preserves its observed states. This proves connection independence.
Unfinished server-restart recovery and selected cross-replica qualification require
additional owner cases. These data and lifecycle checks establish no GPU or visual
rendering guarantee.

The baseline operation has a 300-second deadline. OAuth and MCP connection admissions
have 15 seconds each, catalogs have 30 seconds each, and individual reads have 15
seconds. Task event delivery has at most 180 seconds within the operation deadline.
Usage traversal permits 32 pages per baseline or final read and admits at most 3,000
existing entries before the forecast. Typed intents and known outcomes are written
to a private receipt capped at 1 MiB before execution advances. Subscription and client
handles stay outside the cancellable operation future. Each listener and SDK client
has ten seconds for cleanup after success, failure or timeout. The descriptor keeps the 3600-second preparation budget
and 180-second cleanup allowance; these outer budgets include artifact preparation
and do not extend the owner operation deadline. The optional lifecycle workload has
its own 300-second operation limit. Baseline success, including SDK cleanup and the
written receipt, must precede its dispatch. Both phases register retained listener
and connection slots with the outer scenario owner. Their original consuming close
futures survive operation cancellation and share the owner's cleanup deadline.
Receipts preserve pending or failed earlier closes across reconnection. A timeout
cannot authorize another forecast dispatch or removal of an unresolved retained
fixture.

## Installed Frames References

`frames-reference-installed` selects the existing installation smoke executable
and requires `--installation` and `--evidence-output`. Preparation builds that
executable and its sole prerequisite, `gateway-smoke-support`. The case uses normal
operator OAuth and needs no administrator credential. Frames access requires the
selected profile's scopes, current WorkContext membership and writer policy.

The case creates one uniquely identified world and publishes one revision carrying
typed dynamic-stream references under the fixture-owned `frames-fixture` scheme.
Frames stores these declared addresses without fetching a producer. The case reads
the installed revision back, requires the references to survive owner admission
unchanged, and checks the current dynamic conversion refusal. The current public
UAV `SimulationWorldBinding` consumer must also refuse that read-back revision. These assertions qualify reference admission and the current
consumer refusals. They establish no live producer route, timestamped conversion,
UAV process, GPU execution or rendering behavior.

The world and revision are two retained domain writes. The case journals mutation
intent before dispatch, keeps uncertain outcomes and never retries a mutation to
resolve an expired observation. Its private receipt path must be new. The owner
operation has a 120-second deadline; OAuth, connection, read and tool requests have
15 seconds each within it. The caller owns the SDK client beyond operation
cancellation and allows ten seconds for awaited cleanup. The descriptor preserves
the 3600-second preparation-inclusive and 180-second cleanup budgets. It declares
network and credentials, with no billed effects, headed graphics, NVIDIA workload
or Kubernetes mutation. `clusterMutation:false` does not describe the retained
domain writes as read-only.

## Installed Frames

`frames-installed` selects `installation-smoke frames-installed` and requires both
`--installation` and `--evidence-output`. It uses the existing `frames_installed`
assertions through normal gateway OAuth and the installed operator profile.
Preparation selects the assertion executable and `gateway-smoke-support`, its sole
OAuth helper prerequisite. Installed services supply the domain processes; this
entrypoint builds and launches no local service binaries and activates no provider.
The descriptor preserves the 3600-second preparation-inclusive budget and
180-second cleanup budget. The owner operation and connection deadlines below
apply within that outer preparation budget.

`frames-mcp` selects the isolated local fixture and keeps its existing native
service prerequisites. It accepts no installed-mode inputs. The installed case
requires the installation's operator and administrator credentials. Their admitted
principals must differ. Both clients use ordinary gateway OAuth; Frames gateway
access requires the installation's declared scopes (`operator:use` in the reference
profile), current WorkContext membership and writer policy. Frames declares no
additional domain scope.

Installed execution consumes an acknowledged worlds-resource baseline before
creating a fresh named world, then requires a separate invalidation and a complete
metadata read. It closes that listener after the authored update. Two identical
concurrent publications must return one creation and one replay of the same revision.
Two distinct trees then compete against that revision's expected head; one must
publish and the other must return invalid params. Each publication must preserve the original world identity, name, description and
creation time. The winning head and revision must contain its submitted admitted
tree. Resource reads, source digests and frame metadata must agree with that result. An immutable revision
subscription must terminate with invalid params after any protocol acknowledgement.

The case creates 101 caller-visible worlds and follows actual returned cursors across
100-item world pages. It checks distinct ordered identities and complete metadata for
every owned world. World completion must return 100 of the 101 matching names with
`hasMore`; revision and frame completion must bind their typed parent contexts.
Missing world or revision context cannot produce frame candidates.

One direct conversion and 101 real batch Tasks traverse the published static tree.
Every batch uses `artifact:false`. Groups contain at most four mutation dispatches.
The client observes acknowledged Task notifications, verifies completed current
state and admits the typed result before comparing operation-resource provenance
and immutable revision sources. Conversion points and provenance must name the
requested robot frame rather than merely agree with one another. Gateway Task identities use `CanonicalTaskId`;
usage page entries supply native `TaskId` values. Actual usage records correlate
these results through their operation identity, carry one point without a monetary
charge and must appear in multiple usage pages. Foreign direct and Task-linked
operation reads and exact Task-usage reads must return `INVALID_PARAMS` (`-32602`)
under negotiated MCP 2026-07-28, with the owning Frames missing-resource message
digest. The case rejects the earlier `-32002` wire code. Foreign usage pages cannot
expose the owned Tasks. An acknowledged exact usage listener must deliver the
same current code with the owner's subscription-specific message digest in its
terminal response.

A usage listener consumes its acknowledged baseline before batch dispatch, requires
an authored invalidation after the first completed group and then closes. Each Task
group closes its listener after result verification. Handles and confirmed closure
states remain owned outside the cancellable operation timer. Each caller catalog has
a 32-page traversal ceiling; excessive existing data fails admission to the fixture
budget. More than four unrelated new usage entries during the run fails observation
instead of attributing them to this fixture.

The operation deadline is 300 seconds from entry. OAuth issuance, MCP discovery,
typed resource reads and initial listener admission have at most 15 seconds within it;
resource notifications have 15-second waits and each Task group has a 30-second
delivery limit. The selected operation budget includes dispatch and observation.
Foreign read and subscription probes each have a 15-second limit. Completion and
remaining admissions share the 300-second operation deadline. Final owned listener/client cleanup runs outside that timer with five seconds per
handle. The outer descriptor keeps its 180-second cleanup allowance.

Before network admission, the scenario exclusively creates a mode-0600 receipt and
retains its open handle. Version 3 records each typed intent before its dispatch,
then records independently admitted results, gateway Task routes, native usage
reports and cleanup status as each request resolves. A slow sibling cannot hide an
already received response when the operation timer expires. After an observation
write or validation failure, the collector retains responses from requests already
started and refuses unstarted siblings. The first failure stays owned outside the
operation timer. A failed intent write refuses dispatch. At most 207
mutation requests may be sent: 101 creates, four publications, one direct conversion
and 101 batch Tasks. A successful run retains 101 worlds, two revisions, 102
operation records and 101 Tasks with actual usage. Input-required, lost or malformed
responses never trigger another dispatch. Concurrent identical publications are
predeclared assertions, each with its own persisted intent.

Each foreign probe records its method, typed resource target and expected
`ObservedFailure` before dispatch. The receipt records the received MCP code and
message digest or HTTP status before enforcing the assertion. Unexpected success,
notifications, missing terminal responses and transport failures have separate
classifications. Receipt diagnostics exclude response bodies, credentials and raw
error formatting. An expired probe preserves its requested method and target.

An assertion or awaited cleanup failure with all mutation responses settled records
`fixture_failed_settled`. A dispatched mutation without an admitted response or a
Task without a terminal result records `mutation_unresolved`. Cleanup status is
recorded separately, and the aggregate outcome cannot pass when owned cleanup
fails. Receipt persistence preserves the first operation or observation failure.

Frames exposes no public world/revision deletion. The fixture's append-only records
remain owned by the selected caller; connection cleanup does not claim their
removal. Tasks follow the server's ordinary seven-day TTL; expiry can retire their
usage and hide Task-linked operations. The case does not extend that retention. Receipts from earlier versions remain intact, and this scenario writes only
version 3. Source controls qualify receipt admission, dispatch limits, cursor
continuation and notification handling. Installed delivery requires a reviewed run
against the selected gateway and credentials.

This CPU case publishes static transforms. It does not qualify an external stream
producer's routes or timestamped transform delivery. Current-format operation
readback does not establish crash or restart recovery.


## Installed Frames Settled-State Crash Recovery

`frames-recovery` uses the existing installation-smoke binary and only its normal
`gateway-smoke-support` prerequisite. It requires `--installation`, `--crash-fixture`,
`--marker-output` and `--evidence-output`. The checked installation must provide
operator and administrator identities with distinct principals. The private fixture
selects the declared Frames Deployment and regular container; Ops verifies its live
UIDs, image and runtime process identity before execution. Fixture decoding also
checks those declarations and the local control-plane digest before OAuth or mutation.
A read-only live namespace, workload, Pod, container and image admission must succeed
before the first domain mutation.

One fresh append-only world, one immutable revision, one direct conversion and one
completed artifact-false batch Task establish the retained fixture. Typed reads bind
the world, revision, both operation records, completed Task and payload before the
crash. Task identity and completion payload are persisted before listener cleanup.
The caller owns that listener outside the 300-second aggregate operation timer and
awaits cancellation for up to five seconds on every outcome.

The exact-container native Kubernetes watch must arm after those mutations settle.
Only a complete parsed `ready_for_crash` marker with matching fixture identity and
current receipt digest permits Ops to perform its separately authorized process kill.
An empty reserved marker file is not readiness. The shared observer checks same-Pod,
same-image recovery, the old instance's exit 137, one restart increment and Ready;
positive reported signals must be 9, and unavailable signal information is explicit.
The Ops dispatch receipt supplies the actual kill correlation. OOM or rollout
replacement cannot qualify this case. The observer admits the public contract route
after Kubernetes readiness, within the operation deadline.

A fresh normal OAuth client then rereads the retained typed world/revision, direct
and Task operation provenance, completed Task and exact payload. Another admitted
principal must receive the owner denial for both operation records. SDK TTL and poll
hints may change; stored completion identity, status and payload must agree. After
these reads, a final live fence must confirm the same replacement container, image,
Pod and single restart increment before the receipt can pass.
Watch and all client cleanup execute outside the operation timer, with five-second
bounds. Failures preserve known mutations and partial watch progress in the private
receipt and never authorize another dispatch or kill. The fixture stays append-only.
This case qualifies completed-state crash persistence and owner isolation on selected
installed images; in-flight mutation recovery and other owners require their own cases.
