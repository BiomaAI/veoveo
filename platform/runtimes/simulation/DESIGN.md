# Canonical GPU Simulation Runtime

The simulation runtime is Veoveo's one reusable GPU compatibility lineage for
Isaac-based simulators and renderer workloads. It contains no vehicle, dynamics,
controller, scenario, mission, customer asset, or domain entrypoint. Applications in the fork derive thin overlays from its immutable OCI digest.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| OCI Image Specification | one `linux/amd64` image published by digest with SBOM and provenance |
| `veoveo.io/simulation-runtime-lock/v1` | exact build-input lock for the supported runtime tuple and pod contract |
| `veoveo.io/simulation-runtime-conformance/v1` | hardware result tied to one image digest and qualified node |
| NVIDIA Container Runtime | one visible NVIDIA RTX GPU through `nvidia.com/gpu` and RuntimeClass `nvidia` |
| CUDA | Torch CUDA 12.8 plus the Isaac RTX extension's pinned NVRTC 12.8.61 builtins |
| NVIDIA NVENC API | driver-provided encode API required by live-view profiles |
| USD | render and simulation scene representation supplied by Isaac Sim 6.1.0 |
| SHA-256 | image, archive, wheel, lock, SBOM, provenance, and conformance identity |

Isaac, Kit, CUDA, and NVIDIA live-stream interfaces are implementation dependencies.
The dependency lock and immutable image digest identify the supported runtime.
Domain-facing simulation protocols belong to the hosted server.

## Dependency Profile

The runtime lock selects this tuple:

| Component | Selected identity |
|---|---|
| Isaac Sim | `6.1.0`, platform digest `sha256:af1d2b4e75d553bfa27beb5a401198654aa8d607f3b7a6749196e9ce253def20` |
| Kit | `110.3.0` |
| Python | CPython 3.12 |
| Isaac Lab | tag `v3.0.0-EA`, revision `ae37b028ea415c91ea2bc32609efcd759ed2b974` |
| Warp | `1.16.0`, bundled by Isaac Sim |
| Newton | `1.5.2`, wheel digest `sha256:b432b0db9ee963c1fe570b88952d913b9eb98c9ca190c58f80882f28dd0dd5d6` |
| MuJoCo | `3.11.0`, bundled by Isaac Sim |
| MuJoCo Warp | `3.11.0`, bundled by Isaac Sim |
| Torch | `2.11.0+cu128`, bundled by Isaac Sim |
| Isaac RTX NVRTC builtins | `12.8.61`, retained from the pinned Isaac Sim image |
| NVIDIA AOV live stream | `10.2.1+110.1.2.lx64.r.cp312` |
| NVIDIA RTSP live stream | `10.4.1+110.1.2.lx64.r.cp312` from Isaac Sim |

Isaac Lab `v3.0.0-EA` is a deliberate pre-release dependency. It is the upstream
release paired with Isaac Sim 6.1; no stable 6.1-compatible Lab release exists.
The base retains Isaac Sim's Torch, Warp, MuJoCo, and MuJoCo Warp packages and
replaces Newton in its Kit-owned extension root. Isaac Sim's Newton tensor extension
registers the `newton` backend with `omni.physics.tensors` when Kit enables it.
Applications enable `isaacsim.physics.newton` and its tensors extension in Kit's
initial arguments and set
`SimulationManager`'s default engine to `newton`. Newton registration therefore exists
before any physics-backed application state is created; a later engine assertion fails
closed if Kit did not retain that selection.

The supported Isaac Lab surface contains the core, PhysX, Newton, OV,
camera-render specification, and frame-view packages. Training environments, policy
libraries, task catalogs, teleoperation, Mimic, and application assets remain overlay
dependencies. This boundary gives simulator and renderer overlays the common launch,
camera, transform, and backend APIs without turning the runtime into a domain
application.

## Authoritative Module Roots

Isaac Sim registers Torch, Warp, and Newton as Kit extension payloads. Installing
newer packages only in ordinary site-packages leaves the bundled versions
authoritative after Kit starts. Importing a newer package first creates a mixed graph
when Kit later loads an older package or native dependency from its extension.

The image replaces Newton inside its Kit-owned extension root. Torch 2.11 and
Warp 1.16 remain in the Isaac Sim image; the build checks those roots before
export. The Isaac RTX Hydra extension retains its upstream NVRTC 12.8.61 builtins
at the immutable path referenced by its own symlinks.
`PYTHONPATH` selects the same roots before Kit starts. The identity probe rejects
loaded Torch, Warp, or Newton file modules outside their selected root. An overlay
may add compatible packages, but it cannot replace Isaac Sim, Isaac Lab, Warp,
Newton, MuJoCo, Torch, Python, CUDA, or core Kit versions.

The base owns the complete runtime `PYTHONPATH`. A supported overlay extends that value
with `ENV PYTHONPATH=/opt/overlay:${PYTHONPATH}`. It never reproduces or replaces the
platform roots. Certification compares the final OCI image configurations and rejects
an overlay when any base root is absent or appears in a different relative order. This
check covers Isaac Lab and `/opt/veoveo/python` before a GPU process starts.

