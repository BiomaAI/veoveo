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
| CUDA | CUDA 12.8 user-space selected by Torch 2.10.0 and the Isaac Sim 6.0.1 renderer |
| NVIDIA NVENC API | driver-provided encode API required by live-view profiles |
| USD | render and simulation scene representation supplied by Isaac Sim 6.0.1 |
| SHA-256 | image, archive, wheel, lock, SBOM, provenance, and conformance identity |

Isaac, Kit, CUDA, and NVIDIA live-stream interfaces are implementation dependencies.
They are not a provider-neutral public simulation protocol. The public extension
boundary is the compatibility profile and immutable image digest.

## Compatibility Release

`2026.07.0` selects one tuple:

| Component | Selected identity |
|---|---|
| Isaac Sim | `6.0.1`, platform digest `sha256:b1c542b2ecc549b3d1ebb78c25664aa3bacba1709e6ad8e0a68e09426d57dedb` |
| Kit | `110.1.2` |
| Python | CPython 3.12 |
| Isaac Lab | tag `v3.0.0-beta2.patch1`, revision `ffff603eafc6b74264a5261cc0183d6a65390d78` |
| Warp | `1.15.0`, revision `ffa038b1adf3927a8e893d4536f1a7562f17d749` |
| Newton | `1.4.0`, revision `0597c719e345b4457bf698ca24faad3fd418452d` |
| MuJoCo | `3.10.0`, revision `28009f9105cd92784b7b0b30c0605a5e29107a77` |
| MuJoCo Warp | `3.10.0.3`, revision `710c34ca96745a44bfb701cdbda89e1434845728` |
| Torch | `2.10.0+cu128` |
| NVIDIA AOV live stream | `10.2.0+110.1.2.lx64.r.cp312` |
| NVIDIA WebRTC live stream | `10.3.2+110.0.0.lx64.r.cp312` from Isaac Sim |

Isaac Lab `v3.0.0-beta2.patch1` is a deliberate pre-release dependency. It is
the latest published release with explicit Isaac Sim 6.0.1 support, and no stable
Isaac Lab 3.0 release exists. Veoveo qualifies its source revision with the newer
stable Warp 1.15.0 and Newton 1.4.0 tuple rather than inheriting Isaac Lab's older
declared pair.

The supported Isaac Lab surface contains the core, PhysX, Newton, OV, OVPhysX,
camera-render specification, and frame-view packages. Training environments, policy
libraries, task catalogs, teleoperation, Mimic, and application assets remain overlay
dependencies. This boundary gives simulator and renderer overlays the common launch,
camera, transform, and backend APIs without turning the runtime into a domain
application.

## Authoritative Module Roots

Isaac Sim registers Warp and Newton as Kit extension payloads. Installing newer
packages only in ordinary site-packages leaves the bundled versions authoritative
after Kit starts. Importing the newer packages first creates a mixed graph when Kit
later loads internal Warp modules.

The image replaces the packages inside the Kit-owned extension roots and updates the
Warp extension version. `PYTHONPATH` selects those same roots before Kit starts. The
identity probe rejects every loaded Warp or Newton file module outside its one selected
root. An overlay may add compatible packages, but it cannot replace Isaac Sim, Isaac
Lab, Warp, Newton, MuJoCo, Torch, Python, CUDA, or core Kit versions.

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
- Newton `SensorTiledCamera` output resident on `cuda:0`;
- one authoritative Warp, Newton, and Isaac Lab module graph after Kit startup;
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
