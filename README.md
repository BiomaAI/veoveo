# Veoveo

Veoveo is a self-hosted operations platform for agents working with real and
simulated worlds. It brings rich MCP servers, interactive MCP Apps, durable
work, governed artifacts and recordings, and an operator console into one
installation.

The organization deploying Veoveo owns its cluster, identity provider, storage,
models, policies, domain name, and release process. There is no required vendor
control plane.

[Product tour](#product-tour) · [Executable showcases](#executable-showcases) ·
[Deployment](#deploy-your-installation) · [Technical design](docs/TECH_DESIGN.md) ·
[Screenshot gallery](docs/screenshots/GALLERY.md)

[![Veoveo 3D View MCP App in the operations Console](docs/screenshots/gallery/console-app-view.png)](docs/screenshots/gallery/console-app-view.png)

*A reference installation running the GPU-backed View MCP App over live Google
Photorealistic 3D Tiles.*

## Product Tour

Veoveo gives operators and agents one governed path from an instruction to live
world interaction, durable execution, and evidence. A deployment can begin with
the standard server catalog, add its own MCP extensions, and retain the same
identity and policy boundary throughout.

| Capability | What it provides |
|---|---|
| Rich MCP surfaces | Policy-scoped profiles over tools, resources and templates, prompts, completions, durable tasks, subscriptions, notifications, structured content, and URI identities. |
| Interactive MCP Apps | Server-shipped interfaces for charts, forecasts, maps, and GPU-backed 3D views. The same app can run in the Console or a compatible external MCP host. |
| Durable automation | Recoverable task execution, cancellation, budgets, agent wakes, and retained results for work that outlives one request. |
| Real and simulated worlds | Governed recordings, spatial and time reference systems, traffic simulation, UAV simulation, camera streams, vehicle actuation, and 3D Tiles scenes. |
| Analysis and planning | Sandboxed DuckDB SQL, forecasting, optimization, media processing, perception, and temporal reasoning. |
| Governed evidence | Work Context ownership, invocation provenance, immutable artifact identities, policy decisions, grants, release state, and revocable sharing. |
| Enterprise operation | OIDC/OAuth identity, Kubernetes scheduling, Helm packages, OCI delivery, GitOps reconciliation, audit export, and an offline installation path. |

### MCP Apps travel with the server

An MCP server can deliver a self-contained interface with its protocol result.
The host provides the sandbox and theme; Veoveo retains authorization, task,
artifact, and audit semantics behind each action. The View app below was invoked
from natural language and rendered by an external MCP host.

<p align="center">
  <a href="docs/screenshots/gallery/mcp-app-view-claude.png">
    <img src="docs/screenshots/gallery/mcp-app-view-claude.png" width="560" alt="View MCP App rendering a Golden Gate Bridge scene inside Claude">
  </a>
</p>

| | |
|---|---|
| [![Interactive chart MCP App](docs/screenshots/gallery/console-app-chart.png)](docs/screenshots/gallery/console-app-chart.png) | [![Timeseries forecast MCP App](docs/screenshots/gallery/console-app-timeseries.png)](docs/screenshots/gallery/console-app-timeseries.png) |
| Interactive charts from typed results | Forecast means and uncertainty bands |
| [![Map administration MCP App](docs/screenshots/gallery/console-app-map.png)](docs/screenshots/gallery/console-app-map.png) | [![Reason MCP protocol surface](docs/screenshots/gallery/console-mcp-reason.png)](docs/screenshots/gallery/console-mcp-reason.png) |
| Governed map sources and releases | Tools, prompts, resources, tasks, and scopes |
| [![Map MCP protocol surface](docs/screenshots/gallery/console-mcp-map.png)](docs/screenshots/gallery/console-mcp-map.png) | |
| The map server's complete MCP capability inventory | |

### Operations stay connected to the work

The Console is an authenticated operating surface, not a separate source of
truth. It reads the same task, policy, artifact, recording, MCP, and Kubernetes
state that agents use through the gateway.

| | |
|---|---|
| [![Operations overview](docs/screenshots/gallery/console-overview.png)](docs/screenshots/gallery/console-overview.png) | [![Durable work](docs/screenshots/gallery/console-work.png)](docs/screenshots/gallery/console-work.png) |
| Installation health and recent activity | Durable work across reasoning, perception, and simulation |
| [![Work Context access](docs/screenshots/gallery/console-access.png)](docs/screenshots/gallery/console-access.png) | [![Paged audit trail](docs/screenshots/gallery/console-audit.png)](docs/screenshots/gallery/console-audit.png) |
| Membership, authority, and access requests | Bounded policy decisions with trace context |
| [![Kubernetes cluster inventory](docs/screenshots/gallery/console-cluster.png)](docs/screenshots/gallery/console-cluster.png) | |
| Workloads, placement, storage, readiness, and image identity | |

### Recordings become governed evidence

Rerun recordings retain synchronized world, sensor, pose, and annotation data.
The Console presents each recording as one continuous timeline while bounded
segments remain an internal storage concern. Derived outputs enter the artifact
plane with ownership, provenance, release state, and effective access.

| | |
|---|---|
| [![Governed artifact catalog](docs/screenshots/gallery/console-artifacts.png)](docs/screenshots/gallery/console-artifacts.png) | [![Reasoning artifact detail](docs/screenshots/gallery/console-artifact-reason.png)](docs/screenshots/gallery/console-artifact-reason.png) |
| Immutable outputs and release state | Reasoning result with recording provenance |
| [![Perception video artifact](docs/screenshots/gallery/console-artifact-video.png)](docs/screenshots/gallery/console-artifact-video.png) | [![Continuous recording playback](docs/screenshots/gallery/console-recordings.png)](docs/screenshots/gallery/console-recordings.png) |
| Governed media preview and access | One authorized timeline in embedded Rerun |

## Executable Showcases

The showcases exercise the platform against real simulator runtimes. They are
maintained as deployable workloads with typed MCP contracts, recording paths,
and Rust acceptance tests.

| UAV simulation | SUMO traffic world |
|---|---|
| [![UAV simulation in Rerun](docs/screenshots/gallery/rerun-uav.png)](docs/screenshots/gallery/rerun-uav.png) | [![SUMO traffic simulation in Rerun](docs/screenshots/gallery/rerun-sumo.png)](docs/screenshots/gallery/rerun-sumo.png) |
| Isaac Sim renders Google Photorealistic 3D Tiles while Pegasus and PX4 supply multirotor dynamics and MAVLink control. Camera, pose, telemetry, perception, and reasoning share one governed recording path. [Run the UAV showcase](showcase/uav-sim/README.md). | A pinned SUMO and LuST Luxembourg world exposes traffic reads, signal and vehicle control, network generation, durable batches, live subscriptions, and Rerun recording. [Run the SUMO showcase](showcase/sumo/README.md). |

## Capability Catalog

The gateway assembles hosted servers into named profiles. An operator profile
can expose the complete catalog, while narrower profiles reduce tools and scopes
without changing the underlying server identities.

| Server | Capability |
|---|---|
| `artifact` | Artifact discovery, metadata, access grants, release state, and revocable sharing. |
| `charts` | Chart validation, compilation, static rendering, and an interactive MCP App. |
| `datasheet` | Dataset preview, column statistics, and durable profiling through the Python server template. |
| `duckdb` | Arbitrary SQL, governed ingestion, and immutable exports in bounded owner workspaces. |
| `frames` | WGS84, ECEF, ENU, and NED conversion with durable batch transforms. |
| `map` | Authoritative geography, dataset acquisition and releases, restrictions, routing, and map apps. |
| `media` | Provider-neutral model discovery, schemas, generation, artifact output, and webhook completion. |
| `optimization` | Deterministic single- and multi-agent planning with retained results. |
| `perception` | Local DeepStream and TensorRT detection and tracking over authorized Rerun video. |
| `reason` | Semantic and temporal reasoning over recordings with grounded, audited output. |
| `recording` | Recording discovery, bounded queries, subscriptions, publication, and viewer projection. |
| `rerun` | The bridged Rerun viewer surface. |
| `time` | Authority-bound civil time, calendars, clocks, timelines, and event operations. |
| `timeseries` | Forecasting, uncertainty output, governed artifacts, and an interactive forecast app. |
| `uav-sim` | Live sessions, multi-vehicle missions, bounded dataset capture, and provider-neutral vehicle control. |
| `view` | GPU-backed 3D Tiles views, camera control, and reproducible offscreen frame capture. |

The autonomous-agent runtime adds durable episodes, detach and resume, wakes,
budgets, analytical memory, tool use, and Rerun recording. Domain extensions can
join the same gateway without adopting Veoveo's source build: publish an image
and Helm chart, register the server in the typed control plane, and apply the
installation's trust and policy contract.

## How It Fits Together

```text
MCP hosts and Operations Console
               |
               v
     Gateway identity, profiles,
        policy, and audit boundary
               |
        +------+-------+
        |              |
  Hosted MCP       Autonomous
    servers          agents
        |              |
        +------+-------+
               |
     Durable tasks, artifacts,
       recordings, and events
               |
     +---------+----------+
     |                    |
Platform metadata   Object storage,
 and coordination   GPU worlds, media
```

SurrealDB is the required coordination store, not the platform's defining
surface. It owns durable identity, policy, task, artifact, recording, agent,
audit, and outbox records. S3-compatible storage holds governed bytes, while RRD
segments retain time-and-space history. DuckDB remains an isolated analytical
runtime rather than a platform database.

The normative boundaries and call paths are in
[`docs/ARCHITECTURE_DECISIONS.md`](docs/ARCHITECTURE_DECISIONS.md) and
[`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md).

## Governance Model

Every task, recording, agent, and artifact belongs to a Work Context. The
gateway resolves the actor, delegated authority, or automated invocation before
work begins. Services retain that provenance and apply the context's ownership,
initial grants, classification, labels, and output rules.

Human users authenticate through enterprise OIDC. MCP clients use OAuth grants
bound to the protected resource, and the gateway signs short-lived service
identity assertions for hosted servers. Browser code never receives the
Console's bearer token.

Artifacts use opaque `artifact://{uuidv7}` occurrence identities. Authorized
users can receive explicit grants. A releasable artifact may also receive an
expiring, revocable read-only link with an optional download limit. Hashes serve
integrity and tenant-local deduplication; they are not public addresses.

Read the neutral enterprise contract in
[`docs/WORK_CONTEXT_GOVERNANCE.md`](docs/WORK_CONTEXT_GOVERNANCE.md).

## Deploy Your Installation

Helm is the package contract for every environment. Installation-owned values,
gateway configuration, and Secret references compose the platform without
baking customer state into the product repository.

| Path | Use it for | Guide |
|---|---|---|
| Local k3d | A real local Kubernetes cluster with registry-first image delivery and mandatory NVIDIA validation. | [`deploy/local/k3d`](deploy/local/k3d/README.md) |
| Direct Helm | A connected cluster managed by an existing platform team. | [`deploy/helm/veoveo`](deploy/helm/veoveo/README.md) |
| Enterprise GitOps | Immutable OCI charts and image digests reconciled by the installation owner's Argo CD, Flux, or equivalent controller. | [`docs/ENTERPRISE_DEPLOYMENT.md`](docs/ENTERPRISE_DEPLOYMENT.md) |
| Offline | A verified bundle containing runtime images, charts, schemas, checksums, image identities, and SPDX SBOMs. | [`deploy/offline`](deploy/offline/README.md) |

[`examples/bioma`](examples/bioma/README.md) is the executable reference for the
enterprise flow. Its hostname and infrastructure choices demonstrate one
installation; they are not product dependencies or canonical customer names.

### GPU execution contract

Hardware GPU access is mandatory for simulation, perception, reasoning, 3D
rendering, Rerun, and visual acceptance workflows that declare it. Kubernetes
workloads request an NVIDIA device and fail closed when CUDA, Vulkan, WebGPU, or
WebGL cannot reach hardware. Software rendering is not a supported fallback.

The local cluster applies the same `nvidia.com/gpu` scheduling contract used by
fielded installations. Browser-driven verification proves hardware WebGPU and
WebGL before interacting with a visual surface and stops if either context is
lost.

## Standards And Protocols

Veoveo uses published standards at interoperability boundaries and names its
repository-owned extensions explicitly. The table states the implemented
profile rather than support for every optional feature of each standard.

| Area | Implemented standards and protocols |
|---|---|
| Agent and app interfaces | [Model Context Protocol](https://modelcontextprotocol.io/specification/) over JSON-RPC 2.0 and Streamable HTTP; JSON Schema Draft 2020-12; the [Veoveo final task extension](mcp/task-extension); and [MCP Apps SEP-1865](mcp/apps-extension/DESIGN.md). |
| Identity and authorization | OpenID Connect Core; OAuth 2.0 Authorization Code with S256 PKCE, Client Credentials, and JWT Bearer grants; RFC 8414 metadata; RFC 9728 protected-resource metadata; RFC 8707 resource indicators; JWT, JWS, and JWK; MCP enterprise-managed authorization and ID-JAG. |
| Recordings, data, and media | Rerun RRD and `VideoStream`; versioned protobuf recording ingest; S3-compatible object APIs; DuckDB SQL; Apache Parquet; and OTLP/HTTP telemetry. |
| Geography and time | WGS84/EPSG identities; GeoJSON RFC 7946; OGC JSON-FG and CQL2; GeoParquet 1.0; Mapbox Vector Tile 2.1; MapLibre Style 8; RFC 3339; RFC 9557; IANA TZDB/TZif and leap-second data; TAI and GPS time. |
| 3D and vehicles | OGC 3D Tiles 1.0/1.1; glTF/GLB 2.0; Draco geometry compression; MAVLink 2; and pod-private ROS 2 simulator paths. |
| Packaging and operations | Kubernetes resources, Helm charts, OCI images and charts, S3-compatible storage, and OpenTelemetry. |

The exact supported subsets are collected in
[`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md#standards-and-protocols). Domain
profiles live in their server designs, including
[`map-mcp`](servers/map-mcp/DESIGN.md#standards-and-protocols),
[`time-mcp`](servers/time-mcp/DESIGN.md#standards-and-protocols),
[`view-mcp`](servers/view-mcp/DESIGN.md#standards-and-protocols), and
[`uav-sim-mcp`](servers/uav-sim-mcp/DESIGN.md#standards-and-protocols).

## Develop And Verify

The Rust workspace, Python packages, container images, Helm charts, protocol
conformance clients, and smoke harnesses are all pinned in the repository.
Docker is required for SurrealDB-backed tests and deployment work. Native Map
builds also need a C/C++ toolchain, CMake, pkg-config, SQLite development files,
and PROJ's build dependencies.

```bash
just fmt
just check
just test
just test-python
just helm-check
just showcase-sumo-smoke
just showcase-uav-sim-test
```

Smoke orchestration is implemented in Rust. The `Justfile` keeps short dispatch
commands for humans. Local deployment profiles use the current tool versions
pinned in [`deploy/local/k3d/versions.env`](deploy/local/k3d/versions.env).

## Repository Guide

| Path | Responsibility |
|---|---|
| [`agents/`](agents/) | Autonomous-agent kernel and durable runtime. |
| [`apps/console/`](apps/console/) | Rust Console BFF and React operations interface. |
| [`mcp/`](mcp/) | Shared MCP contracts, task and app extensions, and bridges. |
| [`platform/`](platform/) | Gateway, persistence, task, artifact, recording, and query runtimes. |
| [`servers/`](servers/) | Hosted MCP servers and their domain designs. |
| [`showcase/uav-sim/`](showcase/uav-sim/) | Isaac, Cesium, Pegasus, and PX4 UAV workload. |
| [`showcase/sumo/`](showcase/sumo/) | SUMO, LuST, TraCI, and the traffic-world MCP server. |
| [`deploy/`](deploy/) | Helm, local k3d, and offline installation material. |
| [`examples/bioma/`](examples/bioma/) | Enterprise GitOps reference installation. |
| [`testing/`](testing/) | Protocol conformance and Rust multi-process smoke harnesses. |
| [`tools/screenshots/`](tools/screenshots/) | Repeatable authenticated Console, MCP App, and Rerun captures. |
| [`docs/`](docs/) | Architecture, governance, deployment, recording, and harness documentation. |

Start with the [`code map`](docs/CODEMAP.md) for ownership and call paths, the
[`reference architecture`](docs/architecture/README.md) for system views, or
the [`complete screenshot gallery`](docs/screenshots/GALLERY.md) for the visual
catalog and reproduction guide.
