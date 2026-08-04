# Isaac Sim UAV showcase

This first-party showcase runs a PX4-backed UAV fleet over Google
Photorealistic 3D Tiles in Isaac Sim. It demonstrates how a domain extension
uses Veoveo contracts without implementing platform camera or live-view
features itself.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Isaac Sim | Marketing release 6.0.1; internal build `6.0.1-rc.7+release.42383.32955d8d.gl`. |
| `veoveo.io/simulation-runtime-build-lock/v1` | Exact canonical base inputs, immutable overlay components, and NVIDIA runtime requirements. |
| `veoveo.io/simulation-conformance-result/v1` | Hardware result for one base and overlay digest. |
| `veoveo.io/simulation-view-pose/v1` | Complete newest-value UAV entity poses published through the reusable Python SDK. |
| SPIFFE and TLS 1.3 | Producer-only mutual authentication to private Simulation View pose ingress. |
| Rerun 0.35.0 RRD | Vehicle, transform, sensor, camera, mission, and simulation evidence. |
| NVIDIA CUDA, Vulkan, and RTX | Mandatory hardware simulation, domain sensor rendering, and camera capture. |
| MAVLink 2 | Pod-local PX4 command and telemetry transport. |
| OGC 3D Tiles | Cesium-streamed Google Photorealistic 3D Tiles. |

## Ownership

| Path | Responsibility |
|---|---|
| `../../platform/runtimes/simulation/` | Canonical Isaac Sim, Isaac Lab, Warp, Newton, CUDA, and RTX compatibility lineage. |
| `../../sdk/python/src/veoveo_mcp/simulation_pose.py` | Public pose encoding, framing, newest-value buffering, and TLS worker. |
| `../../platform/simulation/view-isaac/` | Independent renderer-only scene mirror, logical cameras, RTX products, NVENC, and WebRTC. |
| `../../servers/simulation-view-mcp/` | Simulation View sessions, scene declarations, camera capacity, streams, and live App. |
| `runtime/` | Thin Cesium, Pegasus, PX4, domain sensors, recording, private adapter, and UAV telemetry-to-pose mapping. |
| `deploy/helm/` | One GPU-required domain simulator pod, MCP sidecar, recording forwarder, private pose TLS, and NetworkPolicy. |
| `scenarios/` | Frame-world trees and domain acceptance parameters outside the image. |
| `../../servers/uav-sim-mcp/` | Domain tools, resources, tasks, subscriptions, prompts, recording references, and pose-publication health. |

The UAV extension does not contain operator cameras, NVIDIA AOV streaming,
WebRTC signaling, media exposure, stream leases, or an MCP App. Those
capabilities belong to Simulation View and run on its separate GPU workload.

## Canonical Simulation Base

The `simulation-runtime` Bake target is the shared Veoveo base. The
`2026.07.0` lineage pins Isaac Sim 6.0.1, Isaac Lab
`v3.0.0-beta2.patch1`, Warp 1.15.0, Newton 1.4.0, MuJoCo 3.10.0, MuJoCo
Warp 3.10.0.3, Python 3.12.13, CUDA 12.9, and Kit 110.1.2.

The base owns no UAV behavior. `uav-sim-runtime` derives a domain overlay that
adds Cesium, Pegasus, PX4, sensors, recording, and the private adapter without
replacing the locked tuple.

## World Binding

The pod starts `unconfigured` and waits for one immutable Frames world
revision. The acceptance client:

1. creates and publishes the complete ECEF-rooted frame tree;
2. binds the UAV session to that revision and its `isaac-world` frame;
3. waits for Isaac, Cesium, nadir cameras, PX4, recording, and pose delivery;
4. separately declares a render-only scene and cameras to Simulation View.

Helm supplies no origin or frame URI. The accepted revision determines the
Cesium georeference, Pegasus coordinates, local WGS84 conversion, mission
guard, recording metadata, and pose frame identity.

