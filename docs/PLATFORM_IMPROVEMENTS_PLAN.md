# Platform Reliability And Operability Implementation Plan

Status: implementation plan. The plan records accepted direction and delivery order.
it does not claim that the planned contracts or commands exist. Each phase requires a
separate implementation change with its own tests and documentation closure.

Baseline: Veoveo main `b6e2069f` on 2026-08-14. The input was the reviewed client
package `veoveo-platform-improvements-2026-08-14`, containing thirteen requests and
six reference diffs. The diffs describe intent from an earlier integration snapshot.
They are not a merge series.

This plan strengthens agent recovery, resource use, deployment isolation, standalone
MCP App hosting, model configuration, GPU admission, provenance, spatial correctness,
and private dependency builds. It preserves the current provider webhook-only job
completion contract and the mandatory hardware-GPU boundary.

## Standards And Protocols

| Standard or profile | Plan boundary |
|---|---|
| Model Context Protocol `2026-07-28` | sole protocol profile for Veoveo-owned servers, the gateway, Rig, and first-party clients. Resource reads use JSON-RPC `Invalid Params` for unknown URIs and subscriptions use request-scoped `subscriptions/listen` |
| JSON-RPC 2.0 | protocol error envelope for malformed requests, unknown resources, authorization-independent invalid parameters, and internal failures that cannot produce a tool result |
| MCP Tasks extension, SEP-2663 | durable detached work, task updates, input requests, terminal results, and canonical task identities |
| MCP Apps SEP-1865 / `ext-apps` `2026-01-26` | `ui://` application resources, `text/html;profile=mcp-app`, sandboxed frames, host context, lifecycle notifications, and the `postMessage` bridge |
| Veoveo reactive App resource adapter | repository-owned projection from final MCP resource notifications to contentless App wakes. It is not part of SEP-1865 |
| JSON Schema Draft 2020-12 | closed agent, deployment, resource-reference, model, provenance, and GPU-admission schemas |
| OAuth 2.1 draft 13, RFC 6750, RFC 7523, RFC 8414, RFC 8707, RFC 9207, and RFC 9728 | browser and machine authorization, `private_key_jwt`, metadata discovery, resource indicators, issuer validation, and protected-resource discovery |
| RFC 9110 | HTTP authority, header, cache, redirect, and error behavior for the BFF, gateway, and hosted servers |
| RFC 7946 | public longitude/latitude coordinate order used by Map GeoJSON and the Map-specific DuckDB Spatial axis policy |
| DuckDB 1.5.5 and its Spatial extension | typed `POINT_2D` distance path, `geometry_always_xy`, materialized scoring, and restart-stable Map analytics |
| Kubernetes/K3s 1.36.2 and Helm 4.2.3 | selected-release ownership, rendered Secret closure, managed-field inspection, hooks, mutation, and deployment receipts |
| Kubernetes server-side apply managed fields | pre-mutation field-owner conflict detection and reviewed ownership transfer. Helm remains the release owner |
| NVIDIA DRA Driver for GPUs 0.4.1, `resource.nvidia.com/v1beta1`, CUDA, and NVML | exact GPU identity, full-device or MIG capacity, memory admission, and hardware evidence. The repository implements only its declared qualified DRA profile |
| Docker Buildx 0.35.0, BuildKit 0.31.2, and Dockerfile frontend 1.25.0 | secret-mounted private Git credentials and trust inputs, cache isolation, SBOM, and maximum-mode provenance |
| `veoveo.io/deployment/v6` and `veoveo.io/deployment-lock/v6` | delivered deployment baseline. Selected-release ownership makes a hard cut to v7, and accelerator-memory admission makes a later hard cut to v8 |
| `veoveo.io/image-build-plan/v2` and `veoveo.io/image-build-run/v2` | current typed image plan and execution evidence. Credential source paths and bytes never enter either document |

Every dependency or infrastructure component touched during implementation must be
checked against its authoritative upstream release and pinned to the latest stable
compatible version. An immutable fork revision remains valid only when the required
behavior has no stable upstream release and the reason is recorded beside the pin.

## Intended Outcome

Veoveo will make long-running work recoverable without asking a model to reconstruct
platform state. An accepted task, operator input, or resource update survives token
rotation, process exit, and lease transfer. Correctable input errors remain inside the
current episode and carry bounded guidance.

Deployment will separate planning from mutation. A selected release will own a closed
set of Kubernetes objects, Secret requirements, and hooks. The executor will prove that
set against the complete lock before it issues a write. An unselected source will not
receive a mutation request.

Map will use one explicit longitude/latitude interpretation. Standalone MCP Apps will
reuse the Console security boundary and upstream stream fanout. Model and GPU settings
will be admitted through closed typed contracts. Frames and Time will return references
that downstream compilers can admit without searching a catalog. Private Git inputs
will reach dependency fetch only through BuildKit secrets.

## Delivery Ledger

| ID | Priority | Delivery decision | Primary phase |
|---|---|---|---|
| `AGENT-CONTINUATION-001` | P0 | retain the delivered durable runtime. Audit lineage and add crash-window evidence before adding new persistence | 1 |
| `TOOL-DIAGNOSTICS-002` | P0 | implement against final MCP errors and current Rig, not the obsolete reference module | 1 |
| `DEPLOY-SCOPE-003` | P0 | add selected-release ownership through deployment schema v7 | 2 |
| `SECRET-CLOSURE-004` | P0 | require a mutation-free rendered closure before any Kubernetes write | 2 |
| `APP-HOST-005` | P1 | add the standalone route through the existing Console BFF, frame, bridge, and session | 4 |
| `STREAM-FANOUT-006` | P1 | retain the delivered auth-scoped per-URI listener pool. Add capacity and multi-tab evidence | 4 |
| `CREDENTIAL-REFRESH-007` | P0, elevated from P1 | repair the missing agent resource listener and make replacement readiness atomic | 1 |
| `RESOURCE-DISCOVERY-008` | P1 | add typed references, bounded search, canonical handoff, and read budgets | 5 |
| `REASONING-CONTROLS-009` | P1 | add omitted, disabled, and named effort modes with provider capability validation | 6 |
| `ACCELERATOR-ADMISSION-010` | P1 | add declared persistent and peak memory through deployment schema v8 | 7 |
| `SOURCE-PROVENANCE-011` | P2 | add used-source Frames references and effective Time authority references | 8 |
| `SPATIAL-TYPING-012` | P1, elevated from P2 | correct Map axis and distance semantics before further spatial query expansion | 3 |
| `PRIVATE-BUILD-INPUTS-013` | P2 | add one BuildKit-secret contract across every Rust builder family | 9 |

