# Veoveo Code Map

This map identifies ownership boundaries and the shortest path to the code behind a
behavior. It describes only the current hard-cut architecture.

## Documentation Index

General documents define repository-wide contracts and direct readers to the owning
component:

| Document | Purpose |
|---|---|
| [`README.md`](../README.md) | installation entrypoint, development commands, and repository overview |
| [`AGENTS.md`](../AGENTS.md) | mandatory contribution and implementation rules |
| [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md) | normative product and architecture boundaries |
| [`TECH_DESIGN.md`](TECH_DESIGN.md) | current implementation of those architecture decisions |
| [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md) | shared responsibility, continuous containment boundary, and operating proof for always-autonomous agents |
| [`WORK_CONTEXT_GOVERNANCE.md`](WORK_CONTEXT_GOVERNANCE.md) | invocation authority, output ownership, effective access, and rollout |
| [`ENTERPRISE_DEPLOYMENT.md`](ENTERPRISE_DEPLOYMENT.md) | OCI release, enterprise configuration, secrets, GitOps, extensions, and acceptance |
| [`EXTERNAL_EXTENSIONS.md`](EXTERNAL_EXTENSIONS.md) | supported private external-repository contract, artifact ownership, compatibility manifests, and installation composition |
| [`EXTERNAL_REPOSITORY_INTEGRATION.md`](EXTERNAL_REPOSITORY_INTEGRATION.md) | coding-agent runbook for native external build, conformance, private publication, gateway composition, and digest-pinned GitOps integration |
| [`LOCAL_DEPLOYMENT_PROFILES.md`](LOCAL_DEPLOYMENT_PROFILES.md) | disposable k3d showcase profile contract |
| [`CODEMAP.md`](CODEMAP.md) | documentation index, code ownership, and change routing |
| [`RECORDINGS.md`](RECORDINGS.md) | recording ingest, catalog, sealing, and governed read path |
| [`RECORDING_INGEST.md`](RECORDING_INGEST.md) | external/LAN producer protocol, auth, durability, and routing |
| [`DEVELOPMENT_ITERATION.md`](DEVELOPMENT_ITERATION.md) | affected-target staging, digest-locked development rollout, focused acceptance, runtime pressure diagnostics, and iteration budgets |
| [`CONTINUOUS_INTEGRATION.md`](CONTINUOUS_INTEGRATION.md) | temporary host-local test reporting, informational GitHub presentation, and the future full GPU CI architecture |
| [`connectors/README.md`](connectors/README.md) | third-party MCP connector catalog, recipe contract, and governed upstream path |

Exploratory documents preserve open design work. They are not normative and do not
authorize implementation:

| Document | Exploration |
|---|---|
| [`SELF_IMPROVING_HARNESS.md`](SELF_IMPROVING_HARNESS.md) | auth-aware profile strategies, MCP dynamics evidence, evaluation, measured acceptance through the scorer primitive, and possible self-improving harness boundaries |
| [`HARNESS_MEDIATED_MODEL_POST_TRAINING.md`](HARNESS_MEDIATED_MODEL_POST_TRAINING.md) | exact-call trajectories through the deployed harness, rollout-level post-training semantics, governed evaluation, and candidate-model admission boundaries |
| [`FACTORY_ISOLATION.md`](FACTORY_ISOLATION.md) | kernel-isolated runtime for the factory loop: candidate policy domains, the container-socket question, trial acceptance criteria, and the adoption path |
| [`REGULATED_READINESS.md`](REGULATED_READINESS.md) | shared responsibility model, control fabric, gap register, and remediation backlog for regulated work |

Implementation plans describe future hard cuts. A plan's status line records whether
its execution is approved. Existing contracts remain authoritative until each planned
change lands:

| Document | Planned change |
|---|---|
| [`REPOSITORY_HARDENING_PLAN.md`](REPOSITORY_HARDENING_PLAN.md) | compiled repository tooling, contract enforcement, test and smoke ownership, architecture policy, supply-chain hardening, external-extension seams, and governance |
| [`RMCP_3_MIGRATION.md`](RMCP_3_MIGRATION.md) | hard cut to MCP `2026-07-28` and `rmcp` 3, official Tasks and multi-round requests, stateless transport, subscription and replica redesign, Rig migration, duplicate protocol deletion, and acceptance |
| [`PLATFORM_IMPROVEMENTS_PLAN.md`](PLATFORM_IMPROVEMENTS_PLAN.md) | test-driven agent resource continuity and correction, owner-supplied Secret closure, standalone MCP Apps, canonical domain resource handoff, compiler-ready provenance, spatial correctness, and optional or evidence-gated deployment, model, GPU, and private-build work |

MCP designs live with the crate whose public contract they specify:

| Document | Domain |
|---|---|
| [`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md) | the normative MCP `2026-07-28` server contract: Discover, stateless Streamable HTTP, official Tasks and multi-round input, request-scoped subscriptions, replica-safe state, schema bounds, packaging, well-known resources, and compliance |
| [`mcp/conformance/DESIGN.md`](../mcp/conformance/DESIGN.md) | typed domain-neutral hosted-server certification profiles, reports, and standalone distribution |
| [`mcp/composer/DESIGN.md`](../mcp/composer/DESIGN.md) | offline external gateway fragment/binding composition, requirements, and deterministic provenance |
| [`platform/runtimes/simulation/DESIGN.md`](../platform/runtimes/simulation/DESIGN.md) | canonical hardware-GPU Isaac Sim and Isaac Lab runtime, selected extension profile, and conformance probes |
| [`servers/duckdb-mcp/DESIGN.md`](../servers/duckdb-mcp/DESIGN.md) | analytical SQL, Spatial, sandboxing, tasks, and governed data movement |
| [`servers/frames-mcp/DESIGN.md`](../servers/frames-mcp/DESIGN.md) | local coordinate frames and bounded transformations |
| [`mcp/apps-extension/DESIGN.md`](../mcp/apps-extension/DESIGN.md) | the MCP Apps server↔core↔UI contract for domain views and administration |
| [`servers/map-mcp/DESIGN.md`](../servers/map-mcp/DESIGN.md) | Earth geography, map data administration, logistics routing, and immutable Optimization travel models |
| [`servers/optimization-mcp/DESIGN.md`](../servers/optimization-mcp/DESIGN.md) | NVIDIA cuOpt routing, route scenarios, convex and MILP models, independent verification, and GPU execution |
| [`servers/stream-mcp/DESIGN.md`](../servers/stream-mcp/DESIGN.md) | admitted live and replay GStreamer graphs, typed pipeline profiles, live results, and the Stream MCP App |
| [`servers/reason-mcp/DESIGN.md`](../servers/reason-mcp/DESIGN.md) | governed video reasoning, grounding, and audited world-model output |
| [`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) | temporal authority, operational calendars, clock quality, and events |
| [`servers/timeseries-mcp/DESIGN.md`](../servers/timeseries-mcp/DESIGN.md) | timeseries forecasting, preview contract, and the forecast MCP App view |
| [`servers/view-mcp/DESIGN.md`](../servers/view-mcp/DESIGN.md) | governed static scene compositions, 3D Tiles residency, declarative overlays, and GPU frame capture |
| [`servers/uav-sim-mcp/DESIGN.md`](../servers/uav-sim-mcp/DESIGN.md) | governed UAV simulation sessions, principal-to-vehicle authority, Map route admission, exclusive command leases, telemetry, authoritative cameras, shared render products and H.264 fanout, and the UAV App |

Deployment, examples, templates, and fixtures keep their instructions beside the
material they operate:

