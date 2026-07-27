# Isaac Sim UAV showcase

This first-party showcase runs PX4-backed UAV dynamics over Google
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
vehicle body, IMU, and nadir sensor. Operator camera frames are not part of the
domain simulator tree; Simulation View camera rigs derive them from mirrored
entity poses.

## Domain Rendering And Sensors

Isaac renders domain sensor products with `RaytracedLighting` on an assigned
NVIDIA GPU. Cesium asset `2275207` streams Google Photorealistic 3D Tiles.
The active Kit viewport follows the primary nadir sensor because Cesium uses
viewport state for tile selection.

Nadir cameras are simulation sensors and governed recording inputs. They are
not live operator views. The runtime fails closed when NVIDIA rendering,
required extensions, tiles, PX4, recording, or visible sensor content is
unavailable.

The remaining sensor pipeline has explicit GPU migration debt.
`TODO(GPU)` identifies NumPy readback, CPU quality reductions, and PyAV
`libx264` recording. These paths must converge on a canonical CUDA/NVENC
recording fan-out and cannot serve as visual acceptance evidence.

## Pose Publication

`runtime/veoveo_uav_sim/pose.py` is domain glue over the public SDK. It:

- establishes stable `uav-1` through `uav-N` entity identities;
- binds snapshots to the exact Frames revision and renderer epoch;
- maps Pegasus ENU position and XYZW attitude into typed entity poses;
- offers one complete snapshot at render cadence;
- reports publisher counters and lifecycle through domain state.

The adapter deliberately omits velocity. Pegasus exposes the available value
in world ENU, while the pose protocol's optional velocity is body FLU.

Sequence and renderer timestamp remain monotonic across a domain reset. The
SDK keeps only the newest unsent snapshot and performs DNS, certificate
loading, TLS handshakes, reconnection, and socket writes on its worker thread.
A disconnected Simulation View never backpressures physics.

## Recording

The runtime publishes vehicle poses, ENU and NED state, PX4 connection,
battery, collision counts, IMU samples, camera transforms, nadir H.264
samples, tile residency, and mission state as native Rerun messages.

A producer-local forwarder carries those messages to Recording Hub. Public
resources contain only canonical
`recording://recordings/{recording_id}` identities.

## Configuration

The chart requires:

- a Secret containing `cesium-ion-access-token`;
- a producer-only PEM Secret containing a client certificate, private key,
  and the Simulation View pose-ingress CA;
- exact producer, SPIFFE, epoch, endpoint, and entity-table identities;
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
just showcase-uav-sim-test
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
cargo run -p veoveo-smoke --bin smoke -- simulation-certify \
  --base-image "$BASE_REPOSITORY@$BASE_DIGEST" \
  --overlay-image "$OVERLAY_REPOSITORY@$OVERLAY_DIGEST" \
  --overlay-kind first-party-uav \
  --source-revision "$REVISION" \
  --output output/simulation-certification/first-party-uav.result.json
```

The installation-owned live acceptance deploys the UAV simulator and
Simulation View independently. It verifies two GPU workloads, exact pose
delivery, scene mirroring, camera admission, RTX/NVENC streaming through the
Simulation View App, domain recording, perception, and mission completion.

Browser evidence is valid only after a headed browser proves hardware-backed
high-performance WebGPU or WebGL. The browser-side H.264 software-decode
exception does not relax either GPU workload.

The two live commands preserve the ownership boundary:

```sh
just uav-domain-verify <kube-context> https://installation.example
just simulation-view-verify <kube-context> https://installation.example
```

The first command owns UAV flight, tiles, PX4, domain sensors, Recording Hub,
Perception, and Reason. The second owns the anonymous-producer proof for
Simulation View, camera capacity, RTX/NVENC, WebRTC, and its generic App.
Neither command substitutes for the other.

Run the composed showcase only after both independent paths pass:

```sh
google-chrome \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9227 \
  --user-data-dir=/tmp/veoveo-uav-acceptance \
  --window-size=1920,1080 \
  --ozone-platform=x11 \
  --use-angle=vulkan \
  --enable-features=Vulkan \
  https://installation.example/console/

just uav-showcase-verify <kube-context> https://installation.example
```

Chrome must be visible and authenticated through the installation's ordinary
Console login. `--chrome-cdp-url` accepts the HTTP discovery origin shown above
or Chrome's direct local `ws://` browser endpoint when runtime debugging does
not expose `/json/version`. Both paths query `Browser.getVersion` before the
in-page hardware checks. The composed command asks the UAV server for its
governed, digest-bound scene and pose identity, then asks Simulation View to
bind that scene and admit one follow camera. The UAV server still owns no
camera, renderer, stream, or App.

The run verifies the actual Console at takeoff, during the mission, and after
landing. Each checkpoint requires an advancing pose sequence and a healthy
640-by-360 NVIDIA NVENC H.264 stream. It then opens the same flight's governed
recording in the Console Rerun viewer. Headless Chrome, a browser with neither
hardware WebGPU nor hardware WebGL, missing Console APIs, a synthetic App host,
a static frame, or a software-renderer warning from the active visual surface
fails the run.

Evidence is written beneath
`output/acceptance/uav/{source-revision}/{run-id}/`. The typed
`veoveo.io/uav-showcase-acceptance-evidence/v1` manifest records the scene,
producer, camera, recording, pose sequences, flight states, hardware identity,
decode result, screenshot paths, and SHA-256 image digests. The directory is a
run artifact and remains outside source control.