`STREAM-FANOUT-006` does not authorize a second browser or gateway fanout system. The
current Console BFF already reference-counts one upstream listener per auth scope and
resource URI. Implementation changes only its bounds, refresh evidence, and standalone
host coverage unless tests prove a different deficiency.

## Repository Invariants

- The change is a hard cut at every revised schema, resource grammar, cursor domain,
  command, and configuration field. No alias or compatibility reader is added.
- Provider work remains webhook-completed. No phase adds status polling, recovery
  polling, or a polling fallback.
- GPU workloads continue to fail closed without the selected NVIDIA device, driver,
  and accelerated backend. Memory admission cannot select a CPU or alternate model.
- Browser acceptance uses a headed browser with a hardware-backed WebGPU adapter or
  WebGL context. SwiftShader, llvmpipe, and software rasterizers are rejected.
- Known contract shapes use Rust types and closed enums. Raw JSON remains confined to
  provider-owned payloads, Kubernetes object parsing before typed projection, and other
  genuinely open boundaries.
- Authentication, authorization, transport, timeout, and internal diagnostics remain
  generic at the model boundary. No phase exposes database errors, credentials,
  provider bodies, hidden reasoning, or policy internals.
- A selected deployment operation reads the complete lock but mutates only its selected
  ownership set. Read access to unselected sources does not grant mutation authority.
- Secret values never enter a deployment plan, error, log, receipt, build argument,
  cache key, SBOM, provenance statement, or test fixture.
- Smoke lifecycle, retry, assertion, evidence, and cleanup logic remains in Rust. The
  `xtask` command only builds and dispatches the typed harness.

## Reference Patch Policy

| Patch | Use during implementation | Rejected behavior |
|---|---|---|
| `0001-typed-spatial-distance.patch` | port the axis setting, typed overload, materialized score, and cursor-domain hard cut | mechanical application against the older analytics layout |
| `0002-explicit-reasoning-controls.patch` | use its request-rendering test as a narrow example | a `none`-only enum, model-specific comments, and missing provider capability validation |
| `0003-actionable-resource-read-errors.patch` | retain the bounded-normalization and safe/generic classification intent | the deleted `resource_read.rs` baseline, the superseded resource error constant, and old Rig APIs |
| `0004-compiler-ready-frame-provenance.patch` | use as a starting diff after the source-use audit | returning every preloaded revision or retaining an unvalidated digest string |
| `0005-shared-app-frame-sandbox.patch` | extract one shared sandbox constant with a behavioral test | treating a source-text search as the only security proof |
| `0006-standalone-mcp-app-host.patch` | port the same-host route, BFF endpoints, and shared frame composition | duplicated URI grammar, raw principal-subject display, and brittle source-layout assertions |

## Phase And Dependency Order

| Phase | Depends on | Exit condition |
|---|---|---|
| 0. Baseline and contract locks | none | accepted decisions, protocol versions, owners, and hard-cut surfaces are recorded |
| 1. Agent connection and correction | 0 | resource wakes survive refresh. Safe invalid input is corrected in one episode. The crash matrix passes |
| 2. Deployment safety v7 | 0 | selected release and Secret closure pass zero-write and preservation acceptance |
| 3. Map spatial correction | 0 | axis, typed distance, cursor invalidation, and restart evidence pass |
| 4. Standalone Apps | 1 for stable resource listeners | any authorized App opens through the shared host. Multi-tab use stays within explicit capacity |
| 5. Resource identity and discovery | 1 | canonical handoff, lookup, pagination, and byte/read budgets pass across selected domains |
| 6. Model reasoning | 1 | provider capability and exact request-rendering tests pass without fallback |
| 7. GPU memory admission v8 | 2 | every co-located group fits declared capacity with headroom on real hardware |
| 8. Frames and Time provenance | 5 for canonical references | downstream admission accepts exact source references without catalog search |
| 9. Private build inputs | 0 | clean-cache builds pass in every Rust family and evidence contains no canary bytes |
| 10. Integrated closure | 1 through 9 | source gates, cluster checks, GPU checks, browser checks, docs, examples, and deletion audits pass |

Phases 1, 2, and 3 may proceed in parallel after phase 0 because their code ownership is
disjoint. Phase 9 may also proceed independently. The commit order within each phase is
fixed below. A later concern must not be folded into an earlier contract commit merely
to reduce the number of schema revisions.

## Phase 0: Baseline And Contract Locks

### Delivered state to preserve

The agent runtime already persists wakes, episodes, task watches, input requests,
retention pins, and outbox events. Main also detaches accepted MCP Tasks from bounded
model episodes through `agents/kernel/src/background_tasks.rs`. Task settlement creates
the terminal wake transactionally, and episode completion consumes claimed wakes while
releasing task retention pins.

The Console BFF already owns an auth-scoped MCP client pool. Its App subscription table
maps browser subscription UUIDs to resource URIs, opens one upstream listener for the
first subscriber, and cancels that listener after the final subscriber leaves.

Deployment v6 owns exact source revisions and release lists, but its `ResourceSet` is
profile-global. `profile-up` validates the full profile and then applies the namespace,
GPU allocator, raw manifests, ConfigMaps, managed Secrets, gateway activation, and all
Helm releases. It checks the installation gateway Secret only after earlier mutations.

Map loads the trusted Spatial extension but leaves `geometry_always_xy` at the engine
default. Its distance queries use generic point construction, and source-feature
queries repeat their score expression in filtering, cursor comparison, projection, and
ordering. Cursor query digests do not name the distance algorithm revision.

### Contract preparation

1. Update `mcp/contract/DESIGN.md` in the first protocol-changing commit. Record the
   safe diagnostic data subset, canonical resource reference, wire caps, and final MCP
   error mapping.
2. Update `deploy/contract/DESIGN.md` before generating deployment v7. Record ownership
   keys, selected-release planning, Secret closure, field conflict behavior, receipts,
   and the prohibition on touching unselected resources.
3. Update the owning server `DESIGN.md` beside Frames, Time, and Map when each output or
   algorithm changes. Each design keeps its `Standards And Protocols` section current.
4. Update `mcp/apps-extension/DESIGN.md` when the standalone host becomes an advertised
   browser surface. The reactive resource adapter remains a Veoveo extension rather
   than an ext-apps claim.
5. Record any necessary Rig API addition before changing the exact Git pin. Verify
   whether a stable upstream release contains the required listener and request hook.
   Keep the fork only when no stable release supplies the behavior.

