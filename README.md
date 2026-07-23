<p align="center">
  <img src="docs/assets/brand/veoveo-logo.png" width="128" alt="Veoveo lens logo">
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/brand/veoveo.png">
    <img src="docs/assets/brand/veoveo-dark.png" width="340" alt="VEOVEO">
  </picture>
</p>

Veoveo is an operations platform for physical AI. Teams run agents that
observe the physical world, rehearse in simulated worlds, act on real
systems, and turn everything that happened into operational intelligence.
The organization deploying Veoveo owns the whole installation: cluster,
identity, storage, models, policies, domain name, and release process.

[Product tour](#product-tour) · [Agentic apps](#an-agentic-app-platform) ·
[Executable showcases](#executable-showcases) ·
[Deployment](#deploy-your-installation) · [Technical design](docs/TECH_DESIGN.md) ·
[Screenshot gallery](docs/screenshots/GALLERY.md)

[![Veoveo 3D View MCP App in the operations Console](docs/screenshots/gallery/console-app-view.png)](docs/screenshots/gallery/console-app-view.png)

*A reference installation running the View app over live Google
Photorealistic 3D Tiles, rendered on cluster GPUs.*

## What Teams Do With It

- **Fly the mission before flying it.** Launch multirotor missions over
  photorealistic terrain, with real flight dynamics and PX4 autopilot
  firmware, and keep the entire run as a dataset.
- **Operate a city's traffic.** Read live traffic state, retime signals,
  reroute vehicles, and replay outcomes against a full simulated city.
- **See through cameras.** Run detection and tracking over authorized video
  streams on your own GPUs.
- **Ask what happened.** Pose questions over synchronized recordings — world
  state, sensors, poses, annotations on one timeline — and get grounded,
  audited answers.
- **Forecast, plan, and analyze.** Timeseries forecasts with uncertainty,
  deterministic planning for one vehicle or a fleet, and SQL over
  operational data.
- **Hand evidence to anyone.** Every result becomes an artifact with
  ownership, provenance, and release state, shareable through expiring,
  revocable links.
- **Build agentic apps.** Ship interactive apps where agents do the work
  behind a live interface. Each app inherits the installation's identity,
  policy, access, and audit from its first request.

The same installation serves any team whose operations touch the physical
world, from logistics and defense to first responders:

<p align="center"><em>Response teams · Newsrooms · Search & rescue ·
Field & logistics · Humanitarian aid · OSINT desks · Security teams ·
Civic monitoring · Energy & utilities · Construction sites ·
Conservation patrols · Solo operators</em></p>

## Worlds You Can Trust

Physical AI is only as good as its model of the world. Veoveo treats world
models as governed infrastructure: authoritative geography and civil time,
photorealistic 3D scenes streamed from live tiles, simulated cities and
airspace with real vehicle dynamics, and continuous recordings of what
actually happened. Agents reach simulation and reality through the same
interfaces, so a mission rehearsed in a synthetic world carries over to
operations in the real one.

## From Instruction To Intelligence

The product of the platform is operational intelligence: answers, forecasts,
plans, and evidence that an enterprise can act on and defend. Every
instruction, whether typed by an operator or issued by an agent, passes one
identity and policy boundary, runs as durable work that survives
disconnects, and lands as recordings and artifacts with full provenance.
Operators steer and audit the same state agents act on, from the same
Console.

[![The operational loop: the world is recorded and perceived into a world model, an operator assigns a mission, an agent acts through the gateway's authentication, policy, and audit, and evidence feeds back](docs/images/harness-poster.png)](docs/images/harness-poster.png)

## An Agentic App Platform

Agentic apps are one way an installation delivers operational intelligence
to its users. An agentic app pairs an agent that plans and acts with a live
interface that people can see and steer: an operator types an instruction,
the agent drives simulation, perception, or vehicles, and the interface
shows progress, results, and evidence as they land. The capabilities behind
Veoveo's own charts, maps, forecasts, and 3D views are open to your teams.

Apps built on Veoveo are enterprise software from the first request. They
authenticate through the installation's identity provider, act within policy
scopes and Work Context access, run as durable work that survives
disconnects, and leave the same audit trail as every other actor. They
deploy with the installation, scale with it, and run in the Console or in a
compatible external host.

## Roadmap

Veoveo is working toward world models built from your operational reality:
digital twins assembled from the geography, recordings, and telemetry an
installation already governs, so simulation, rehearsal, and prediction start
from the world you actually operate.

## Product Tour

A deployment can begin with the standard server catalog, add its own
extensions, and retain the same identity and policy boundary throughout.

| Capability | What it provides |
|---|---|
| Real and simulated worlds | Governed recordings, spatial and time reference systems, traffic simulation, UAV simulation, camera streams, vehicle actuation, and 3D Tiles scenes. |
| Analysis and planning | Sandboxed DuckDB SQL, forecasting, optimization, media processing, perception, and temporal reasoning. |
| Durable automation | Recoverable task execution, cancellation, budgets, agent wakes, and retained results for work that outlives one request. |
| Interactive apps | Interfaces that ship with each server for charts, forecasts, maps, and 3D views rendered on cluster GPUs. The same app can run in the Console or a compatible external MCP host. |
| Governed evidence | Work Context ownership, invocation provenance, immutable artifact identities, policy decisions, grants, release state, and revocable sharing. |
| Open protocol surfaces | Profiles scoped by policy over tools, resources and templates, prompts, completions, durable tasks, subscriptions, notifications, structured content, and URI identities. |
| Enterprise operation | OIDC/OAuth identity, Kubernetes scheduling and scaling, Helm packages, OCI delivery, GitOps reconciliation, audit export, and an offline installation path. |

### Operations stay connected to the work

The Console is an authenticated operating surface. It reads the same task,
policy, artifact, recording, MCP, and Kubernetes state that agents use
through the gateway.

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

### UAV flight in Isaac Sim

| San Salvador | Midtown Manhattan |
|---|---|
| [![Isaac Sim UAV flight over San Salvador](docs/screenshots/gallery/isaac-uav-san-salvador.png)](docs/screenshots/gallery/isaac-uav-san-salvador.png) | [![Isaac Sim UAV flight over Midtown Manhattan](docs/screenshots/gallery/isaac-uav-new-york.png)](docs/screenshots/gallery/isaac-uav-new-york.png) |
| UAV and PX4 flight above the Jorge “Mágico” González stadium district | Dense New York photogrammetry around Times Square and Central Park |

Both frames come from the live headless Isaac Sim RTX viewport. The showcase
camera follows the Pegasus vehicle after PX4 reaches the configured flight
altitude. [Run the UAV showcase](showcase/uav-sim/README.md).

| Governed UAV recording | SUMO traffic world |
|---|---|
| [![UAV simulation in Rerun](docs/screenshots/gallery/rerun-uav.png)](docs/screenshots/gallery/rerun-uav.png) | [![SUMO traffic simulation in Rerun](docs/screenshots/gallery/rerun-sumo.png)](docs/screenshots/gallery/rerun-sumo.png) |
| Camera, pose, telemetry, perception, and reasoning share one governed recording path. | A pinned SUMO and LuST Luxembourg world exposes traffic reads, signal and vehicle control, network generation, durable batches, live subscriptions, and Rerun recording. [Run the SUMO showcase](showcase/sumo/README.md). |

## Built On The Model Context Protocol

Every capability above reaches agents and operators through the
[Model Context Protocol](https://modelcontextprotocol.io/specification/):
tools, resources, prompts, completions, durable tasks, subscriptions, and
notifications behind one identity and policy boundary. Any compatible MCP
host can drive the platform, and the Console speaks the same protocol that
agents use.

### Apps travel with the server

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
| `view` | 3D Tiles views rendered on cluster GPUs, camera control, and reproducible offscreen frame capture. |

The runtime for autonomous agents adds durable episodes, detach and resume,
wakes, budgets, analytical memory, tool use, and Rerun recording. Your own
agentic apps follow the same path as domain extensions and can join the
gateway without adopting Veoveo's source build: publish an image and Helm
chart, register the server in the typed control plane, and apply the
installation's trust and policy contract.

[![The agent runtime cycle: task results, timers, and messages wake the agent, which assembles context, runs an episode, persists, and sleeps, backed by state, memory, and log](docs/images/agent-loop.png)](docs/images/agent-loop.png)

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

SurrealDB is the required coordination store. It owns durable identity,
policy, task, artifact, recording, agent, audit, and outbox records.
S3-compatible storage holds governed bytes, while RRD segments retain history
across time and space. DuckDB remains an isolated analytical runtime.

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
expiring, revocable read-only link with an optional download limit. Hashes
serve integrity and deduplication within a tenant, while access always flows
through grants and release links.

Read the neutral enterprise contract in
[`docs/WORK_CONTEXT_GOVERNANCE.md`](docs/WORK_CONTEXT_GOVERNANCE.md).

## Deploy Your Installation

One Helm package contract covers every environment, from a laptop cluster to
datacenter GPUs to an edge site with no outbound network. Kubernetes
schedules GPU worlds onto hardware and scales the stateless servers with
demand. Operations span sites: field producers stream recordings through the
same authenticated gateway from Kubernetes, a local network, or the public
edge, while the offline bundle serves air-gapped installations.
Installation-owned values, gateway configuration, and Secret references
compose the platform without baking customer state into the product
repository.

[![Edge, cluster, air-gap, and hybrid installations, all one platform](docs/images/deployment-map.png)](docs/images/deployment-map.png)

| Path | Use it for | Guide |
|---|---|---|
| Local k3d | A real local Kubernetes cluster with registry-first image delivery and mandatory NVIDIA validation. | [`deploy/local/k3d`](deploy/local/k3d/README.md) |
| Direct Helm | A connected cluster managed by an existing platform team. | [`deploy/helm/veoveo`](deploy/helm/veoveo/README.md) |
| Enterprise GitOps | Immutable OCI charts and image digests reconciled by the installation owner's Argo CD, Flux, or equivalent controller. | [`docs/ENTERPRISE_DEPLOYMENT.md`](docs/ENTERPRISE_DEPLOYMENT.md) |
| Offline | A verified bundle containing runtime images, charts, schemas, checksums, image identities, and SPDX SBOMs. | [`deploy/offline`](deploy/offline/README.md) |

[`examples/bioma`](examples/bioma/README.md) is the executable reference for the
enterprise flow. Its hostname and infrastructure choices demonstrate one
installation; each deployment substitutes its own.

### GPU execution contract

Hardware GPU access is mandatory for simulation, perception, reasoning, 3D
rendering, Rerun, and visual acceptance workflows that declare it. Kubernetes
workloads request an NVIDIA device and fail closed when CUDA, Vulkan, WebGPU, or
WebGL cannot reach hardware. Software rendering is not a supported fallback.

The local cluster applies the same `nvidia.com/gpu` scheduling contract used by
fielded installations. Browser verification proves hardware WebGPU and WebGL
before interacting with a visual surface and stops if either context is lost.

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

## Tech Stack

Veoveo is built from technology many engineers already run in production:
Rust services and Python servers, a React and TypeScript Console, Kubernetes
and Helm underneath, SurrealDB for coordination, DuckDB for analysis, Rerun
for recordings, and NVIDIA runtimes for simulation and perception. If these
tools feel like home, so will this repository.

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="docs/assets/stack/rust.svg" height="40" alt="Rust" title="Rust"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://www.python.org/"><img src="docs/assets/stack/python.svg" height="40" alt="Python" title="Python"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://www.typescriptlang.org/"><img src="docs/assets/stack/typescript.svg" height="40" alt="TypeScript" title="TypeScript"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://react.dev/"><img src="docs/assets/stack/react.svg" height="40" alt="React" title="React"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://kubernetes.io/"><img src="docs/assets/stack/kubernetes.svg" height="40" alt="Kubernetes" title="Kubernetes"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://helm.sh/"><img src="docs/assets/stack/helm.svg" height="40" alt="Helm" title="Helm"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://www.docker.com/"><img src="docs/assets/stack/docker.svg" height="40" alt="Docker" title="Docker"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://opentelemetry.io/"><img src="docs/assets/stack/opentelemetry.svg" height="40" alt="OpenTelemetry" title="OpenTelemetry"></a>
</p>
<p align="center">
  <a href="https://surrealdb.com/"><img src="docs/assets/stack/surrealdb.svg" height="40" alt="SurrealDB" title="SurrealDB"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://duckdb.org/"><img src="docs/assets/stack/duckdb.png" height="40" alt="DuckDB" title="DuckDB"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://rerun.io/"><img src="docs/assets/stack/rerun.png" height="40" alt="Rerun" title="Rerun"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://developer.nvidia.com/isaac/sim"><img src="docs/assets/stack/nvidia.svg" height="40" alt="NVIDIA Isaac Sim" title="NVIDIA Isaac Sim"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://px4.io/"><img src="docs/assets/stack/px4.png" height="40" alt="PX4 Autopilot" title="PX4 Autopilot"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://eclipse.dev/sumo/"><img src="docs/assets/stack/sumo.png" height="40" alt="Eclipse SUMO" title="Eclipse SUMO"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://cesium.com/"><img src="docs/assets/stack/cesium.svg" height="40" alt="Cesium" title="Cesium"></a>&nbsp;&nbsp;&nbsp;
  <a href="https://maplibre.org/"><img src="docs/assets/stack/maplibre.svg" height="40" alt="MapLibre" title="MapLibre"></a>
</p>

*All logos belong to their respective projects.*

The platform is also designed to be extended, deployed, and operated with
coding agents. Toolchains are pinned, verification runs as executable
harnesses, and deployments prove themselves with smoke tests. The same
boundary that governs human operators governs agents: every action is
authenticated, scoped by policy, bounded by budgets, and audited, so an
installation can hand real work to agents and stay in control of what they
touch.

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
| [`agents/`](agents/) | Kernel and durable runtime for autonomous agents. |
| [`apps/console/`](apps/console/) | Rust Console BFF and React operations interface. |
| [`mcp/`](mcp/) | Shared MCP contracts, task and app extensions, and bridges. |
| [`platform/`](platform/) | Gateway, persistence, task, artifact, recording, and query runtimes. |
| [`servers/`](servers/) | Hosted MCP servers and their domain designs. |
| [`showcase/uav-sim/`](showcase/uav-sim/) | Isaac, Cesium, Pegasus, and PX4 UAV workload. |
| [`showcase/sumo/`](showcase/sumo/) | SUMO, LuST, TraCI, and the traffic world MCP server. |
| [`deploy/`](deploy/) | Helm, local k3d, and offline installation material. |
| [`examples/bioma/`](examples/bioma/) | Enterprise GitOps reference installation. |
| [`testing/`](testing/) | Protocol conformance and Rust multi-process smoke harnesses. |
| [`tools/screenshots/`](tools/screenshots/) | Repeatable authenticated Console, MCP App, and Rerun captures. |
| [`docs/`](docs/) | Architecture, governance, deployment, recording, and harness documentation. |

Start with the [`code map`](docs/CODEMAP.md) for ownership and call paths, the
[`reference architecture`](docs/architecture/README.md) for system views, or
the [`complete screenshot gallery`](docs/screenshots/GALLERY.md) for the visual
catalog and reproduction guide.
