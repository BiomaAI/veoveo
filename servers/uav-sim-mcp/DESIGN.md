# UAV simulation MCP server

This document is the normative contract for the `uav-sim-mcp` server. The
server governs interactive and durable work against one or more UAV simulation
sessions without exposing simulator-native control ports through the Veoveo
gateway.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with direct controls, task-only scenarios and missions, resources and templates, prompts, completions, subscriptions, and notifications. |
| [MCP Apps](https://apps.extensions.modelcontextprotocol.io/) | Version `2026-01-26`; `ui://uav-sim/live.html` drives the canonical stream tools and reads the same typed resources as every other client. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Session, world, vehicle, mission, command, recording, and structured-result contracts. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; live scenarios, missions, and dataset captures are durable tasks with `interrupted_indeterminate` recovery. |
| [MAVLink 2](https://mavlink.io/en/) | Pod-local PX4 command, acknowledgement, heartbeat, mission-position, and vehicle-state transport. The adapter uses `COMMAND_INT` and `MAV_FRAME_GLOBAL_INT` for repositioning. |
| ROS 2 | Private simulator data plane. High-rate topics are not projected into MCP tools. |
| OGC 3D Tiles | Google Photorealistic 3D Tiles streamed through Cesium ion into the simulator; tile readiness and residency are typed session state. |
| WGS 84, ECEF, ENU, and NED | Durable Frames origin, local simulator stage, Pegasus body state, and PX4 navigation frame. Axis and handedness mappings remain explicit. |
| [Rerun](https://rerun.io/docs/) RRD and `VideoStream` | Vehicle, sensor, transform, mission, tile, and camera evidence. Camera samples use H.264 Annex B with simulation timestamps. |
| Veoveo recording ingest | Version `2026-07-21`; a producer-local forwarder carries the simulator's native Rerun messages to the gateway and Recording Hub. |
| [NVIDIA Kit WebRTC](https://docs.omniverse.nvidia.com/kit/docs/omni.kit.livestream.webrtc/latest/Overview.html) | The persistent follow viewport is encoded once through NVIDIA NVENC and delivered as H.264 WebRTC. The browser client is pinned to `@nvidia/ov-web-rtc` `6.6.0`. |
| Cluster-private HTTP/JSON | Typed MCP-server-to-simulator adapter boundary. Simulator, MAVLink, ROS 2, and the private Kit signaling port never become public gateway routes. |

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

Headless startup authors Cesium's typed `IonOfficial` server prim before the
tileset and binds asset `2275207` to that server. Interactive Kit normally does
this from a stage-opened UI callback, but Isaac enables the extension after its
stage is open. The runtime then reloads Cesium's native stage registry and
feeds the active UAV camera through the native viewport contract on every Kit
update.

Cesium's streamed mesh is not the simulator's collision authority. Isaac adds
a bounded, invisible launch and landing surface at the typed local origin so a
PX4 vehicle has deterministic physical support before arming. This surface is
an initialization mechanism; Google Photorealistic 3D Tiles remain the core
rendered world and their product use is not narrowed by this contract.

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

## PX4 control link

The pod-local GCS link binds `14550 + instance` and seeds the matching PX4
endpoint at `18570 + instance`. This preserves one bidirectional MAVLink peer
for heartbeat, command acknowledgement, waypoint control, and vehicle state.
The adapter sends each WGS84 waypoint as `MAV_CMD_DO_REPOSITION` through
`COMMAND_INT` with `MAV_FRAME_GLOBAL_INT`, then verifies horizontal position,
absolute altitude, and the requested hold interval from PX4 telemetry. This
keeps an already-airborne vehicle out of PX4 AUTO Mission mode. The adapter
maintains a one-second GCS heartbeat while the vehicle is live. Commands fail
unless PX4 returns an explicit accepted acknowledgement. Arm completion also
requires the subsequent PX4 heartbeat to report the armed state, which makes
an immediate takeoff command deterministic. A land command interrupts an
active waypoint loop before acquiring the MAVLink command channel.

The HTTP adapter is concurrency-safe. A durable operation does not hold the MCP
server's adapter boundary, so task reads, readiness probes, state resources,
and emergency land commands remain available throughout a mission. The fake
adapter keeps its own narrow mutex because its in-memory state is mutable.

## Typed domain model

The controlled vocabulary includes:

- `SessionId`, `VehicleId`, `MissionId`, `RecordingId`, `StreamId`, and
  `FrameUri` validated identity types.
- `SimulationLifecycle`: `starting`, `ready`, `running`, `paused`, `stopping`,
  `stopped`, or `failed`.
- `TileLoadState`: `connecting`, `streaming`, `ready`, or `failed` with counts
  and a redacted diagnostic.
- `CameraLifecycle`: `warming`, `ready`, `degraded`, or `failed`, accompanied by
  frame count, mean luma, dynamic range, and non-black-pixel fraction measured
  from the exact RGB8 image delivered to the H.264 encoder.
- `LiveStreamCapability` fixes the public source to `follow_camera`, codec to
  `h264`, and hardware encoder to `nvidia_nvenc`. Resolution, frame rate,
  lifecycle, and connected-viewer count remain typed state.
- `LiveStreamState` records one owner-scoped lease without its credential.
  `LiveStreamConnection` adds the short-lived redacted access-token type only
  to open and renew results.
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
| `get_simulation_state` | Reads the current session, tile, camera-content, recording, and vehicle summary. |
| `pause_simulation` | Pauses one running session. |
| `resume_simulation` | Resumes one paused session. |
| `reset_simulation` | Resets the stage and vehicles to the declared scenario start. |
| `step_simulation` | Advances a paused session by a bounded number of physics steps. |
| `arm_vehicle` | Arms one PX4-backed vehicle after adapter safety checks. |
| `takeoff_vehicle` | Starts a bounded takeoff to a typed relative altitude. |
| `land_vehicle` | Commands one vehicle to land. |
| `open_live_stream` | Opens the single owner-scoped follow-camera lease and returns its authenticated WebRTC endpoint. |
| `renew_live_stream` | Extends the active owner-scoped lease without changing the credential used by the connected client. |
| `close_live_stream` | Revokes the lease and disconnects its authenticated signaling path. |

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
uav-sim://session/{session_id}/streams
uav-sim://session/{session_id}/stream/{stream_id}
uav-sim://mission/{mission_id}
uav-sim://usage
uav-sim://usage/task/{task_id}
ui://uav-sim/live.html
```

Session, world, tiles, vehicles, individual vehicles, mission, and recording
resources support subscriptions. Mutations publish resource-update
notifications after the adapter acknowledges the new state. Task usage reuses
the shared usage model. Stream list and item resources are filtered by the
complete caller owner, which includes principal, tenant, profile, data labels,
and invocation authority. They never serialize the WebRTC access token.

### Live follow-camera App

The App opens the same typed stream tools advertised to model clients. It reads
the session index once per second for low-rate flight context, renders the
NVIDIA WebRTC video in a view-only element, reports transport statistics, and
renews the lease halfway to expiry. Teardown terminates the client and closes
the lease before acknowledging the host. The App requires Media Capabilities
to report H.264 WebRTC decoding as supported and power-efficient before it
opens a lease; an unproven software-decoder path fails closed.

The source tree carries a diagnostic client stub for ordinary Rust tests. The
production MCP image downloads `@nvidia/ov-web-rtc` `6.6.0` from NVIDIA's
registry, verifies the tarball and UMD bundle hashes, and embeds that exact
bundle during the Rust build. NVIDIA client files are not copied into the
repository.

### Prompts and completions

`uav-sim-mission-plan` prepares a typed mission request against declared
vehicles and frames. `uav-sim-session-review` inspects tile, camera-content,
vehicle, collision, mission, recording, and task evidence. Completions resolve visible session,
vehicle, mission, and task identities without crossing tenant ownership.

## Recording integration

The simulator emits a nadir `camera/down` stream, camera-content health, poses,
transforms, IMU values, vehicle state, mission state, collision events, and
tile-loading diagnostics. Image up follows vehicle forward, which keeps aerial
recordings oriented with the flight path. The co-located recording adapter
converts these values into typed Rerun entities under:

```text
/world/uav-sim/{session_id}/...
```

It pushes to the private Recording Hub endpoint using the repository-pinned
Rerun SDK. Camera output is H.264 Annex B with simulation timestamps. The
encoder emits a decoder-reentrant IDR with repeated SPS/PPS once per second of
simulation time, which gives Perception's bounded recent-proxy capture a stable
preroll point under GPU load. The adapter reports only its private application
and recording keys. The UAV MCP resolves those keys through the platform
recording catalog and publishes only the canonical UUIDv7
`recording://recordings/{recording_id}` identity. That URI becomes the input to
Perception MCP. The runtime publishes video samples only when the measured RGB8
frame contains visible content. Runtime and MCP readiness require three
consecutive visible frames, and a camera that remains black for 30 seconds
after Google tiles become resident fails the simulation. Native Recording Hub
ports are not exposed publicly.

`TODO(GPU)` marks the remaining NumPy readback, camera-quality reductions, and
PyAV `libx264` recording path. They must converge on the canonical NVENC packet
fan-out when Recording Hub accepts pre-encoded packets. This debt does not sit
on the live-view path, which stays on the GPU from Kit rendering through
NVENC.

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

Simulator, MAVLink, ROS 2, and Kit's native signaling port remain pod-local.
The chart exposes only the authenticated signaling proxy over TCP and encrypted
WebRTC media over UDP. Local k3d maps those ports to `49101` and `47998`.
Fielded deployments set an exact public signaling URL and media host; an
optional Ingress terminates WSS while the media endpoint remains a directly
reachable UDP address. The live stream is the canonical browser path. Durable
recording remains a separate governed sensor product.

## Security

Gateway internal assertions are mandatory. Every task carries principal,
tenant, profile, and data-label ownership. The pod disables service-account
token mounting, drops Linux capabilities where the NVIDIA runtime permits it,
uses the Isaac image's non-root user, and accepts only Secret references for
credentials.

NetworkPolicy permits Cesium ion and required NVIDIA asset egress, internal
SurrealDB and recording traffic, the gateway-to-MCP path, authenticated
WebSocket signaling, and SRTP media. The signaling proxy admits one connected
viewer only after a constant-time comparison against the MCP-issued lease
token. Closing or expiry tears down signaling. The MCP App declares its exact
signaling origin through `_meta.ui.csp`; the Console rejects wildcard,
credential-bearing, path-bearing, or unsupported origins before constructing
the iframe CSP. The MCP server never proxies arbitrary URLs.

## Acceptance

Unit tests use a deterministic fake adapter. Helm and gateway checks are local
and credential-free. The live acceptance is a separately invoked, billed test
that requires `CESIUM_ION_ACCESS_TOKEN` and NVIDIA registry access.

The live test passes only when all of the following are true at once:

1. Isaac Sim, View, and Perception deployments remain available.
2. Cesium authenticates and Google Photorealistic 3D Tiles become resident in
   the Isaac stage.
3. Pegasus spawns a PX4-backed UAV against the declared frame.
4. The UAV arms, climbs to the 300 m aerial acceptance altitude, follows a
   bounded mission, and lands.
5. MCP resources, mutations, tasks, ownership, and notifications work through
   the gateway.
6. The typed follow-camera capability reports H.264 through NVIDIA NVENC, and
   an owner-scoped stream lease opens and closes through the gateway.
7. Camera, pose, telemetry, mission, and tile state reach Recording Hub.
8. Camera content passes the RGB8 luma, dynamic-range, and non-black-pixel
   acceptance gate before the session becomes ready.
9. Perception reads the governed H.264 recording and publishes derived output.
10. No credential appears in a rendered manifest, resource, task result, log,
   USD layer, or retained artifact.

Missing credentials, unavailable tiles, unsupported Pegasus APIs, PX4 startup
failure, frame disagreement, and recording failure all fail explicitly. No
older Isaac, Cesium, Pegasus, PX4, variable name, or protocol path is retained
as a fallback.