### Exit evidence

- The accepted and rejected patch behavior is represented in design text.
- Every new public or installation schema has one owner and one version transition.
- No implementation phase relies on an unstated legacy alias.

## Phase 1: Agent Connection, Correction, And Continuation

### 1.1 Restore declared resource listeners

Current `GatewayConnection::rotate` creates a replacement Rig guard, records the
manifest subscription count, publishes the new epoch, and closes the old guard. It does
not open a `subscriptions/listen` request or connect resource notifications to
`WakeBus`. The manifest promise is therefore unfulfilled on initial connection and on
refresh.

Implementation ownership:

| Concern | Owner |
|---|---|
| final MCP listener handle and accepted-filter result | Rig MCP adapter or a minimal typed Rig API addition |
| token mint, stale threshold, epoch publication, listener swap | `agents/kernel/src/connection.rs` |
| resource-update notification to durable wake | kernel connection listener and `agents/kernel/src/wake.rs` |
| declared filter validation | `agents/kernel/src/manifest.rs` |
| task watcher reconstruction on an epoch change | `agents/kernel/src/tasks.rs` |

The replacement connection becomes live only after all of these steps succeed:

1. Mint one fresh access token through the configured client-credentials flow.
2. Connect the replacement MCP client and verify Discover through the existing Rig
   lifecycle.
3. Build one deterministic `SubscriptionFilter` from the complete sorted manifest URI
   set. Duplicate URIs have already failed manifest validation.
4. Open `subscriptions/listen` and require the server's successful listener response
   before treating the filter as active.
5. Start the notification pump. Only `ResourceUpdatedNotification` values whose URI is
   in the accepted filter become durable resource wakes. List-change notifications
   invalidate discovery state but do not fabricate a domain resource wake.
6. Publish the resolver and listener as one `ConnectionEpoch`.
7. Cancel the previous listener with bounded acknowledgement, then cancel its guard.

The live state owns the guard, listener cancellation handle, listener task, token
timestamps, and epoch. A failed token mint, connect, listener acknowledgement, or pump
startup leaves the previous live state untouched. Cleanup failure after a successful
swap is observable but does not roll back the new epoch.

Credential freshness moves from episode-start-only behavior to the MCP request
boundary. The adapter checks the stale fraction before dispatch. Concurrent requests
share one serialized refresh. An authorization response permits at most one forced
refresh when the transport can prove that the request did not reach domain execution.
A mutating tool call is never replayed after an indeterminate dispatch.

### 1.2 Add the current-profile resource access boundary

The kernel needs one governed resource adapter that can call `resources/read` through
the active MCP client without exposing an unrestricted protocol peer to model code.
Place connection-independent policy, read accounting, content validation, and model
error mapping in a focused kernel resource module. Keep token rotation and listener
lifecycle in `connection.rs`.

Introduce closed types with these responsibilities:

| Type | Responsibility |
|---|---|
| `ResourceReadLimits` | maximum contents, item bytes, response bytes, cumulative episode bytes, reads per episode, reads per family, wall time, and pagination depth |
| `ResourceReadLedger` | episode-local atomic accounting and the remaining safe budget reported after rejection |
| `SafeCorrectionDiagnostic` | bounded code, kind, requested URI, normalized guidance, and `automatic_retry = false` |
| `ResourceReadFailure` | controlled invalid resource, prohibited content, budget exhausted, unavailable connection, authorization, timeout, transport, or internal failure |

The adapter admits only absolute governed resource URIs. It rejects credential-bearing
URIs, fragments, browser-only `ui://` content, local file and network schemes, HTML,
event streams, and content that exceeds a configured bound. Binary resource bytes are
not inserted into the model context unless a domain contract explicitly admits their
bounded MIME type.

Unknown resource URIs remain JSON-RPC `Invalid Params` (`-32602`) on the MCP wire. The
kernel may project a safe, bounded invalid-resource message to the model when the
upstream error contains only admitted diagnostic data. It discards upstream error data
by default. Authorization, internal, timeout, and transport failures use fixed generic
messages.

Domain tool validation follows the existing final-profile rule. A schema-valid call
with bad domain input returns a completed `CallToolResult` with `isError: true` and
actionable structured content. A protocol failure remains a protocol failure. The same
classification must survive official Tasks.

### 1.3 Keep correction inside the episode

Rig receives `SafeCorrectionDiagnostic` as a non-retryable tool input result. The model
may issue a corrected call in the same episode while its turn, tool, byte, and wall-time
budgets remain. The kernel does not launch an outer episode retry for a controlled
input error.

Lifecycle compare-and-set errors expose only the current allowed state, record
revision, accepted field names, and a stable error code. Storage text and query details
remain internal. The shared server handler path should own this conversion rather than
duplicating it across domain servers.

### 1.4 Audit continuation lineage before extending the schema

The audit traces operator message, episode, detached Task, multi-round input request,
terminal task wake, consuming episode, retention release, and outbox receipt. Existing
records remain authoritative. Add a new operator-root or continuation receipt type only
when this trace cannot identify one root and one terminal consume transaction without
parsing free text.

If a gap exists, add strong identities to the agent runtime and platform-store
migration. The root is persisted before episode dispatch. Child tasks and input
requests carry it. Terminal consumption writes one durable receipt in the same
transaction that acknowledges the wake and releases retention. A unique database key
rejects duplicate receipt creation.

The audit does not replace canonical MCP Task IDs, wake IDs, episode IDs, or input
request IDs with a universal string. Those identities retain their current contracts
and reference the root through typed fields.

### Agent acceptance matrix

| Test | Required proof |
|---|---|
| initial connection | the exact manifest filter is acknowledged and every selected resource update creates one durable wake |
| successful rotation | the replacement listener is live before epoch publication and the previous listener receives bounded cancellation |
| failed rotation | the prior epoch continues serving requests and resource wakes. No partial epoch is visible |
| token expires during an episode | the request boundary performs one serialized refresh and every declared resource remains subscribed |
| concurrent refresh | many callers cause one token mint and one listener replacement |
| invalid resource | the requested URI and normalized safe guidance reach the model within the byte cap |
| protected failure | authorization, transport, timeout, and internal strings do not reach model-visible output |
| same-episode correction | one intentionally wrong URI is followed by a successful corrected read without another episode |
| task completion crash | termination after task settlement but before wake consumption yields one terminal wake and one consumption receipt after restart |
| wake claim crash | an expired lease reclaims the wake without double consumption |
| input answer crash | answer and wake remain atomic across restart |
| budget exhaustion | the machine-readable result reports exhausted dimension and remaining permitted actions without performing a read |

