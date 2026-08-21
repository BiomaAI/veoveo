<p align="center">
  <img src="docs/assets/brand/veoveo-logo.png" width="128" alt="Veoveo lens logo">
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/brand/veoveo.png">
    <img src="docs/assets/brand/veoveo-dark.png" width="340" alt="VEOVEO">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/BiomaAI/veoveo/actions/workflows/local-test-report.yml"><img src="https://github.com/BiomaAI/veoveo/actions/workflows/local-test-report.yml/badge.svg?branch=main" alt="Build"></a>
</p>

<h3 align="center">Autonomous agents that run operations in the physical world.<br>
On infrastructure you own.</h3>

Veoveo is an operations platform for physical AI. Teams run agents that
observe the physical world, rehearse in simulated worlds, act on real
systems, and turn everything that happened into operational intelligence.
The organization deploying Veoveo owns the whole installation: cluster,
identity, storage, models, policies, domain name, and release process.
What an engagement leaves behind is not a subscription. It is the factory.

[Product tour](#product-tour) · [Agentic apps](#an-agentic-app-platform) ·
[Compared to Palantir](#compared-to-palantir) ·
[Executable showcases](#executable-showcases) ·
[Connectors](#enterprise-connectors) ·
[Deployment](#deploy-your-installation) ·
[Software factory](#a-software-factory) ·
[Technical design](docs/TECH_DESIGN.md) ·
[Screenshot gallery](docs/screenshots/GALLERY.md)

[![Veoveo 3D View MCP App in the operations Console](docs/screenshots/gallery/console-app-view.png)](docs/screenshots/gallery/console-app-view.png)

*A reference installation running the View app over live Google
Photorealistic 3D Tiles, rendered on cluster GPUs.*

## What Teams Do With It

- **Fly the mission before flying it.** Launch
  [multirotor missions](#uav-flight-in-isaac-sim) over photorealistic
  terrain, with real flight dynamics and PX4 autopilot firmware, and keep
  the entire run as a dataset.
- **Operate a city's traffic.** Read live traffic state, retime signals,
  reroute vehicles, and replay outcomes against a full simulated city.
- **See through cameras.** Run detection and tracking over authorized video
  streams on your own GPUs.
- **See the whole operation.** Stream camera, telemetry, and vehicle state
  from field work into one governed timeline, from a single sensor to a
  working fleet.
- **Ask what happened.** Pose questions over synchronized recordings — world
  state, sensors, poses, annotations on one timeline — and get grounded,
  audited answers.
- **Forecast, plan, and analyze.** Timeseries forecasts with uncertainty,
  GPU vehicle routing and mathematical optimization with independently
  verified solutions, and SQL over operational data.
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
Civic monitoring · Energy & utilities · Industrial operations ·
Construction sites · Ports & terminals · Conservation patrols ·
Solo operators</em></p>

## Worlds You Can Trust

Physical AI is only as good as its model of the world. Veoveo treats world
models as governed infrastructure: authoritative geography and civil time,
photorealistic 3D scenes streamed from live tiles, simulated cities,
terrain, and airspace with real vehicle dynamics, and continuous recordings
of what actually happened. Agents reach simulation and reality through the same
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

The harness is deliberately unprivileged. NVIDIA's
[agent-stack security guidance](https://developer.nvidia.com/blog/where-security-fits-in-an-ai-agent-stack)
draws the boundary in the same place: "The harness guides what an agent
tries. The infrastructure controls what an agent can do." In Veoveo,
authority lives below every harness, so any compatible MCP host can drive
an installation without carrying authority of its own, and the boundary
extends past files and API calls to physical actuation behind command
leases.

<a href="docs/images/harness-poster.png">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/harness-poster-dark.png">
    <img src="docs/images/harness-poster.png" alt="The operational loop: live encoded media enters Stream directly, recording remains an independent governed evidence path, Reason grounds answers in Stream results and authorized recording snapshots, and agents act through the gateway's identity, policy, and audit boundary">
  </picture>
</a>

## Compared To Palantir

**Palantir rents you an operational intelligence product. Veoveo is how
you come to own one.** One installation covers the governed core of AIP,
the operational ground of Gotham, and the delivery job of Apollo, with a
working slice of Foundry beside them:

| Palantir product | What it does | Where Veoveo stands |
|---|---|---|
| AIP | Governed AI agents acting on enterprise systems through a controlled action layer | The closest analog. Veoveo's gateway does the same job of identity, policy, durable tasks, and audit over the open Model Context Protocol, so any compatible host and any model can drive it. |
| Gotham / Maven | Defense operational intelligence: sensor fusion, mission command, decision support | Shared domain, opposite approach. Veoveo comes at operations from the runtime side: rehearse the mission in simulation, act on real vehicles, and record everything that happened. |
| Foundry | Enterprise data integration, ontology, and operational applications | A deliberate slice. Work Contexts, artifacts, and analytical stores form a governed data plane, and MCP Apps are operational interfaces shipped by the server that owns them. The ontology depth stays with Foundry. |
| Apollo | Vendor-operated software delivery into customer environments | Inverted. Veoveo's GitOps factory does the same job, and the installation owner operates it. Nobody delivers into you. |

What none of them carry is physical AI as a first-class citizen. Real
simulator runtimes with real autopilot firmware, live video pipelines,
vehicle actuation, and a world-state timeline that makes every mission
replayable as evidence. Palantir integrates data about operations. Veoveo
runs, rehearses, and records the operations themselves.

Veoveo is the substrate for building an operational intelligence
capability an organization owns outright: its sensors, its doctrine, its
applications, inside its own boundary. A Foundry deployment can even sit beside it:
Palantir appears in the [connector catalog](docs/connectors/README.md)
like any other enterprise platform.

## An Agentic App Platform

Agentic apps are one way an installation delivers operational intelligence
to its users. An agentic app pairs an agent that plans and acts with a live
interface that people can see and steer: an operator types an instruction,
the agent drives simulation, live stream processing, or vehicles, and the interface
shows progress, results, and evidence as they land. The capabilities behind
Veoveo's own charts, maps, forecasts, and 3D views are open to your teams.

Apps built on Veoveo are enterprise software from the first request. They
authenticate through the installation's identity provider, act within policy
scopes and Work Context access, run as durable work that outlives any single
session, and leave the same audit trail as every other actor. They
deploy with the installation, scale with it, and run in the Console or in a
compatible external host.

The platform itself is built the same way: a small team working with coding
agents can extend, deploy, and operate an installation inside the same
identity, policy, and audit boundary. The repository is organized as a
[software factory](#a-software-factory) for exactly that work.

## Product Tour

A deployment can begin with the standard server catalog, add its own
extensions, and retain the same identity and policy boundary throughout.

| Capability | What it provides |
|---|---|
| Real and simulated worlds | Governed recordings, spatial and time reference systems, traffic simulation, UAV simulation, camera streams, vehicle actuation, and 3D Tiles scenes. |
| Analysis and planning | Sandboxed DuckDB SQL, forecasting, optimization, admitted live and replay stream processing, and temporal reasoning. |
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
| Installation health and recent activity | Durable work across Reason, Stream, and simulation |
| [![Work Context access](docs/screenshots/gallery/console-access.png)](docs/screenshots/gallery/console-access.png) | [![Paged audit trail](docs/screenshots/gallery/console-audit.png)](docs/screenshots/gallery/console-audit.png) |
| Membership, authority, and access requests | Bounded policy decisions with trace context |
| [![Kubernetes cluster inventory](docs/screenshots/gallery/console-cluster.png)](docs/screenshots/gallery/console-cluster.png) | <a href="docs/images/operations-loop.png"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/operations-loop-dark.png"><img src="docs/images/operations-loop.png" alt="Reactive and proactive operational loops running nonstop"></picture></a> |
| Workloads, placement, storage, readiness, and image identity | Reactive and proactive loops that run nonstop |

### Recordings become governed evidence

Rerun recordings retain synchronized world, sensor, pose, and annotation data.
The Console presents each recording as one continuous timeline while bounded
segments remain an internal storage concern. Derived outputs enter the artifact
plane with ownership, provenance, release state, and effective access.

| | |
|---|---|
| [![Governed artifact catalog](docs/screenshots/gallery/console-artifacts.png)](docs/screenshots/gallery/console-artifacts.png) | [![Reasoning artifact detail](docs/screenshots/gallery/console-artifact-reason.png)](docs/screenshots/gallery/console-artifact-reason.png) |
| Immutable outputs and release state | Reasoning result with recording provenance |
| [![Stream detection video artifact](docs/screenshots/gallery/console-artifact-video.png)](docs/screenshots/gallery/console-artifact-video.png) | [![Continuous recording playback](docs/screenshots/gallery/console-recordings.png)](docs/screenshots/gallery/console-recordings.png) |
| Stream-derived media preview and access | One authorized timeline in embedded Rerun |

## Executable Showcases

The showcases exercise the platform against real simulator runtimes. They are
maintained as deployable workloads with typed MCP contracts, recording paths,
and acceptance tests.

### UAV flight in Isaac Sim

Address one durable pilot in ordinary language:

> Fly uav-1 to Times Square now. Read your active UAV control grant, ask Map MCP to
> resolve and route this named location from current telemetry, then use UAV MCP to admit
> and execute the mission only for your bound vehicle. Report the terminal result.

<p align="center">
  <a href="showcase/uav-sim/assets/uav-e2e-001-flight-timelapse.mp4">
    <img src="showcase/uav-sim/assets/uav-e2e-001-flight-timelapse.gif" width="640" alt="Recorded downward camera view from uav-1 crossing New York during its mission from the Statue of Liberty area to Times Square">
  </a>
</p>

*The actual leader-camera recording accelerated 30×. The full
[26-second H.264 replay](showcase/uav-sim/assets/uav-e2e-001-flight-timelapse.mp4)
comes from the governed Recording Hub archive.*

The first accepted run covered 9.227 km in 13 minutes 10 seconds. It completed all four
admitted waypoints, arrived at 40.7580° N, 73.9855° W with zero collisions, and released
its command lease.

| Boundary | What happened |
|---|---|
| Addressed agent | `uav-1-pilot` accepted the operator message and ran one durable episode. |
| Map MCP | Resolved Times Square and returned the admitted route from current telemetry. |
| UAV Simulation MCP | Enforced the pilot-to-`uav-1` grant and protected execution with one command lease. |
| Recording Hub | Archived the leader camera, pose, telemetry, and mission lifecycle across the complete execution interval. |

The prompt carried no coordinates and granted no vehicle authority. The signed-in
Console and headless conversation projection returned the same terminal result.
[Inspect the Console evidence](showcase/uav-sim/assets/uav-e2e-001-console-complete.png)
or repeat the
[`UAV-E2E-001` acceptance](showcase/uav-sim/ACCEPTANCE.md#uav-e2e-001-per-agent-named-location-mission-e2e).

| San Salvador | Midtown Manhattan |
|---|---|
| [![Isaac Sim UAV flight over San Salvador](docs/screenshots/gallery/isaac-uav-san-salvador.png)](docs/screenshots/gallery/isaac-uav-san-salvador.png) | [![Isaac Sim UAV flight over Midtown Manhattan](docs/screenshots/gallery/isaac-uav-new-york.png)](docs/screenshots/gallery/isaac-uav-new-york.png) |
| A multirotor under PX4 control above the Jorge “Mágico” González stadium district | Dense New York photogrammetry around Times Square and Central Park |

Both frames come from the live headless Isaac Sim RTX viewport. The showcase
camera follows the Pegasus vehicle after PX4 reaches the configured flight
altitude. [Explore the complete UAV showcase](showcase/uav-sim/README.md).

| Governed UAV recording | SUMO traffic world |
|---|---|
| [![UAV simulation in Rerun](docs/screenshots/gallery/rerun-uav.png)](docs/screenshots/gallery/rerun-uav.png) | [![SUMO traffic simulation in Rerun](docs/screenshots/gallery/rerun-sumo.png)](docs/screenshots/gallery/rerun-sumo.png) |
| Camera, pose, telemetry, Stream-derived detections, and reasoning share governed evidence without making live processing wait for recording. | A pinned SUMO and LuST Luxembourg world exposes traffic reads, signal and vehicle control, network generation, durable batches, live subscriptions, and Rerun recording. [Run the SUMO showcase](showcase/sumo/README.md). |

## Built On The Model Context Protocol

Every capability above reaches agents and operators through the
[Model Context Protocol](https://modelcontextprotocol.io/specification/):
tools, resources, prompts, completions, durable tasks, subscriptions, and
notifications behind one identity and policy boundary. A client connects
once and reaches every capability; a system joins once and reaches every
client. Any compatible MCP host can drive the platform, and the Console
speaks the same protocol that agents use.

<a href="docs/images/integration-matrix.png">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/integration-matrix-dark.png">
    <img src="docs/images/integration-matrix.png" alt="Point-to-point wiring costs N times M integrations; one protocol and harness costs N plus M contracts">
  </picture>
</a>

*From N × M integrations to N + M contracts.*

### Apps travel with the server

An MCP server can deliver a self-contained interface with its protocol result.
The host provides the sandbox and theme; Veoveo retains authorization, task,
artifact, and audit semantics behind each action. The Console also ships a
standalone authenticated app host, so an installation can hand a single app
to its users as a full-page surface without the rest of the Console around
it. The View app below was invoked from natural language and rendered by an
external MCP host.

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
| [![Map MCP protocol surface](docs/screenshots/gallery/console-mcp-map.png)](docs/screenshots/gallery/console-mcp-map.png) | <a href="docs/images/task-sleepwake.png"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/task-sleepwake-dark.png"><img src="docs/images/task-sleepwake.png" alt="Durable task sequence: call, task handle, sleep, wake, result"></picture></a> |
| The map server's complete MCP capability inventory | Official durable Tasks: call, sleep, wake, result |

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
| `optimization` | NVIDIA cuOpt vehicle routing, scenario batches, convex and MILP solving, and independently verified problem/run/solution evidence. |
| `reason` | Semantic and temporal reasoning over recordings with grounded, audited output. |
| `recording` | Recording discovery, bounded queries, subscriptions, publication, and viewer projection. |
| `rerun` | The bridged Rerun viewer surface. |
| `stream` | Operator-admitted live and replay GStreamer pipelines, typed detection profiles, and an MCP App for encoded video with overlays. |
| `time` | Authority-bound civil time, calendars, clocks, timelines, and event operations. |
| `timeseries` | Forecasting, uncertainty output, governed artifacts, and an interactive forecast app. |
| `uav-sim` | Authoritative multi-vehicle simulation, missions, datasets, simulator-hosted operator cameras, shared NVENC products, governed viewer leases, and a WebRTC App. |
| `view` | 3D Tiles views rendered on cluster GPUs, camera control, and reproducible offscreen frame capture. |

The runtime for autonomous agents adds durable episodes, detach and resume,
wakes, budgets, analytical memory, tool use, and Rerun recording. Your own
agentic apps follow the same path as domain extensions and can join the
gateway without adopting Veoveo's source build: publish an image and Helm
chart, register the server in the typed control plane, and apply the
installation's trust and policy contract. Existing enterprise systems join
the same way: put an MCP server in front of a system of record and it
becomes a governed capability of the installation.

<a href="docs/images/agent-loop.png">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/agent-loop-dark.png">
    <img src="docs/images/agent-loop.png" alt="The agent runtime cycle: task results, timers, and messages wake the agent, which assembles context, runs an episode, persists, and sleeps, backed by state, memory, and log">
  </picture>
</a>

## Enterprise Connectors

Veoveo meets an enterprise on the platforms it already runs. Connector
recipes let a coding agent install a vendor's MCP server beside the Veoveo
connector and put both to work in one session, from lakehouse queries to
satellite tasking to incident response. The
[connector catalog](docs/connectors/README.md) records the verified install
surface, auth model, and status for every platform. Systems that still speak
the MCP `2025-11-25` revision join through an isolated
[legacy bridge](mcp/bridges/legacy/DESIGN.md) that keeps the installation's
own protocol surface uncompromised.

<table align="center" aria-label="Enterprise connector logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://www.databricks.com/"><img src="docs/assets/connectors/databricks.svg" height="36" alt="Databricks" title="Databricks"></a></td>
      <td align="center" width="72"><a href="https://www.snowflake.com/"><img src="docs/assets/connectors/snowflake.svg" height="36" alt="Snowflake" title="Snowflake"></a></td>
      <td align="center" width="72"><a href="https://clickhouse.com/"><img src="docs/assets/connectors/clickhouse.svg" height="36" alt="ClickHouse" title="ClickHouse"></a></td>
      <td align="center" width="72"><a href="https://motherduck.com/"><img src="docs/assets/connectors/duckdb.png" height="36" alt="MotherDuck DuckDB" title="MotherDuck / DuckDB"></a></td>
      <td align="center" width="72"><a href="https://grafana.com/"><img src="docs/assets/connectors/grafana.svg" height="36" alt="Grafana" title="Grafana"></a></td>
      <td align="center" width="72"><a href="https://www.datadoghq.com/"><img src="docs/assets/connectors/datadog.svg" height="36" alt="Datadog" title="Datadog"></a></td>
    </tr>
  </tbody>
</table>
<table align="center" aria-label="Enterprise connector logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://www.dynatrace.com/"><img src="docs/assets/connectors/dynatrace.svg" height="36" alt="Dynatrace" title="Dynatrace"></a></td>
      <td align="center" width="72"><a href="https://www.splunk.com/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/splunk-dark.svg"><img src="docs/assets/connectors/splunk.svg" height="36" alt="Splunk" title="Splunk"></picture></a></td>
      <td align="center" width="72"><a href="https://www.mapbox.com/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/mapbox-dark.svg"><img src="docs/assets/connectors/mapbox.svg" height="36" alt="Mapbox" title="Mapbox"></picture></a></td>
      <td align="center" width="72"><a href="https://www.tomtom.com/"><img src="docs/assets/connectors/tomtom.svg" height="36" alt="TomTom" title="TomTom"></a></td>
      <td align="center" width="72"><a href="https://carto.com/"><img src="docs/assets/connectors/carto.svg" height="36" alt="CARTO" title="CARTO"></a></td>
      <td align="center" width="72"><a href="https://www.openstreetmap.org/"><img src="docs/assets/connectors/openstreetmap.svg" height="36" alt="OpenStreetMap" title="OpenStreetMap"></a></td>
    </tr>
  </tbody>
</table>
<table align="center" aria-label="Enterprise connector logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://www.planet.com/"><img src="docs/assets/connectors/planet.svg" height="36" alt="Planet" title="Planet"></a></td>
      <td align="center" width="72"><a href="https://www.earthdata.nasa.gov/"><img src="docs/assets/connectors/nasa.svg" height="36" alt="NASA Earthdata" title="NASA Earthdata"></a></td>
      <td align="center" width="72"><a href="https://www.palantir.com/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/palantir-dark.svg"><img src="docs/assets/connectors/palantir.svg" height="36" alt="Palantir" title="Palantir Foundry"></picture></a></td>
      <td align="center" width="72"><a href="https://www.ros.org/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/ros-dark.svg"><img src="docs/assets/connectors/ros.svg" height="36" alt="ROS" title="ROS"></picture></a></td>
      <td align="center" width="72"><a href="https://www.autodesk.com/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/autodesk-dark.svg"><img src="docs/assets/connectors/autodesk.svg" height="36" alt="Autodesk" title="Autodesk"></picture></a></td>
      <td align="center" width="72"><a href="https://www.pagerduty.com/"><img src="docs/assets/connectors/pagerduty.svg" height="36" alt="PagerDuty" title="PagerDuty"></a></td>
    </tr>
  </tbody>
</table>
<table align="center" aria-label="Enterprise connector logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://slack.com/"><img src="docs/assets/connectors/slack.svg" height="36" alt="Slack" title="Slack"></a></td>
      <td align="center" width="72"><a href="https://www.atlassian.com/"><img src="docs/assets/connectors/atlassian.svg" height="36" alt="Atlassian" title="Atlassian"></a></td>
      <td align="center" width="72"><a href="https://linear.app/"><img src="docs/assets/connectors/linear.svg" height="36" alt="Linear" title="Linear"></a></td>
      <td align="center" width="72"><a href="https://github.com/"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/connectors/github-dark.svg"><img src="docs/assets/connectors/github.svg" height="36" alt="GitHub" title="GitHub"></picture></a></td>
      <td align="center" width="144"><a href="https://www.crowdstrike.com/"><img src="docs/assets/connectors/crowdstrike.svg" height="36" alt="CrowdStrike" title="CrowdStrike Falcon"></a></td>
    </tr>
  </tbody>
</table>

*The catalog spans geospatial, Earth observation, weather, data, observability,
industrial operations, defense, and incident platforms. All logos belong to
their respective owners.*

## How It Fits Together

<a href="docs/images/system-map.png">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/system-map-dark.png">
    <img src="docs/images/system-map.png" alt="Live H.264 enters Stream directly while optional recording flows through a producer-local forwarder; Stream results and recording snapshots feed Reason, and agents reach 16 hosted servers through the governed gateway">
  </picture>
</a>

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

<a href="docs/images/deployment-map.png">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/deployment-map-dark.png">
    <img src="docs/images/deployment-map.png" alt="Edge, cluster, air-gap, and hybrid installations, all one platform">
  </picture>
</a>

| Path | Use it for | Guide |
|---|---|---|
| Local k3d | A real local Kubernetes cluster with registry-first image delivery and mandatory NVIDIA validation. | [`deploy/local/k3d`](deploy/local/k3d/README.md) |
| Direct Helm | A connected cluster managed by an existing platform team. | [`deploy/helm/veoveo`](deploy/helm/veoveo/README.md) |
| Enterprise GitOps | Immutable OCI charts and image digests reconciled by the installation owner's Flux or equivalent controller. | [`docs/ENTERPRISE_DEPLOYMENT.md`](docs/ENTERPRISE_DEPLOYMENT.md) |
| Offline | A verified bundle containing runtime images, charts, schemas, checksums, image identities, and SPDX SBOMs. | [`deploy/offline`](deploy/offline/README.md) |

The [Autonomy Harness shared-responsibility contract](docs/AUTONOMY_HARNESS.md)
defines how continuously autonomous agents remain contained across identity, data,
network, compute, spend, capabilities, and side effects.

[`examples/bioma`](examples/bioma/README.md) is the executable reference for the
enterprise flow. Its hostname and infrastructure choices demonstrate one
installation; each deployment substitutes its own.

### GPU execution contract

Hardware GPU access is mandatory for optimization, simulation, perception,
reasoning, 3D rendering, Rerun, and visual acceptance workflows that declare
it. Kubernetes workloads request an NVIDIA device and fail closed when cuOpt,
CUDA, Vulkan, WebGPU, or WebGL cannot reach hardware. CPU solving and software
rendering are not supported fallbacks.

The local cluster applies the same `nvidia.com/gpu` scheduling contract used by
fielded installations. Browser verification proves that high-performance
WebGPU or WebGL reaches hardware before interacting with a visual surface. It
probes both APIs when available and stops if neither remains hardware-backed.

## Roadmap

Veoveo is working toward world models built from your operational reality:
digital twins of the sites and fleets you operate, assembled from the
geography, recordings, and telemetry an installation already governs, so
simulation, rehearsal, and prediction start from the world you actually
operate.

## Standards And Protocols

Veoveo uses published standards at interoperability boundaries and names its
repository-owned extensions explicitly. The table states the implemented
profile rather than support for every optional feature of each standard.

| Area | Implemented standards and protocols |
|---|---|
| Agent and app interfaces | Model Context Protocol `2026-07-28` over JSON-RPC 2.0 and stateless Streamable HTTP; official durable Tasks; JSON Schema 2020-12; and [MCP Apps](mcp/apps-extension/DESIGN.md). |
| Identity and authorization | OpenID Connect Core; OAuth 2.0 Authorization Code with S256 PKCE, Client Credentials, and JWT Bearer grants; RFC 8414 metadata; RFC 9728 protected-resource metadata; RFC 8707 resource indicators; JWT, JWS, and JWK; MCP enterprise-managed authorization and ID-JAG. |
| Recordings, data, and media | Rerun RRD and `VideoStream`; versioned protobuf recording ingest; S3-compatible object APIs; DuckDB SQL; Apache Parquet; and OTLP/HTTP telemetry. |
| Geography and time | WGS84/EPSG identities; GeoJSON RFC 7946; OGC JSON-FG and CQL2; GeoParquet 1.0; Mapbox Vector Tile 2.1; MapLibre Style 8; RFC 3339; RFC 9557; IANA TZDB/TZif and leap-second data; TAI and GPS time. |
| Optimization | NVIDIA cuOpt 26.06 on CUDA 13.2; `veoveo.io/travel-model-artifact/v1` for the Map handoff; and the private pod-local `veoveo.io/cuopt-executor/v1` adapter protocol. |
| 3D and vehicles | OGC 3D Tiles 1.0/1.1; glTF/GLB 2.0; Draco geometry compression; MAVLink 2; and pod-private ROS 2 simulator paths. |
| Packaging and operations | Kubernetes resources, Helm charts, OCI images and charts, S3-compatible storage, and OpenTelemetry. |

The exact supported subsets are collected in
[`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md#standards-and-protocols). Domain
profiles live in their server designs, including
[`map-mcp`](servers/map-mcp/DESIGN.md#standards-and-protocols),
[`optimization-mcp`](servers/optimization-mcp/DESIGN.md#standards-and-protocols),
[`time-mcp`](servers/time-mcp/DESIGN.md#standards-and-protocols),
[`view-mcp`](servers/view-mcp/DESIGN.md#standards-and-protocols), and
[`uav-sim-mcp`](servers/uav-sim-mcp/DESIGN.md#standards-and-protocols).

## Tech Stack

Veoveo is built from technology many engineers already run in production:
services that share one ontology of work, evidence, and worlds, MCP servers
in any language that speaks the protocol, a Console that runs in any modern
browser, Kubernetes and Helm underneath, SurrealDB for coordination, DuckDB
for analysis, Rerun for recordings, and NVIDIA runtimes for cuOpt decisions,
simulation, and Stream perception profiles. If these tools feel like home, so
will this repository.

<table align="center" aria-label="Technology stack logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://www.rust-lang.org/"><img src="docs/assets/stack/rust.svg" height="40" alt="Rust" title="Rust"></a></td>
      <td align="center" width="72"><a href="https://www.python.org/"><img src="docs/assets/stack/python.svg" height="40" alt="Python" title="Python"></a></td>
      <td align="center" width="72"><a href="https://www.typescriptlang.org/"><img src="docs/assets/stack/typescript.svg" height="40" alt="TypeScript" title="TypeScript"></a></td>
      <td align="center" width="72"><a href="https://react.dev/"><img src="docs/assets/stack/react.svg" height="40" alt="React" title="React"></a></td>
      <td align="center" width="72"><a href="https://kubernetes.io/"><img src="docs/assets/stack/kubernetes.svg" height="40" alt="Kubernetes" title="Kubernetes"></a></td>
      <td align="center" width="72"><a href="https://helm.sh/"><img src="docs/assets/stack/helm.svg" height="40" alt="Helm" title="Helm"></a></td>
    </tr>
  </tbody>
</table>
<table align="center" aria-label="Technology stack logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://www.docker.com/"><img src="docs/assets/stack/docker.svg" height="40" alt="Docker" title="Docker"></a></td>
      <td align="center" width="72"><a href="https://opentelemetry.io/"><img src="docs/assets/stack/opentelemetry.svg" height="40" alt="OpenTelemetry" title="OpenTelemetry"></a></td>
      <td align="center" width="72"><a href="https://surrealdb.com/"><img src="docs/assets/stack/surrealdb.svg" height="40" alt="SurrealDB" title="SurrealDB"></a></td>
      <td align="center" width="72"><a href="https://duckdb.org/"><img src="docs/assets/stack/duckdb.png" height="40" alt="DuckDB" title="DuckDB"></a></td>
      <td align="center" width="72"><a href="https://rerun.io/"><img src="docs/assets/stack/rerun.png" height="40" alt="Rerun" title="Rerun"></a></td>
      <td align="center" width="72"><a href="https://developer.nvidia.com/isaac/sim"><img src="docs/assets/stack/nvidia.svg" height="40" alt="NVIDIA Isaac Sim" title="NVIDIA Isaac Sim"></a></td>
    </tr>
  </tbody>
</table>
<table align="center" aria-label="Technology stack logos">
  <tbody>
    <tr>
      <td align="center" width="72"><a href="https://px4.io/"><img src="docs/assets/stack/px4.png" height="40" alt="PX4 Autopilot" title="PX4 Autopilot"></a></td>
      <td align="center" width="72"><a href="https://eclipse.dev/sumo/"><img src="docs/assets/stack/sumo.png" height="40" alt="Eclipse SUMO" title="Eclipse SUMO"></a></td>
      <td align="center" width="72"><a href="https://cesium.com/"><img src="docs/assets/stack/cesium.svg" height="40" alt="Cesium" title="Cesium"></a></td>
      <td align="center" width="72"><a href="https://maplibre.org/"><img src="docs/assets/stack/maplibre.svg" height="40" alt="MapLibre" title="MapLibre"></a></td>
    </tr>
  </tbody>
</table>

*All logos belong to their respective projects.*

## A Software Factory

The platform is designed to be extended, deployed, and operated with
coding agents. Veoveo ships no coding harness of its own: the factory
admits whatever agents a team already trusts, from a terminal session to a
full MCP host, and meets them with the material a new engineer would ask
for on day one. Engineering conventions live in [`AGENTS.md`](AGENTS.md)
at the root and beside every hosted server, ownership and call paths in
the [`code map`](docs/CODEMAP.md), and each server carries a design
document bound to the normative [server contract](mcp/contract/DESIGN.md).

Toolchains are pinned, contracts reject invalid work before it deploys,
verification runs as executable harnesses, and deployments prove themselves
with smoke tests. The same boundary that governs human operators governs
agents: every action is authenticated, scoped by policy, bounded by
budgets, and audited, so an installation can hand real work to agents and
stay in control of what they touch.

Delivery follows the same model. A forward-deployed engineer can stand up
an installation inside the customer's environment and, working with agents
against these contracts, encode the domain's knowledge into its policies,
profiles, and extensions. What an engagement leaves behind is the factory
itself: cluster, identity, models, policies, and release process, owned
end to end by the organization that runs it.

## Develop And Verify

The service workspace, Python packages, container images, Helm charts, protocol
conformance clients, and smoke harnesses are all pinned in the repository.
Docker is required for SurrealDB-backed tests and deployment work. Native Map
builds also need a C/C++ toolchain, CMake, pkg-config, SQLite development files,
and PROJ's build dependencies.

```bash
cargo fmt --all
cargo xtask enforce rust
cargo test --workspace
cargo xtask enforce python
cargo xtask smoke helm-config
cargo xtask smoke sumo-push
cargo test -p veoveo-uav-sim-mcp --all-targets
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.5.1 --with aiohttp==3.14.1 \
  --with pymavlink==2.4.49 --with fastcrc==0.3.6 --python 3.13 \
  python -m unittest discover -s showcase/uav-sim/runtime/tests -v
```

Smoke orchestration is platform code, held to the same review and testing bar
as everything it verifies. `cargo xtask smoke` builds the typed harness and
its scenario-specific local binary prerequisites, then dispatches it. Local
deployment profiles use the current tool versions pinned in
[`deploy/local/k3d/versions.env`](deploy/local/k3d/versions.env).

During the current single-host development phase, checks run locally and their
informational result is committed for GitHub to display:

```bash
cargo xtask test-report run --name rust-workspace -- cargo xtask enforce rust
cargo xtask test-report show
```

This status does not block pushes or deployments. The temporary workflow and the
planned dedicated GPU CI architecture are described in
[`docs/CONTINUOUS_INTEGRATION.md`](docs/CONTINUOUS_INTEGRATION.md).

## Repository Guide

| Path | Responsibility |
|---|---|
| [`agents/`](agents/) | Kernel and durable runtime for autonomous agents. |
| [`apps/console/`](apps/console/) | Console BFF and React operations interface. |
| [`mcp/`](mcp/) | Shared MCP contracts, task and app extensions, and bridges. |
| [`platform/`](platform/) | Gateway, persistence, task, artifact, recording, and query runtimes. |
| [`servers/`](servers/) | Hosted MCP servers and their domain designs. |
| [`extensions/`](extensions/) | Release, compatibility, and conformance contracts for externally owned extensions. |
| [`sdk/`](sdk/) | Python SDK shared by showcase runtimes and external clients. |
| [`templates/`](templates/) | Python MCP server template behind the datasheet server. |
| [`showcase/uav-sim/`](showcase/uav-sim/) | Isaac, Cesium, Pegasus, and PX4 UAV workload. |
| [`showcase/sumo/`](showcase/sumo/) | SUMO, LuST, TraCI, and the traffic world MCP server. |
| [`deploy/`](deploy/) | Helm, local k3d, and offline installation material. |
| [`examples/bioma/`](examples/bioma/) | Enterprise GitOps reference installation. |
| [`testing/`](testing/) | Protocol conformance and multi-process smoke harnesses. |
| [`tools/xtask/`](tools/xtask/) | Typed repository commands: doctor, enforce, image, release, smoke, test-report. |
| [`tools/screenshots/`](tools/screenshots/) | Repeatable authenticated Console, MCP App, and Rerun captures. |
| [`docs/`](docs/) | Architecture, governance, deployment, recording, and harness documentation. |

Start with the [`code map`](docs/CODEMAP.md) for ownership and call paths, the
[`reference architecture`](docs/architecture/README.md) for system views, or
the [`complete screenshot gallery`](docs/screenshots/GALLERY.md) for the visual
catalog and reproduction guide.