| Document | Purpose |
|---|---|
| [`configs/stream/README.md`](../configs/stream/README.md) | operator-admitted Stream graph, profile, model, and live-ingress configuration |
| [`configs/reason/README.md`](../configs/reason/README.md) | reason catalog and runtime configuration |
| [`deploy/contract/DESIGN.md`](../deploy/contract/DESIGN.md) | typed development profile and local registry declarations shared by operational tools |
| [`deploy/helm/veoveo/README.md`](../deploy/helm/veoveo/README.md) | Kubernetes installation contract |
| [`deploy/offline/README.md`](../deploy/offline/README.md) | offline bundle construction and loading |
| [`docs/IMAGE_BUILDS.md`](IMAGE_BUILDS.md) | typed Bake planning, managed builder, cache families, and immutable image publication |
| [`docs/IMAGE_BUILD_PERFORMANCE.md`](IMAGE_BUILD_PERFORMANCE.md) | image-graph baseline, cold and warm measurements, digest equality, and incremental-build acceptance |
| [`examples/bioma/README.md`](../examples/bioma/README.md) | enterprise GitOps reference and owner-local compiled acceptance over k3d, OCI charts, Entra, and Cloudflare Tunnel |
| [`showcase/README.md`](../showcase/README.md) | showcase entrypoint |
| [`showcase/sumo/README.md`](../showcase/sumo/README.md) | SUMO/TraCI integration and operations |
| [`showcase/uav-sim/README.md`](../showcase/uav-sim/README.md) | Isaac/Cesium/Newton/Warp/PX4 UAV simulation integration and operations |
| [`showcase/uav-sim/ACCEPTANCE.md`](../showcase/uav-sim/ACCEPTANCE.md) | deployed UAV acceptance catalog and the repeatable per-agent named-location mission E2E runbook |
| [`templates/python-mcp/README.md`](../templates/python-mcp/README.md) | canonical Python MCP server template |
| [`timesfm-showcase/README.md`](../servers/timeseries-mcp/testdata/timesfm-showcase/README.md) | TimesFM test fixture provenance and use |

The canonical long-form sources are
[`veoveo-whitepaper-print.html`](veoveo-whitepaper-print.html) and
[`autonomy-harness-print.html`](autonomy-harness-print.html). Their source headers carry
the headed Chrome `Page.printToPDF` contract that produces the
[`whitepaper PDF`](veoveo-whitepaper.pdf) and
[`harness PDF`](autonomy-harness.pdf). [`autonomy-harness.html`](autonomy-harness.html)
is the browser edition of the harness document. These three publication
surfaces describe the same 16-server catalog and use the normative component
designs above for Stream, Reason, authoritative UAV live view, Recording
Hub, administration, and GPU policy.

## Root

| Path | Ownership |
|---|---|
| `Cargo.toml` | Rust workspace membership and pinned shared dependencies |
| `rust-toolchain.toml` | canonical Rust toolchain |
| `docker-bake.hcl` | local OCI image build groups |
| `.env.example` | required installation configuration and secrets |
| `configs/gateway.local.json` | generic gateway control-plane configuration |
| `configs/gateway.smoke.json` | isolated smoke control plane |
| `configs/deployments.json` | deployment contract examples |
| `configs/stream/` | admitted GStreamer graph, typed profile, TensorRT model, and live-ingress catalog example |
| `configs/reason/` | world-model checkpoint reason catalog example and deployment contract |
| `configs/view/` | server-side 3D scene-layer catalog without provider secret values |
| `deploy/contract/` | multi-source deployment v6 profiles and locks, platform/workload/extension ownership, exact platform-image and managed DRA closure, rendered Secret-reference closure, split source/installation Helm values, typed registry transport, physical-GPU topology, collision-free publication preflight, schema generation, and pure validation |
| `docs/GPU_PLACEMENT.md` | managed NVIDIA DRA artifacts, installation schema, lifecycle, conflict transition, validation, upgrade, rollback, and recovery contract |
| `extensions/contract/` | typed external artifact, compatibility-manifest, extension-release, simulation build-lock/result/evidence, and schema contracts |
| `extensions/examples/` | anonymous external fragment and installation-binding examples |
| `deploy/local/k3d/` | GPU-capable local Kubernetes cluster and values |
| `AGENTS.md` | hard-cut, task, type, module, and smoke-test rules |
| `docs/` | general architecture, code index, recording design, and rendered publications |
| `agents/` | agent kernel and durable agent runtime |
| `apps/` | user-facing applications and their service boundaries |
| `mcp/` | shared MCP protocol contracts, extensions, and bridges |
| `platform/` | internal platform services, persistence, and reusable runtimes |
| `servers/` | independently deployed MCP servers and protocol projections |
| `testing/` | conformance tooling and multi-process smoke harnesses |
| `sdk/` | language SDK workspaces |
| `deploy/helm/veoveo/` | Kubernetes installation chart, chart-owned first-party service definitions, and typed component/server presets |
| `showcase/uav-sim/deploy/helm/` | authoritative GPU simulator, UAV MCP server, isolated generic pilot agents, shared H.264 stream ingress, continuous camera-product configuration, and viewer authorization |
| `testing/smoke/src/bin/smoke/deployment.rs` | profile validation and orchestration, pre-mutation Secret presence closure, immutable gateway activation, and ordered Helm release inputs |
| `testing/smoke/src/bin/smoke/deployment/gpu.rs` | managed NVIDIA DRA orchestration, ResourceSlice inventory, persistent-claim preservation, and workload placement proof |
| `testing/smoke/src/bin/smoke/deployment/gpu/helm.rs` | Helm v4 release metadata, exact allocator artifact and render verification, and atomic installation |
| `testing/smoke/src/bin/smoke/deployment/gpu/admission.rs` | kubelet-plugin selector, DaemonSet readiness, node taint, and pod scheduling diagnostics |
| `testing/smoke/src/bin/smoke/deployment/gpu/workloads.rs` | typed Deployment selector, current ReplicaSet ownership, Ready Pod/container, replica-count, and in-container GPU evidence targeting |
| `testing/deployment-smoke/` | focused deployment-profile and exact-revision GitOps convergence CLI that avoids compiling unrelated protocol and visual scenarios |
| `testing/browser-smoke/` | focused headed-browser acceptance over an already-running simulation, mandatory Console and standalone App host preflights, and explicit native live-view container-restart recovery evidence |
| `deploy/helm/veoveo-extension/` | private reusable extension-chart helper API and immutable chart package source |
| `deploy/offline/` | pinned image manifest, bundle builder/loader, offline values |
| `showcase/sumo/` | real SUMO/TraCI domain showcase |
| `showcase/uav-sim/` | Google 3D Tiles UAV simulation showcase over Isaac, Cesium, Newton, Warp, and PX4 |
| `examples/bioma/` | executable enterprise GitOps reference with Bioma-owned desired state |
| `examples/bioma/platform/flux/` | exact Flux controller fixture for the local Bioma cluster; it is installed before installation desired state and remains outside Veoveo runtime ownership |
| `examples/bioma/gitops/` | Flux Git source, OCI chart sources, platform and extension Helm releases, and installation-owned edge resources |
| `examples/bioma/gateway.json` | the reference installation's complete control plane: 16-server MCP catalog, OAuth clients, policy rules, and routes |
| `examples/bioma/acceptance/` | owner-local compiled composition checks over the Bioma desired state |
| `sdk/python/` | Python platform package for hosted MCP servers |
| `templates/python-mcp/` | canonical Python server template (`datasheet`) |
| `testing/fixtures/extension-helm-consumer/` | anonymous cross-release Helm library acceptance fixture |
| `testing/fixtures/external-extension-installation/` | anonymous deployment v5 platform selection and exact Artifact/Frames/Map/Media/Recording/RRD image-closure acceptance |
| `testing/fixtures/external-simulation-extension/` | isolated contract-only Python simulation extension with typed authoritative camera and render-product declarations; it is never visual GPU evidence |
| `testing/fixtures/external-simulation-installation/` | independent platform/extension source composition, installation-owned gateway binding, and contract-only simulation deployment closure |
| `deploy/contract/tests/multi_repository.rs` | anonymous acceptance using independent platform, extension, and installation Git histories with one combined deployment lock |
| `testing/fixtures/simulation-overlay/` | repository-neutral overlay identity and CUDA probe for canonical simulation-base acceptance |
| `tools/image-build/` | registry-neutral managed BuildKit base configuration, shared Rust builder inputs, and the source-locked first-party Datasheet image environment |
| `tools/xtask/` | compiled repository command, enforcement, local test reporting, typed smoke prerequisite builds and dispatch, exact image planning, profile-registry builder configuration, and release orchestration |

## Placement Rules

The top-level directories express ownership rather than implementation language. A Rust
crate belongs beside the system it implements; Rust is not an architectural boundary.

| Root | Put code here when it owns |
|---|---|
| `servers/` | a hosted MCP server with its own protocol surface, deployment image, and domain behavior |
| `mcp/` | protocol contracts, transport extensions, or bridges shared by more than one server |
| `extensions/` | cross-cutting contracts and reference material for independently owned extension repositories |
| `platform/` | internal control/data-plane services, durable stores, and reusable execution runtimes |
| `agents/` | autonomous agent behavior or durable agent scheduling |
| `apps/` | a user-facing application and its application-specific backend |
| `testing/` | cross-component conformance, smoke, and deployment verification |
| `sdk/` | a language-native client or server-development package |
| `showcase/` | an end-to-end domain integration that is not part of the core installation |

