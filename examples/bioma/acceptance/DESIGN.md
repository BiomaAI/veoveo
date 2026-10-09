# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.3.0 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |
| Native MCP clients | Official Rust SDK MCP 2026-07-28 Discover, Tasks, resource reads and request-scoped subscriptions over Streamable HTTP; bearer issuance uses the runtime-owned fixture adapter |
| Installed Stream | Owner-library run identities, completion products and analysis results; cross-replica DeepStream replay requires NVIDIA hardware |
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
| Installed Frames evidence | `veoveo.ai/frames-installed-evidence/v3` camelCase JSON with typed dispatch intents and observations, owner world/revision/native usage identities, gateway Task routes and separate cleanup outcomes |
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