Focused source gates:

```sh
cargo test -p veoveo-agent-kernel
cargo test -p veoveo-agent-runtime
cargo test -p veoveo-mcp-contract
cargo test -p veoveo-task-runtime
```

Integration evidence uses the Rust agent smoke harness and a real SurrealDB instance.
The token-expiry scenario needs a short-lived test issuer and the final Gateway MCP
path. It must not replace resource-notification evidence with polling.

## Phase 2: Deployment Safety And Selected Ownership

### 2.1 Hard cut deployment v6 to v7

Selected release ownership changes the controlled profile shape and therefore requires
`veoveo.io/deployment/v7` and `veoveo.io/deployment-lock/v7`. The implementation removes
v6 parsing when v7 lands. Examples, external fixtures, compatibility manifests,
generated schemas, lock files, and documentation move in the same change.

Add these typed concepts to `deploy/contract`:

| Type | Contract |
|---|---|
| `DeploymentReleaseId` | unique `(source, release)` identity within one profile |
| `KubernetesObjectKey` | group, version, kind, namespace or cluster scope, and name |
| `ReleaseOwnership` | exact raw manifests, ConfigMaps, managed Secrets, gateway activation objects, Helm-rendered objects, and hooks owned by one release |
| `SharedResourceOwnership` | an explicit profile-level owner for intentionally shared prerequisites. A selected operation can read but cannot rewrite it unless selected |
| `ReleaseHook` | closed phase, object identity, timeout, and ownership. Arbitrary shell hooks are excluded |
| `DeploymentReceipt` | profile and lock digests, selected release, planned ownership digest, executed object keys, hook results, runtime image digests, and terminal result |

Every mutable profile resource acquires exactly one owner. The validator rejects an
unowned resource, duplicate ownership, a namespaced object without the profile
namespace, a cluster-scoped object without explicit installation ownership, and an
ownership collision across selected and unselected releases.

The canonical repository command is:

```sh
cargo xtask smoke profile-release-up \
  --profile "$PROFILE" \
  --lock "$LOCK" \
  --source "$SOURCE_ID" \
  --release "$RELEASE_ID"
```

Full-profile `cargo xtask smoke profile-up` remains a distinct operation for initial
convergence. It invokes the same planner with all release identities selected. `xtask`
builds and dispatches the typed Rust deployment harness. The harness owns planning,
execution, assertions, evidence, and cleanup. There is no optional selector whose
omission changes the meaning of one command.

### 2.2 Build a pure deployment planner

Split the current orchestration into plan and execute modules before adding behavior.
The planner may read files, exact Git objects, registry manifests, chart archives, and
the Kubernetes API. It cannot invoke a mutating Kubernetes, Helm, Docker, Git, or hook
operation.

The plan contains:

- the complete validated profile and lock digest.
- the selected source revision and release identity.
- exact chart archive, values, and runtime image digests.
- rendered Kubernetes objects as typed metadata plus canonical bytes.
- the selected ownership set and selected hooks.
- unselected ownership keys used only for overlap rejection.
- managed and external Secret requirements.
- managed-field conflicts.
- the intended mutation order and a digest over the complete plan.

Source resolution reads the complete lock but checks out only source content needed by
the selected release. It does not fetch unrelated large-file objects. The planner
rejects an unresolved exact revision, mutable chart coordinate, missing registry
manifest, runtime/attestation digest confusion, or values injection into a chart that
does not declare the matching contract.

### 2.3 Close Secret requirements before mutation

Parse every selected rendered Kubernetes document. Walk the admitted workload kinds
and collect non-optional references from:

- container and init-container `env[].valueFrom.secretKeyRef`.
- container and init-container `envFrom[].secretRef`.
- Secret volumes and projected Secret sources.
- pod `imagePullSecrets`.
- Ingress and Gateway TLS certificate references.
- chart-owned custom-resource fields explicitly registered by the deployment contract.

Each requirement records object key, field path, Secret name, optional key, optionality,
and ownership class. Unknown workload kinds with unregistered Secret-bearing shapes fail
planning rather than passing an incomplete closure.

Managed Secret values are loaded and format-validated in memory during planning. The
plan retains only names, key names, value-presence flags, and validation results. An
external Secret read accepts authoritative NotFound as a missing dependency. Forbidden,
timeout, malformed, and transport results fail closed under distinct operator-facing
codes without suggesting that the Secret is absent.

Execution creates a managed Secret with an atomic create operation. An AlreadyExists
result triggers an ownership and content-digest comparison. It never falls through to
an overwrite. A concurrently created incompatible object aborts the release before any
later mutation.

### 2.4 Preflight managed fields

For every selected live object, compare the planned field set with `metadata.managedFields`.
The planner reports the conflicting manager, operation, API version, object key, and
exact field paths. It rejects conflicts before hooks and writes.

A separate reviewed ownership-transfer operation may use server-side apply only after a
dry-run proves that the selected field values and every unowned field remain unchanged.
It preserves Helm labels, annotations, and release metadata. Stored Helm manifests are
never replayed as raw Kubernetes YAML.

### 2.5 Execute the selected plan

The executor accepts only a plan produced in the same process and rechecks its profile,
lock, selected release, and live precondition digests immediately before mutation. It
runs only selected hooks and objects in declared order. Helm uses bounded atomic
behavior where the chart path supports it. A failed release cannot leave an unrelated
release rolling.

The executor must not issue `apply`, `patch`, `create`, `delete`, `replace`, Helm
upgrade, or hook requests for an unselected ownership key. This request-level audit is
the primary preservation proof. Before-and-after canonical owned-field bytes provide a
secondary proof for every unselected object. API-generated metadata and controller
status are recorded separately because controllers may update them without a Veoveo
request.

### Deployment acceptance matrix

| Test | Required proof |
|---|---|
| missing managed environment input | planning fails and the fake or audited Kubernetes client records zero writes |
| missing external Secret key | planning fails before Namespace or allocator creation |
| forbidden Secret read | failure is not classified as NotFound and no write occurs |
| concurrent managed Secret create | incompatible AlreadyExists aborts without overwrite |
| ownership overlap | selected and unselected object collision is named before mutation |
| selected release | only selected object keys and hooks appear in the execution receipt |
| unselected extension | no mutation request targets it and canonical owned-field bytes remain unchanged |
| managed-field conflict | manager and field paths are reported before hook execution |
| ownership transfer | dry-run and live result have zero unintended spec differences and retain Helm metadata |
| exact closure | live container image IDs equal runtime digests from the selected lock |
| idempotent rerun | a second selected apply produces no unexpected object or hook change |