MCP servers do not live under a generic `tools/` root. They expose resources, prompts,
tasks, subscriptions, notifications, and structured content in addition to tools, so
`servers/` names the deployable boundary without narrowing the protocol.

## Shared Contracts

### `mcp/apps-extension`

MCP Apps (SEP-1865 / ext-apps "2026-01-26") support: pinned protocol
constants (`io.modelcontextprotocol/ui`, `text/html;profile=mcp-app`), typed
`_meta.ui` shapes, server helpers (capability declaration, `ui://` app
resources, tool links), and host helpers (capability declaration, app
detection, and visibility checks). The Console owns the generic reactive-resource
adapter that carries final-profile `subscriptions/listen` wakes across the pinned Apps
bridge without exposing domain payloads. `mcp/apps-extension/DESIGN.md` is the
canonical server↔core↔UI contract: domain reads are resources, domain
mutations are tools, domain views are `ui://` apps — never bespoke admin
REST or hardcoded console pages.

### `mcp/bridges`

| Component | Responsibility |
|---|---|
| `stdio` | Same-version MCP `2026-07-28` bridge for one explicitly owned local child; the HTTP endpoint is stateless and discovers the child through the final lifecycle |
| `legacy` | Optional, isolated MCP `2025-11-25` external connector for one configured local stdio child or remote HTTP endpoint; it exposes only MCP `2026-07-28` tools, resources, prompts, and completions toward Veoveo and never fabricates Tasks, MRTR, subscriptions, or deprecated capabilities |


### `mcp/contract`

This crate owns vocabulary shared across services. It must not absorb a domain tool
schema merely because the server is first-party.

| File | Responsibility |
|---|---|
| `access.rs` | artifact access levels, user/group subjects, grant composition |
| `agents.rs` | authenticated operator-message, durable input-request decision, wake-receipt, and pending-input view contracts |
| `artifact_service.rs` | artifact-plane requests, capabilities, share links, native async port |
| `duckdb.rs` | shared DuckDB source types and safe read-function SQL fragments |
| `coordinates.rs` | shared coordinate spaces, world/revision/frame identities, complete frame-tree vocabulary, WGS84 positions, and operation provenance |
| `docs.rs` | build-embedded server documents, once-built revision/compliance declarations, compliance parsing, and canonical llms.txt rendering; observed capabilities come from Discover and list methods |
| `uri.rs` | canonical hosted-server resource URI construction and shared one-segment document URI parsing |
| `storage.rs` | artifact metadata, release state, compliance labels |
| `gateway.rs` | gateway control-plane aggregate and public re-exports |
| `gateway/ids.rs` | validated identity and configuration newtypes, including bounded principal display metadata that never participates in authorization |
| `gateway/auth_config.rs` | IdP, authorization server, OAuth client surfaces |
| `gateway/server_config.rs` | hosted server and profile exposure contracts, including exact cross-server App resource dependencies |
| `gateway/policy.rs` | actions, targets, rules, effects, audit reason model |
| `gateway/runtime_state.rs` | durable auth/runtime record contracts, including display metadata continuity across authorization-code and refresh grants |
| `gateway/validation.rs` | fail-closed cross-reference and invariant validation |
| `gateway/composition.rs` | typed external server fragments, installation bindings, deterministic pure composition, requirements, and provenance |
| `internal_auth.rs` | Ed25519 signing keys, JWKS trust, internal issuer/verifier |
| `deployment.rs` | Connected/offline Kubernetes topology contract |
| `bootstrap.rs` | generic installation-time server bootstrap envelope, constants, and semantics |
| `tasks.rs` | platform task ownership and durable routing vocabulary; official MCP Task wire types come from `rmcp` |
| `provider.rs` | provider job/event contracts; no status polling API |
| `subscriptions.rs` | request-scoped resource and list-change event hub for final `subscriptions/listen` streams |
| `protocol.rs` | sole final MCP revision, shared cache lifetimes, and bounded W3C trace metadata validation |
| `transport.rs` | canonical stateless Streamable HTTP configuration, no-session adapter, and whole-response 8 MiB final JSON budget enforcement |
| `telemetry.rs` | tracing/log initialization and guards |

### `mcp/composer`

Owns the offline `gateway-compose` native/OCI command. It reads matched anonymous or
private extension fragments and installation bindings, calls the pure contract
composer, and writes one ordinary validated control plane plus requirements and
path-free content provenance.

### `platform/recordings/rrd`

Owns cross-domain Rerun/RRD spacetime types, adapters, and encoded-video boundary
inspection. Domain results that do not overlap Rerun concepts stay local to their MCP
crate.

### `platform/recordings/video`

Owns governed video selection and task-start materialization shared by Stream replay and
Reason. It consumes Recording MCP read plans, combines immutable archive shards with
complete acknowledged live ingest parts, and remuxes the bounded H.264 range without
re-encoding.

### `platform/recordings/protocol`

Owns the versioned protobuf contract for authenticated recording streams, batches,
checkpoints, discovery, and errors. Gateway, Record Hub, the forwarder, and smoke tests
share this crate.

## SurrealDB Platform Store

### `platform/store`

The only durable platform persistence layer.

| File | Responsibility |
|---|---|
| `config.rs` | root/database auth configuration and validation |
| `migrations.rs` | ordered SurrealDB 3.2 schema migrations |
| `models.rs` | persisted Rust record and enum definitions |
| `ids.rs`, `table.rs` | domain-specific record IDs and table identities |
| `administration.rs` | bootstrap, runtime user, migration administration |
| `identity.rs` | tenant/principal/group resolution |
| `gateway_runtime.rs` | control revisions, auth state, refresh/JWT runtime records |
| `artifacts.rs` | blob, occurrence, grant, share, capability transactions |
| `coordinates.rs`, `frame_worlds.rs` | coordinate-operation persistence plus authored frame worlds and immutable tree revisions |
| `map.rs` | source, release, active-pointer, mobility, restriction, snapshot, route, matrix, and acquisition persistence |
| `map_authoring.rs` | Work Context-scoped feature layers, immutable schema/style/feature revisions, atomic changesets, heads, publications, and authoring outbox events |
| `map_presentations.rs` | Immutable publication products plus governed, publication-pinned map compositions and revisions |
| `time.rs` | authority sources and releases, active pointers, acquisitions, calendars, epochs, clock policy, and events |
| `recordings.rs` | recording and segment catalog |
| `recording_ingest.rs`, `recording_blueprints.rs` | producer streams, idempotent batch checkpoints, immutable producer Blueprint revisions, and journal state |
| `usage.rs` | shared domain/media usage records |
| `outbox.rs`, `changefeed.rs` | transactional events, checkpoints, LIVE acceleration |
| `live_views.rs` | append-only audit records for authoritative simulation live-view products and ephemeral viewer authorizations |
| `migrations/0040_uav_vehicle_authority.surql` | UAV-owned principal-to-vehicle grants, admitted single-vehicle mission plans, and exclusive command leases scoped by tenant and Work Context |
| `store.rs` | connection and transaction helpers over domain records |

Migrations `0001` through the current version live under `migrations/`. Runtime services
never apply them; installation bootstrap does.

## Durable Tasks

### `platform/task-runtime`

| File | Responsibility |
|---|---|
| `types.rs` | runtime configuration, recovery classes, pins, claims, outcomes |
| `runtime.rs` | create/idempotency, lease, update, cancel, finish, recover, prune |
| `mcp.rs` | projection from durable state into official RMCP Task and DetailedTask types |
| `service.rs` | protocol-neutral durable service boundary delegated from RMCP handlers |
| `lib.rs` | focused public API |

The runtime is the source of truth. RMCP owns the sole Tasks wire model.

## Gateway

### Library surface: `platform/gateway/src`

