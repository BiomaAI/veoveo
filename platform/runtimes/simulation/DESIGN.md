# Canonical GPU Simulation Runtime

The simulation runtime is Veoveo's one reusable GPU compatibility lineage for
Isaac-based simulators and renderer workloads. It contains no vehicle, dynamics,
controller, scenario, mission, customer asset, or domain entrypoint. First-party and
external repositories derive thin overlays from its immutable OCI digest.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| OCI Image Specification | one `linux/amd64` image published by digest with SBOM and provenance |
| `veoveo.io/simulation-runtime-lock/v1` | exact build-input lock for the supported runtime tuple and pod contract |
| `veoveo.io/simulation-runtime-conformance/v1` | hardware result tied to one image digest and qualified node |
| NVIDIA Container Runtime | one visible NVIDIA RTX GPU through `nvidia.com/gpu` and RuntimeClass `nvidia` |
| CUDA | Torch CUDA 13.0 plus the Isaac RTX extension's pinned NVRTC 12.8.61 builtins |
| NVIDIA NVENC API | driver-provided encode API required by live-view profiles |
| USD | render and simulation scene representation supplied by Isaac Sim 6.0.1 |
| SHA-256 | image, archive, wheel, lock, SBOM, provenance, and conformance identity |

Isaac, Kit, CUDA, and NVIDIA live-stream interfaces are implementation dependencies.
They are not a provider-neutral public simulation protocol. The public extension
boundary is the compatibility profile and immutable image digest.

## Compatibility Release

`2026.08.0` selects one tuple:

| Component | Selected identity |
|---|---|
| Isaac Sim | `6.0.1`, platform digest `sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb` |
| Kit | `110.1.2` |
| Python | CPython 3.12 |
| Isaac Lab | tag `v3.0.0-beta2.patch1`, revision `ffff603eafc6b74264a5261cc0183d6a65390d78` |
| Warp | `1.16.0`, revision `86ec8b78cbef8bb570a9877e351ac0f365718e30` |
| Newton | `1.5.0`, revision `cca3bb8a17a3620a1343df3cf12c625e4161b317` |
| MuJoCo | `3.11.0`, revision `b85fdca54f0e0038b804af146a0b4e94199e00d0` |
| MuJoCo Warp | `3.11.0`, revision `dbc52e3ea69a63e14026e969cb055e0c3c2f0c83` |
| Torch | `2.12.0+cu130` |
| Isaac RTX NVRTC builtins | `12.8.61`, retained from the pinned Isaac Sim image |
| NVIDIA AOV live stream | `10.2.0+110.1.2.lx64.r.cp312` |
| NVIDIA WebRTC live stream | `10.3.2+110.0.0.lx64.r.cp312` from Isaac Sim |

Isaac Lab `v3.0.0-beta2.patch1` is a deliberate pre-release dependency. It is
the latest published release with explicit Isaac Sim 6.0.1 support, and no stable
Isaac Lab 3.0 release exists. Veoveo qualifies its source revision with Warp 1.16.0,
Newton 1.5.0, and Newton's required MuJoCo 3.11 line. A narrow source patch replaces
Newton's removed `SolverNotifyFlags` import with `ModelFlags`; the build rejects the
old import after applying that patch. The image also removes Isaac Sim's obsolete
`ls_parallel` MuJoCo solver field because Newton 1.5 no longer accepts that constructor
argument. The live Isaac adapter therefore initializes the pinned GPU solver without
retaining the removed configuration surface. A second pinned patch routes
`SimulationManager` tensor views through Newton's native tensor factory. Isaac Sim
6.0.1 otherwise asks the legacy tensor plugin for an unregistered `newton` backend.
Applications enable `isaacsim.physics.newton` in Kit's initial arguments and set
`SimulationManager`'s default engine to `newton`. Newton registration therefore exists
before any physics-backed application state is created; a later engine assertion fails
closed if Kit did not retain that selection.

The supported Isaac Lab surface contains the core, PhysX, Newton, OV, OVPhysX,
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

The image replaces each package inside its Kit-owned extension root and updates the
Warp extension version. Torch 2.12 and its CUDA 13.0, NCCL, Triton, and Python support
packages replace the complete Kit ML archive graph. The Isaac RTX Hydra extension
retains its upstream NVRTC 12.8.61 builtins at the immutable path referenced by its
own symlinks; those files are renderer dependencies and do not enter Torch's CUDA 13
module graph. The bundled TorchVision and TorchAudio payloads are removed because
they are outside the supported subset and do not have a stable Torch 2.12 pair.
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
hardware RTX path, or NVENC API fails conformance. The initial `linux/amd64` profile
uses NVIDIA's minimum driver floor `570.169`; release qualification uses the tested
Isaac Sim driver `595.58.03`.

## Authoritative Live Cameras

A domain simulator overlay renders operator cameras inside the same authoritative Isaac
process that owns physics and the USD/Cesium world. Logical cameras own the final
authoritative poses. A bounded preallocated viewer slot copies one selected pose into an
isolated camera clone and activates one RTX render product, NVENC H.264 encode, and native
WebRTC peer. Viewer count never creates another scene or Cesium consumer, but each admitted
viewer deliberately consumes one measured render-and-encode slot.

Operator-camera smoothing consumes the current authoritative entity transform on every
render tick and changes only the camera pose. It never buffers, interpolates, or delays
simulation state. Physical sensor capture retains its own declared cadence and exact
mount, independent of the operator-camera cadence. The reusable base supplies GPU,
camera, RTX, and NVENC compatibility; camera rigs, product admission, governance, and
signaling remain the domain overlay's responsibility.

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

An external extension selects the runtime from the Veoveo compatibility manifest and
uses its digest in `FROM`. Replacing an `ARG` default with a mutable tag is not a
supported release workflow.

## Hardware Conformance

The build executes `probes/identity.py` as a structural check. That check is not GPU
acceptance.

`probes/gpu.py` launches Isaac through Isaac Lab and then proves:

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