The scenario tree contains the ECEF root, Times Square ENU anchor, Isaac stage,
and the body, IMU, and nadir-sensor frames for every fleet vehicle. Operator
camera frames are not part of the domain simulator tree; Simulation View
camera rigs derive them from mirrored entity poses.

## Domain Rendering And Sensors

Isaac renders domain sensor products with `RaytracedLighting` on an assigned
NVIDIA GPU. Cesium asset `2275207` streams Google Photorealistic 3D Tiles.
The active Kit viewport follows the primary nadir sensor because Cesium uses
viewport state for tile selection.

Fleet dynamics use one CUDA-backed PhysX tensor view for every admitted body.
The reusable Warp host tensors are also the NumPy accumulator storage; each
physics step uploads those live force and torque values before clearing the
buffers. This ownership is deliberate because Warp 1.15 copies
`wp.from_numpy` input instead of retaining a shared view.

Nadir cameras are simulation sensors and governed recording inputs. They are
not live operator views. The runtime fails closed when NVIDIA rendering,
required extensions, tiles, PX4, recording, or visible sensor content is
unavailable.

The sensor encoder fails closed on PyAV's NVIDIA `h264_nvenc` implementation.
One Annex B packet stream feeds live Stream publication and Rerun recording
without a second encode. `TODO(GPU)` still identifies the NumPy readback and
CPU quality reduction that precede that encoder. Those paths must move to a
direct CUDA render-product handoff and cannot serve as rendering-quality
acceptance evidence.

## Pose Publication

`runtime/veoveo_uav_sim/pose.py` is domain glue over the public SDK. It:

- establishes stable `uav-1` through `uav-N` entity identities;
- binds snapshots to the exact Frames revision and renderer epoch;
- maps Pegasus ENU position and XYZW attitude into typed entity poses;
- selects complete snapshots from fixed physics steps at an exact 20 Hz;
- emits them on a bounded wall-clock queue independently of native camera rendering;
- reports publisher counters and lifecycle through domain state.

The adapter deliberately omits velocity. Pegasus exposes the available value
in world ENU, while the pose protocol's optional velocity is body FLU.

Sequence and renderer timestamp remain monotonic across a domain reset. A
500 ms producer queue absorbs the serialized Isaac/Cesium render boundary and
paces complete snapshots before they reach the SDK's newest-value transport.
The fixed-step scheduler owns real-time pacing; PX4 actuator replies are
consumed asynchronously instead of serializing one lockstep wait per vehicle.
Each physics step drains a bounded number of pending MAVLink packets, which
keeps current actuator controls ahead of lower-rate telemetry without letting
one vehicle monopolize Kit's simulation thread.
The SDK performs DNS, certificate loading, TLS handshakes, reconnection, and
socket writes on its worker thread. The cadence emitter acknowledges its first
snapshot before filling the newest-value slot. A disconnected Simulation View
never backpressures physics after that initial admission boundary.

The reference profile runs physics at 60 Hz, native nadir-camera rendering at
2 Hz, and Simulation View pose publication at 20 Hz. These clocks are separate
because four RTX/Cesium sensor products share Kit's render thread, while the
independent Simulation View renderer requires uniform authoritative poses.

## Always-On Fleet

The reference installation runs four vehicles. After PX4 connects, each
vehicle arms and enters a closed route derived from the immutable Times Square
ENU origin. The elongated circuit encloses Manhattan from the harbor to the
northern end of the island, keeping the cityscape inside the camera path.
Nested ellipses and separate altitudes keep the vehicles distinct. The loop
continues for the life of the simulator process, and a pod restart reconstructs
it from Helm configuration. The typed takeoff deadline covers the full climb to
the configured showcase altitude and fails the runtime if PX4 never completes it.
Each PX4 instance has a distinct pod-local GCS send and receive port pair, which
keeps command and telemetry heartbeats isolated during concurrent cold starts.

