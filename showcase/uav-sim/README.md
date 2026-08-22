# Isaac Sim UAV showcase

This first-party showcase runs a four-vehicle PX4 fleet over streamed
photorealistic 3D Tiles in Isaac Sim. The fleet follows an always-on route until a
mission claims a vehicle. The same authoritative runtime renders operator cameras and
publishes their NVIDIA NVENC products to the governed live-view App.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Isaac Sim | Marketing release `6.0.1`; internal build `6.0.1-rc.7+release.42383.32955d8d.gl`. |
| Isaac Experimental API and Newton | Experimental prims and objects drive one Newton `1.5.0` rigid-body fleet on CUDA. The classic Core API and PhysX UAV path are absent. |
| Warp and MuJoCo | Warp `1.16.0`, MuJoCo `3.11.0`, and MuJoCo Warp `3.11.0` are one certified simulation tuple. Repository Warp kernels own the UAV plant and HIL sensors. |
| `veoveo.io/simulation-runtime-build-lock/v1` | Exact base inputs, immutable overlay components, and NVIDIA runtime requirements. |
| `veoveo.io/live-view/v3` | Authoritative operator cameras, stable camera-owned encoded products, ephemeral viewer authorizations, and typed GPU capacity. |
| `veoveo.io/uav-runtime-event/v2` | Private authenticated HTTP/1.1 NDJSON stream with an `adapter_ready` edge for immutable world-binding reapplication and a final `ready` edge for live-camera recovery. |
| WebSocket and H.264 | One continuous NVIDIA NVENC product per camera, delivered as Annex B H.264 access units to every authenticated viewer. |
| Native sensor video | `omni.kit.livestream.aov` `10.2.0` and `omni.kit.livestream.rtsp` `10.2.3`, packaged by Isaac Sim `6.0.1`, for CUDA-AOV-to-NVENC H.264 output. |
| RTSP, RTP, and H.264 | Pod-local RTSP 1.0 with interleaved RTP/RTCP and RFC 6184 single-NAL, STAP-A, and FU-A packetization. |
| Rerun RRD | Version `0.36.0` telemetry, leader-camera video, and producer Blueprint publication. |
| NVIDIA CUDA, Vulkan, RTX, and NVENC | Mandatory simulation, rendering, and server-side video encoding. |
| MAVLink 2 | Pod-local PX4 `1.17.0` command, telemetry, actuator, and HIL sensor integration. |
| OGC 3D Tiles | Cesium Omniverse `0.29.0` and its pinned Cesium Native revision stream photorealistic terrain and buildings. A repository-owned internal event extension reports redacted load lifecycle state. |
| WGS 84, ECEF, ENU, NED, and FLU | Explicit Frames-governed world, physics, entity, sensor, and operator-camera mappings. |

## Supporting Research