Focused gates:

```sh
cargo test -p veoveo-deploy-contract
cargo test -p veoveo-deployment-smoke
```

Cluster acceptance runs against a disposable Kubernetes/K3s installation through the
Rust deployment harness. The missing-Secret scenario captures the API audit log from
before planning and proves that no mutating verb was issued.

## Phase 3: Map Spatial Axis And Distance Hard Cut

### 3.1 Type the engine axis policy

Replace a raw boolean proposal with a closed DuckDB runtime setting such as
`SpatialAxisPolicy::{Native, GeoJsonLongitudeLatitude}`. The default remains `Native`.
The non-native variant requires the trusted Spatial extension and configures
`geometry_always_xy` after loading that extension but before external access and
configuration are locked.

Map selects `GeoJsonLongitudeLatitude`. The general DuckDB MCP server explicitly
selects `Native`, because arbitrary SQL owns its own declared geometry interpretation.
The runtime readiness query reads `current_setting('geometry_always_xy')` and fails when
the effective setting does not match the selected enum.

### 3.2 Use one typed score

Map distance calls use `ST_Point2D` for longitude/latitude inputs. A geometry centroid
is explicitly cast to `POINT_2D` before `ST_Distance_Sphere`. Each distance query creates
one `WITH ... AS MATERIALIZED` score relation and reuses `distance_m` for maximum
distance, cursor comparison, projection, and ordering.

Cursor validation requires a distance for distance-ordered results and rejects one for
feature-ordered results. Non-finite and negative distance values fail before query
execution.

### 3.3 Invalidate pre-fix cursors

Domain-separate the request digest with a new constant such as
`veoveo.io/map/source-feature-query/v2\0`. The cursor decoder accepts only the new
domain. No v1 cursor reader or fallback is retained. Update the Map design and tool
description to state the axis and cursor hard cut.

### Spatial acceptance matrix

| Test | Required proof |
|---|---|
| runtime validation | non-native axis policy without the Spatial extension fails |
| effective setting | Map reads `geometry_always_xy = true`. DuckDB MCP retains its native setting |
| typed overload | one longitudinal degree at a representative latitude returns the admitted meter range |
| materialization | query-plan or instrumentation evidence shows one distance score per candidate row |
| cursor shape | distance and non-distance cursor fields are mutually consistent |
| hard cut | a cursor generated with the previous digest domain is rejected |
| restart | the same representative query and ordering survive database close and reopen |

Focused gates:

```sh
cargo test -p veoveo-duckdb-runtime
cargo test -p veoveo-map-mcp
```

The extension-backed tests require the repository-selected DuckDB Spatial extension.
A test skipped because the extension is absent is not acceptance evidence for this
phase.

## Phase 4: Standalone MCP App Host

### 4.1 Define one route authority

The BFF owns a strong `StandaloneAppRoute` parsed from `/apps/{server}/{page...}`. The
server segment uses `ServerSlug`. The page path is a bounded sequence of decoded path
segments ending in the exact application document name. Empty segments, dot segments,
encoded separators, backslashes, credentials, queries used as identity, control
characters, invalid UTF-8, and overlong paths fail before catalog access.

The BFF maps the validated route to one `ui://{server}/{page...}` resource and checks it
against the caller's authorized App catalog. Browser TypeScript never reconstructs this
URI. It receives the authorized `AppDescriptor` from a same-origin BFF bootstrap call.

### 4.2 Reuse the Console boundary

The standalone page imports the delivered `AppFrame`, bridge, theme projection, frame
endpoint, resource-read adapter, resource event stream, internal-link rules, and CSRF
client. Extract one exported sandbox constant and keep `allow-scripts` without
`allow-same-origin`. Console and standalone hosts pass that same constant to the same
component.

The entry document may remain public and `no-store`. Its first authenticated bootstrap
request uses the existing OAuth transition. Rename `ConsoleReturnPath` to the accurate
hard-cut name `BrowserReturnPath` and admit exactly `/console/` and `/apps/`. The parser
retains same-origin enforcement and the existing length bound. No second OAuth flow,
session cookie, bearer bridge, or callback endpoint is created.

The minimal host header displays the App's authorized title and a return link. It does
not display the raw principal subject. Any later display identity uses the bounded
principal display metadata already separated from authorization identity.

### 4.3 Keep one upstream stream per URI

The existing auth-scoped listener pool remains canonical. Add an explicit maximum for
active upstream App resource listeners and active downstream browser subscriptions per
auth scope. Capacity exhaustion returns a bounded machine-readable response. It never
silently drops a listener or opens a second gateway connection.

EventSource reconnect with the same UUID remains idempotent. A refreshed Console token
creates a new auth-scoped MCP client. The browser reconnect re-establishes the selected
resource against that client, and the old listener receives bounded cancellation.
Closing the final tab releases the upstream listener deterministically.

### App acceptance matrix

| Test | Required proof |
|---|---|
| route grammar | valid nested page paths map exactly. Traversal, encoded separator, authority, and length attacks fail |
| catalog authority | a syntactically valid but unauthorized App route returns no App metadata or frame bytes |
| shared frame | both hosts render the same component, sandbox constant, referrer policy, and bridge |
| CSP parity | the frame response for one App has byte-identical CSP under Console and standalone navigation |
| cold login | authentication returns to the complete original `/apps/...` path |
| bridge parity | tools, resources, links, theme, sizing, and lifecycle notifications match Console behavior |
| multi-tab fanout | two tabs for one resource create one upstream listener and independent downstream deliveries |
| capacity | exceeding the configured bound returns an explicit error and leaves existing streams healthy |
| cleanup | final tab close and token replacement both cancel obsolete listeners within the bound |

Focused gates:

```sh
cargo test -p veoveo-console-bff
cd apps/console/web && npm test
cd apps/console/web && npm run lint
cd apps/console/web && npm run build
```

Browser acceptance belongs in the Rust browser-smoke harness. It launches a headed
browser only after proving a hardware-backed WebGPU adapter or WebGL context. Loss of
the last hardware graphics API stops the run. API-only checks and screenshots cannot
substitute for the visual workflow.

## Phase 5: Canonical Resource Identity And Discovery

### 5.1 Add shared reference types

Add a focused module under `mcp/contract` for canonical resource handoff. Update
`docs/CODEMAP.md` in that implementation change. The closed types are:

