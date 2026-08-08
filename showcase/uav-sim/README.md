# Isaac Sim UAV showcase

This first-party showcase runs a four-vehicle PX4 fleet over streamed
photorealistic 3D Tiles in Isaac Sim. The fleet follows an always-on route until a
mission claims a vehicle. The same authoritative runtime renders operator cameras and
publishes their NVIDIA NVENC products to the governed live-view App.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Isaac Sim | Marketing release `6.0.1`; internal build `6.0.1-rc.7+release.42383.32955d8d.gl`. |
| `veoveo.io/simulation-runtime-build-lock/v1` | Exact base inputs, immutable overlay components, and NVIDIA runtime requirements. |
| `veoveo.io/live-view/v2` | Authoritative operator cameras, stable encoded products, ephemeral viewer leases, and typed capacity. |
| `veoveo.io/uav-runtime-event/v1` | Private pod-local Unix datagram with an `adapter_ready` edge for immutable world-binding reapplication and a final `ready` edge for live-camera recovery. |
| WebRTC and H.264 | One isolated native Omniverse WebRTC and NVIDIA NVENC product per active viewer lease. |
| Native sensor video | `omni.kit.livestream.aov` `10.2.0` and `omni.kit.livestream.rtsp` `10.2.3`, packaged by Isaac Sim `6.0.1`, for CUDA-AOV-to-NVENC H.264 output. |
| RTSP, RTP, and H.264 | Pod-local RTSP 1.0 with interleaved RTP/RTCP and RFC 6184 single-NAL, STAP-A, and FU-A packetization. |
| Rerun RRD | Version `0.35.0` telemetry, leader-camera video, and producer Blueprint publication. |
| NVIDIA CUDA, Vulkan, RTX, and NVENC | Mandatory simulation, rendering, and server-side video encoding. |
| MAVLink 2 and ROS 2 Jazzy | Pod-local PX4 command, telemetry, and simulator integration. |
| OGC 3D Tiles | Cesium-streamed photorealistic terrain and buildings. |
| WGS 84, ECEF, ENU, NED, and FLU | Explicit Frames-governed world, physics, entity, sensor, and operator-camera mappings. |

## Ownership

| Path | Responsibility |
|---|---|
| `../../platform/runtimes/simulation/` | Canonical Isaac, Isaac Lab, Warp, Newton, CUDA, and RTX lineage. |
| `runtime/` | Cesium, Pegasus, PX4, fleet physics, domain sensors, authoritative operator cameras, Hydra/NVENC products, recording, and the pod-local adapter. |
| `../../servers/uav-sim-mcp/` | Domain tools, resources, tasks, subscriptions, camera/product projection, viewer leases, signaling, audit, and the live App. |
| `deploy/helm/` | One GPU-required simulator pod, Rust MCP companion, recording forwarder, stable media ports, cache, and NetworkPolicy. |
| `scenarios/` | Installation-independent Frames trees and acceptance parameters. |

There is one stage, one Cesium world, one runtime cache, and one GPU allocation. No
visualization process mirrors entity poses or rebuilds the scene.

## Canonical Runtime

The `simulation-runtime` Bake target is the shared base. The UAV overlay adds the domain
runtime without replacing the lock. An installation may bind the pod to one immutable
Frames world revision through a read-only ConfigMap. The MCP companion validates and
applies that document once during startup, before it admits users. Missing, malformed,
cross-revision, or conflicting bindings fail startup directly. No controller polls or
replays renderer state. An installation that omits the ConfigMap admits the same binding
once through `configure_world`.

The selected revision determines the Cesium georeference, Pegasus coordinates, local
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

Fleet dynamics use CUDA-backed PhysX tensor views. The fixed-step clock advances at most
one physics interval per scheduler pass; a missed wall-clock deadline slows simulation
instead of replaying stale actuator commands. Rerun serialization, browser traffic, and
recording retries run outside that authority boundary.

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
`/World/OperatorCameras` and a separate preallocated viewer-slot pool under
`/World/OperatorViewerCameras`. Supported rigs are fixed, look-at, orbit, follow, chase,
stabilized mounted, and formation overview. Every assigned viewer slot owns its camera
clone, Hydra texture, product ID, native signaling port, UDP media port, RTX render, and
NVENC session.

