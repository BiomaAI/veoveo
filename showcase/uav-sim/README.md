# Isaac Sim UAV showcase

This showcase runs a PX4-backed UAV over Google Photorealistic 3D Tiles in
Isaac Sim. Veoveo governs the simulation through a provider-neutral MCP server,
records typed evidence in Rerun, and serves one low-latency follow-camera view
encoded by NVIDIA NVENC.

## Components

| Path | Responsibility |
|---|---|
| `runtime/` | Isaac Sim, Cesium for Omniverse, Pegasus, PX4, RTX cameras, NVIDIA WebRTC, the private adapter, and Rerun publication. |
| `deploy/helm/` | One GPU-required interactive simulator pod, authenticated signaling and media services, durable cache and forwarder queues, and network policy. |
| `scenarios/` | Reusable frame-world trees and live acceptance parameters that remain outside the runtime image. |
| `../../servers/uav-sim-mcp/` | Typed MCP tools, resources, tasks, subscriptions, prompts, stream leases, and the live App. |

The public MCP identity is `uav-sim`. Provider names stay inside the adapter and
deployment.

## Runtime world binding

The pod starts in the `unconfigured` lifecycle. It proves CUDA and NVENC
availability, starts the private adapter, and waits without constructing a
stage.

The acceptance client then:

1. creates an empty world through Frames MCP;
2. publishes the complete ECEF-rooted frame tree as an immutable revision;
3. binds the UAV session to that revision and its `isaac-world` frame with
   `configure_world`;
4. waits for Isaac, Cesium, cameras, PX4, and recording to become ready.

The tree in `scenarios/new-york-aerial.json` contains the Earth ECEF root, a
Times Square ENU anchor, the Isaac stage, vehicle body, IMU, nadir camera, and
follow-camera frames. Static transforms travel with the revision. Live vehicle
and follow-camera transforms name their stream URI and entity path.

Helm supplies no origin or frame URI. The accepted revision is the only source
for the Cesium georeference, Pegasus global coordinates, local WGS84
conversion, mission revision guard, and recording metadata. A second,
different binding is rejected.

## Rendering and video

Isaac renders the follow viewport with `RaytracedLighting` on the assigned
NVIDIA GPU. Cesium asset `2275207` streams Google Photorealistic 3D Tiles into
that viewport. The runtime fails if CUDA, NVENC, required extensions, tile
residency, PX4, or visible camera content is unavailable.

Kit encodes the persistent follow camera once through NVIDIA NVENC as H.264
WebRTC. The authenticated signaling proxy admits one owner-scoped MCP lease.
The native Kit signaling port remains private.

The browser checks the exact stream configuration through Media Capabilities.
`supported && smooth` is required. `powerEfficient` selects the displayed
decode label:

- true: `hardware H.264 decode`;
- false: `software H.264 decode`.

Software decode is the repository's narrow browser playback exception. A
headed browser with hardware-backed high-performance WebGPU and WebGL remains
mandatory for visual acceptance. Server rendering and encoding never fall back
to the CPU.

The recording camera still contains explicitly marked GPU migration debt.
`TODO(GPU)` identifies NumPy readback, CPU camera-quality reductions, and the
PyAV `libx264` recording path. They must converge on NVENC packet fan-out; they
are not acceptance evidence for the live stream.

## Recording

The runtime publishes:

- vehicle poses, ENU and NED state, PX4 connection, battery, and collisions;
- IMU values and camera transforms;
- nadir H.264 camera samples with simulation timestamps;
- tile residency and camera-content diagnostics;
- mission lifecycle and the immutable world revision identity.

A producer-local forwarder carries Rerun messages through the gateway to
Recording Hub. Public resources expose only canonical
`recording://recordings/{recording_id}` identities.

## Configuration

The generic chart requires:

- a Secret containing `cesium-ion-access-token`;
- an exact public base URL and WebRTC signaling/media addresses;
- platform database and recording-forwarder credentials;
- `nvidia.com/gpu: 1` and the NVIDIA runtime class;
- pinned image digests in production.

Camera optics, mounts, follow offsets, physics cadence, rendering cadence, and
cache policy remain typed chart values. World coordinates do not.

The chart supports the MCP-configured interactive pod only. The former batch
Job was removed because it had no authenticated route for the required
write-once world binding.

## Verification

Run credential-free contract, adapter, and chart checks:

```sh
just showcase-uav-sim-test
helm lint showcase/uav-sim/deploy/helm
cargo test -p veoveo-smoke --bin smoke
```

The live acceptance command is installation-owned. It must point at
`showcase/uav-sim/scenarios/new-york-aerial.json` and a deployed gateway. The
test publishes the world, configures the session, verifies concurrent GPU
workloads, flies the mission, checks the live NVENC lease, and validates
Recording, Perception, and Reason results.

No screenshot or browser result counts unless the headed browser reports
hardware WebGPU and WebGL and the live `<video>` element visibly renders the
stream.