| Type | Contract |
|---|---|
| `ResourceToken` | `r_` plus the base64url-no-pad encoding of a full 32-byte SHA-256 identity digest. Rust and JSON Schema enforce exactly 45 ASCII bytes and the lowercase prefix |
| `CanonicalResourceReference` | canonical resource URI, token, kind, immutable revision when applicable, and typed SHA-256 digest when content-addressed |
| `ResourceIndexItem` | token, URI, kind, short status, bounded discriminator fields, and no full payload |
| `ResourceSearchRequest` | exact known identity or bounded filters, result limit, and opaque cursor |
| `ResourceReadBudget` | per-response, cumulative episode, read-count, family-count, wall-time, page-depth, and result-count limits |

`ResourceToken` is derived from the canonical complete internal identity with a
domain-separated SHA-256 input. The service stores or can deterministically resolve the
mapping. Truncation is forbidden. A token and its resource document identifier are
byte-identical.

When an existing domain uses a different public identifier, its migration is a domain
hard cut. The old URI is removed when the new output lands. No alias reader remains.
Rollout may proceed one domain at a time because each server owns its URI scheme, but a
single domain cannot publish both shapes.

### 5.2 Canonical task handoff

A successful task result presents exactly one public follow-on `result_uri` in
structured content. Its adjacent text contains a short status without another opaque
identifier. Full provenance, artifacts, child records, and internal task routing remain
in the canonical resource document.

The platform Task status URI remains the lifecycle identity. It does not compete with
the domain `result_uri`: task status answers how work progressed, while the result URI
identifies the completed domain product. Terminal results name both fields by role when
both are present.

### 5.3 Search and wire bounds

Every growing collection exposes pagination. Older known items remain reachable by
exact identity or bounded search without loading the current full collection. Index
responses include only discriminator fields needed to choose an item.

Apply the serialized response-byte cap after final MCP serialization rather than by an
estimate of Rust heap size. A result that exceeds the cap fails closed before transport
with a machine-readable budget diagnostic. Structured content appears once. Text blocks
carry only a short identity-free status unless the protocol requires a human-readable
explanation.

Start the domain rollout with Optimization and one task-heavy server because they
exercise canonical result handoff. Continue with Map, Frames, Time, Media, Stream,
Reason, View, and Recording after the shared conformance profile passes. Static
well-known docs retain their existing canonical URIs.

### Resource acceptance matrix

| Test | Required proof |
|---|---|
| grammar | runtime and generated schema accept the same exact token length and alphabet |
| canonical handoff | a terminal task contains one domain result URI and no competing identifier in text |
| lookup | an older known token resolves without listing the full collection |
| pagination | every growing index returns stable order and an opaque next cursor |
| duplicate presentation | the model sees structured payload once and a short status beside it |
| response cap | serialization beyond the limit produces no partial response |
| episode budget | read count, family count, bytes, time, and page depth fail independently |
| correction | an unknown token reports the requested URI and safe copy-and-retry guidance |

## Phase 6: Provider-Neutral Reasoning Controls

### 6.1 Model the three states

Represent omitted reasoning as `None` at the manifest field. A present value is a closed
`ReasoningMode`:

- `Disabled`, rendered only when the provider profile declares an exact supported wire
  value.
- `Effort(ReasoningEffort)`, where the closed repository enum is `Minimal`, `Low`,
  `Medium`, `High`, `XHigh`, or `Max`. Each provider capability profile admits an exact
  subset.

Omission preserves the provider default. Disabled does not mean low effort. No mode
injects a preamble, changes sampling, selects a different model, or removes tools.

### 6.2 Validate the provider profile

Add a closed endpoint class and capability profile beside model configuration. The
profile records whether reasoning is unsupported, disable-capable, or effort-capable,
and names its admitted levels. Provider-specific wire fields remain in the adapter.
Manifest loading rejects a mode that the selected endpoint class or model does not
support.

The rendered diagnostic surface reports endpoint class, effective model ID, reasoning
state, tool availability, and non-secret sampling controls. It excludes the API key,
authorization headers, provider response bodies, and base URLs containing credentials.

### Reasoning acceptance matrix

| Test | Required proof |
|---|---|
| omitted | no reasoning field is emitted and provider defaults remain untouched |
| disabled | the exact provider-supported disable value reaches the request with tools enabled |
| named effort | every admitted level renders exactly and retains tool schemas |
| unsupported | manifest validation fails before the agent connects |
| restart | the same manifest produces the same effective reasoning diagnostic |
| no fallback | an unsupported response does not retry with another mode, model, endpoint, or sampling configuration |

The reference patch's `none`-only enum is not an intermediate state. The implementation
lands the full closed model and capability validation in one hard cut.

## Phase 7: Accelerator Memory Admission

### 7.1 Hard cut deployment v7 to v8

GPU memory fields change the controlled deployment profile after selected ownership has
landed. Introduce `veoveo.io/deployment/v8` and `veoveo.io/deployment-lock/v8`, then
remove v7 parsing. Keep this schema transition separate from phase 2 commits.

Extend each `GpuWorkloadPlacement` with typed positive MiB reservations:

- persistent memory held while the workload is ready.
- peak memory required during its admitted maximum operation.
- an exact workload/model/runtime revision that qualifies those values.

Persistent memory cannot exceed peak memory. A group declares its minimum unallocated
headroom. Full-device capacity comes from qualified DRA device inventory and a matching
NVML hardware probe. MIG capacity comes from the admitted MIG profile and the allocated
partition. Missing or inconsistent capacity fails closed.

### 7.2 Use conservative peak admission

The first implementation admits a same-device group only when the sum of every replica's
declared peak plus group headroom fits the selected physical device or MIG partition.
This conservative rule requires no cross-service runtime scheduler and prevents an
unsafe overlap before startup.

Do not claim serialized transient admission until a durable shared admission service
owns leases across Reason, View, Stream, simulation, and Optimization. Such a service is
a separate design and is not required for the first memory-safe profile.

The Reason vLLM `gpuMemoryUtilization` remains explicit and schema-validated. Its
qualified memory reservation must agree with the selected model and device capacity.
The cuOpt RMM pool, renderer allocations, simulation baseline, inference engines, and
encode/decode reservations receive their own workload entries rather than inheriting a
generic GPU number.

### 7.3 Report reservations and observations

Deployment diagnostics show physical UUID, partition identity, total memory, declared
persistent and peak reservations, headroom, and current observed use. Observed use is
evidence and drift detection. It never grants admission beyond the declaration.

Pod replacement and release upgrade re-evaluate the same locked memory plan before the
new process starts. A changed model digest or runtime revision invalidates its previous
qualification.

### GPU acceptance matrix