The camera update reads current authoritative entity transforms directly. Its
frame-rate-independent filter smooths only the final operator-camera position and
orientation. Target changes, camera revisions, simulation generations, long gaps, and
teleports reset the filter. No entity-pose history or visualization interpolation exists.

Unassigned viewer products remain inactive. Assignment copies the chosen logical pose
into one slot, enables its Hydra texture, and completes only after a drawable event
proves the first assigned GPU frame and the native signaling socket is listening. The
adapter uses a bounded event wait and releases the slot on activation failure. Multiple
users, profiles, or tabs receive independent leases, camera clones, RTX renders, NVENC
sessions, and peer state even when they select the same logical camera.

The live App runs in an opaque-origin sandbox. It disables the pinned NVIDIA browser
client's optional Compute Pressure telemetry before client initialization because that
browser API is unavailable to an opaque origin. RTX rendering, NVENC encoding, and the
native WebRTC data plane remain unchanged.

The qualified one-GPU profile targets 16 FPS for each of two simultaneous 1280×720
viewer products. Acceptance requires at least 12 delivered FPS, source-to-render p95
below 85 ms after the 256-event warm window, and a conservative composed
motion-to-photon upper bound below 250 ms. These are measured product limits rather
than adaptive downgrade rules; admission never rewrites a camera's requested optics or
codec.

After a simulator restart, the runtime emits a nonblocking `adapter_ready` edge when its
preconfiguration endpoint can accept the immutable installation binding. The MCP
companion reapplies that binding and waits for the runtime's final `ready` edge, emitted
after physics, the streamed world, and logical cameras are current. The companion turns
the final edge into a live-camera resource notification, and selected App tiles reconnect
with fresh leases. No browser, MCP server, or missing datagram consumer can delay the
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

One bounded recording contains four-vehicle poses, velocities, geographic positions,
IMU values, changing health state, and leader video. The producer Blueprint opens Fleet
3D, Leader camera, and Fleet map views. Installation-owned browser map credentials never
enter RRD bytes or Blueprint metadata.

Recording publication uses a bounded worker queue. Queue pressure may shed recording
observations according to policy, but it cannot delay physics, PX4, or operator rendering.
The producer-local forwarder moves batches to Recording Hub and can recover its own
durable queue independently.

## Configuration

The chart requires:

- one installation Secret containing the streamed-world provider credential;
- one immutable Frames world and simulation frame, with an optional installation-owned
  startup binding ConfigMap for restart-stable always-on operation;
- bounded fleet route, takeoff, vehicle, and PX4 parameters;
- a strict logical operator-camera collection and a bounded physical viewer-slot pool;
- stable native signaling and public media port ranges;
- bounded live-view lease, viewer, frame-age, and product-activation limits;
- a leader identity and bounded recording cadence and queue;
- platform database and recording-forwarder credentials;
- `nvidia.com/gpu: 1`, the NVIDIA runtime class, and driver capabilities
  `compute,graphics,utility,video`;
- digest-pinned images in production.

The public signaling URL is credential-free. Provider credentials are mounted from a
Secret and never enter ConfigMaps, MCP resources, logs, recordings, or evidence.

`liveView.mediaService.nodePortBase` is required when an installation maps a fixed
public UDP range through Kubernetes NodePorts. The chart assigns one contiguous
NodePort per physical viewer slot and rejects a range that exceeds the Kubernetes
NodePort boundary. A normal LoadBalancer installation leaves the value null and uses
allocator-owned NodePorts.

## Credential-Free Verification

```sh
cargo test -p veoveo-uav-sim-mcp --all-targets
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.5.1 --with pymavlink==2.4.49 --python python3 \
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
acceptance proves the always-on fleet, authoritative camera health, one isolated product
per active viewer, RTX/NVENC/WebRTC playback, simultaneous same-camera viewer isolation,
one-App multi-camera grid isolation, sensor separation, simulation real-time factor,
source-to-render latency, and browser motion-to-photon latency. Stream, Recording, and
mission acceptance remain independent consumer checkpoints.

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
headed App document and viewer identity, fresh lease IDs, advancing video, hardware
graphics proof, screenshots, and digests.