An explicit mission or direct flight command first claims every named vehicle.
That claim interrupts its default loop and waits for the MAVLink channel before
the requested command runs. The default loop stays retired for that vehicle;
vehicles not named by the command keep flying. This makes the fleet useful as
an always-on review source without allowing background control to compete with
operator intent.

## Recording

The runtime publishes vehicle poses, ENU and NED state, PX4 connection,
battery, collision counts, IMU samples, camera transforms, nadir H.264
samples, tile residency, and mission state as native Rerun messages.

A producer-local forwarder carries those messages to Recording Hub. Public
resources contain only canonical
`recording://recordings/{recording_id}` identities.

The domain acceptance starts a Stream live session before flight and sends each
newly encoded camera access unit directly to its admitted RTP/H.264 ingress.
Typed results must remain within the configured freshness bound while the
mission is flying. Recording Hub receives the sensor recording independently.
After the mission, Stream replay and Reason use an exact acknowledged source
snapshot and the preceding H.264 IDR for decoder preroll.

## Configuration

The chart requires:

- a Secret containing `cesium-ion-access-token`;
- a producer-only PEM Secret containing a client certificate, private key,
  and the Simulation View pose-ingress CA;
- exact producer, SPIFFE, epoch, endpoint, and entity-table identities;
- an explicit pose cadence and bounded producer buffer duration;
- bounded fleet-loop center offsets, radii, altitude, separation, waypoint
  count, and speed;
- platform database and recording-forwarder credentials;
- `nvidia.com/gpu: 1` and the NVIDIA runtime class;
- pinned image digests in production.

The runtime image receives the shared pose SDK as a named immutable build
context. The chart labels the pod as a Simulation View pose producer, mounts
the TLS Secret read-only, and exposes only the MCP Service. Public signaling
and media deployment belongs to the platform Simulation View component.

## Verification

Run credential-free checks:

```sh
cargo test -p veoveo-uav-sim-mcp --all-targets
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.5.1 --with pymavlink==2.4.49 --python python3 \
  python -m unittest discover -s showcase/uav-sim/runtime/tests -v
helm lint showcase/uav-sim/deploy/helm
cargo test -p veoveo-smoke --bin smoke
```

Build the canonical base and overlay:

```sh
cargo xtask image plan --group showcase-uav-sim-overlay-acceptance
cargo xtask image build --group showcase-uav-sim-overlay-acceptance
```

Release certification accepts only digest-addressed registry manifests with
SBOM and provenance attestations:

```sh
cargo xtask smoke simulation-certify \
  --deployment-lock "$DEPLOYMENT_LOCK" \
  --base-image "$BASE_REPOSITORY@$BASE_DIGEST" \
  --overlay-image "$OVERLAY_REPOSITORY@$OVERLAY_DIGEST" \
  --overlay-kind first-party-uav \
  --source-revision "$REVISION" \
  --output output/simulation-certification/first-party-uav.result.json
```

The overlay extends the canonical base `PYTHONPATH`; it does not duplicate platform or
Isaac Lab roots. Certification verifies that monotonic environment from the published
image configurations. The deployment lock authorizes one registry authority and its
transport for inspection and materialization, and the command preserves a sibling
transcript on success or failure.

The installation-owned live acceptance deploys the UAV simulator and
Simulation View independently. It verifies two GPU workloads, exact pose
delivery, scene mirroring, camera admission, RTX/NVENC streaming through the
Simulation View App, the Stream App with live encoded video and typed overlays,
domain recording, and mission completion.

Browser evidence is valid only after a headed browser proves hardware-backed
high-performance WebGPU or WebGL. The browser-side H.264 software-decode
exception does not relax either GPU workload.

The two live commands preserve the ownership boundary:

```sh
cargo xtask smoke uav-domain-verify \
  --context <kube-context> \
  --public-base-url https://installation.example
cargo xtask smoke simulation-view-verify \
  --context <kube-context> \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222
```