| Test | Required proof |
|---|---|
| schema | zero, persistent-above-peak, unknown revision, and missing headroom fail validation |
| unsafe group | summed peak beyond qualified capacity is rejected before workload startup |
| device proof | DRA allocation, in-container UUID, NVML capacity, and profile capacity agree |
| MIG proof | partition profile and allocatable bytes agree with the locked reservation |
| pod replacement | the replacement receives the same physical identity and passes admission again |
| release upgrade | changed workload revision requires matching new memory evidence |
| real concurrency | resident and transient workloads reach peak on hardware without allocation failure |
| fallback audit | no CPU, alternate model, alternate runtime, or unselected GPU path appears |

Acceptance requires an accessible NVIDIA GPU. Mocked CUDA, software rendering, and a
container that merely exposes an `nvidia.com/gpu` resource cannot close this phase.

## Phase 8: Compiler-Ready Frames And Time Provenance

### 8.1 Add a shared digest type

Introduce one general `Sha256Digest` in the shared contract rather than reusing the
gateway-specific `CompositionDigest` or adding another unconstrained string. It accepts
only `sha256:` followed by 64 lowercase hexadecimal digits. Migrate touched provenance
fields to this type in the same hard cut. Unrelated legacy strings can move when their
own contracts change.

### 8.2 Report only Frames revisions actually used

Add `FrameSourceReference` with revision URI, revision ID, and `Sha256Digest` to the
Frames output. Instrument source and target resolution to collect a set when a world
revision participates in a point conversion. Sort and deduplicate that used set before
returning it.

Do not return every revision loaded into `ResolvedWorlds`. A resolver may prefetch more
than the operation consumes. WGS84-to-ECEF conversion without a world revision returns
an empty source list and retains the coordinate-operation provenance.

Direct and Task-based conversion return the same reference shape. Artifact metadata and
usage records include the canonical source references without a second descriptive
identity.

### 8.3 Return effective Time authority

Add typed references for the effective TZDB and leap-second releases. Each reference
contains its canonical `time://` release URI, release ID, source ID, immutable source
digest, version label, and acquisition identity when the release came through the
acquisition workflow. Bootstrap data uses an explicit bootstrap source kind rather than
pretending that an acquisition occurred.

Every deterministic conversion returns the authority pair that actually governed it.
Clock-derived operations such as current time additionally return the effective clock
policy, observed quality, and holdover state. Deterministic conversion of a supplied
instant does not attach current node clock quality, because that observation did not
govern the calculation.

Approximation remains explicit in Frames. Time ambiguity and uncertainty remain typed
in Time. A downstream compiler copies the supplied references and verifies them against
its admitted revision set. It does not search unrelated catalogs by display name.

### Provenance acceptance matrix

| Test | Required proof |
|---|---|
| used Frames sources | source and target world revisions appear once. Prefetched unused revisions do not appear |
| no-world conversion | the source list is empty and operation provenance remains complete |
| digest type | malformed prefixes, uppercase hex, short values, and long values fail schema and runtime validation |
| Time data authority | conversion returns the exact active TZDB and leap release references and digests |
| clock authority | current-time output reports effective policy, quality, and holdover without changing deterministic conversion output |
| restart | persisted revisions produce byte-identical source references after restart |
| downstream admission | a compiler fixture accepts returned references without a catalog search and rejects an unadmitted digest |

Focused gates:

```sh
cargo test -p veoveo-mcp-contract
cargo test -p veoveo-frames-mcp
cargo test -p veoveo-time-mcp
```

## Phase 9: Private Git And Trust Inputs For Image Builds

### 9.1 Define one operator interface

Use repository-owned names:

```text
VEOVEO_GIT_CREDENTIALS_FILE=/absolute/owner-only/path
VEOVEO_GIT_CA_BUNDLE_FILE=/absolute/owner-only/path
VEOVEO_GIT_HOST_ADDRESS=git.example.internal=192.0.2.10
```

The first two values are optional secret source files. The host mapping is optional
validated configuration, not a secret. It accepts exactly one DNS host and IP pair and
must match the host of a locked Git dependency. It cannot override a registry, model,
provider, or arbitrary build destination.

`xtask` requires absolute regular files, rejects symlinks, and checks owner-only mode for
the credential store. It parses the CA bundle before invoking Buildx. The execution plan
records only whether each optional input is present. Paths, file metadata beyond the
admitted mode, and bytes are excluded.

### 9.2 Separate fetch from compile

Every Rust builder family performs a dependency-fetch step with these optional mounts:

- `veoveo-git-credentials` at a fixed `/run/secrets` path.
- `veoveo-git-ca-bundle` at a fixed `/run/secrets` path.

The process configures Git's credential helper and CA path for that command only. TLS
verification remains enabled. Cargo uses the Git CLI for credential-helper support.
After fetch completes, the compile step runs offline without either secret mount.

Apply the same contract to the shared trixie and bookworm artifact builders and every
standalone Rust builder still executing `cargo build`. Prefer moving a standalone image
onto the shared artifact family when that removes duplicate build logic without
changing its runtime base. Otherwise reuse the exact secret IDs, process configuration,
and tests.

Build arguments cannot carry credentials or CA bytes. A missing credential causes the
immutable private revision fetch to fail. It never substitutes a public dependency,
mutable branch, or alternate registry source.

### 9.3 Prove evidence isolation

Acceptance uses a unique non-production canary credential and an empty dependency
cache. Scan image history, exported OCI filesystem, local and registry cache exports,
SBOM, maximum-mode provenance, image build plan, run evidence, BuildKit trace, stdout,
and stderr for the complete canary and its encoded forms.

The source URL remains credential-free in Cargo metadata and the lock. A cache may
contain fetched immutable source and a credential-free remote URL. It cannot contain a
credential helper file or authenticated URL.

### Build acceptance matrix

| Test | Required proof |
|---|---|
| clean cache | the locked private revision resolves with the secret in each Rust builder family |
| missing credential | fetch fails closed without a fallback and without printing the credential-free URL as an authenticated URL |
| CA | the admitted custom root succeeds. Malformed or absent required trust fails with verification still enabled |
| host mapping | only the locked Git host can receive the single validated override |
| compile boundary | compile runs offline and has no secret mount |
| layer and cache scan | no canary bytes or helper file occur in image layers or exported cache |
| evidence scan | plan, run, trace, SBOM, provenance, logs, and metadata contain no canary bytes or source path |

Focused gates include `veoveo-image-build-control` tests and qualified builds for both
shared Rust bases plus every standalone Rust family. BuildKit versions remain exact.