| Path | Responsibility |
|---|---|
| `catalog.rs` | validated active catalog and profile/server lookup |
| `control_store.rs` | immutable SurrealDB control revisions and activation |
| `auth/` | access tokens, OIDC, ID-JAG, client assertions, immutable principals, and independently typed OIDC display labels |
| `policy.rs` | policy evaluation entrypoint |
| `mcp_support.rs` | MCP URI projection, including declared cross-server resource identities |
| `mcp/authorization.rs` | per-method/profile/server target authorization |
| `mcp/discovery.rs` | exact-authority catalog discovery cache, bounded concurrency, per-server failure isolation, and list-change invalidation |
| `mcp/tools.rs` | aggregated tool projection with explicit helper gating; failure-isolated by default and list-failing for `fail_closed` discovery profiles |
| `mcp/resources.rs` | failure-isolated resource/list projection plus fail-closed read/subscribe routing |
| `mcp/prompts.rs`, `completion.rs` | prompt and completion projection |
| `mcp/tasks.rs` | canonical upstream task client and explicit weak-client task projection |
| `mcp/health.rs` | explicit `health_url` GET probing where only a success status is healthy |
| `mcp/upstream*.rs` | authenticated Streamable HTTP, session-local protocol state, and catalog-revision-scoped sharing of transport-equivalent HTTP/TLS clients |
| `state/audit.rs` | durable policy/audit evidence |
| `state/auth_state.rs` | durable OAuth authorization and replay state |
| `state/refresh_tokens.rs` | refresh family issue/rotate/replay/revoke/GC plus signed-in display-label continuity |
| `state/subscriptions.rs` | durable subscription ownership and forwarding |
| `secrets.rs` | secret-source models and environment/file/Vault resolution |

### Binary surface: `platform/gateway/src/bin/gateway`

| Path | Responsibility |
|---|---|
| `server.rs` | router assembly only |
| `runtime.rs` | shared application state and HTTP clients |
| `oauth/`, `oauth_grants/` | authorize/callback/token and grant handlers |
| `admin/control_plane.rs` | control revision read/update |
| `admin/tasks.rs` | policy-checked cancellation through the owning server's official Tasks endpoint |
| `admin/artifacts.rs` | release/grant/link mutations through artifact service |
| `admin/console/mod.rs` | console snapshot handler, trusted display-name projection for every principal with authenticated-identity precedence, branding, stream cursor bootstrap |
| `admin/console/projection.rs` | tenant projection load and per-entity summary builders |
| `admin/console/stream.rs` | live console SSE: LIVE wake hub, changefeed replay, tenant filtering, limits |
| `admin/console/health.rs` | background MCP server health prober and cache |
| `admin/server_proxy.rs` | generic policy-checked proxy to a hosted server's contract-defined admin API |
| `artifact_download.rs` | authorized/audited large download proxy |
| `recording_playback.rs` | authorized/audited playback manifest and framed live-stream pass-through |
| `audit.rs` | common admin authorization and operation audit helpers |

`gateway.rs` remains the thin CLI/serve entrypoint.

## Artifact Plane

### `platform/artifacts/service`

| File | Responsibility |
|---|---|
| `service.rs` | policy enforcement, grants, release, shares, quotas, retention |
| `ledger.rs` | repository contract and in-memory test implementation |
| `ledger/surreal.rs` | canonical SurrealDB repository adapter |
| `store.rs` | memory/S3 blob storage and signed download behavior |
| `auth.rs` | internal assertion verification and plane caller |
| `http.rs` | internal artifact API plus `/s/{token}` redemption |
| `config.rs` | fail-closed store/database/audience configuration |

### `platform/artifacts/client`

HTTP implementation of the `ArtifactPlane` interface used by domain servers and the
gateway. It forwards the caller's existing signed identity; it never signs one.

### `servers/artifact-mcp`

The canonical MCP-facing artifact projection. `handler.rs` owns tools/resources,
`prompts.rs` owns reusable workflows, and `subscriptions.rs` owns update notification
plumbing.

## Domain Servers

The Rust MCP server pattern is intentionally consistent:

| Local module | Responsibility |
|---|---|
| `contract.rs`, `contract/`, or `domain/` | tool, resource, problem, and result types owned by the domain |
| `engine.rs`, `forecast.rs`, `compiler/`, or another focused engine module | pure domain computation or deterministic provider compilation |
| `verification/` | provider-independent checks when the domain publishes separately verified results |
| `executor/` | private typed provider protocol and client; never a second public API |
| `state.rs` | server-local provider and domain models, not task persistence |
| `uris.rs` | canonical server resource identities |
| `artifacts.rs` | task-bound capability preparation/redemption |
| `administration.rs` | transport-neutral domain administration behind `map:…`-style admin-scoped MCP tools |
| `assets/` | self-contained `ui://` MCP App views served through `read_resource` |
| `bin/server/config.rs` | validated CLI/environment configuration |
| `bin/server/internal_auth.rs` | required gateway assertion middleware |
| `bin/server/ownership.rs` | principal/tenant/label task ownership |
| `bin/server/task_extension.rs` | final extension adapter over TaskRuntime |
| `bin/server/app_state.rs` | dependency composition and recovery |
| `bin/server/outputs.rs` | result models, resource links, and usage projection |

Current MCP crates under `servers/` are indexed here:

| Path | Primary ownership |
|---|---|
| `servers/artifact-mcp` | MCP resources, tools, prompts, and subscriptions over the artifact plane |
| `servers/duckdb-mcp` | arbitrary analytical SQL, governed ingest/export, and DuckDB Spatial |
| `servers/frames-mcp` | complete rooted frame worlds, immutable revisions, coordinate conversion, and operation provenance |
| `servers/map-mcp` | Earth geography, governed feature authoring and products, source and raster releases, reusable spatial derivation, mobility validation, logistics routing, and immutable cuOpt travel models |
| `servers/media-mcp` | webhook-completed provider media work and governed outputs |
| `servers/optimization-mcp` | typed cuOpt routing and route-scenario problems, convex and MILP models, GPU execution, independent verification, and immutable problem/run/solution evidence |
| `servers/stream-mcp` | admitted live and replay GStreamer execution, typed pipeline profiles and results, encoded preview, and the Stream MCP App |
| `servers/reason-mcp` | local recorded-video reasoning, grounding, and Rerun annotations |
| `servers/recording-mcp` | governed recording catalog, queries, subscriptions, and sealing |
| `servers/timeseries-mcp` | time-series analysis, forecasting, evaluation, canonical artifact handoff, and artifacts |
| `servers/timeseries-mcp/src/bin/server/usage_index.rs` | bounded authority-filtered usage discovery with stable task ordering and opaque cursors |
| `servers/time-mcp` | temporal authority, clock assessment, operational calendars, mission timelines, and events |
| `servers/view-mcp` | immutable governed scene compositions, owner and Work Context scoped geospatial views, shared 3D Tiles streaming, GPU overlays, and captured frames |
| `servers/uav-sim-mcp` | provider-neutral UAV simulation sessions, principal-to-vehicle grants, Map route admission, exclusive command leases, missions, telemetry, tasks, recording references, authoritative logical cameras, one shared tiled GPU product, authenticated H.264 fanout, and the UAV App |

The packaged Node chart server keeps its Veoveo boundary beside the image:

| Path | Responsibility |
|---|---|
| `servers/chart-mcp/server.mjs` | canonical mounted Streamable HTTP lifecycle, hosted identity, well-known resources, and derived declaration over the pinned upstream chart server |
| `servers/chart-mcp/internal-auth.mjs` | fail-closed Ed25519 gateway-token verification shared by the chart MCP and admin docs routes |

### UAV Simulation Integration