The first command owns UAV flight, tiles, PX4, domain sensors, direct live
Stream processing, independent Recording Hub evidence, replay, and Reason. The
second owns the anonymous-producer proof for Simulation View, camera capacity,
RTX/NVENC, WebRTC, and its generic App.
Neither command substitutes for the other.

When the independent Simulation View proof uses another local k3d profile on
the same workstation, stop that profile before the composed showcase. Each
k3d scheduler advertises the same host GPU independently. Concurrent local
GPU clusters can therefore overcommit it without either scheduler reporting
pressure. Keep every required workload enabled in the active profile.

Run the composed showcase only after both independent paths pass:

```sh
CHROME_PROFILE_DIR="${CHROME_PROFILE_DIR:-$HOME/.config/google-chrome-veoveo-dev}"
test -d "$CHROME_PROFILE_DIR"

google-chrome-stable \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9222 \
  --user-data-dir="$CHROME_PROFILE_DIR" \
  --window-size=1920,1080 \
  --ozone-platform=x11 \
  https://installation.example/console/

cargo xtask smoke uav-showcase-verify \
  --context <kube-context> \
  --public-base-url https://installation.example \
  --chrome-cdp-url http://127.0.0.1:9222
```

Chrome must be visible and authenticated through the installation's ordinary
Console login. The local development workstation keeps that session in
`$HOME/.config/google-chrome-veoveo-dev`; another workstation sets
`CHROME_PROFILE_DIR` to its existing authenticated profile. The directory
existence check is intentional because Chrome would otherwise create an empty,
logged-out profile. A temporary or newly created profile is not an acceptable
substitute for the operator's authenticated session. Do not force a particular
ANGLE or WebGPU backend: acceptance probes both APIs and requires hardware-backed
WebGPU or WebGL. `--chrome-cdp-url` accepts the HTTP discovery origin shown above
or Chrome's direct local `ws://` browser endpoint when runtime debugging does
not expose `/json/version`. Both paths query `Browser.getVersion` before the
in-page hardware checks. The composed command asks the UAV server for its
governed, digest-bound scene and pose identity, then asks Simulation View to
bind that scene and admit one follow camera. The UAV server still owns no
camera, renderer, stream, or App.

The run verifies the actual Console at takeoff, during the mission, and after
landing. Each checkpoint requires an advancing pose sequence and a healthy
640-by-360 NVIDIA NVENC H.264 stream. During the mission it opens the Stream
MCP App and requires advancing direct-live H.264, fresh typed results, the
overlay canvas, and an exact Media Capabilities decode result. It then opens
the same flight's governed recording in the Console Rerun viewer. Recording
acceptance requires a successful manifest response, scoped Redap responses,
successful `WhoAmI`, `FindEntries`, `ReadDatasetEntry`, `GetRrdManifest`, and
`GetSegmentTableSchema` calls, zero legacy archive-shard requests, and measured
nonblank plots, imagery, or spatial content inside the Rerun viewport. Chrome
may report a redundant request as a canceled `net::ERR_ABORTED` only when the
same Redap path also completed successfully during the capture. The evidence
records that supersession separately; every other network abort remains a
playback failure. Headless Chrome, a browser with
neither hardware WebGPU nor hardware WebGL, missing Console APIs, a synthetic
App host, a static frame, stale Stream results, an archive loading surface, or
a software-renderer warning from the active visual surface fails the run.

Evidence is written beneath
`output/acceptance/uav/{source-revision}/{run-id}/`. The typed
`veoveo.io/uav-showcase-acceptance-evidence/v2` manifest records the scene,
producer, camera, Stream session, recording, pose sequences, flight states,
hardware identities, decode results, Redap request counts, viewport render
measurements, screenshot paths, and SHA-256 image digests. The directory is a
run artifact and remains outside source control.