## Phase 10: Integrated Closure

### Source gates

Run focused gates with each commit. Before the final rollout, run the repository-wide
non-visual source gate:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --bins
```

Run Console tests, lint, and production build. Regenerate and validate every changed
JSON Schema, deployment example, compatibility manifest, and lock. Use
`git diff --check` and hard-cut searches for removed schema versions, old cursor
domains, old return-path type names, obsolete resource error constants, and abandoned
configuration fields.

### Integration gates

| Gate | Environment | Required result |
|---|---|---|
| agent recovery | Gateway, SurrealDB, short-lived OAuth issuer, and controlled crash points | one continuation and terminal receipt, same-episode correction, and preserved resource wakes |
| selected deployment | disposable Kubernetes/K3s cluster with API audit evidence | zero writes on failed closure and no mutation request for unselected ownership |
| spatial restart | pinned DuckDB Spatial extension and persistent test database | stable distance, order, and new cursor behavior after reopen |
| standalone App | headed hardware-backed browser and authenticated Gateway | cold return, shared CSP/sandbox/bridge, and one upstream listener across tabs |
| GPU admission | qualified NVIDIA DRA cluster and concurrent real workloads | declared peak fits hardware, survives replacement, and uses no fallback |
| provenance | Frames and Time servers plus downstream compiler fixture | exact references admit directly without catalog search |
| private builds | managed BuildKit with empty local and registry caches | immutable fetch succeeds and canary scans remain empty |

No integration gate may replace a missing event-driven path with provider polling,
resource polling, task polling beyond the official Tasks correctness contract, API-only
visual checks, or CPU rendering.

## Commit Plan

Each row is a reviewable checkpoint. A commit may split further when one concern grows,
but adjacent rows should not be collapsed across ownership boundaries.

| Order | Commit concern | Minimum coherent result |
|---|---|---|
| 1 | Rig listener/request surface | exact pinned dependency exposes typed listener readiness and request freshness without Veoveo policy leakage |
| 2 | agent listener rotation | acknowledged make-before-break listener and durable resource wakes |
| 3 | resource reads and safe diagnostics | bounded read tool, final MCP error classification, and same-episode unit test |
| 4 | continuation crash evidence | lineage audit, required migration only if a gap exists, and crash matrix |
| 5 | deployment v7 ownership contract | types, validation, schemas, examples, locks, compatibility manifest, and design |
| 6 | pure deployment plan and Secret closure | rendered closure with zero-write tests |
| 7 | managed fields and selected executor | conflict preflight, canonical command, execution receipt, and preservation tests |
| 8 | DuckDB spatial axis type | runtime setting, Map/DuckDB selections, and extension-backed tests |
| 9 | Map distance query hard cut | typed score, materialized CTE, cursor v2, restart evidence, and Map design |
| 10 | standalone App route and auth return | typed route, renamed return-path contract, authorized bootstrap, and shared host |
| 11 | App stream bounds and browser acceptance | explicit capacity, refresh cleanup, multi-tab proof, and Helm ingress |
| 12 | shared resource reference contract | token, reference, index, budget, and conformance types |
| 13 | domain resource rollout | one commit per domain hard cut, beginning with Optimization |
| 14 | reasoning capability contract | full typed model, provider validation, exact requests, and diagnostics |
| 15 | deployment v8 GPU memory contract | schema, locks, examples, admission math, and focused tests |
| 16 | GPU runtime evidence | DRA/NVML diagnostics, replacement proof, and hardware acceptance |
| 17 | Frames provenance | used-source references, artifact projection, and tests |
| 18 | Time provenance | effective data authority, clock authority distinction, and tests |
| 19 | BuildKit private inputs | canonical interface, every builder family, evidence redaction, and docs |
| 20 | integrated closure | repository-wide gates, smoke evidence, hard-cut audit, and final plan ledger update |

Deployment v7 and v8 are deliberately separate. The first release-safety change can
ship without waiting for GPU memory qualification, and the later memory fields receive
their own schema review and acceptance.

## Documentation And Generated Artifact Closure

Each implementation commit updates its owning documentation. A phase is incomplete
when code and tests pass but its public or installation contract still describes the
old behavior.

Required documentation updates include:

- `mcp/contract/DESIGN.md` for resource errors, references, budgets, and handoff.
- `docs/RMCP_3_MIGRATION.md` if the post-migration listener repair changes its
  implementation report or remaining rollout evidence.
- `docs/AUTONOMY_HARNESS.md` and `docs/TECH_DESIGN.md` for delivered continuation and
  model behavior.
- `deploy/contract/DESIGN.md`, `docs/LOCAL_DEPLOYMENT_PROFILES.md`, and
  `docs/ENTERPRISE_DEPLOYMENT.md` for v7 and v8.
- `mcp/apps-extension/DESIGN.md` and Console documentation for the standalone host.
- Map, Frames, Time, Reason, and Optimization `DESIGN.md` files for their owned
  contract changes.
- `docs/IMAGE_BUILDS.md` and `docs/EXTERNAL_REPOSITORY_INTEGRATION.md` for private
  dependency inputs.
- `docs/CODEMAP.md` whenever a module, document, component, or ownership boundary is
  added or moved.

Generated schemas, compatibility manifests, example profiles, deployment locks, Helm
values schemas, and conformance fixtures change with their source types. Generated
outputs are never edited independently of their typed owner.

## Definition Of Done

The program is complete when all ledger rows have delivered evidence and these claims
are simultaneously true:

- resource notifications remain active across initial connection, token rotation,
  process restart, and lease handoff.
- correctable resource and domain input errors can be repaired within one bounded
  episode without leaking protected diagnostics.
- continuation crash points produce one terminal wake, one consume transaction, and one
  durable receipt.
- a failed Secret or managed-field preflight causes no Kubernetes mutation.
- a selected release cannot write an unselected ownership key.
- Map uses the explicit longitude/latitude axis and new cursor domain on every open.
- standalone Apps share the Console security and stream boundaries.
- resource results expose one canonical follow-on URI with bounded discovery and read
  behavior.
- reasoning modes are explicit, provider-admitted, restart-stable, and never silently
  substituted.
- GPU placement is admitted by device identity and memory headroom on real NVIDIA
  hardware.
- Frames and Time return exact typed source references consumed directly by a
  downstream compiler.
- private Git credentials and trust inputs never escape their dependency-fetch secret
  mounts.
- hard-cut searches find no old deployment schemas, route aliases, cursor readers,
  configuration aliases, obsolete protocol constants, CPU fallback, or provider status
  polling.
