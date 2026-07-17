# UAV simulation MCP server

This document is the normative contract for the `uav-sim-mcp` server. The
server governs interactive and durable work against one or more UAV simulation
sessions without exposing simulator-native control ports through the Veoveo
gateway.

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

The public name is provider-neutral. Isaac Sim, Cesium for Omniverse, Pegasus,
and PX4 implement the first adapter, but their names do not enter canonical
tool or resource identities.

## Ownership boundary

The server owns the typed simulation protocol, caller ownership, task state,
policy-visible operations, resource identities, subscriptions, prompts, audit
context, and recording references. It serializes mutations for each session.

The simulator adapter owns Isaac stage mutation, timeline stepping, Cesium tile
residency, Pegasus vehicles, PX4 process lifecycle, MAVLink transport, sensor
capture, and the translation between simulator-native state and this contract.
Its HTTP endpoint is cluster-private and accepts only typed requests from the
MCP server.

MAVLink and ROS 2 remain data-plane protocols. They are never projected as
high-rate MCP tools. Autonomous workloads may use private Kubernetes Services
for those protocols while MCP governs missions, scenarios, inspection, and
bounded actuation.

## Core world contract

Google Photorealistic 3D Tiles rendered inside Isaac Sim through Cesium ion are
part of the core delivery. A healthy session reports the configured ion asset,
Cesium connection state, tile load progress, resident tile count, and the
georeference used by the stage. View MCP's direct Google source is an
independent capability and cannot satisfy UAV simulation acceptance.

The Isaac container receives `CESIUM_ION_ACCESS_TOKEN` from a Kubernetes
Secret dedicated to the UAV runtime, under the key
`cesium-ion-access-token`. The value is never accepted as a tool argument,
stored in a ConfigMap, returned by a resource, or included in a log field.
Cesium requires the token on its tileset schema. The adapter authors it only
into the anonymous session layer, clears the attribute during shutdown, and
never exports that layer.

## Spatial frames

Every session names one durable `frames://frame/{frame_id}` origin. Installation
bootstrap creates that definition through Frames MCP before the simulator
becomes ready. The UAV chart supplies the same typed origin to the Cesium
georeference and Pegasus world. Live acceptance reads the durable resource and
requires it to agree with simulator state. The adapter performs high-rate
ENU/NED conversion locally and attaches the frame URI to recorded transforms.
It does not make MCP calls in the physics loop.

The canonical chain is:

```text
WGS84/ECEF -- Frames MCP definition --> local ENU stage
local ENU  -- typed adapter mapping --> Pegasus body state
local ENU  -- explicit axis mapping --> PX4 NED
```

Axes, handedness, units, ellipsoid height, and origin are recorded. Missing or
incompatible frame information fails session creation.

## Typed domain model

The controlled vocabulary includes:

- `SessionId`, `VehicleId`, `MissionId`, `RecordingId`, and `FrameUri` validated
  identity types.
- `SimulationLifecycle`: `starting`, `ready`, `running`, `paused`, `stopping`,
  `stopped`, or `failed`.
- `TileLoadState`: `connecting`, `streaming`, `ready`, or `failed` with counts
  and a redacted diagnostic.
- `VehicleFlightState`: `initializing`, `standby`, `armed`, `taking_off`,
  `flying`, `landing`, `landed`, or `failed`.
- WGS84, local ENU, PX4 NED, velocity, attitude, battery, collision, sensor, and
  mission progress records.
- `SimulationCommand` and `MissionCommand` tagged enums for the private adapter
  boundary.

Raw JSON is not used for shapes controlled by Veoveo. Opaque upstream payloads
may appear only as bounded, explicitly labeled diagnostic metadata.

## MCP surface

### Direct tools

| Tool | Behavior |
|---|---|
| `get_simulation_state` | Reads the current session, tile, recording, and vehicle summary. |
| `pause_simulation` | Pauses one running session. |
| `resume_simulation` | Resumes one paused session. |
| `reset_simulation` | Resets the stage and vehicles to the declared scenario start. |
| `step_simulation` | Advances a paused session by a bounded number of physics steps. |
| `arm_vehicle` | Arms one PX4-backed vehicle after adapter safety checks. |
| `takeoff_vehicle` | Starts a bounded takeoff to a typed relative altitude. |
| `land_vehicle` | Commands one vehicle to land. |

The state read is read-only. Mutations are marked destructive where they alter
live vehicle or world state. A direct result returns a typed acknowledgement
and the affected resource URI.

### Durable tools

| Tool | Recovery | Behavior |
|---|---|---|
| `run_scenario` | `interrupted_indeterminate` | Runs a bounded live scenario against the loaded Google 3D Tiles world. |
| `execute_mission` | `interrupted_indeterminate` | Executes typed waypoints and actions for one or more vehicles. |
| `capture_dataset` | `interrupted_indeterminate` | Captures a bounded sensor interval and returns governed recording identities. |