| Path | Responsibility |
|---|---|
| `servers/uav-sim-mcp/src/server/live_view.rs` | actor/browser stream authorization, shared camera-product projection, token renewal, closure, expiry, and connection state |
| `servers/uav-sim-mcp/src/server/live_stream.rs` | authenticated WebSocket admission and byte-transparent H.264 forwarding from simulator-owned camera products |
| `servers/uav-sim-mcp/src/server/live_view_audit.rs` | durable audit projection for camera, product, authorization, denial, expiry, and revocation events |
| `servers/uav-sim-mcp/src/server/runtime_events.rs` | strict authenticated adapter-ready and final-ready HTTP stream ingestion, immutable binding reapplication trigger, and subscribed live-camera notification |
| `servers/uav-sim-mcp/assets/live-app.html` | self-contained authoritative-camera selection and multi-view live App |
| `showcase/uav-sim/agents/` | reviewed parameterized pilot manifest and durable memory schema; geographic work data remains outside agent memory |
| `showcase/uav-sim/map/` | Map-owned named-place and operational air-network source fixture for the showcase |
| `showcase/uav-sim/runtime/` | thin domain overlay on the canonical Isaac runtime with Cesium, a repository-owned batched Warp plant, Newton Experimental rigid views, PX4 HIL lifecycle, RTX domain sensors, authoritative logical cameras, shared camera-owned RTX/NVENC products, direct Stream publication, and Rerun publication |
| `showcase/uav-sim/runtime/veoveo_uav_sim/fleet_runtime.py` | 30 Hz CUDA fleet authority, direct Newton Experimental tensor-state writes, and ordered 60 Hz PX4 HIL publication without MuJoCo-Warp stepping |
| `showcase/uav-sim/runtime/veoveo_uav_sim/plant_warp.py` | one fused CUDA kernel for batched motors, force, torque, native Newton body integration, launch-surface contact, and HIL sensor sampling |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera.py` | operator-camera orchestration over focused rig, smoothing, product, and health modules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_rigs.py` | authoritative target sampling and desired poses for every supported camera rig |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_smoothing.py` | frame-rate-independent position and shortest-arc orientation filters with typed reset rules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_products.py` | one continuous tiled RTX/NVENC product and keyframe-aware H.264 access-unit ring shared by every streamable logical camera |
| `showcase/uav-sim/runtime/veoveo_uav_sim/render_pose.py` | bounded agreement diagnostics between authoritative camera poses and rendered Hydra frames |
| `showcase/uav-sim/runtime/veoveo_uav_sim/physical_camera.py` | exact authoritative body-and-mount USD sensor camera, distinct from smoothed operator views |
| `showcase/uav-sim/runtime/veoveo_uav_sim/hydra_camera.py` | physical-camera Hydra product, CUDA AOV-to-native-RTSP configuration, encoded-frame pairing, and nonblocking sensor health |
| `showcase/uav-sim/runtime/veoveo_uav_sim/rtsp_h264.py` | pod-local RTSP client, interleaved RTP parser, RFC 6184 depacketizer, and native encoded access-unit delivery |
| `showcase/uav-sim/runtime/veoveo_uav_sim/h264.py` | strict native Annex B GOP parsing and decoder-reentrant SPS/PPS/IDR qualification |
| `showcase/uav-sim/runtime/veoveo_uav_sim/runtime_events.py` | retained nonblocking adapter-ready and final-ready lifecycle edges over the authenticated private HTTP stream |
| `showcase/uav-sim/runtime/veoveo_uav_sim/tile_lifecycle.py` | generation-safe reduction of redacted native Cesium load events, cache-preserving provider-session replacement proved by prepared geometry, prepared materials, and rendered coverage, and current coverage state without polling |
| `showcase/uav-sim/runtime/patches/cesium-0.29.0-external-viewports.patch` | pinned headless viewport-authority switch that prevents interactive window discovery from clearing simulator-managed Cesium views |
| `showcase/uav-sim/runtime/patches/cesium-0.29.0-lifecycle-events.patch` | pinned Omniverse extension events, load generations, and query-secret log redaction |
| `showcase/uav-sim/runtime/patches/cesium-native-ca0311f-tile-load-events.patch` | pinned Cesium Native child-content failure delivery through the existing tileset callback |
| `servers/uav-sim-mcp/src/server/world_bootstrap.rs` | strict startup application and reactive same-binding reapplication of an installation-owned immutable world binding |
| `showcase/uav-sim/deploy/` | commit-addressed OCI publication, MCP-configured GPU simulator workload, four identity- and storage-isolated generic pilot workloads, shared H.264 ingress, continuous camera products, versioned persistent cache, typed sensor configuration, and network policy |
| `showcase/uav-sim/scenarios/` | reusable world trees plus strongly typed live mission and acceptance parameters outside the Isaac image context |
| `examples/bioma/uav-sim-values.yaml` | reference authoritative camera, product, public gateway origin, and recording tenant binding |
| `testing/smoke/src/bin/smoke/scenarios/uav_sim.rs` | runtime world publication plus credentialed Google tiles, PX4, independent live Stream processing, Recording Hub replay, Reason, and concurrent GPU acceptance |
| `testing/smoke/src/bin/smoke/scenarios/uav_sim/showcase.rs` | showcase-owned authoritative UAV cameras and products, real authenticated Console checkpoints, governed Rerun playback, and revision-qualified evidence |
| `testing/smoke/src/bin/smoke/scenarios/uav_sim/browser.rs` | shared headed Chrome attachment, hardware WebGPU-or-WebGL enforcement, opaque-origin App hosting, Map workspace viewport acceptance, dedicated simultaneous-viewer windows, Console live-view interaction, and screenshots |
| `testing/browser-smoke/src/main.rs` | focused headed-browser commands and versioned evidence manifests for the Map workspace and UAV visual workflows |
| `testing/browser-smoke/src/restart.rs` | focused same-document native live-view recovery across independent MCP-pod and simulator-container restarts, including proof that MCP replacement leaves the GPU pod unchanged |
| `testing/smoke/src/bin/smoke/scenarios/uav_sim/browser/recording_acceptance.rs` | scoped Redap network evidence, live-source continuity, archive-request rejection, and nonblank Rerun viewport measurement |

### Geospatial Domains

The geospatial hard cut has three canonical servers:

| Path | Responsibility |
|---|---|
| `servers/map-mcp` | Earth geography, complete immutable source features, governed COG rasters and terrain derivations, reusable spatial geometry and mobility validation, authored GeoJSON/JSON-FG layers, bounded OGC GeoPackage vector transfer, source acquisition, release activation, DuckDB Spatial analytics, CRS and geodesic work, geofences, restrictions, Valhalla land routing, governed network routing, matrices, Optimization travel models, and reachable areas |
| `servers/frames-mcp` | ECEF-rooted world trees, geodetic/static/dynamic transforms, immutable revisions, bounded coordinate conversion, durable batch work, operation provenance, artifacts, and usage |
| `servers/view-mcp` | governed static scene compositions, configured 3D scene layers, camera rigs, exact Map/Frames/Artifact inputs, bounded overlays, NVIDIA-accelerated rendering, and frame resources |

The crate-local design documents own their protocol, administration, persistence, and
deployment details.

View composition work starts in `src/contract/composition.rs`, which owns
strong identities, governed inputs, Frames bindings, overlay geometry, styles,
validity, and bounds. `src/composition.rs` resolves exact artifact bytes and
converts validated overlays into GPU render products. `src/state.rs` owns
principal and Work Context scoped composition, view, capture snapshot, and
frame state. `src/mcp.rs` publishes the canonical tools and resources, while
`src/server/tasks.rs` persists recoverable capture snapshots.

Map authoring is split by responsibility. `src/contract/features.rs` owns feature wire
types and bounds, while `src/contract/compositions.rs` owns publication products and
composition contracts. `src/contract/transfers.rs` owns durable import, export, and
vector-product task contracts. `src/authoring/service.rs` applies Work Context policy
and optimistic concurrency. `src/authoring/projection.rs` consumes canonical SurrealDB outbox events,
while `src/authoring/query.rs` owns the parameterized DuckDB Spatial and bounded CQL2
query projection. `src/authoring/query/performance.rs` owns the 10k, 100k, and
million-feature R-tree plan, correctness, maintenance, latency, throughput, and
storage gates. `src/authoring/presentations.rs` governs immutable products and
composition revisions. `src/authoring/transfers.rs` owns canonical bounded GeoJSON
and RFC 8142 transfer plus GeoParquet 1.0 and MVT 2.1 products.
`data/src/map_data/feature_package.py` and `src/feature_packages.rs` own the
pinned-GDAL OGC GeoPackage inspection and conversion boundary.
`src/mcp/authoring.rs` publishes the write and query tools. `src/server/tasks.rs` owns
durable execution and task-local staging, while
`src/server/tasks/feature_transfers.rs` owns GeoPackage-aware transfer execution.
`app/` owns the exact MapLibre bundle pipeline and the permission-aware source,
while `assets/workspace-app.html` is the generated self-contained Map MCP App
for composition viewing, feature authoring, and administration. The canonical SurrealDB schemas are
`platform/store/migrations/0025_map_authoring.surql`
and `platform/store/migrations/0026_map_authoring_products.surql`.

Immutable acquisition products use a separate analytical path.
`src/contract/source_products.rs` owns complete source-feature, raster-product,
query, and derivation contracts. `src/release_products.rs` projects activated
GeoJSON, GeoJSON Sequence, and raster metadata into DuckDB Spatial.
`data/src/map_data/adapters/` owns bounded normalization, while
`data/src/map_data/raster_ops.py` performs the controlled GDAL derivations.
`src/raster.rs` supervises that helper and `src/server/tasks.rs` owns its
durable artifact publication. `src/geography.rs` owns direct governed
position, location, and corridor inspection plus restriction publication,
surfaced through tools such as `inspect_position`, while `src/analytics.rs`
owns the sandboxed DuckDB Spatial engine behind those reads.

