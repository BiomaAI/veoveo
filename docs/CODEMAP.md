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
| [`CODEMAP.md`](CODEMAP.md) | documentation index, code ownership, and change routing |
| [`RECORDINGS.md`](RECORDINGS.md) | recording ingest, catalog, sealing, and governed read path |
| [`RECORDING_INGEST.md`](RECORDING_INGEST.md) | external/LAN producer protocol, auth, durability, and routing |

MCP designs live with the crate whose public contract they specify:

| Document | Domain |
|---|---|
| [`servers/duckdb-mcp/DESIGN.md`](../servers/duckdb-mcp/DESIGN.md) | analytical SQL, Spatial, sandboxing, tasks, and governed data movement |
| [`servers/frames-mcp/DESIGN.md`](../servers/frames-mcp/DESIGN.md) | local coordinate frames and bounded transformations |
| [`servers/map-mcp/DESIGN.md`](../servers/map-mcp/DESIGN.md) | Earth geography, map data administration, and logistics routing |
| [`servers/optimization-mcp/DESIGN.md`](../servers/optimization-mcp/DESIGN.md) | planning problem models and optimization |
| [`servers/perception-mcp/DESIGN.md`](../servers/perception-mcp/DESIGN.md) | governed local sensor inference and derived annotations |
| [`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) | temporal authority, operational calendars, clock quality, and events |
| [`servers/timeseries-mcp/DESIGN.md`](../servers/timeseries-mcp/DESIGN.md) | timeseries forecasting, preview contract, and the forecast MCP App view |
| [`servers/view-mcp/DESIGN.md`](../servers/view-mcp/DESIGN.md) | headless geospatial points of view, 3D Tiles residency, and GPU frame capture |
| [`servers/uav-sim-mcp/DESIGN.md`](../servers/uav-sim-mcp/DESIGN.md) | governed UAV simulation sessions, missions, vehicles, tiles, and recordings |

Deployment, examples, templates, and fixtures keep their instructions beside the
material they operate:

| Document | Purpose |
|---|---|
| [`configs/perception/README.md`](../configs/perception/README.md) | perception catalog and runtime configuration |
| [`deploy/helm/veoveo/README.md`](../deploy/helm/veoveo/README.md) | Kubernetes installation contract |
| [`deploy/offline/README.md`](../deploy/offline/README.md) | offline bundle construction and loading |
| [`examples/bioma/README.md`](../examples/bioma/README.md) | isolated Bioma k3d cluster, Entra profile, and Cloudflare Tunnel operations |
| [`showcase/README.md`](../showcase/README.md) | showcase entrypoint |
| [`showcase/sumo/README.md`](../showcase/sumo/README.md) | SUMO/TraCI integration and operations |
| [`showcase/uav-sim/README.md`](../showcase/uav-sim/README.md) | Isaac/Cesium/Pegasus/PX4 UAV simulation integration and operations |
| [`templates/python-mcp/README.md`](../templates/python-mcp/README.md) | canonical Python MCP server template |
| [`timesfm-showcase/README.md`](../servers/timeseries-mcp/testdata/timesfm-showcase/README.md) | TimesFM test fixture provenance and use |

The canonical long-form sources are
[`veoveo-whitepaper-print.html`](veoveo-whitepaper-print.html) and
[`autonomy-harness-print.html`](autonomy-harness-print.html). `just docs-pdf` produces
the [`whitepaper PDF`](veoveo-whitepaper.pdf) and
[`harness PDF`](autonomy-harness.pdf). [`autonomy-harness.html`](autonomy-harness.html)
is the browser edition of the harness document.

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
| `configs/perception/` | TensorRT/DeepStream perception catalog example and deployment contract |
| `configs/view/` | server-side 3D scene-layer catalog without provider secret values |
| `deploy/local/k3d/` | GPU-capable local Kubernetes cluster and values |
| `Justfile` | short human dispatch commands only |
| `AGENTS.md` | hard-cut, task, type, module, and smoke-test rules |
| `docs/` | general architecture, code index, recording design, and rendered publications |
| `agents/` | agent kernel and durable agent runtime |
| `apps/` | user-facing applications and their service boundaries |
| `mcp/` | shared MCP protocol contracts, extensions, and bridges |
| `platform/` | internal platform services, persistence, and reusable runtimes |
| `servers/` | independently deployed MCP servers and protocol projections |
| `testing/` | conformance tooling and multi-process smoke harnesses |
| `sdk/` | language SDK workspaces |
| `deploy/helm/veoveo/` | Kubernetes installation chart |
| `deploy/offline/` | pinned image manifest, bundle builder/loader, offline values |
| `showcase/sumo/` | real SUMO/TraCI domain showcase |
| `showcase/uav-sim/` | Google 3D Tiles UAV simulation showcase over Isaac, Cesium, Pegasus, and PX4 |
| `examples/bioma/` | optional Bioma Entra/Cloudflare deployment overlay |
| `sdk/python/` | Python platform package for hosted MCP servers |
| `templates/python-mcp/` | canonical Python server template (`datasheet`) |

## Placement Rules

The top-level directories express ownership rather than implementation language. A Rust
crate belongs beside the system it implements; Rust is not an architectural boundary.

| Root | Put code here when it owns |
|---|---|
| `servers/` | a hosted MCP server with its own protocol surface, deployment image, and domain behavior |
| `mcp/` | protocol contracts, transport extensions, or bridges shared by more than one server |
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
detection, visibility checks).


### `mcp/contract`

This crate owns vocabulary shared across services. It must not absorb a domain tool
schema merely because the server is first-party.

| File | Responsibility |
|---|---|
| `access.rs` | artifact access levels, user/group subjects, grant composition |
| `artifact_service.rs` | artifact-plane requests, capabilities, share links, native async port |
| `duckdb.rs` | shared DuckDB source types and safe read-function SQL fragments |
| `coordinates.rs` | current shared coordinate ids, frame kinds, geofence rules, and operation provenance |
| `storage.rs` | artifact metadata, release state, compliance labels |
| `gateway.rs` | gateway control-plane aggregate and public re-exports |
| `gateway/ids.rs` | validated identity and configuration newtypes |
| `gateway/auth_config.rs` | IdP, authorization server, OAuth client surfaces |
| `gateway/server_config.rs` | hosted server and profile exposure contracts |
| `gateway/policy.rs` | actions, targets, rules, effects, audit reason model |
| `gateway/runtime_state.rs` | durable auth/runtime record contracts |
| `gateway/validation.rs` | fail-closed cross-reference and invariant validation |
| `internal_auth.rs` | Ed25519 signing keys, JWKS trust, internal issuer/verifier |
| `deployment.rs` | Connected/offline Kubernetes topology contract |
| `bootstrap.rs` | generic installation-time server bootstrap envelope, constants, and semantics |
| `tasks.rs` | shared task ownership and platform task vocabulary |
| `provider.rs` | provider job/event contracts; no status polling API |
| `subscriptions.rs` | resource subscription hub |
| `telemetry.rs` | tracing/log initialization and guards |

### `platform/recordings/rrd`

Owns cross-domain Rerun/RRD spacetime types and adapters. Domain results that do not
overlap Rerun concepts stay local to their MCP crate.

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
| `coordinates.rs` | frames and coordinate-operation persistence |
| `map.rs` | source, release, active-pointer, mobility, restriction, snapshot, route, matrix, and acquisition persistence |
| `time.rs` | authority sources and releases, active pointers, acquisitions, calendars, epochs, clock policy, and events |
| `recordings.rs` | recording and segment catalog |
| `recording_ingest.rs` | producer streams, idempotent batch checkpoints, and journal state |
| `usage.rs` | shared domain/media usage records |
| `outbox.rs`, `changefeed.rs` | transactional events, checkpoints, LIVE acceleration |
| `store.rs` | connection and transaction helpers over domain records |

Migrations `0001` through the current version live under `migrations/`. Runtime services
never apply them; installation bootstrap does.

## Durable Tasks

### `platform/task-runtime`

| File | Responsibility |
|---|---|
| `types.rs` | runtime configuration, recovery classes, pins, claims, outcomes |
| `runtime.rs` | create/idempotency, lease, update, cancel, finish, recover, prune |
| `lib.rs` | focused public API |

### `mcp/task-extension`

| File | Responsibility |
|---|---|
| `models.rs` | final extension request/response/discovery wire types |
| `projection.rs` | platform task snapshot to protocol task projection |
| `adapter.rs` | native-async handler contract, JSON-RPC middleware, SSE listen |

The runtime is the source of truth. The extension is transport only.

## Gateway

### Library surface: `platform/gateway/src`

| Path | Responsibility |
|---|---|
| `catalog.rs` | validated active catalog and profile/server lookup |
| `control_store.rs` | immutable SurrealDB control revisions and activation |
| `auth/` | access tokens, OIDC, ID-JAG, client assertions, principals |
| `policy.rs` | policy evaluation entrypoint |
| `mcp/authorization.rs` | per-method/profile/server target authorization |
| `mcp/tools.rs` | aggregated tool projection and explicit helper gating |
| `mcp/resources.rs` | resource/list/read/subscribe projection |
| `mcp/prompts.rs`, `completion.rs` | prompt and completion projection |
| `mcp/final_tasks.rs` | canonical upstream final task client/projection |
| `mcp/tasks.rs` | explicit weak-client task projection |
| `mcp/upstream*.rs` | authenticated streamable HTTP and cache behavior |
| `state/audit.rs` | durable policy/audit evidence |
| `state/auth_state.rs` | durable OAuth authorization and replay state |
| `state/refresh_tokens.rs` | refresh family issue/rotate/replay/revoke/GC |
| `state/subscriptions.rs` | durable subscription ownership and forwarding |
| `secrets.rs` | secret-source models and environment/file/Vault resolution |

### Binary surface: `platform/gateway/src/bin/gateway`

| Path | Responsibility |
|---|---|
| `server.rs` | router assembly only |
| `runtime.rs` | shared application state and HTTP clients |
| `oauth/`, `oauth_grants/` | authorize/callback/token and grant handlers |
| `admin/control_plane.rs` | control revision read/update |
| `admin/tasks.rs` | policy-checked cancellation through owning task extension |
| `admin/artifacts.rs` | release/grant/link mutations through artifact service |
| `admin/console/mod.rs` | console snapshot handler, branding, stream cursor bootstrap |
| `admin/console/projection.rs` | tenant projection load and per-entity summary builders |
| `admin/console/stream.rs` | live console SSE: LIVE wake hub, changefeed replay, tenant filtering, limits |
| `admin/console/health.rs` | background MCP server health prober and cache |
| `admin/server_proxy.rs` | generic policy-checked proxy to a hosted server's contract-defined admin API |
| `artifact_download.rs` | authorized/audited large download proxy |
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
| `contract.rs` | tool/resource request and result types owned by the domain |
| `engine.rs`, `forecast.rs`, or `planning.rs` | pure domain computation |
| `state.rs` | server-local provider and domain models, not task persistence |
| `uris.rs` | canonical server resource identities |
| `artifacts.rs` | task-bound capability preparation/redemption |
| `admin/` | optional contract-defined domain administration under the server mount |
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
| `servers/frames-mcp` | local frame derivation, coordinate conversion, and operation provenance |
| `servers/map-mcp` | Earth geography, source administration, releases, and logistics routing |
| `servers/media-mcp` | webhook-completed provider media work and governed outputs |
| `servers/optimization-mcp` | planning problem models, solver execution, validation, and mission outputs |
| `servers/perception-mcp` | local recorded-sensor inference and Rerun annotations |
| `servers/recording-mcp` | governed recording catalog, queries, subscriptions, and sealing |
| `servers/timeseries-mcp` | time-series analysis, forecasting, evaluation, and artifacts |
| `servers/time-mcp` | temporal authority, clock assessment, operational calendars, mission timelines, and events |
| `servers/view-mcp` | owner-scoped geospatial views, shared 3D Tiles streaming, offscreen Bevy rendering, and captured frames |
| `servers/uav-sim-mcp` | provider-neutral UAV simulation sessions, missions, vehicles, tasks, subscriptions, and recording references |

### UAV Simulation Integration

| Path | Responsibility |
|---|---|
| `showcase/uav-sim/runtime/` | pinned Isaac Sim 6.0.1 image, Cesium/Pegasus compatibility, PX4 process lifecycle, pod-private adapter, and Rerun publication |
| `showcase/uav-sim/deploy/helm/` | interactive Isaac-plus-MCP Deployment, Isaac batch Job, Secret references, GPU requests, ephemeral tile storage, and network policy |
| `examples/bioma/uav-sim-values.yaml` | Bioma session, Frames origin, public gateway origin, and recording tenant binding |
| `testing/smoke/src/bin/smoke/scenarios/uav_sim.rs` | credentialed Google tiles, PX4, Recording Hub, Perception, and concurrent GPU workload acceptance |

### Geospatial Domains

The geospatial hard cut has three canonical servers:

| Path | Responsibility |
|---|---|
| `servers/map-mcp` | Earth geography, governed source acquisition, release activation, DuckDB Spatial analytics, CRS and geodesic work, geofences, restrictions, Valhalla land routing, governed network routing, matrices, and reachable areas |
| `servers/frames-mcp` | WGS84, ECEF, ENU, and NED local-frame derivation and conversion, durable batch work, operation provenance, artifacts, and usage |
| `servers/view-mcp` | configured 3D scene layers, camera poses and target rigs, shared tile caching, NVIDIA-accelerated offscreen rendering, and frame resources |

The crate-local design documents own their protocol, administration, persistence, and
deployment details.

### Temporal Domain

| Path | Responsibility |
|---|---|
| `servers/time-mcp` | authority-bound time resolution and conversion, calendar expansion, timeline validation, interval algebra, clock assessment, mission epochs, and temporal events |
| `servers/time-mcp/src/acquisition/` | bounded IANA TZDB and leap-second acquisition, validation, compilation, and staging |
| `platform/store/src/time.rs` | tenant temporal catalog, optimistic release activation, owner events, and clock policy |

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
| `platform/runtimes/duckdb/` | bounded engine runtime and sandbox primitives |
| `mcp/contract/src/duckdb.rs` | cross-server governed source vocabulary |
| `servers/duckdb-mcp/src/contract.rs` | server-local tool request and result types |
| `servers/duckdb-mcp/src/engine.rs` | adapter from server results to the shared runtime |
| `servers/duckdb-mcp/src/bin/server/ownership.rs` | derived owner workspaces and database resolution |
| `servers/duckdb-mcp/src/bin/server/sql_ops.rs` | direct and task SQL operation contracts and interruption behavior |

## Recordings

### `platform/recordings/protocol`

`proto/veoveo/recording/ingest/v1/ingest.proto` is the canonical public wire schema.
`src/lib.rs` owns media types, route constants, limits, digest validation, and generated
types.

### `platform/recordings/hub`

| File | Responsibility |
|---|---|
| `ingest_http.rs` | cluster-internal authenticated protobuf routes and typed error projection |
| `ingest.rs` | producer authorization, journal durability, quota-bound append, restart reconciliation, and materialization |
| `spool.rs` | segment encode/flush/fsync/freeze/recovery |
| `catalog.rs` | per-stream identity, segment verification, and catalog publication |
| `query.rs` | governed RRD query/readback |
| `config.rs` | validated raw gRPC spool and segment limits |
| `bin/spooler.rs` | thin composition of internal ingest, raw cluster spool, catalog, and shutdown |
| `bin/hub_smoke.rs` | Rust crash/restart/rollover/catalog smoke scenarios |

### `platform/recordings/forwarder`

| File | Responsibility |
|---|---|
| `src/batch.rs` | per-recording accumulation, complete RRD encoding, and byte-bounded splitting |
| `src/queue.rs` | fsynced producer queue, stream identity, checkpoint acknowledgement, and disk backpressure |
| `src/oauth.rs` | RFC 8414 discovery and `private_key_jwt` client-credentials tokens |
| `src/client.rs` | typed protobuf discovery, open, append, and finish operations |
| `src/runner.rs` | loopback Rerun receiver, retry loop, restart resume, and graceful drain |
| `src/config.rs` | validated network, key, queue, batching, and shutdown configuration |

### `servers/recording-mcp`

`contract.rs` owns query/publication types, `service.rs` owns authorized MCP behavior,
`uris.rs` owns recording identities, and `bin/server/state.rs` composes platform store,
spool access, subscriptions, and artifact publication.

### `servers/perception-mcp`

| Path | Responsibility |
|---|---|
| `src/contract.rs` | analysis, sampling, detection, timeline, and output types |
| `src/catalog.rs` | validated TensorRT model and DeepStream pipeline catalog |
| `src/source.rs` | authorized durable/recent Rerun video materialization |
| `src/executor.rs` | bounded C++ runner protocol and response validation |
| `src/annotation.rs` | derived Rerun bounding-box annotation layers |
| `src/artifacts.rs` | shared artifact-plane adapter |
| `src/uris.rs` | canonical `perception://` identities |
| `src/bin/server/` | auth, tasks, prompts, resources, notifications, and composition |
| `deepstream-runner/` | native DeepStream decode/infer/track metadata runner |
| `Dockerfile` | DeepStream 9 development/runtime multi-stage image |

`recording-mcp::service::read` owns the reusable governed local read plan. It
projects only frozen/sealed segment paths after tenant and label authorization;
perception persists recording identities rather than those paths.

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
| `task_extension/` | final task extension models, ASGI middleware, projection |
| `tasks/` | durable SurrealDB task runtime port: leases, CAS transitions, outbox, recovery, prune |
| `artifacts.py` | artifact-plane HTTP client and capability redemption |

### `templates/python-mcp`

The canonical template for new Python servers, shipped as the working
`datasheet` dataset-profiling server. `contract.py` and `engine.py` own the
domain; `server/` mirrors the Rust per-server module split (config, ownership,
task extension adapter, durable task, MCP surface, composition).

## Agents

### `agents/runtime`

SurrealDB-backed agent, episode, task watcher, wake, lease, and scheduling persistence.

### `agents/kernel`

| File | Responsibility |
|---|---|
| `manifest.rs` | agent, model, profile, tool, and budget configuration models |
| `episode.rs` | bounded reasoning episode lifecycle |
| `tools.rs` | MCP tool dispatch and durable task descriptor capture |
| `tasks.rs` | detached watcher lease/resume/result-to-wake flow |
| `wake.rs` | outbox/changefeed wake delivery |
| `memory.rs` | durable memory API over analytical stores |
| `rrd.rs`, `recorder.rs` | episode/world Rerun recording |
| `budget.rs` | enforced episode/tool/cost budgets |
| `connection.rs` | reconnectable gateway epoch and task resumer |

## Console

### `apps/console/bff`

| File | Responsibility |
|---|---|
| `oauth.rs` | PKCE login, token exchange, refresh rotation |
| `session.rs` | XChaCha20-Poly1305 cookies and CSRF material |
| `api.rs` | snapshot, SSE stream proxy, mutation, download, and explicit server-admin BFF projections |
| `apps.rs`, `mcp_client.rs` | MCP Apps host backend: gateway MCP session pool, app catalog, sandboxed frame serving, allowlisted tool-call proxy |
| `config.rs` | validated public/gateway/resource configuration |

### `apps/console/web/src`

| File | Responsibility |
|---|---|
| `App.tsx` | application shell: navigation, topbar, view routing, drawer mounting |
| `views/` | one module per operational view (overview, work, artifacts, agents, recordings, MCP, map, access, audit, cluster) |
| `drawers/` | artifact and task detail drawers with mutation workflows |
| `components/` | reusable primitives, tables, toolbar, and the promise-based confirm dialog |
| `queries.ts`, `queryClient.ts` | TanStack Query keys, snapshot/map/cluster queries, mutation hooks with targeted cache patches |
| `live.ts` | EventSource console stream feeding row upserts into the snapshot cache |
| `apps/` | MCP Apps host: sandboxed iframe component and the postMessage bridge |
| `api.ts` | same-origin BFF calls and CSRF rotation |
| `types.ts` | TypeScript snapshot and mutation response shapes |
| `styles.css` | responsive work-focused visual system with accessible type-scale tokens |

## Testing And Conformance

| Path | Responsibility |
|---|---|
| `testing/mcp-conformance` | external protocol/configuration CLI and fake services |
| `testing/smoke/src/bin/smoke.rs` | smoke command dispatcher |
| `testing/smoke/src/bin/smoke/scenarios/` | Rust process/deployment scenarios |
| `testing/smoke/src/bin/smoke/support/` | process, HTTP, auth, fixture, usage helpers |
| `testing/smoke/tests/` | static deployment/offline contract tests |
| component-local `tests/` | focused live SurrealDB and service integration tests |
| `.github/workflows/ci.yml` | formatting, clippy, tests, UI, Keycloak, deployment CI |

There should be no smoke lifecycle, retry, assertion, or cleanup logic in shell recipes.

## Change Routing

- Change shared identity/policy/artifact semantics in `mcp/contract`, then update the
  platform store and every affected boundary.
- Change persistence shape in `platform/store` with an ordered migration and matching Rust API.
- Change task lifecycle in `platform/task-runtime`; change wire behavior in
  `mcp/task-extension`.
- Change a domain tool schema in its owning `servers/*-mcp` server, not the gateway.
- Change MCP Apps protocol constants or helpers in `mcp/apps-extension`; app views live beside
  their server (`servers/{server}-mcp/assets/`), and the console host surface in
  `apps/console/bff` (`apps.rs`) plus `apps/console/web/src/apps/`.
- Change a domain admin API in its owning server, retain the generic gateway proxy, and
  add an explicit Console BFF projection when the browser represents that workflow.
- Change browser behavior through `apps/console/bff` plus `apps/console/web`; do not expose gateway
  tokens to JavaScript.
- Change public routes in Helm ingress, then extend the Rust deployment smoke.
- Change installation image/config content in Helm, the offline lock/builder, and
  deployment contract together.
- Deliver install-time domain configuration through the generic `serverBootstrap` values and
  the `mcp/contract` bootstrap envelope; the payload schema and `bootstrap-validate` verb live
  in the owning `servers/*-mcp` crate, never in core templates.