## Runtime Contract

The process runs as UID and GID `10001`. Its home is `/var/lib/veoveo`. Kit cache and
data, XDG cache and data, NVIDIA shader cache, and Veoveo runtime cache paths are
writable by that identity.

Every Kubernetes workload requests one `nvidia.com/gpu`, selects the `nvidia`
RuntimeClass, and mounts a private memory-backed `/dev/shm` of at least 2 GiB.
Production defaults to an exclusive GPU. A sharing profile requires separate measured
capacity evidence and cannot be enabled by resource configuration alone.

The image has no CPU rendering or simulation fallback. A missing GPU, CUDA driver,
hardware RTX path, or NVENC API fails conformance. The `linux/amd64` qualification floor is NVIDIA's tested Isaac Sim 6.1 driver
`595.58.03`. An installation with an older driver cannot qualify this candidate.

## Authoritative Live Cameras

A domain simulator overlay renders operator cameras inside the same authoritative Isaac
process that owns physics and the USD/Cesium world. Logical cameras own the final
authoritative poses. An overlay may bind its bounded logical-camera set into one typed
view-tiled RTX product and one NVENC H.264 atlas. Every authorized viewer receives the
exact same encoded product through WebSocket fanout. Viewer count creates no camera,
scene, Cesium consumer, render product, or encoder session.

Operator-camera smoothing consumes the current authoritative entity transform on every
render tick and changes only the camera pose. It never buffers, interpolates, or delays
simulation state. Physical sensor capture retains its own declared cadence and exact
mount, independent of the operator-camera cadence. The reusable base supplies GPU,
camera, RTX, and NVENC compatibility; camera rigs, stream authorization, governance,
and fanout remain the domain overlay's responsibility.

## Build And Publication

`simulation-runtime.lock.json` is the tuple authority. `requirements.lock` is a generated,
hash-complete CPython 3.12 dependency lock for the supported Isaac Lab subset. The
Dockerfile checks every independently downloaded archive and wheel before installation.

The Bake target publishes `veoveo/simulation-runtime`. A release records:

- the source revision and image manifest digest;
- the dependency lock and generated Python lock;
- an OCI SBOM and build provenance;
- the qualified node identity and driver;
- the hardware conformance result tied to the final image digest.

A simulator workload selects the runtime build target or a qualified image digest
in `FROM`. Replacing an `ARG` default with a mutable tag is not a
supported release workflow.

## Hardware Conformance

The build executes `probes/identity.py` as a structural check. That check is not GPU
acceptance.
The pinned Isaac 6.1 parent links Warp's license-only APIC initializer to a teleop
file. The Dockerfile materializes that file within Warp before checking module roots.

`probes/gpu.py` initializes Kit before calling the CUDA and NVENC driver APIs. This
keeps Isaac's Vulkan and RTX startup in the same order as the application runtime.
The probe then proves:

- writable cache and data paths plus a private 2 GiB `/dev/shm`;
- CUDA driver initialization and a visible hardware device;
- the NVENC API version and session entrypoint;
- Torch and Warp kernels on `cuda:0`;
- a Newton `SolverMuJoCo` rigid-body step and `SensorTiledCamera` output on `cuda:0`;
- `SimulationManager` initialization, a playing externally stepped Newton timeline, and
  an Experimental `RigidPrim` that rises under a CUDA-resident applied force through
  Newton's native tensor view;
- one authoritative Torch, Warp, Newton, and Isaac Lab module graph after Kit startup;
- a CUDA-resident Isaac Lab RTX RGB batch with nonblank, distinct cameras.

The canonical invocation is:

```sh
docker run --rm \
  --runtime=nvidia \
  --gpus all \
  --network none \
  --shm-size=2g \
  --entrypoint /isaac-sim/python.sh \
  veoveo/simulation-runtime@sha256:IMAGE_DIGEST \
  /opt/veoveo/simulation-runtime/probes/gpu.py \
  --image-digest sha256:IMAGE_DIGEST \
  --cameras 4 \
  --width 160 \
  --height 120 \
  --output /tmp/simulation-runtime-conformance.json
```

The image is a candidate until the hardware result, UAV overlay acceptance, and an
anonymous external overlay result all identify its final digest. A runtime upgrade
requires all three gates again.

`cargo xtask smoke simulation-certify` accepts an optional deployment lock. Without one,
the managed builder uses TLS. A supplied `veoveo.io/deployment-lock/v5` document
authorizes its exact registry authority and may explicitly select `insecure-http`.
Both image references must retain that authority. Buildx inspection, attestation
resolution, and digest-addressed materialization use the same managed BuildKit
configuration. Docker runs only the local materialization with pulls disabled, while
the conformance result records the original registry coordinates.

Certification creates a sibling `*.transcript.log` before registry access. It streams
the GPU process output into that file and keeps the partial transcript after a command
failure or timeout. BuildKit materializes each exact overlay into a digest-keyed local
Docker cache with a source-identity label. Later runs reuse only an exact label match.
Operators remove those large images explicitly:

```sh
cargo xtask image certification-cache-prune \
  --confirm veoveo-simulation-certify-cache
```