Reusable spatial planning is split from routing. `src/contract/spatial.rs`
owns the bounded operation and persisted-result schemas. `src/spatial/derive.rs`
implements pure geometry, `src/spatial/projection.rs` owns the exact local
projection profile, and `src/spatial/validation.rs` resolves mobility envelopes
and active restrictions. `src/spatial/mod.rs` binds catalog authority,
provenance, and DuckDB persistence.

### Optimization And Travel Models

| Path | Responsibility |
|---|---|
| `servers/map-mcp/src/contract/travel_models.rs` | exact `veoveo.io/travel-model-artifact/v1` cross-server wire profile, controlled location and vehicle-type IDs, bounds, provenance, and Map record |
| `servers/map-mcp/src/routes/service.rs` | governed route and Valhalla matrix construction, immutable mobility-profile versions, persisted operational snapshots, unavailable arcs, and the validated `veoveo.io/map-route-handoff/v1` cross-server projection |
| `servers/map-mcp/src/server/tasks.rs` | durable travel-model publication, owner visibility, neutral artifact manifest identity, and resource notifications |
| `servers/optimization-mcp/src/domain/` | public routing, route-scenario, convex, MILP, solution, verification, and solver-profile contracts |
| `servers/optimization-mcp/src/compiler/` | deterministic conversion into cuOpt routing arrays and sparse mathematical structures |
| `servers/optimization-mcp/src/verification/` | cuOpt-independent routing feasibility, mathematical feasibility, integrality, and objective checks |
| `servers/optimization-mcp/src/executor/` | private bounded Unix-socket protocol and Rust client |
| `servers/optimization-mcp/executor/` | pinned Python cuOpt 26.06 GPU adapter and hardware health check |
| `servers/optimization-mcp/src/bin/server/` | MCP tasks, GPU queue, problem/run/solution resources, artifact publication, prompts, and identity |
| `servers/optimization-mcp/src/bin/server/index.rs` | authority-scoped exact domain lookup, compact stable pages, opaque collection cursors, bounded completion search, and usage discovery |
| `deploy/contract/src/lib.rs` | portable Optimization capability, exact Optimization image closure, and mandatory `cuopt-executor` GPU scheduling declaration |
| `deploy/helm/veoveo/definitions/domain-services.yaml` | single Optimization Pod, CPU control container, one-GPU cuOpt sidecar, shared socket, memory-backed shared memory, and persistent workspace |
| `examples/bioma/images.lock.yaml` | immutable Bioma release digests for both Optimization control and cuOpt executor images |
| `testing/smoke/src/bin/smoke/scenarios/agent_kernel.rs` | full Pilot mission flow through gateway task dispatch, cuOpt MILP execution, independent verification, wake delivery, and durable memory |

### Temporal Domain

| Path | Responsibility |
|---|---|
| `servers/time-mcp` | authority-bound time resolution and conversion, calendar expansion, timeline validation, interval algebra, clock assessment, mission epochs, and temporal events |
| `servers/time-mcp/src/acquisition/` | bounded IANA TZDB and leap-second acquisition, validation, compilation, and staging |
| `platform/store/src/time.rs` | tenant temporal catalog, optimistic release activation, exact acquisition-to-release provenance lookup, owner events, and clock policy |