Live simulator work is not replayed after an unclean interruption because the
external physical state cannot be inferred safely. Resumption marks such tasks
interrupted and requires an operator decision. The task extension remains the
canonical task protocol; compatibility task tools are not added.

### Resources and templates

```text
uav-sim://sessions
uav-sim://session/{session_id}
uav-sim://session/{session_id}/world
uav-sim://session/{session_id}/tiles
uav-sim://session/{session_id}/vehicles
uav-sim://session/{session_id}/vehicle/{vehicle_id}
uav-sim://session/{session_id}/recordings
uav-sim://mission/{mission_id}
uav-sim://usage
uav-sim://usage/task/{task_id}
```

Session, world, tiles, vehicles, individual vehicles, mission, and recording
resources support subscriptions. Mutations publish resource-update
notifications after the adapter acknowledges the new state. Task usage reuses
the shared usage model.

### Prompts and completions

`uav-sim-mission-plan` prepares a typed mission request against declared
vehicles and frames. `uav-sim-session-review` inspects tile, vehicle, collision,
mission, recording, and task evidence. Completions resolve visible session,
vehicle, mission, and task identities without crossing tenant ownership.

## Recording integration

The simulator emits camera streams, poses, transforms, IMU values, vehicle
state, mission state, collision events, and tile-loading diagnostics. The
co-located recording adapter converts them into typed Rerun entities under:

```text
/world/uav-sim/{session_id}/...
```

It pushes to the private Recording Hub endpoint using the repository-pinned
Rerun SDK. Camera output is H.264 Annex B with simulation timestamps. The
adapter reports only its private application and recording keys. The UAV MCP
resolves those keys through the platform recording catalog and publishes only
the canonical UUIDv7 `recording://recordings/{recording_id}` identity. That URI
becomes the input to Perception MCP. Native Recording Hub ports are not exposed
publicly.

## Kubernetes deployment

The reference deployment is `showcase/uav-sim/deploy/helm`. One interactive
session is one pod containing the Isaac runtime, UAV MCP server, and recording
adapter. Batch sessions use Jobs with the same container contract. Independent
worlds never share an Isaac process.

Isaac requests `nvidia.com/gpu: 1`. The chart does not disable, scale down, or
replace View or Perception. Bioma runs Isaac, View, and Perception concurrently,
and Kubernetes places every declared request on available cluster capacity.
Optional affinity and tolerations select capable nodes without imposing an
exclusive node topology.

GPU discovery remains a cluster concern. Fielded installations provide the
standard resource and runtime class with the pinned NVIDIA GPU Operator;
development k3d provides the same contract through its device-plugin manifest.
The UAV chart does not install or own node-level drivers.

Simulator, MAVLink, ROS 2, and RTSP ports use private Services or pod-local
transport. Only the MCP HTTP endpoint is registered with the gateway. WebRTC is
an explicit diagnostic option and is not the canonical browser or recording
path.

## Security

Gateway internal assertions are mandatory. Every task carries principal,
tenant, profile, and data-label ownership. The pod disables service-account
token mounting, drops Linux capabilities where the NVIDIA runtime permits it,
uses the Isaac image's non-root user, and accepts only Secret references for
credentials.

NetworkPolicy permits Cesium ion and required NVIDIA asset egress, internal
SurrealDB and recording traffic, and the gateway-to-MCP path. A future autonomy
data-plane Service must declare its peers explicitly. The MCP server never
proxies arbitrary URLs.

## Acceptance

Unit tests use a deterministic fake adapter. Helm and gateway checks are local
and credential-free. The live acceptance is a separately invoked, billed test
that requires `CESIUM_ION_ACCESS_TOKEN` and NVIDIA registry access.

The live test passes only when all of the following are true at once:

1. Isaac Sim, View, and Perception deployments remain available.
2. Cesium authenticates and Google Photorealistic 3D Tiles become resident in
   the Isaac stage.
3. Pegasus spawns a PX4-backed UAV against the declared frame.
4. The UAV arms, takes off, follows a bounded mission, and lands.
5. MCP resources, mutations, tasks, ownership, and notifications work through
   the gateway.
6. Camera, pose, telemetry, mission, and tile state reach Recording Hub.
7. Perception reads the governed H.264 recording and publishes derived output.
8. No credential appears in a rendered manifest, resource, task result, log,
   USD layer, or retained artifact.

Missing credentials, unavailable tiles, unsupported Pegasus APIs, PX4 startup
failure, frame disagreement, and recording failure all fail explicitly. No
older Isaac, Cesium, Pegasus, PX4, variable name, or protocol path is retained
as a fallback.
