# UAV simulation MCP server

This document is the normative design for `uav-sim-mcp`. The server governs
one UAV domain simulation without exposing simulator-native control or
high-rate state through the Veoveo gateway.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with direct controls, task-only operations, resources, prompts, completions, subscriptions, and notifications. The server has no MCP App. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Strict typed session, world, vehicle, mission, command, recording, and pose-publication health shapes. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; scenarios, missions, and dataset captures use `interrupted_indeterminate` recovery. |
| Veoveo Frames contract | One immutable ECEF-rooted world revision and one revision-scoped ENU simulation frame bind each session. |
| `veoveo.io/simulation-view-scene/v1` | Strongly typed governed scene declaration prepared from the authoritative UAV session and shared through `veoveo-simulation-scene`. |
| `veoveo.io/simulation-view-pose/v1` | Complete newest-value entity snapshots in local ENU metres with FLU entity axes and XYZW quaternions. This server reports publication health; snapshots travel on the private data plane. |
| Veoveo Artifact plane | Caller-authorized publication of self-contained declarative OpenUSD environment and UAV prototype assets. |
| SPIFFE and TLS 1.3 | Installation-issued producer identity and mutually authenticated private transport to Simulation View pose ingress. No certificate, private key, or ingress endpoint appears in MCP state. |
| [MAVLink 2](https://mavlink.io/en/) | Pod-local PX4 command, acknowledgement, heartbeat, mission-position, and vehicle-state transport. |
| ROS 2 | Optional private simulator data plane. High-rate topics are not MCP tools or resources. |
| OGC 3D Tiles | Google Photorealistic 3D Tiles stream through Cesium ion into the domain simulator. Tile readiness and residency are session state. |
| WGS 84, ECEF, ENU, NED, FRD, and FLU | Immutable world identity, local simulation, PX4 navigation, vehicle telemetry, and renderer pose publication use explicit mappings. |
| [Rerun 0.35.0](https://rerun.io/docs/) RRD and `VideoStream` | Vehicle, sensor, transform, mission, tile, and nadir-camera evidence. |
| Veoveo recording ingest | Version `2026-07-24`; a producer-local forwarder carries native Rerun messages to Recording Hub. |
| Cluster-private HTTP/JSON | Typed MCP-server-to-simulator adapter boundary. Simulator, MAVLink, ROS 2, pose TLS, and recording ports are not public gateway routes. |

## Identity

```text
crate       veoveo-uav-sim-mcp
folder      servers/uav-sim-mcp
slug        uav-sim
URI scheme  uav-sim
endpoint    /uav-sim/mcp
health      /uav-sim/healthz
ready       /uav-sim/readyz
port        8802
```

The public surface is provider-neutral. Isaac Sim, Cesium for Omniverse,
Pegasus, and PX4 implement the first adapter but do not enter canonical tool
or resource names.

## Ownership Boundary

The UAV extension owns domain dynamics, controls, sensors, recordings, its
private adapter, scene declarations, and its pose publisher. The MCP server
owns the typed domain protocol, caller ownership, task state, resource
identities, subscriptions, prompts, audit context, and governed recording
references.

Simulation View is an independent Veoveo service. It owns the render-only
OpenUSD mirror, logical follow, chase, mounted, orbit, fixed, look-at, and
formation-overview cameras, RTX render products, capacity admission, NVIDIA
NVENC, WebRTC, leases, and the live-view App. The UAV runtime does not create
operator cameras, load the NVIDIA live-stream extensions, expose signaling or
media ports, or implement a domain live-view App.

The UAV runtime publishes poses through the reusable Python SDK. Its adapter
only maps UAV telemetry into SDK types and reports publisher status. Encoding,
framing, bounded newest-value buffering, TLS 1.3, and reconnect behavior live
in the SDK. DNS, certificate loading, TLS handshakes, and socket writes run on
the SDK worker and never block physics or rendering.

## World And Frame Binding

Every session starts `unconfigured`. `configure_world` accepts one immutable
Frames world revision and one static simulation frame within it. The server
verifies the tree digest, revision membership, and static ancestry to a
geodetic tangent anchor.

The binding is write-once. Repeating the same request is idempotent; a
different revision or frame is rejected. The runtime derives the Cesium
georeference, Pegasus origin, WGS84 conversion, recording metadata, and
Simulation View frame-revision identity from that binding. Helm carries no
frame URI or geographic origin.

The canonical transform chain is:

```text
WGS84/ECEF -- immutable Frames revision --> local ENU stage
local ENU  -- typed adapter mapping --> Pegasus vehicle state
local ENU  -- explicit axis mapping --> PX4 NED and body FRD
local ENU  -- complete pose snapshots --> Simulation View entity FLU
```

The pose producer publishes the exact Frames revision URI and
`sha256:<digest>`. The entity table is deterministic for the configured
vehicle count. A changed entity set requires a new entity-table revision and
digest.

## Pose Publication

One snapshot contains every declared UAV entity. Entity order is canonical,
identities are stable, and sequence numbers are strictly increasing within
the configured renderer epoch. The renderer timestamp advances at render
cadence independently of a domain timeline reset. This prevents a reset from
moving the Simulation View epoch backward.

Each entity includes local ENU position and normalized XYZW orientation. FLU
velocity is optional. The first adapter omits velocity because its source
velocity is expressed in world ENU; it does not mislabel ENU components as
body FLU.

The publisher keeps one unsent value. A new offer replaces an older pending
snapshot, while the simulation thread performs only validation, deterministic
encoding, and a bounded lock acquisition. A disconnected renderer causes
newest-value replacement rather than backpressure.

`PosePublicationState` exposes:

- the exact protocol schema;
- producer, SPIFFE, epoch, entity-table, and cadence identity;
- `starting`, `connecting`, `ready`, `degraded`, `failed`, or `stopped`;
- offered, sent, and replaced counters;
- the last sent sequence and a redacted transport diagnostic.

It never exposes an address, certificate path, credential, or access token.
Runtime readiness requires at least one sent snapshot and a currently ready
publisher. Missing pose authorization or transport is an operational failure,
not a reason to run an embedded viewer.

## View Scene Declaration

`prepare_view_scene` publishes the UAV server's self-contained OpenUSD
environment and vehicle prototype through the caller-authorized Artifact
plane. It derives the scene session, epoch, Frames revision, simulation frame,
producer identity, SPIFFE identity, entity-table revision, and entity-table
digest from current simulator state. A caller cannot provide contradictory
values.

The resulting `PreparedViewScene` uses the provider-neutral
`veoveo-simulation-scene` contract. Repeating the request for the same caller
and unchanged session returns the same prepared declaration. The prepared
resource is owner-scoped and never exposes the caller's bearer or the private
pose endpoint.

Scene declaration does not transfer renderer ownership. The UAV server
publishes no camera, stream, lease, signaling, or App tool. Simulation View
decides whether to admit, materialize, and render the declaration.

## Domain Runtime

Google Photorealistic 3D Tiles rendered through Cesium ion are part of this
showcase's domain evidence. A healthy session reports the configured asset,
load progress, resident tiles, failed tiles, and the accepted georeference.
Simulation View may render a separately declared environment; that does not
replace domain-simulator tile acceptance.

`CESIUM_ION_ACCESS_TOKEN` comes from a dedicated Secret. The runtime authors it
only into the anonymous USD session layer required by Cesium, clears the
attribute during shutdown, and never exports that layer.

The active viewport follows the primary nadir sensor because Cesium performs
tile selection from a Kit viewport. Nadir cameras remain domain sensors and
recording inputs. They are not operator views.

The pod-local GCS link binds `14550 + instance` and the matching PX4 endpoint
at `18570 + instance`. Commands require explicit accepted acknowledgements.
Arm completion also requires an armed heartbeat. A land command interrupts an
active waypoint loop before acquiring the MAVLink command channel.

## Typed Domain Model

Controlled types include:

- `SessionId`, `VehicleId`, `MissionId`, `RecordingId`, `PoseProducerId`,
  `SpiffeId`, `EpochId`, `FrameWorldRevisionUri`, and `WorldFrameUri`;
- `SimulationLifecycle`, `TileLifecycle`, `CameraLifecycle`,
  `PosePublicationLifecycle`, `VehicleFlightState`, and `MissionLifecycle`;
- WGS84, ENU, NED, quaternion, vehicle, sensor, recording, and publication
  records;
- tagged `SimulationCommand` and `DurableOperation` enums.

Raw JSON is not used for shapes controlled by Veoveo.

## MCP Surface

### Direct Tools

| Tool | Behavior |
|---|---|
| `configure_world` | Binds the session once to a verified Frames revision and static simulation frame. |
| `get_simulation_state` | Reads domain lifecycle, world, tiles, sensors, pose-publication health, recordings, and vehicles. |
| `prepare_view_scene` | Publishes governed UAV visual assets and returns the authoritative typed scene and pose-producer binding. |
| `pause_simulation` | Pauses one running session. |
| `resume_simulation` | Resumes one paused session. |
| `reset_simulation` | Resets domain dynamics without resetting the renderer epoch. |
| `step_simulation` | Advances a paused session by a bounded number of physics steps. |
| `arm_vehicle` | Arms one PX4-backed vehicle after adapter checks. |
| `takeoff_vehicle` | Starts a bounded takeoff to a typed relative altitude. |
| `land_vehicle` | Commands one vehicle to land. |

Simulation View tools create and govern operator cameras and streams. They are
not duplicated or aliased by this server.

### Durable Tools

| Tool | Recovery | Behavior |
|---|---|---|
| `run_scenario` | `interrupted_indeterminate` | Runs a bounded live scenario. |
| `execute_mission` | `interrupted_indeterminate` | Executes typed waypoints and actions. |
| `capture_dataset` | `interrupted_indeterminate` | Captures a bounded sensor interval and returns governed recording identities. |

Live work is not replayed after an unclean interruption.

### Resources

```text
uav-sim://sessions
uav-sim://session/{session_id}
uav-sim://session/{session_id}/world
uav-sim://session/{session_id}/tiles
uav-sim://session/{session_id}/vehicles
uav-sim://session/{session_id}/vehicle/{vehicle_id}
uav-sim://session/{session_id}/recordings
uav-sim://session/{session_id}/view-scene
uav-sim://mission/{mission_id}
uav-sim://usage
uav-sim://usage/task/{task_id}
```

Session, world, tile, vehicle, mission, recording, and prepared view-scene
resources support the contract's subscriptions and notifications. Task usage
reuses the shared usage model.

## Recording Integration

The simulator emits vehicle poses, transforms, IMU samples, vehicle state,
mission state, collision events, tile diagnostics, and nadir camera samples.
The adapter publishes native Rerun messages and reports only its private
application and recording keys. The MCP server resolves those keys through
the recording catalog and returns canonical
`recording://recordings/{recording_id}` identities.

`TODO(GPU)` marks the existing NumPy sensor readback, camera-quality
reductions, and PyAV `libx264` recording path. Those paths must move to a
canonical CUDA/NVENC recording fan-out. They are domain recording debt and are
not evidence for Simulation View, whose renderer and encoder remain fully
GPU-backed.

## Deployment

The chart under `showcase/uav-sim/deploy/helm` renders one GPU-required domain
simulator pod with the UAV MCP sidecar and recording forwarder. It mounts a
producer-only PEM TLS Secret and sends poses to the installation-selected
Simulation View pose service. A pod label admits that private connection under
the platform NetworkPolicy.

The chart exposes only the MCP Service. It has no public signaling Service,
media Service, live-view Ingress, or media port. Simulation View is selected
as a separate GPU renderer component and has its own deployment, capacity,
security, and public media composition. Production therefore schedules the
domain simulator and renderer as separate GPU workloads.

## Security

Gateway internal assertions are mandatory. Every task carries principal,
tenant, profile, and data-label ownership. The pod disables service-account
token mounting, uses the NVIDIA runtime class, requests `nvidia.com/gpu: 1`,
and accepts credentials only through Secret references.

NetworkPolicy permits the gateway-to-MCP path, installation-internal services,
DNS, and public TLS needed by Cesium. Simulation View pose ingress separately
admits only labeled producers and authenticates their SPIFFE identities with
TLS 1.3 mutual authentication.

## Acceptance

Credential-free acceptance covers strict contract serialization, fake adapter
behavior, exact cross-language pose encoding, complete entity snapshots,
monotonic renderer sequencing, gateway registration, Helm schema validation,
and rendered NetworkPolicy and Secret wiring.

Live acceptance requires all of the following:

1. The UAV simulator and independent Simulation View renderer each receive an
   NVIDIA GPU and reject software rendering.
2. Frames publishes the complete immutable world, and the UAV session binds it
   before stage construction.
3. Cesium tiles, PX4 vehicles, nadir sensors, recording, and pose publication
   become ready.
4. The authorized SPIFFE producer delivers exact pose snapshots to Simulation
   View without blocking domain physics.
5. Simulation View mirrors the scene, admits logical cameras, renders through
   RTX, encodes through NVENC, and serves its own provider-neutral live App.
6. Recording and perception consume governed domain evidence.
7. No credential appears in a manifest value, MCP resource, task result, log,
   USD export, or retained artifact.

Missing authorization, frame disagreement, unavailable NVIDIA hardware,
unavailable tiles, PX4 failure, recording failure, or pose transport failure
fails explicitly. No UAV-owned live-view compatibility path remains.