[`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) owns the complete
protocol, authority, administration, deployment, and synchronization-observation
contract.

Media-specific ownership:

| Path | Responsibility |
|---|---|
| `servers/media-mcp/src/provider.rs` | provider-neutral registry/submission adapter |
| `servers/media-mcp/src/webhook.rs` | signature parsing and constant-time verification |
| `servers/media-mcp/src/bin/server/generation_task.rs` | durable submission/WebhookWait/terminal flow |
| `servers/media-mcp/src/bin/server/artifact_tools.rs` | explicit small-content compatibility helper |
| `servers/media-mcp/src/bin/server/retention.rs` | platform-owned retention reconciliation |

DuckDB-specific ownership:

| Path | Responsibility |
|---|---|
| `servers/duckdb-mcp/DESIGN.md` | public contract, runtime boundary, tasks, persistence, deployment, and limits |
| `platform/runtimes/duckdb/` | bounded engine runtime, closed Spatial axis policy, effective-setting verification, and sandbox primitives |
| `mcp/contract/src/duckdb.rs` | cross-server governed source vocabulary |
| `mcp/contract/src/digest.rs` | canonical typed SHA-256 provenance digest shared by server contracts |
| `servers/duckdb-mcp/src/contract.rs` | server-local tool request and result types |
| `servers/duckdb-mcp/src/engine.rs` | adapter from server results to the shared runtime |
| `servers/duckdb-mcp/src/bin/server/ownership.rs` | derived owner workspaces and database resolution |
| `servers/duckdb-mcp/src/bin/server/sql_ops.rs` | direct and task SQL operation contracts and interruption behavior |

Simulation runtime ownership:

| Path | Responsibility |
|---|---|
| `platform/runtimes/simulation/Dockerfile` | canonical Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, RTX streaming, and non-root runtime image |
| `platform/runtimes/simulation/simulation-runtime.lock.json` | typed exact compatibility identity, source revisions, immutable components, GPU boundary, and driver floor |
| `platform/runtimes/simulation/requirements.lock` | hash-locked Python dependency closure for the selected Isaac Lab profile |
| `platform/runtimes/simulation/probes/` | import-identity and hardware-GPU conformance evidence |
| `tools/image-build/control/` | shared pinned Buildx, BuildKit registry configuration, and cross-worktree builder lease used by image release and certification |
| `testing/smoke/src/bin/smoke/scenarios/simulation.rs` | deployment-lock registry authorization, published environment invariants, local image materialization, GPU certification, and retained transcripts |

Authoritative simulation live-view ownership:

| Path | Responsibility |
|---|---|
| `mcp/contract/src/live_view.rs` | shared provider-neutral logical-camera, camera-product, viewer-authorization, GPU-capacity, health, and WebSocket H.264 contract |
| `servers/uav-sim-mcp/src/contract.rs` | UAV session, control-grant, Map handoff consumer, mission-plan, and authoritative live-view schemas built from the shared contract |
| `servers/uav-sim-mcp/src/server/state.rs` | composed simulator, durable control-authority, task, logical-camera, and product services |
| `servers/uav-sim-mcp/src/server/control_authority.rs` | Work Context-scoped principal-to-vehicle grants, strict Map handoff admission, mission plans, and exclusive command-lease lifecycle |
| `servers/uav-sim-mcp/src/server/live_view.rs` | actor/browser stream authorizations, stable shared-product selection, expiry, closure, and connection telemetry |
| `servers/uav-sim-mcp/src/server/live_stream.rs` | authenticated browser WebSocket sessions that forward camera-owned Annex B H.264 products |
| `servers/uav-sim-mcp/src/server/live_view_audit.rs` | append-only live-view audit writes without runtime coupling |
| `servers/uav-sim-mcp/src/server/runtime_events.rs` | strict authenticated adapter-ready and final-ready stream receiver, immutable binding reapplication trigger, and MCP subscription projection |
| `servers/uav-sim-mcp/src/server/service.rs` | MCP tools, resources, subscriptions, well-known surface, and authoritative live-view orchestration |
| `servers/uav-sim-mcp/assets/live-app.html` | self-contained all-camera WebCodecs MCP App with shared H.264 delivery |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera.py` | simulator-tick camera orchestration, frame transforms, and shared camera/target time |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_rigs.py` | desired-pose computation for follow, chase, orbit, look-at, stabilized-mounted, formation, and fixed rigs |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_smoothing.py` | half-life translation/quaternion filtering and reset rules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_products.py` | one continuous tiled RTX/NVENC product, RTSP receiver, and viewer-independent H.264 ring for the complete logical-camera set |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_health.py` | CUDA, RTX, NVENC, camera-product, frame, and latency evidence |
| `showcase/uav-sim/runtime/veoveo_uav_sim/runtime_events.py` | retained nonblocking adapter-ready edge before world admission and final-ready edge after authoritative visual admission |
| `showcase/uav-sim/runtime/veoveo_uav_sim/tile_lifecycle.py` | reactive, deduplicated provider generation state derived from native Cesium lifecycle events and render coverage observations, including expired provider-session reset |
| `showcase/uav-sim/runtime/veoveo_uav_sim/server.py` | simulator-local control boundary for camera and product realization |
| `platform/store/src/live_views.rs` | durable audit persistence for camera, product, authorization, denial, expiry, and revocation facts |
| `platform/store/migrations/0036_remove_simulation_view_mirror_state.surql` | forward-only removal of obsolete mirrored desired/runtime state |

## Recordings

### `platform/recordings/protocol`

`proto/veoveo/recording/ingest/v1/ingest.proto` is the canonical public wire schema.
`src/lib.rs` owns media types, route constants, limits, digest validation, and generated
types.

### `platform/recordings/hub`

| File | Responsibility |
|---|---|
| `ingest_http.rs` | cluster-internal authenticated protobuf routes and typed error projection |
| `ingest.rs` | producer authorization, atomic no-clobber journal and Blueprint publication, quota-bound append, ordered live parts, rollover merge, and restart reconciliation |
| `diagnostics.rs` | bounded authenticated-ingest acceptance, duplication, materialization-backlog, and last-success counters |
| `blueprint.rs` | complete Blueprint-store validation, application association, and confined immutable paths |
| `spool.rs` | segment encode/flush/fsync/freeze, idle completion, and recovery |
| `catalog.rs` | per-stream identity, capture timestamps, segment verification, and catalog publication |
| `query.rs` | governed RRD query/readback |
| `config.rs` | validated raw gRPC spool and segment limits |
| `archive.rs` | one-time object-store compaction, GoP rebatching, footer encoding, and atomic archive publication |
| `ingest.rs` | authenticated durable-part journal projection, compact static-context snapshots, and decoder-reentrant rollover |
| `spool.rs` | direct loopback writer, decoder-reentrant rollover, and archive freeze |
| `bin/spooler.rs` | thin composition of authenticated ingest, loopback Rerun receiver, catalog, and shutdown |
| `bin/hub_smoke.rs` | Rust crash/restart/rollover/catalog smoke scenarios |

### `platform/recordings/forwarder`

| File | Responsibility |
|---|---|
| `src/batch.rs` | per-recording accumulation, per-video-sample batches, decoder-reentrant boundary detection, complete RRD encoding, and byte-bounded splitting |
| `src/queue.rs` | bounded-memory fsynced producer queue, stream identity, checkpoint acknowledgement, and disk backpressure |
| `src/oauth.rs` | RFC 8414 discovery and `private_key_jwt` client-credentials tokens |
| `src/client.rs` | typed protobuf discovery, open, append, Blueprint publication, and finish operations |
| `src/runner.rs`, `src/blueprint.rs` | canonical-host transport routing, reactive loopback Rerun burst draining, Blueprint demux, failure backoff, restart resume, and graceful drain |
| `src/config.rs` | validated identity origin, installation transport, key, queue, batching, and shutdown configuration |
| `Dockerfile` | production sidecar image with the forwarder and loopback readiness utility |

### `platform/recordings/video`

The shared governed recorded-video access crate. `src/lib.rs` owns the
`RecordingVideoSelection`/`IndexRange`/`VideoTimelineKind` selection contract,
bounded `VideoSourceLimits`, read-plan-authorized clip materialization with
no-transcode MP4 remux, and canonical recording-URI validation. Every server
that consumes `VideoStream` recordings (Stream replay and Reason) uses this crate
instead of a private video path.

### `servers/recording-mcp`

`contract.rs` owns query, publication, playback-manifest v8, archive-catalog, Blueprint, and live
descriptor types. `service.rs` resolves authorized MCP and playback plans.
`playback.rs` owns stable dataset identity, bounded playback sessions, the derived
append-only Rerun catalog, finite governed Blueprint source, and the recording-scoped read-only Redap service.
`live_playback.rs` retains recording-scoped static context across ingest generations, filters
bounded temporal history, and rewrites messages to the stable playback identity.
`live_stream.rs` frames complete RRD batches for the authorized WebViewer `LogChannel` and
distinguishes an empty-channel bootstrap from a current-head transport resume.
`uris.rs` owns recording identities, and `bin/server.rs` composes
the authenticated manifest, framed live route, Redap, and MCP transports.
`bin/server/state.rs` composes platform store, spool access, playback, subscriptions, and
artifact publication.

### `servers/stream-mcp`

| Path | Responsibility |
|---|---|
| `src/contract.rs` | live-session, replay, video, result, sampling, detection, timeline, and output types |
| `src/catalog.rs` | validated admitted GStreamer graphs, typed profiles, live ingress, and immutable model catalog |
| `src/executor.rs` | bounded native replay-runner protocol and response validation |
| `src/annotation.rs` | derived Rerun bounding-box annotation layers |
| `src/artifacts.rs` | shared artifact-plane adapter |
| `src/uris.rs` | canonical `stream://` identities |
| `src/bin/server/live.rs` | owner-scoped live runner lifecycle plus bounded result and encoded-preview rings |
| `src/bin/server/recording_output.rs` | optional bounded non-blocking fan-out of existing H.264 units to the pod-local Recording forwarder |
| `src/bin/server/app.rs`, `assets/live.html` | self-contained Stream MCP App resource for actual encoded video, typed overlays, and exact decode-path reporting |
| `src/bin/server/` | auth, replay tasks, live sessions, prompts, resources, notifications, and composition |
| `gst-runner/` | native operator-admitted GStreamer graph execution with NVIDIA decode/inference and typed event output |
| `Dockerfile` | DeepStream 9 development/runtime multi-stage image |

`recording-mcp::service::read` owns the reusable governed local read plan, and
`platform/recordings/video` owns selection and materialization over it;
Stream replay persists recording identities rather than segment paths. Live Stream
sessions consume their admitted ingress directly and do not depend on Recording Hub.

### `servers/reason-mcp`

| Path | Responsibility |
|---|---|
| `src/contract.rs` | reasoning tasks, decode policy, grounding, results, and output types |
| `src/catalog.rs` | validated world-model checkpoint and reasoning pipeline catalog |
| `src/executor.rs` | bounded world-model runner protocol and response validation |
| `src/grounding.rs` | typed Stream-results grounding subset extraction |
| `src/annotation.rs` | derived Rerun provenance and event annotation layers |
| `src/artifacts.rs` | shared artifact-plane adapter |
| `src/uris.rs` | canonical `reason://` identities |
| `src/bin/server/` | auth, tasks, prompts, resources, notifications, and composition |
| `runner/` | Python world-model runner: typed protocol, frame sampling, vLLM inference |
| `Dockerfile` | vLLM runtime image with the server binary and installed runner |

Reason consumes governed video through `platform/recordings/video` exactly as
Stream replay does and embeds a bounded grounding subset in the durable request at
submission time; it persists neither segment paths, artifact URLs, nor caller
bearers. The runner binary belongs to
the deployable image and the engine is a site-compiled deployment input, so the
server fails readiness until both are present.

## Python Servers

### `sdk/python`

The shared platform package for hosted Python MCP servers. It is the Python
counterpart of the workspace crates a Rust server composes; the Rust side
stays the source of truth for every wire shape and schema.

| Module | Responsibility |
|---|---|
| `contract/` | identity, artifact-plane, and usage wire models |
| `internal_auth.py` | gateway Ed25519 assertion verification and ASGI middleware |
| `host.py` | host-authority validation and 421 rejection |
| `deployment.py`, `pagination.py` | mount identities and cursor pagination |
| `schema.py` | self-contained JSON Schema 2020-12 generation for MCP tool inputs |
| `task_extension/` | typed official Tasks SDK-hook adapter, models, and projection |
| `tasks/` | durable SurrealDB task runtime port: leases, CAS transitions, outbox, recovery, prune |
| `artifacts.py` | artifact-plane HTTP client and capability redemption |

### `templates/python-mcp`

The canonical template for new Python servers, shipped as the working
`datasheet` dataset-profiling server. `contract.py` and `engine.py` own the
domain; `server/` mirrors the Rust per-server module split (config, ownership,
official Tasks adapter, durable task, MCP surface, composition).

## Agents

### `agents/runtime`

SurrealDB-backed agent, episode, task watcher, wake, lease, and scheduling persistence.

| File | Responsibility |
|---|---|
| `control.rs` | database-authenticated, exact-context external operator messages and input-request decisions with UUIDv7 idempotency, durable wakes, actor attribution, and a domain-neutral conversation projection over wakes and episodes |
| `runtime.rs` | lease-fenced agent mutations, inactive-manifest reconciliation, race-safe durable input-request terminal waits, and atomic terminal-delivery consumption with first-party Task retention release |

### `agents/kernel`

| File | Responsibility |
|---|---|
| `manifest.rs` | agent, model, profile, tool, budget, and bounded MCP resource-subscription configuration models |
| `episode.rs` | bounded reasoning episode lifecycle |
| `background_tasks.rs` | immediate model-visible handoff from accepted durable tool calls to credential-rotating kernel watchers |
| `tools.rs` | MCP tool dispatch and durable task descriptor capture |
| `tasks.rs` | detached watcher lease/resume/result-to-wake flow |
| `wake.rs` | outbox/changefeed wake delivery, including heartbeat-only batches acknowledged without an episode |
| `memory.rs` | durable memory API over analytical stores |
| `timeline.rs` | snapshot dataframe read-back over the agent's RRD segments with bounded most-recent-rows output because rows become model input |
| `context.rs` | per-episode context assembly as a view over the memory planes |
| `llm.rs` | episode LLM construction from the manifest's provider-neutral model config |
| `input.rs` | durable application input for protocol-neutral deferred tools |
| `replay.rs`, `summary.rs` | domain-truth rebuild from the decision log and deterministic episode summaries |
| `rrd.rs`, `recorder.rs` | episode/world Rerun recording |
| `budget.rs` | enforced episode/tool/cost budgets |
| `connection.rs` | final-profile gateway client epoch, serialized request-boundary credential freshness, acknowledged request-scoped listener restoration, and deferred-task resolver |
| `resource.rs` | governed current-profile resource reads, episode-local accounting, admitted text validation, and bounded correction diagnostics |

### `platform/gateway/src/bin/gateway/admin`

| File | Responsibility |
|---|---|
| `agents.rs` | policy and audit boundary for user or service agent messages, actor-attributed conversation reads, pending input-request reads, and decisions; resolves the caller's tenant and Work Context before using the runtime control plane |

## Console

### `apps/console/bff`

| File | Responsibility |
|---|---|
| `oauth.rs` | PKCE login, token exchange, refresh rotation, and shared Console/standalone-App return settlement |
| `session.rs` | XChaCha20-Poly1305 cookies, CSRF material, and bounded same-origin `BrowserReturnPath` authority |
| `app_host.rs` | typed `/apps/{server}/{page...}` route authority, public no-store entry document, and caller-authorized App bootstrap |
| `api.rs` | snapshot, SSE, mutation, artifact preview/download, and same-origin CSRF-protected agent-message/input-request BFF projections; browser credentials and database authority never enter an MCP App |
| `recording_playback.rs` | authenticated playback-manifest and framed live-stream pass-through; no archive bytes or BFF session store |
| `apps.rs`, `mcp_client.rs` | MCP Apps host backend: auth-scoped final-profile client pool, public gateway authority preservation, reactive failure-isolated app catalog, standalone descriptors, sandboxed frame serving, declared agent-message targets, allowlisted tool calls, explicit resource-read settlement, configured listener/subscription admission, bounded token-replacement cancellation, and one multiplexed resource-wake stream per App |
| `config.rs`, `viewer_config.rs` | validated public/gateway/OAuth-resource/MCP-transport and embedded-map configuration, exact profile binding, redacted provider credentials, and the authenticated no-store Rerun map projection |
| `outbound_http.rs` | additive installation CA trust shared by Console HTTP, streaming, live, MCP, and Kubernetes clients |

### `apps/console/web/src`

| File | Responsibility |
|---|---|
| `App.tsx` | application shell: platform navigation plus catalog-driven MCP App entries, topbar, view routing, drawer mounting |
| `appHost.tsx`, `StandaloneAppHost.tsx`, `standaloneBootstrap.ts` | minimal standalone App entry, authorized same-path bootstrap, shared OAuth/CSRF settlement, authorized title, and Console return link |
| `views/Recordings.tsx` | searchable lifecycle browser and lazy Rerun playback workspace |
| `components/GovernedRerunViewer.tsx`, `rerunSources.ts`, `rerunLiveChannel.ts`, `recordingRrdFetch.ts`, `rerunMap.ts` | persistent WebViewer lifecycle, producer Blueprint-first opening, one native incremental-RRD or lazy-archive receiver, exact same-origin RRD authorization, duplicate-free current-head reconnect, event-driven rollover without cursor forcing, archive-only credential renewal, and installation-owned browser map-provider activation |
| `views/Agents.tsx`, `agentControl.ts` | reactive agent state, actor-attributed conversation, durable message submission, pending input-request decisions, and client-owned UUIDv7 retry identity |
| `views/` | remaining platform-plane views (overview, work, artifacts, MCP, apps, access, audit, cluster); domain views ship as MCP Apps, never here |
| `drawers/ArtifactDrawer.tsx` | artifact preview, recording provenance, download, release, grant, and share-link workflows |
| `drawers/` | remaining detail drawers with mutation workflows |
| `components/ArtifactPreview.tsx` | bounded text and inline image/audio/video/PDF previews with explicit governed-access failures |
| `components/GovernedRerunViewer.tsx` | recording-scoped exclusive Redap archive or bounded-live delivery with a separate producer Blueprint presentation store |
| `identity.ts`, `components/IdentityText.tsx` | trusted display-name resolution for arbitrary principal ids with UUID-compacting fallback, rendered across access, agents, and artifact views |
| `components/` | reusable primitives, tables, toolbar, and the promise-based confirm dialog |
| `queries.ts`, `queryClient.ts` | TanStack Query keys, snapshot/apps/cluster queries, mutation hooks with targeted cache patches |
| `live.ts` | EventSource console stream feeding row upserts into the snapshot cache |
| `theme.ts`, `ThemeProvider.tsx` | persisted Console theme registry, semantic palette selection, and MCP App light/dark host context |
| `apps/` | MCP Apps host: one exported opaque-origin sandbox policy, shared iframe component, stable postMessage bridge, closed internal navigation, declared agent messages, explicit resource-read adapter, and fetch-backed multiplexed SSE wake decoder |
| `auth.ts` | one-way authentication transition shared by every 401 handler |
| `api.ts` | ordered same-origin BFF calls and CSRF rotation |
| `types.ts` | TypeScript snapshot and mutation response shapes |
| `styles.css` | responsive work-focused visual system with accessible type-scale tokens |

## Testing And Conformance

| Path | Responsibility |
|---|---|
| `mcp/conformance` | reusable domain-neutral MCP certification library, thin CLI, schemas, profiles, authenticated same-origin well-known-surface checks, live declaration binding, and standalone image |
| `testing/smoke/src/bin/smoke.rs` | smoke command dispatcher and digest-addressed simulation certification entrypoint |
| `testing/smoke/src/bin/smoke/scenarios/` | Rust process/deployment scenarios |
| `testing/smoke/src/bin/smoke/support/` | process, HTTP, auth, fixture, usage helpers |
| `testing/smoke/tests/` | static deployment/offline contract tests |
| component-local `tests/` | focused live SurrealDB and service integration tests |
| `testing/local-test-report.json` | committed informational result of checks executed on the qualified development host, bound to product build inputs rather than documentation-only content |
| `.github/workflows/local-test-report.yml` | lightweight presentation of the committed local test report; it performs no substantive build, deployment, GPU, or browser acceptance |

There should be no smoke lifecycle, retry, assertion, or cleanup logic in shell recipes.

## Change Routing

- Change shared identity/policy/artifact semantics in `mcp/contract`, then update the
  platform store and every affected boundary.
- Change persistence shape in `platform/store` with an ordered migration and matching Rust API.
- Change durable task lifecycle and its official MCP projection in
  `platform/task-runtime`; hosted handlers use `rmcp` Task types directly.
- Change a domain tool schema in its owning `servers/*-mcp` server, not the gateway.
- Change MCP Apps protocol constants or helpers in `mcp/apps-extension`; app views live beside
  their server (`servers/{server}-mcp/assets/`), and the console host surface in
  `apps/console/bff` (`apps.rs`) plus `apps/console/web/src/apps/`.
- Change a domain administrative surface in its owning server as scope-gated MCP tools and
  resources, and represent it in the browser through the server's MCP App view — never a
  bespoke admin REST router, BFF proxy route, or hardcoded console page.
- Change browser behavior through `apps/console/bff` plus `apps/console/web`; do not expose gateway
  tokens to JavaScript.
- Change public routes in Helm ingress, then extend the Rust deployment smoke.
- Change installation image/config content in Helm, the offline lock/builder, and
  deployment contract together.
- Deliver install-time domain configuration through the generic `serverBootstrap` values only
  when the owning server defines an installation-time contract. Frames worlds are runtime MCP
  state and must never be placed in Helm bootstrap.
