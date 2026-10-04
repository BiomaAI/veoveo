# Platform Improvements Plan

Status: requests `014`–`023` contain open work. The completed `001`–`013` cycle and
recording implementation history live in Git. The
[Recording design](../servers/recording-mcp/DESIGN.md) owns the delivered recording
contract. [Fork development](FORK_DEVELOPMENT.md) governs customization and replaces
the extension-release proposal; this plan cannot reintroduce extension packages,
compatibility manifests or gateway fragment composition.

[Contract Evolution](CONTRACT_EVOLUTION.md) governs provider completion, verification,
and dependency choices. The [consolidated plan](CONTRACT_CONSISTENCY_PLAN.md) independently owns identifier changes,
resource corrections, audit and knowledge delivery.

## Standards And Protocols

The [current cycle register](#standards-and-protocols-1) states its proposed standards.
Owning designs describe implemented profiles. Recheck a dependency's current stable
release when adding or upgrading its pin.

## Current Cycle: Requests 014–023

Status: active delivery record. Phase 2, the Recording Catalog Hard Cut, is complete and
activated as of 2026-08-27. The other current-cycle phases remain planned unless their own
sections record a later state. Phase 1 may begin after its focused resolver contract is recorded.
Decisions needed only by later phases do not block unrelated work. Existing component designs
remain normative until each phase lands and deletes the surface it replaces.

Planning baseline: Veoveo main `6edf9d6b08886a3503067daa7803098d5ea7bc12` on 2026-08-26.
The implementation review was refreshed at `e1d72227dc681363e47bef5f06e26e5caa738cfd` on the
same date. The input is the downstream review package
`veoveo-platform-improvements-2026-08-26`, which reviewed
`2e77072ac7c0b88b5eba6336381f621a87171974`. Its ten requests are numbered `014` through `023`.
Its four patches demonstrate behavior against an older integration snapshot and are not a merge
series.

This plan accepts the downstream outcomes where they strengthen exact App resolution,
recording use, artifact admission, release visibility, diagnostics, external simulation
adoption, accelerator admission, provider configuration, and deployment ownership. It replaces
suggested mechanics that would create a second authority, a second recording truth, buffered
large-file paths, or a risky rewrite of an already qualified live-view runtime.

### Standards And Protocols

| Standard or profile | Plan boundary |
|---|---|
| Model Context Protocol `2026-07-28` | sole MCP profile for Gateway aggregation, exact App reads, App-scoped resource listing, tools, resources, Tasks, subscriptions, notifications, and server discovery |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | sandboxed `ui://` applications, `text/html;profile=mcp-app`, host context, standard bridge methods, and repository-owned host extensions declared by the App resource |
| JSON-RPC 2.0 | frame bridge request and error envelope. App-scoped methods use typed invalid-parameter, forbidden, unavailable, and internal outcomes without leaking policy or transport internals |
| JSON Schema Draft 2020-12 | closed App resolution, upload, release, recording, live-view, GPU, reasoning, and deployment contracts |
| W3C Trace Context and Baggage | bounded `traceparent`, `tracestate`, and `baggage` propagation. The authenticated HTTP boundary establishes trust and untrusted App frames never choose trace authority |
| OpenTelemetry Protocol | export of connected Console, Gateway, hosted-server, and bounded data-plane spans. Operator correlation exposes only a shortened trace identity and safe result category |
| Rerun `0.36.3` RRD | canonical immutable multimodal recording bytes at this baseline. Planned pin changes verify upstream support and qualify the complete compatible stack under AGENTS.md; consumer edits may retain the qualified pin |
| Rerun Data Protocol | official `rerun.cloud.v1alpha1.RerunCloudService` read profile for governed datasets, segments, layers, schema, manifests, chunk queries, and chunk fetches over HTTP/2 or gRPC-Web |
| Rerun Catalog SDK and DataFusion | native Python and Rust dataset reads, segment filtering, content filtering, typed dataframe operations, and Arrow conversion. Veoveo does not claim unsupported Catalog mutation or maintenance profiles |
| Apache Arrow IPC streaming format | sole browser-native recording projection payload. It is a query result over RRD, not another recording format or durable copy |
| Rerun WebViewer `0.36.3` | direct Redap archive playback, producer and dataset Blueprints, and the existing incremental `LogChannel` live path |
| H.264 Annex B, WebCodecs, Media Capabilities, CUDA, RTX, and NVENC | existing tiled live-view v4 media path and its qualified hardware boundary. Browser software H.264 decode remains the one documented exception when the exact configuration is supported and smooth |
| `veoveo.ai/live-view/v4` | provider-neutral camera, shared stream-product, authorization, renewal, closure, and health contract. This plan publishes adoption artifacts without changing v4 runtime semantics |
| Kubernetes/K3s, Helm, and server-side apply managed fields | deployment compilation and mutation. A component is selectable only at an atomic Helm release or explicit raw-manifest boundary. Managed fields provide diagnostics and never become a replacement ownership authority |
| NVIDIA DRA Driver for GPUs and `resource.nvidia.com/v1beta1` | physical-device or MIG allocation remains the placement authority. Veoveo adds qualified memory admission before Kubernetes mutation |
| JSON Web Token, OAuth 2.1, RFC 6750, RFC 8707, and SHA-256 | authenticated browser and machine access, resource-bound capabilities, immutable content identity, and short-lived Redap sessions |

Every dependency, image, toolchain, or infrastructure component touched by implementation must
be checked against its authoritative upstream release. Stable versions are pinned exactly.
Experimental Rerun dataloading may be evaluated after the catalog read profile is complete, but
it cannot become production acceptance evidence without an explicit product reason and its own
qualification.

### Intended Outcome

An operator who opens a known App will resolve that App and its declared closure without waiting
for unrelated servers. The host will explain recoverable dependency states, admit only declared
file uploads, and attach a safe correlation identity to failures. Independently installed
extensions will appear as exact immutable releases without becoming part of App discovery.

Recording becomes a governed Rerun catalog rather than a recorder that constructs a temporary
catalog only when one recording is opened. RRD remains canonical. Semantic datasets contain
recording segments, immutable shards become layers, and the same authorized data serves the
WebViewer, Catalog SDK, DataFusion, and bounded browser projections. The current JSON row query
and per-recording catalog model are deleted in the cut.

External simulators will receive live-view schemas, typed server helpers, a browser client, and a
parameterized hardware acceptance harness. The working UAV live-view runtime remains the
behavioral reference. It does not move to shared code until the extracted package proves exact
parity on Linux with the existing five-user hardware gate.

Deployment will reject unsafe GPU memory closure and cross-owner release overlap before mutation.
Provider reasoning configuration will distinguish absence, disablement, and named effort without
silent substitution.

### Decision Ledger

| ID | Priority | Decision | Replacement or constraint |
|---|---|---|---|
| `APP-RESOLVE-014` | P0 | accept | one Gateway-owned exact resolver supplies every direct App operation. Broad discovery remains only for the App gallery |
| `APP-HEALTH-015` | P0 | accept | the exact resolver supplies the sole dependency snapshot. The host answers scoped `resources/list` and owns a typed retry state machine |
| `APP-UPLOAD-017` | P1 | accept with a different byte path | the host owns file selection and streams `File` bytes end to end. The frame receives only the immutable receipt. Do not pass an `ArrayBuffer` through `postMessage` or buffer the body in the BFF and Gateway |
| `TRACE-019` | P1 | accept narrowly | complete W3C and OTLP continuity and surface a safe correlation identity. Do not add the reference patch's Grafana reverse proxy |
| `LIVEVIEW-020` | P1 downstream ergonomics | narrow | publish external adoption artifacts around live-view v4. Freeze the qualified first-party runtime until an extracted client passes behavior and hardware parity |
| `GPU-ADMISSION-021` | P1 | accept | use qualified persistent and maximum-peak bytes, replica multiplication, device or MIG capacity, and minimum headroom. Current observed free memory is diagnostic only |
| `REASONING-022` | P2 | accept when the provider capability inventory exists | absence preserves the provider default, disabled maps to one admitted disable value, and named effort maps through a validated provider adapter. No fallback is permitted |
| `DEPLOY-SCOPE-023` | P1 | accept as a deployment-contract hard cut | selections operate on atomic owned components, render the complete object closure before mutation, reject overlap, and issue zero writes to unselected components |

The client priorities describe operator value rather than implementation order. Exact App
resolution precedes App health and upload because those features must consume one authority.
Recording contract and storage work precede every new recording client. Extension inventory and
component-scoped deployment share one ownership vocabulary. GPU admission precedes any newly
qualified shared-device deployment.

### Repository Invariants

- Revised names, schemas, methods, routes, manifest versions, and query shapes are hard cuts. No
  aliases, compatibility readers, dual writes, or hidden fallback paths remain after a phase.
- Gateway policy remains the authorization authority. Console may cache one successful resolution
  only inside the authenticated browser or OAuth authority scope and active control and policy
  revisions. MCP transport identity is not a session authority.
- Exact resolution cannot grant a tool, resource family, subscription, upload profile, or agent
  target that broad authorized discovery would withhold.
- Unknown or unauthorized App state never reveals whether hidden resources exist.
- No credential, cookie, bearer, Redap token, storage coordinate, private URL, or trace baggage
  enters an opaque App frame, MCP content, model context, log, or error panel.
- RRD is the only durable recording data format. Arrow IPC is a finite query response. Rerun
  Blueprints remain presentation state and never become recording authority.
- Frozen recording bytes and their SHA-256 identities are immutable. Catalog indexes are derived
  and rebuildable.
- Raw Redap chunk access is granted only when the caller is authorized for the complete selected
  recording segments. Projection-only callers never receive a Redap bearer.
- The current live RRD channel keeps its exclusive receiver, reconnect, rollover, and Blueprint
  continuity behavior. Archive changes cannot introduce polling or cursor forcing into Live mode.
- Existing Media work retains its webhook profile. Trace work consumes recorded provider
  events and does not independently add provider polling; adapter changes follow Contract Evolution.
- GPU visual, simulation, rendering, encoding, perception, and visual verification remain
  hardware-only. Admission never creates a CPU profile or optional GPU mode.
- Browser acceptance stays headed and requires hardware WebGPU or WebGL. SwiftShader, llvmpipe,
  and software rasterizers are rejected.
- Known shapes use typed Rust, Python, and TypeScript models with closed enums. Raw JSON remains at
  genuinely open provider or Kubernetes decode boundaries only.
- Every growing list has an explicit bound. Paged surfaces have a stable cursor; deliberately
  closed snapshots reject overflow. Every potentially large byte path is streaming or bounded
  task-local materialization.
- Helm and the installation reconciliation controller retain mutation ownership. Smoke code
  verifies declared lifecycle and never becomes a parallel installer.

### Delivery Tracks And Dependency Order

| Track | Requests | Depends on | May proceed alongside |
|---|---|---|---|
| A: exact App authority | `014`, `015` | Phase 0 contracts | recording foundation and release inventory |
| B: recording catalog hard cut | `016` | Phase 0 identity and retention decisions | exact App authority until browser projection integration |
| C: App upload and tracing | `017`, `019` | exact App authority; existing Artifact and telemetry boundaries | release projection |
| D: extension and deployment ownership | `018`, `023` | common component and release identities | recording and live-view packaging |
| E: external live view | `020` | published v4 contract; exact App host for external acceptance | GPU contract design |
| F: resource admission | `021`, `022` | qualified workload/provider evidence | nonvisual contract work |
| G: integrated closure | all | every owning phase | none |

The recording cut is one release even though implementation uses several commits. New storage,
new catalogs, and old playback cannot be independently activated. App resolution may ship before
recording if it passes its own complete host acceptance.

### Phase 0: Contract Locks And Focused Red Evidence

Phase 0 records the decisions in owning design documents before production code changes.

Only the exact resolver contract blocks Phase 1. Before its production code begins, the owning
design records one typed operation, its request and response, typed errors, transport projection,
and focused limits. It reuses existing MCP limits and notification machinery where they fit. It
does not create a general resolver protocol, cache framework, or discovery service.

The recording retention choice and its focused publication and Arrow-delivery contracts block
only Phase 2 activation. Live-view qualification, GPU evidence, provider capability inventory,
and deployment ownership remain gates for their own phases.

#### Required design updates

| Owner | Required decision |
|---|---|
| `mcp/apps-extension/DESIGN.md` | exact resolved-App projection, App-scoped resource-family metadata, host upload request and receipt, and frame-visible error categories |
| `mcp/contract/DESIGN.md` | bounded trace propagation and any Gateway-owned exact-resolution metadata that crosses MCP |
| `servers/recording-mcp/DESIGN.md` | governed dataset catalog, read grants, Redap profile, projection endpoint, hard-cut manifest, and live/archive relationship |
| `docs/RECORDINGS.md` | new canonical identities, active and immutable storage, Rerun dataset hierarchy, retention, recovery, migration, and acceptance |
| `extensions/contract/DESIGN.md` | immutable release inventory projection and relationship to deployment ownership |
| `deploy/contract/DESIGN.md` | atomic component ownership and qualified memory admission |
| `docs/GPU_PLACEMENT.md` | capacity sources, qualification identity, formula, drift evidence, and fail-closed operation |
| `agents/kernel/DESIGN.md` and owning model-provider design | three-state reasoning configuration, capability validation, and effective-state diagnostics |
| `servers/uav-sim-mcp/src/contract/live_view.rs` owning design section or a focused adjacent design | published live-view v4 server, simulator-adapter, browser-client, and conformance boundary |

`docs/CODEMAP.md` changes in the same implementation commits when a new SDK package, catalog
module, or conformance component is placed.

#### Required red evidence

- exact App load fails when the existing broad catalog waits on an unrelated server;
- App bridge rejects `resources/list` because the host advertises but does not implement it;
- frame load produces an unowned blank or terminal iframe response when the owner server restarts;
- the existing recording query proves JSON string conversion and cannot select exact component
  columns or return typed Arrow;
- the existing per-recording Redap catalog cannot expose two authorized recordings as segments of
  one dataset;
- an App upload attempt proves there is no declared host capability and no streaming ingress;
- a BFF-originated call proves where trace continuity currently breaks;
- the external simulation fixture proves that it redeclares live-view models and cannot import a
  reusable browser client or generic five-user harness;
- GPU profile validation accepts a same-device group without memory closure;
- model manifests cannot distinguish absent and disabled reasoning;
- a selected platform update proves that its current mutation plan includes an extension-owned
  release or cannot demonstrate zero writes.

Tests remain focused at their owning boundary. Linux deployment, GPU, and headed-browser failures
are recorded only in the applicable remote acceptance environment. Expected failures may be
captured locally to prove the missing behavior, but commits stay green. A focused regression test lands with the implementation that makes it pass.

### Phase 1: Exact App Authority And Recovering Hosts

This phase implements `APP-RESOLVE-014` and `APP-HEALTH-015` as one authority change.

#### Canonical resolution

The Gateway owns a typed `ResolvedApp` operation in its MCP library. Console does not reconstruct
the result from independent calls. The resolver accepts the active profile, authenticated actor,
mounted owner server, and exact projected `ui://{server}/{page}` URI.

Resolution performs this ordered work:

1. Parse the URI with the existing closed App grammar and reject traversal, query, fragment,
   missing page, and owner mismatch.
2. Resolve the mounted owner from the active control revision without enumerating other servers.
3. Apply the ordinary profile, scope, label, policy, and App-link decisions.
4. Read the exact owner resource.
5. Read only the owner and declared dependency listings needed to prove linked tools and admitted
   resource families.
6. Return the immutable App resource, HTML content, admitted tools, resource families,
   subscriptions, agent targets, upload grant, owner/dependency status, CSP inputs, actor display
   projection, policy revision, and resolution revision.
7. Record one aggregate policy/audit decision. Internal per-listing checks cannot create a second
   user-visible authorization result.

The projected App resource may carry repository-owned exact-resolution metadata for transport,
but the Gateway library result is the authority. A BFF-only `/console/api/apps/resolve` route is a
same-origin projection, not a new authorization service.

Successful resolutions use single-flight caching keyed by authenticated browser or OAuth
authority scope, control revision, policy revision, actor authority fingerprint, owner server,
and exact App URI. The entry uses the existing bounded MCP cache lifetime. Failures are not
retained. Relevant owner `resources/list_changed`, `tools/list_changed`, exact subscribed resource
updates, policy or control activation, owner replacement, or authority-scope replacement
invalidates the entry. A broad App-catalog refresh cannot invalidate an unrelated exact
resolution unless one of those inputs changes. This is a focused cache beside the resolver, not a
repository-wide cache subsystem.

#### Required consumers

The following paths must consume `ResolvedApp` and must stop calling the broad App catalog:

- Console direct App navigation;
- standalone App bootstrap;
- frame HTML read;
- App tool admission and task ownership;
- App resource read, list, subscription, and resource-event admission;
- agent-message target admission;
- App artifact-upload grant resolution;
- host CSP and descriptor construction.

The broad catalog remains for the installation App gallery and its reactive partial-degradation
UX. It is never a prerequisite for a known App route.

#### App-scoped `resources/list`

The host answers standard bridge `resources/list` with one finite, cursor-free view of the
already resolved closure. The owning contract sets fixed limits for declared dependencies,
families, concrete resources, total bytes, and resolution duration and returns one typed overflow
outcome. It reuses existing repository limits where suitable. It does not call broad Gateway
discovery again.

Each family record contains:

- projected scheme and non-root URI prefix;
- owner or declared dependency server;
- admitted operations from the closed `AppResourceOperation` set;
- server state `ready`, `unavailable`, or `answered_empty`;
- whether policy admitted the family;
- optional bounded public diagnostic category.

The resource array contains only concrete resources already visible in the resolved owner and
dependency listings. An unavailable family remains in metadata without fabricated resource
entries. Undeclared servers and families are absent. A policy-withheld declared family can report
`admitted: false` because the App already declares that dependency, but it cannot reveal hidden
resource identity or count.

#### Recovering host state

Console and standalone hosts share one state machine:

| State | Meaning | Automatic action |
|---|---|---|
| `signing_in` | no valid Console session | enter the existing authentication transition |
| `resolving` | exact authority and resource are pending | wait within the bounded resolution deadline |
| `starting` | descriptor is ready and the sandbox is booting | mount one frame |
| `retrying` | owner or dependency is temporarily unavailable | capped exponential retry with jitter and manual retry |
| `forbidden` | authenticated caller lacks the App authority | terminal safe panel; no existence disclosure |
| `not_found` | the admitted owner answered and the exact App resource is absent | terminal safe panel |
| `failed` | non-retryable contract or internal failure | terminal safe panel with correlation identity |

The host never replaces a healthy mounted frame because an unrelated dependency changes. A
retryable resolution failure does not mount an empty document. Successful recovery remounts once
with the same route and host policy.

#### Ownership and deletion

| Path | Work |
|---|---|
| `mcp/apps-extension` | shared `ResolvedApp`, family-status, upload-grant, and safe host-error types |
| `mcp/contract/src/gateway` | exact resolver configuration and validation types |
| `platform/gateway/src/mcp/` | targeted resolution, policy, status, bounded listing, caching, and invalidation |
| `apps/console/bff/src/mcp_client.rs` | session-scoped exact-resolution client and single flight |
| `apps/console/bff/src/apps.rs`, `app_host.rs` | same-origin projection and migration of every direct consumer |
| `apps/console/web/src/appHost.tsx`, `StandaloneAppHost.tsx`, `apps/bridge.ts` | shared state machine and scoped resource-list response |

Delete every direct-route call to `app_catalog()`. Delete any duplicate family derivation in the
web client. Retain `app_catalog()` only for gallery listing and gallery change events.

#### Acceptance

- Console and standalone direct routes load while an unrelated MCP server never answers.
- Owner unavailability produces `retrying`, then recovers after restart without shell reload.
- An admitted owner that returns no exact resource produces `not_found`.
- Withheld authority produces `forbidden` and no hidden server or resource details.
- Scoped listing distinguishes unavailable, answered-empty, and policy-withheld declared families.
- Console and standalone descriptors, CSP, tools, resources, and host states are identical.
- Exact resolution cannot add any capability absent from an equivalent complete authorized
  catalog.

### Phase 3: Host-Mediated Streaming Artifact Upload

The [Artifact service design](../platform/artifacts/service/DESIGN.md#upload-qualification-limits)
describes the delivered public upload API and its qualification limits. Direct API and Console
uploads are independent of exact MCP App resolution. This phase still owns the
embedded App host-picker integration and consumes that delivered ingestion foundation.

This phase implements `APP-UPLOAD-017` after exact App authority exists.

An App resource declares one closed upload grant containing admitted media types, maximum bytes,
maximum file count per request, filename limits, Artifact policy profile, and optional purpose.
Absence grants no upload method. Exact resolution projects the validated grant into the host
descriptor.

The frame invokes a repository-owned host method with a UUIDv7 request ID and optional safe prompt
text. The host opens the file picker. The selected `File` object stays in the host origin. Its
bytes never cross the opaque-frame `postMessage` bridge.

The host accepts the request only during browser-observed transient user activation and opens the
picker before asynchronous work can consume that activation. Background messages, retries, and
restored frames cannot reopen the picker. A retry begins only after the user selects the file and
reuses the same immutable request identity.

The host validates count, name, media type, declared size, and grant before opening the data path.
It streams `File.stream()` through the BFF and Gateway to the existing Artifact plane. Each hop
enforces a hard byte ceiling and cancellation. The Artifact service computes the digest while
writing and commits only after the received byte count matches the declared count.

UUIDv7 idempotency is durable at the Artifact service and keyed by actor, Work Context, App URI,
grant revision, and request ID. A retry returns the original receipt when every immutable input
matches. A conflicting retry fails without replacing data.

The frame receives only:

- canonical artifact URI;
- SHA-256 digest;
- media type;
- exact byte length;
- immutable occurrence or receipt revision.

The reference patch's `ArrayBuffer` bridge and Axum `Bytes` handlers are rejected. Implementors
must extend the Artifact client and service with a streaming body when their current boundary
buffers the object.

Acceptance proves admitted upload, pre-transfer rejection, exact digest, durable idempotency,
cancel-on-frame-close, zero stored bytes after rejected or incomplete transfer, and absence of
file bytes from MCP, model, logs, and telemetry.

### Phase 4: Distributed App Tracing

#### Trace continuity

`TRACE-019` completes one trace across:

1. signed-in Console or standalone host request;
2. BFF exact App operation;
3. Gateway MCP authorization and selected upstream request;
4. hosted MCP server work;
5. optional Artifact or recording projection request;
6. terminal result or retry.

The BFF and Gateway HTTP routers extract valid W3C context at authenticated boundaries and set it
as the parent before request spans begin. Every owned outbound HTTP and MCP transport injects the
current context. MCP metadata remains bounded and sanitized by the existing contract. Opaque App
frames may supply a host-local request ID for promise settlement but cannot supply `traceparent`,
`tracestate`, or baggage.

Safe span attributes are limited to method class, mounted server, bounded public App URI or its
hash, policy revision, result category, retry count, duration, request/response byte counts, and
non-secret catalog or release revision. Tool arguments, prompts, resource bodies, filenames,
emails, tokens, private URLs, provider payloads, database errors, and policy internals are never
recorded.

Retries remain children of the original operation and carry an attempt number. The host error
panel displays a short correlation value derived from the trace ID plus a safe category. It does
not display full trace context. The installation observability backend retains the complete trace
under its existing access controls.

The reference patch's Grafana configuration, authentication proxy, and `/console/observability`
routes are outside this request and must not be ported. A future observability UI requires its own
authorization and product decision.

Acceptance joins one success and one failure across BFF, Gateway, server, and one bounded data
plane. Redaction tests inspect exported span data, not only log text.

### Phase 5: External Live-View Adoption Without Runtime Disruption

`LIVEVIEW-020` is a developer-experience and productization request. It is not an operator UX
rewrite. Current main already owns:

- provider-neutral live-view v4 Rust types in `servers/uav-sim-mcp/src/contract/live_view.rs`;
- one shared tiled RTX/NVENC H.264 product with camera regions;
- independent viewer authorization, renewal, closure, and audit;
- browser WebCodecs decode and crop in the UAV App;
- an independently packaged anonymous simulation extension;
- headed five-user hardware acceptance proving one product and one NVENC session.

The missing adoption surface is packaging. The Python fixture currently redeclares live-view
models. The browser decoder and renewal logic remain inline in the first-party UAV HTML. The
five-user harness is parameterized around UAV routes, camera names, and DOM evidence.

#### Published artifacts

| Artifact | Owner | Required content |
|---|---|---|
| generated live-view v4 JSON Schema | UAV contract feature, live-view model export | exact camera, region, product, endpoint, authorization, health, open, renew, and close shapes |
| Python server models and helpers | `sdk/python` | typed v4 models, validation, redacted token handling, authorization lifecycle helpers, and simulator-adapter interfaces without renderer code |
| browser client package | a focused package under `sdk/` selected after CODEMAP update | WebCodecs capability check, hardware/software decode label, Annex B keyframe handling, shared-product socket ownership, region crop, renewal, reconnect, teardown, cancellation, and typed evidence callbacks |
| simulator adapter contract | Python SDK plus schema | logical camera inventory, source region, one stable stream-product identity, product health, and private stream endpoint binding |
| generic headed acceptance | `testing/browser-smoke` or a focused Rust crate if responsibilities require it | manifest-driven App URI, camera set, evidence selectors, five users, hardware graphics proof, product/encoder invariance, renewal, close, restart, and screenshots |
| external reference | `testing/fixtures/fork-workload` | consume released SDK types, stop redeclaring the v4 contract, and remain independent of first-party vehicle or simulator source |

The packages never include a renderer, encoder, simulator dependency tuple, vehicle schema, scene
mirror, or physics state. External implementations retain their own GPU qualification.

#### Non-disruption sequence

1. Generate and publish schemas from the existing Rust contract without modifying the UAV runtime.
2. Move the external fixture to the Python types and pass isolated packaging/conformance tests.
3. Build the browser client against protocol fixtures and an external simulator implementation.
4. Parameterize the existing Rust browser harness while retaining the current UAV scenario as an
   unchanged concrete configuration.
5. Run the external implementation and current UAV implementation through the same Linux headed
   five-user hardware gate.
6. Consider moving the UAV App to the shared browser package only after exact DOM, authorization,
   product-count, encoder-count, frame-rate, latency, restart, and teardown parity passes.
7. If adoption is approved, hard-cut the UAV App to the one shared implementation and delete the
   inline duplicate in the same change. If parity is not proven, keep the current UAV code and do
   not claim the client package as the first-party runtime.

No initial phase changes `servers/uav-sim-mcp/src/server/live_view.rs`, simulator camera products,
NVENC behavior, WebSocket framing, or qualified camera layouts.

Schema, SDK, fixture, and generic-harness work may proceed before a qualified external GPU
implementation is available. The external five-user hardware claim closes only after an owner
names that implementation and it passes the existing gate. The contract-only fixture is not
visual or GPU acceptance evidence.

### Phase 6: Accelerator Memory And Reasoning Admission

#### Qualified accelerator memory

`GPU-ADMISSION-021` extends the current physical GPU placement contract. Each workload placement
adds:

- persistent reservation bytes;
- maximum admitted peak bytes;
- replica count already owned by placement;
- exact image digest;
- model/checkpoint digest when applicable;
- runtime and CUDA compatibility revision;
- qualification evidence digest and timestamp.

Each same-device group adds minimum headroom bytes and a capacity source. Capacity comes from the
qualified full-device ResourceSlice identity or an exact MIG profile. It never comes from current
NVML free memory.

Validation requires `persistent <= peak` for every workload and computes with checked integers:

```text
sum(workload.maximum_peak_bytes * workload.replicas) + group.minimum_headroom_bytes
    <= qualified_device_or_mig_capacity_bytes
```

The persistent sum plus headroom must also fit, even though valid peaks normally make that
inequality redundant. A changed image, model, runtime, CUDA tuple, replica count, device product,
MIG profile, or evidence digest invalidates qualification.

Admission runs during pure deployment-profile compilation before Helm rendering or Kubernetes
mutation. NVML measurements after startup become drift evidence. They may fail acceptance or
require requalification, but they cannot admit an otherwise unsafe profile.

The owning workload records peak bytes from its existing qualified hardware scenario, including
the declared replica concurrency and a fixed safety margin. The image, model, runtime, CUDA, and
device tuple identifies that evidence. Phase 6 may refine the scenario when a workload needs it;
the platform does not add a general benchmark authority or infer a maximum from an unbounded run.

Tests cover overflow, missing values, persistent greater than peak, insufficient headroom,
replica multiplication, capacity-source mismatch, evidence invalidation, and a safe exact fit.
Linux acceptance proves that an unsafe group creates no Pod or Helm mutation.

#### Three-state reasoning

`REASONING-022` uses an optional closed object:

```json
{"reasoning":{"mode":"disabled"}}
```

or:

```json
{"reasoning":{"mode":"effort","effort":"high"}}
```

When `reasoning` is absent, the provider default is preserved. JSON `null`, an explicit
`"omitted"` mode, arbitrary strings, and provider payload fragments are rejected.

A provider capability registry is keyed by endpoint class, provider, exact model identity, and
adapter revision. It declares whether disablement is supported and the closed effort values that
may be mapped. Validation occurs when the model manifest loads, before any provider connection.
The adapter emits only the admitted provider field and value.

There is no retry with a different mode, model, endpoint, temperature, or sampling configuration.
Unsupported configuration fails the manifest. Diagnostics expose the effective non-secret state
as `provider_default`, `disabled`, or the named admitted effort without exposing hidden reasoning
or provider request bodies.

Implementation starts only after an owner supplies the provider/model capability inventory. It
must not guess current provider behavior from model names. Missing reasoning inventory blocks only
reasoning work; it does not block GPU admission or earlier phases.

### Phase 7: Component-Scoped Deployment Hard Cut

`DEPLOY-SCOPE-023` advances the deployment and lock schema together. It builds on current source
roles and image ownership but changes mutation selection.

#### Atomic component model

Each deployment source declares components with:

- globally unique component ID;
- owning source and immutable source revision;
- role `platform`, `workload`, `extension`, or `installation`;
- one or more atomic Helm release identities or explicit raw-manifest sets;
- namespace and permitted object identities;
- values and image closure;
- dependencies on other component IDs;
- extension-release identity where applicable.

A Helm release is indivisible. If platform and extension resources share one release, they cannot
be selected independently. The implementation must split that chart into independently owned
releases before component selection is enabled. It must not approximate partial ownership with
label selectors or post-render object deletion.

#### Selection and preflight

A release operation names exact component IDs. The compiler expands dependencies, renders every
selected release and raw-manifest set, and computes the complete Kubernetes object identity set
before mutation. It also loads the locked object identities of unselected components.

Preflight rejects:

- duplicate ownership;
- selected/unselected object overlap;
- a selected source that differs from the deployment lock;
- a Helm release containing objects owned by two components;
- undeclared namespaces or cluster-scoped objects;
- image or values input outside the selected component closure;
- missing dependency components;
- an operation whose tool cannot target the atomic release exactly.

After preflight, orchestration invokes Helm only for selected releases and applies only selected
raw manifests. It does not render or apply an unselected release as a side effect. Managed fields
may verify the expected manager and report drift. They do not authorize taking ownership.

This orchestration remains the disposable deployment-profile path. Enterprise installations use
the declared GitOps and reconciliation boundary; component selection does not create a second
imperative enterprise installer.

The receipt records selected components, expanded dependencies, source revisions, rendered object
digests, mutation verbs, and zero-write evidence for unselected objects. It contains no Secret
bytes.

Acceptance uses independent platform and extension Git histories. A platform-only update changes
one platform image and proves zero writes, patches, applies, deletes, rollouts, or Helm revisions
for extension objects. An extension-only update proves the inverse. Deliberate overlap fails
before the first API write.

### Phase 8: Integrated Closure

The final release closes only when every included request passes its focused tests and the
cross-component paths pass together.

#### Integrated scenarios

1. Open a known external App while an unrelated server is down. Observe exact resolution,
   unavailable dependency metadata, retry, recovery, and one connected trace.
2. Use the host picker to stream an admitted file. Receive an immutable artifact receipt in the
   frame and pass that receipt through a bounded domain tool call.
3. Open one recording in the WebViewer while a native App loads an Arrow projection from the same
   segment and clock. Seek repeatedly and prove deterministic state.
4. Query two admitted recording segments with the Rerun Catalog SDK while a third segment in the
   same tenant remains undiscoverable.
5. Project two independent extension releases. Perform a platform-only update and prove zero
   writes to both extension releases.
6. Run one external live-view implementation and the first-party UAV reference with five headed
   users on qualified NVIDIA hardware. Product and encoder counts remain constant.
7. Reject an unsafe GPU memory profile before mutation. Start the qualified profile and compare
   NVML drift evidence with its admitted bounds.
8. Validate absent, disabled, and named reasoning configurations. Restart and prove the effective
   configuration remains exact.

#### Cross-cutting security inspection

Acceptance captures browser messages, BFF and Gateway HTTP headers, MCP requests, OTLP exports,
logs, artifact metadata, recording projection metadata, and deployment receipts. It proves that
credentials, file bytes, prompts, private URLs, object coordinates, provider payloads, policy
internals, and hidden resource identities are absent from every unauthorized surface.

#### Test evidence discipline

Every build-input change runs its affected checks with their native commands before it is
committed.

New smoke work follows [Contract Evolution](CONTRACT_EVOLUTION.md): the owning Rust, browser,
or SDK harness supplies bounded lifecycle, assertions, cleanup and evidence. Visual browser
acceptance uses a headed hardware-backed browser. Recording and live-view GPU evidence is collected only on
the Linux deployment with accessible NVIDIA hardware. macOS review or software rendering cannot
close any visual gate.

### Reference Patch Policy

| Patch | Retain as guidance | Reject |
|---|---|---|
| `0001-exact-app-resolution.patch` | exact URI resolution, single flight, direct-route migration, and unrelated-server isolation tests | mechanical application, duplicated authority in BFF, and broad listing/cache changes not required by the targeted resolver |
| `0002-app-scoped-resource-list.patch` | bridge settlement, typed family metadata, bounded one-page result, and unavailable versus empty tests | a second independent BFF listing pass after exact resolution exists |
| `0003-governed-app-artifact-upload.patch` | explicit App grant, UUIDv7 identity, exact media/size checks, governed receipt, and CSRF-preserving host path | frame-to-host `ArrayBuffer`, Axum `Bytes`, cloned bodies, BFF/Gateway whole-file buffering, and in-memory idempotency |
| `0004-distributed-app-call-tracing.patch` | HTTP extraction/injection helpers, BFF and Gateway continuation, upstream transport injection, and redaction tests | Grafana configuration, reverse proxy, identity headers, observability routes, and any unrelated Console product surface |

The package contains no recording patch. Its recording prose supplies requirements, while this
plan replaces the proposed generic binary format with the Rerun-native hard cut above.

### Commit Plan

Commits stay small and coherent even though activation is a hard cut.

1. Add Phase 0 contract types, design updates, and schemas while keeping the repository green.
2. Implement Gateway exact App resolution and its policy/cache tests.
3. Migrate BFF and both hosts, add scoped resource listing and recovery, then delete direct broad
   catalog use.
4. Add durable recording dataset/layer identities and migrations.
5. Add freeze-time Store ID normalization, object publication, properties, and recovery.
6. Implement governed virtual Redap catalogs and the supported read profile.
7. Implement bounded Arrow projection and its exact App integration.
8. Migrate Console, Catalog SDK fixtures, playback manifest, smoke tests, and optional offline data
   migration. Delete the old recording architecture.
9. Add streaming Artifact admission from host picker through durable receipt.
10. Project extension releases and complete trace continuity in separate commits.
11. Publish live-view schemas and Python helpers, migrate the external fixture, then add the browser
    package and generic harness without changing the first-party runtime.
12. Add GPU memory and reasoning contracts with their validation evidence.
13. Advance deployment component ownership, split any mixed Helm releases, and add zero-write
    acceptance.
14. Run integrated Linux acceptance, update current architecture documents, and close the plan's
    implementation record.

No commit enables a new route before its authorization, bounds, and focused regression tests are
present and green. Focused tests land with the implementation that satisfies them. No commit
removes an old recording path before all in-repository consumers compile against the new contract.
The activation commit contains the final configuration and deletion together.

### Documentation And Generated Artifact Closure

Implementation updates:

- `docs/CODEMAP.md` for every new package, module, test owner, and moved responsibility;
- `docs/RECORDINGS.md` and `servers/recording-mcp/DESIGN.md` for the final catalog contract;
- `mcp/apps-extension/DESIGN.md` for host extensions and exact authority;
- `extensions/contract/DESIGN.md`, extension examples, and external integration guidance;
- `deploy/contract/DESIGN.md`, `docs/GPU_PLACEMENT.md`, and deployment schemas/examples;
- Python SDK and external simulation documentation;
- Console and standalone App host documentation;
- generated JSON Schemas, compatibility manifests, deployment locks, Helm values schemas, and
  conformance profiles affected by the cuts.

Temporary migration instructions are removed after the final supported installation migrates.
User-facing documents describe only the new canonical routes, manifests, identities, and
configuration.

### Definition Of Done

This plan is complete when:

- every request in the decision ledger has either shipped under its approved boundary or has an
  explicit owner-recorded block that satisfies repository blocking policy;
- exact App routes never depend on installation-wide discovery;
- App hosts provide typed scoped resources, recovery, upload, and safe correlation behavior;
- Recording Hub uses the canonical dataset/segment/layer hierarchy and immutable object storage;
- governed Redap serves WebViewer and Catalog SDK reads across exact authorized segments;
- browser recording projections are bounded deterministic Arrow and JSON row query is gone;
- live recording and live-view behavior retain their qualified hardware acceptance;
- external simulators consume published live-view artifacts without first-party domain imports;
- GPU memory and reasoning configuration fail before connection or workload mutation when unsafe;
- selected deployment operations prove zero writes to unselected owners;
- reference-patch-only mechanisms rejected by this plan are absent;
- obsolete schemas, routes, names, files, tests, examples, and compatibility behavior are deleted;
- owning designs and `docs/CODEMAP.md` describe the implemented architecture rather than this
  future plan.