Gao, Pagnucco, Bednarz, and Song survey Isaac Sim's architecture, GPU-accelerated
physics and rendering, synthetic-data pipeline, application patterns, and practical
limitations in [*NVIDIA Isaac Sim: Enabling Scalable, GPU-Accelerated Simulation for
Robotics*](https://arxiv.org/html/2606.03551v1), arXiv:2606.03551v1 (2026). The survey
provides background for this showcase's choice of an authoritative GPU simulator. It
does not validate Veoveo's PX4, Warp plant, MCP, streaming, or acceptance
implementation; those claims remain tied to the repository's pinned runtime and
executable evidence.

## What Can This Do?

Address one vehicle's pilot as a durable agent and give it a destination in ordinary
language:

> Fly uav-1 to Times Square now. Read your active UAV control grant, ask Map MCP to
> resolve and route this named location from current telemetry, then use UAV MCP to admit
> and execute the mission only for your bound vehicle. Report the terminal result.

<p align="center">
  <a href="assets/uav-e2e-001-flight-timelapse.mp4">
    <img src="assets/uav-e2e-001-flight-timelapse.gif" width="640" alt="Recorded downward camera view from uav-1 crossing New York during its mission from the Statue of Liberty area to Times Square">
  </a>
</p>

*This 26.2-second replay uses the actual 640×480 leader camera sampled at 2 fps and
accelerated 30×. Open the [H.264 MP4](assets/uav-e2e-001-flight-timelapse.mp4) for
the full-quality recording.*

The pilot does not invent coordinates or acquire authority from the prompt. It reads its
active vehicle-control grant, asks Map MCP to resolve the place and build the route, then
hands that governed route to UAV Simulation MCP. The UAV server checks the authenticated
pilot, exact vehicle, mobility profile, and world revision before it acquires an exclusive
command lease and sends the mission to the PX4-backed runtime.

The first deployed run flew `uav-1` 9.227 km from the Statue of Liberty area to Times
Square. It completed all four admitted waypoints in 13 minutes 10 seconds, arrived at
40.7580° N, 73.9855° W, and released its command lease. The task survived an MCP
credential rotation without replaying mission execution. The signed-in Console and the
headless conversation projection returned the same durable terminal result.

The leader camera recorded the flight throughout a database outage. Its durable
forwarder retained the pending batches, and Recording Hub materialized the complete
mission interval after service recovery. The replay above contains 1,538 archived camera
samples selected from the mission's exact 4,930.8-5,720.8 second simulation interval.

[![The signed-in Console showing uav-1-pilot's completed Times Square mission](assets/uav-e2e-001-console-complete.png)](assets/uav-e2e-001-console-complete.png)

*The actual signed-in Console result from the first accepted run. Open the image to inspect
the pilot identity, terminal position, PX4 state, collision count, recording reference,
and durable wake receipt.*

The repeatable evidence contract is
[`UAV-E2E-001: Per-Agent Named-Location Mission E2E`](ACCEPTANCE.md#uav-e2e-001-per-agent-named-location-mission-e2e).
It names the prerequisites, expected MCP sequence, binding proof, timing model,
headless requests, pass criteria, and evidence record for another run.

## Ownership

| Path | Responsibility |
|---|---|
| `../../platform/runtimes/simulation/` | Canonical Isaac, Isaac Lab, Warp, Newton, CUDA, and RTX lineage. |
| `runtime/` | Cesium, the repository-owned Warp UAV plant, PX4 HIL, Newton fleet state, domain sensors, authoritative operator cameras, Hydra/NVENC products, recording, and the cluster-private adapter. |
| `../../servers/uav-sim-mcp/` | Domain tools, resources, tasks, subscriptions, camera/product projection, stream authorization, WebSocket delivery, audit, and the live App. |
| `agents/` | Reviewed showcase packaging for isolated generic pilot agents. |
| `map/` | Map-owned named places and operational air-network fixture used by the showcase. |
| `deploy/helm/` | Independent GPU runtime and MCP Deployments, isolated agent Deployments, recording forwarder, shared-stream ingress, cache, and NetworkPolicy. |
| `scenarios/` | Installation-independent Frames trees and acceptance parameters. |

There is one stage, one Cesium world, one runtime cache, and one GPU allocation. No
visualization process mirrors entity poses or rebuilds the scene.

## Pilot Agents And Vehicle Binding

The reference installation runs four generic agent-kernel processes. Each process has a
distinct OAuth client, private signing key, persistent data volume, and reviewed manifest.
The manifest requests one vehicle id, but that value carries no authority. UAV Simulation
MCP binds the authenticated principal to one session and vehicle with an explicit control
grant, admits only Map-owned route handoffs against the grant's mobility profile and the
session's Frames revision, and holds an exclusive vehicle command lease during execution.

The live UAV App reads its exact agent choices from Apps resource metadata and submits
operator text through the Console's generic authenticated message bridge. The iframe
receives no agent credential. Headless users use the same actor-attributed agent message
API, while each pilot wakes from its own durable queue and talks to Map, Time, and UAV
Simulation MCP through the generic `agent` gateway profile.

Coordination remains an optional composition outside vehicle authority. A human or
headless client may submit related instructions to several exact pilot targets through
the generic agent message API. Each message enters a separate actor-attributed
conversation, and each pilot still needs its own grant, Map admission, and command lease.
The reference installation does not deploy a privileged fleet coordinator.

## Canonical Runtime

The `simulation-runtime` Bake target is the shared base. The UAV overlay adds the domain
runtime without replacing the lock. An installation may bind the pod to one immutable
Frames world revision through a read-only ConfigMap. The MCP companion validates and
applies that document once during startup, before it admits users. Missing, malformed,
cross-revision, or conflicting bindings fail startup directly. No controller polls or
replays renderer state. An installation that omits the ConfigMap admits the same binding
once through `configure_world`.

The selected revision determines the Cesium georeference, Newton fleet coordinates, local
geographic conversion, mission guard, recording metadata, sensor frames, and
operator-camera world.

The stage uses `RaytracedLighting` and the pinned Cesium extension. The headless runtime
is the sole owner of Cesium's active viewport list. It submits every active domain
sensor and operator camera during the same Kit update; the extension's interactive
viewport-window callback is disabled for this process because an empty window inventory
would otherwise erase those authoritative viewports between frames. The runtime does
not create another provider connection or tile cache for live views.

Moving cameras use hole-free tile refinement. Cesium retains a loaded parent until its
replacement children are ready, while ancestor and sibling preloading keep the next
camera footprint warm. The chart admits 20 concurrent tile loads by default and keeps
the decoded cache bounded. A fast nadir camera therefore sees lower-detail coverage
during refinement instead of the renderer clear color.

The image builds the exact Cesium Omniverse `0.29.0` source commit and exact upstream
Cesium Native submodule revision in the digest-pinned upstream builder. Two reviewed
patches add child-content failure delivery, load generations, query-secret log redaction,
and a fresh ion endpoint bootstrap for every native tileset generation. Existing
material, viewport-authority, and vendor-install patches apply to the resulting extension
package. No runtime download or locally rebuilt installation is accepted.

Native message-bus events drive streamed-world recovery. A tile-content HTTP 400 marks
the current provider generation rejected. The runtime keeps its resident geometry and
materials mounted while a separately named tileset obtains a fresh root session. The
endpoint bootstrap bypasses Cesium's in-memory and SQLite endpoint responses, while the
persistent tile-content cache stays intact. Hundreds of child failures from the rejected
generation remain one replacement action. Native load completion begins validation; it
does not retire the resident tileset. Loaded material and geometry growth plus stable
rendered coverage prove the replacement before promotion. A failed or unregistered
replacement is removed after two minutes and settles in a typed degraded state. Other
HTTP and transport failures never trigger speculative replacement. An isolated child
transport or provider error remains recorded without withdrawing resident textured
coverage. Loss of rendered geometry or loaded materials still removes visual readiness;
Cesium can restore it by producing stable textured coverage again.

The runtime projects `provider_generation`, `event_sequence`, `refresh_count`, loaded and
rendered geometry counts, loaded material count, and a typed `last_failure` without a URL
or credential. Visibility counters remain render observations. They never infer network
failure, poll a provider, or stop simulation. Existing operator streams retain the
resident generation while its replacement loads. Kubernetes visual readiness remains
current during that overlap only while textured resident coverage is still present.

PX4 remains the autopilot, estimator, mission executor, and MAVLink authority. Veoveo
owns the vehicle asset, rotor model, aerodynamics, coordinate transforms, HIL sensor
model, and process lifecycle. Every PX4 instance runs against an isolated writable root
and one dedicated concurrent HIL transport.

One Experimental `RigidPrim` resolves the whole fleet. A single Warp launch updates all
motor states and body wrenches on CUDA, then a second launch samples every HIL sensor.
The plant consumes the pinned Isaac 6.0.1 Newton view's cached transform and velocity
tensors directly, preserving the backend's native `xyzw` layout without cloned gathers.
The only per-step device readback is one compact 30-float packet per vehicle for MAVLink.
Controls cross back as one four-float packet per vehicle. Isaac's classic `World`, classic
prim views, the PhysX UAV path, NumPy dynamics, and per-vehicle physics loops are absent.
Sensor publication enters a bounded per-PX4 queue without waiting for transport or
actuator arrival. Each worker preserves frame order, the next fixed step consumes the
latest PX4 controls, and queue overflow is terminal instead of silently dropping HIL
state or blocking the GPU simulation loop.

The Isaac timeline remains playing because Newton gates integration on that state, while
app-driven physics stepping is disabled. `SimulationManager` alone advances Newton at the
exact 30 Hz fixed cadence. Each state sample produces two ordered PX4 HIL frames, which
keeps the estimator's IMU transport at 60 Hz while barometer and magnetometer fields
update at 30 Hz and GPS fields update at 10 Hz. Elapsed monotonic time determines the
number of authoritative steps due on each scheduler pass. The clock retains bounded debt
instead of dropping elapsed time. When rendering misses visual
deadlines, the runtime advances every due physics step and renders only the newest state.
Rerun serialization, browser traffic, native encode, and recording retries remain outside
that authority boundary and cannot slow the simulation timeline.

## Always-On Fleet

The reference configuration launches four vehicles. After PX4 connects, every vehicle
arms and enters a closed route around the configured city anchor. Nested paths and
separate altitudes keep the vehicles distinct. The loop continues for the lifetime of
the process.

A mission command claims each named vehicle before control begins. That claim retires
the background loop for the vehicle. Unnamed vehicles keep flying. A pod restart
reconstructs the default route from immutable configuration.

## Operator Cameras

At startup the runtime creates a bounded logical-camera set under
`/World/OperatorCameras`. Supported rigs are fixed, look-at, orbit, follow, chase,
stabilized mounted, and formation overview. Every streamable camera owns one Hydra
texture, product ID, pod-loopback RTSP port pair, RTX render, NVENC session, and shared
H.264 access-unit ring.

The camera update reads current authoritative entity transforms directly. Its
frame-rate-independent filter smooths only the final operator-camera position and
orientation. Target changes, camera revisions, simulation generations, long gaps, and
teleports reset the filter. No entity-pose history or visualization interpolation exists.

Each streamable logical camera owns a continuous Hydra texture, RTX render, NVENC session,
and keyframe-aware H.264 ring. Multiple users, profiles, or tabs receive independent
authorizations and cursors while sharing that exact camera product. Viewer count does not
change GPU product count.

The live App runs in an opaque-origin sandbox and decodes the shared Annex B stream with
WebCodecs. RTX rendering and NVENC encoding remain inside the authoritative simulator.

The qualified one-GPU profile targets 16 FPS for each 1280×720 camera product. Acceptance
opens all five cameras for five simultaneous browser users and requires at least 12
delivered FPS, source-to-render p95
below 85 ms after the 256-event warm window, and a conservative composed
motion-to-photon upper bound below 250 ms. These are measured product limits rather
than adaptive downgrade rules; admission never rewrites a camera's requested optics or
codec.

After a simulator restart, the runtime retains a nonblocking `adapter_ready` edge when
its preconfiguration endpoint can accept the immutable installation binding. The MCP
server receives that edge on an authenticated HTTP stream, reapplies the binding, and
waits for the runtime's final `ready` edge, emitted
after physics, the streamed world, and logical cameras are current. The companion turns
the final edge into a live-camera resource notification, and selected App tiles reconnect
with fresh authorizations. No browser, MCP server, or disconnected event consumer can delay the
simulation loop.

## Domain Sensor And Recording

Only the leader owns the admitted nadir sensor and recorded H.264 source. Followers emit
telemetry without duplicating camera capture. Its root-level USD camera receives the
exact authoritative body-and-mount transform without operator smoothing. Cesium receives
the physical sensor viewport on every Kit update. Its Hydra product renders at the
declared sensor rate and transfers the CUDA-resident `LdrColor` AOV directly into Isaac's
native RTSP/NVENC extension. Replicator orchestration and CPU pixel capture are absent
from this path.

The runtime consumes the pod-local encoded RTSP/RTP stream. It depacketizes H.264 without
decoding or re-encoding, qualifies normal GOP access units, and fans those same bytes to
Rerun and the optional live RTP publisher. SPS and PPS from SDP or the stream are attached
to IDR boundaries when needed for independent Recording shards. Runtime state reports the
declared rate, observed access-unit count, last encoded size, and keyframe state. There is
no PyAV encoder, duplicate encode, or GPU-to-CPU pixel readback.

Recording and live RTP publication own separate bounded, nonblocking queues. Recording
reconnects therefore cannot reset the live publisher's SSRC, RTP sequence origin, or
timestamp origin. A live transport failure sheds only that consumer's queued access units;
it never delays simulation, native rendering, NVENC, or governed Recording publication.

One bounded recording contains four-vehicle poses, velocities, geographic positions,
IMU values, changing health state, and leader video. The producer Blueprint opens Fleet
3D, Leader camera, and Fleet map views. Installation-owned browser map credentials never
enter RRD bytes or Blueprint metadata.

Recording publication uses its own bounded worker queue. Queue pressure may shed
recording observations according to policy, but it cannot delay physics, PX4, operator
rendering, or the optional live RTP path. The producer-local forwarder moves batches to
Recording Hub and can recover its own durable queue independently.

## Configuration

The chart requires:

- one installation Secret containing the streamed-world provider credential;
- one installation Secret containing the private adapter bearer token;
- one immutable Frames world and simulation frame, with an optional installation-owned
  startup binding ConfigMap for restart-stable always-on operation;
- bounded fleet route, takeoff, vehicle, and PX4 parameters;
- a strict logical operator-camera collection and one continuous product per camera;
- pod-loopback RTSP port pairs and one authenticated public WebSocket route;
- bounded stream authorization and frame-age limits without a viewer quota;
- a leader identity and bounded recording cadence and queue;
- platform database and recording-forwarder credentials;
- `nvidia.com/gpu: 1`, the NVIDIA runtime class, and driver capabilities
  `compute,graphics,utility,video`;
- digest-pinned images in production.

The public stream URL is credential-free. Provider and adapter credentials are
mounted from distinct Secrets and never enter ConfigMaps, MCP resources, logs,
recordings, or evidence.

## Credential-Free Verification

```sh
cargo test -p veoveo-uav-sim-mcp --all-targets
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.3.1 --with aiohttp==3.14.1 \
  --with pymavlink==2.4.49 --with fastcrc==0.3.6 --python 3.13 \
  python -m unittest discover -s showcase/uav-sim/runtime/tests -v
helm lint showcase/uav-sim/deploy/helm
cargo test -p veoveo-smoke --bin smoke
```

Build the canonical base and overlay through the repository image graph:

```sh
cargo xtask image plan --group showcase-uav-sim-overlay-acceptance
cargo xtask image build --group showcase-uav-sim-overlay-acceptance
```

Release certification accepts only digest-addressed images with the coordinated lock,
SBOM, and provenance.

## Hardware Acceptance

The installation-owned acceptance deploys one simulator GPU workload. Live-view
acceptance proves the always-on fleet, authoritative camera health, one shared product
per camera, RTX/NVENC/WebCodecs playback, five users with all five camera streams,
same-camera fan-out, sensor separation, simulation real-time factor,
source-to-render latency, and browser motion-to-photon latency. Stream, Recording, and
mission acceptance remain independent consumer checkpoints.

Named-location mission acceptance follows
[`UAV-E2E-001`](ACCEPTANCE.md#uav-e2e-001-per-agent-named-location-mission-e2e).
That functional test remains independent of the live-view performance commands below.

```sh
cargo xtask smoke uav-showcase-up \
  --context <kube-context> \
  --public-base-url https://installation.example

cargo xtask smoke uav-showcase-browser-verify \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222

cargo xtask smoke uav-showcase-live-restart-verify \
  --context <kube-context> \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222
```

Chrome must be visible and authenticated through the ordinary Console login. Acceptance
probes high-performance WebGPU and WebGL and fails when neither is hardware-backed.
SwiftShader, llvmpipe, headless capture, a static frame, software server rendering, or a
software encoder cannot satisfy the visual gate. Browser H.264 software decode remains
allowed only when Media Capabilities reports the exact stream as supported and smooth,
and the UI labels that path explicitly.

Evidence is written beneath
`output/acceptance/uav-browser/{source-revision}/{run-id}/` and stays outside source
control. The manifest records camera and product identity, viewer isolation, frame
advancement, a simultaneous multi-camera grid, source-to-render and motion-to-photon p95,
simulation and sensor isolation, browser hardware, decode identity, screenshots, and
SHA-256 evidence digests.

Restart evidence is written beneath
`output/acceptance/uav-live-restart/{source-revision}/{run-id}/`. It records the exact
pod, immutable image, before-and-after container IDs and restart counts, the unchanged
headed App document and viewer identity, fresh authorization IDs, advancing video, hardware
graphics proof, screenshots, and digests.
