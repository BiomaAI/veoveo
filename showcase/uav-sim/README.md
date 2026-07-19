# UAV simulation showcase

This showcase runs UAV simulation as a first-class Bioma workload. Isaac Sim
renders Google Photorealistic 3D Tiles through Cesium ion, Pegasus supplies the
multirotor dynamics and PX4 bridge, and Veoveo governs the resulting sessions,
missions, recordings, and perception work.

The core path is:

```text
CESIUM_ION_ACCESS_TOKEN
  -> Cesium ion
  -> Google Photorealistic 3D Tiles in Isaac Sim
  -> Pegasus vehicles and PX4 SITL
  -> UAV Simulation MCP and authenticated recording forwarder
  -> Recording Hub
  -> View, Perception, agents, and governed artifacts
```

View MCP retains its direct `GOOGLE_MAPS_API_KEY` source. That path is
complementary and does not replace the tiles loaded inside Isaac.

## Component boundary

- [`../../servers/uav-sim-mcp/DESIGN.md`](../../servers/uav-sim-mcp/DESIGN.md)
  owns the provider-neutral MCP contract.
- `runtime/` owns the immutable Isaac, Cesium, Pegasus, and PX4 dependency
  base, the thin runtime overlay, the pod-private typed adapter, and conversion
  of sensor and world state into Rerun.
- `deploy/` owns commit-addressed OCI publication and the interactive and batch
  Kubernetes workloads.
- `scenarios/` owns runtime-loaded live mission and acceptance inputs. These
  files are deliberately outside the Isaac image build context.
- `dependencies.lock.json` records every upstream version, commit, digest, and
  verification source used by the image and chart.

The simulator chart installs beside the normal Veoveo chart. It references the
existing SurrealDB, artifact, recording, gateway-trust, and installation Secret
contracts rather than creating alternate platform services.

The Isaac runtime sends Rerun only to a producer-local forwarder. Interactive
and batch workloads have separate durable queue claims. Each forwarder
authenticates through the gateway before Recording Hub accepts a versioned
protobuf batch; no Hub raw-ingest Service exists.

## Credentials

Local Bioma provisioning reads `CESIUM_ION_ACCESS_TOKEN` and
`VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM` from the main worktree's `.env`.
It writes them to the dedicated `veoveo-uav-sim-secrets` and
`veoveo-recording-producer` Secrets. The platform installation Secret remains
the authority for gateway trust. Helm accepts only Secret names and keys. It
never accepts either credential value.

The runtime does not accept `GOOGLE_MAPS_API_KEY`. Cesium for Omniverse loads
ion asset `2275207`, the canonical Google Photorealistic 3D Tiles asset, with
the ion token. The token is authored only into the anonymous USD session layer
required by Cesium, cleared during shutdown, and never exported.

Streamed tile geometry remains the core rendered world. The runtime adds one
bounded, invisible collision surface at the configured local origin for PX4
launch and landing because Cesium's streamed mesh is not an Isaac physics
authority. The surface does not replace, filter, or prescribe how the tiles
are used.

NVIDIA registry credentials use a normal `imagePullSecret`. `ACCEPT_EULA=Y` is
set explicitly for the Isaac container. Privacy consent remains a separate
operator decision.

## Concurrency

Isaac Sim, View, and Perception run concurrently. Each workload declares its
GPU request, while the cluster provides enough schedulable capacity for the
complete profile. No chart mode suspends an existing GPU workload to admit a
simulation session.

The application chart consumes the standard NVIDIA runtime class and
`nvidia.com/gpu` resource contract. A fielded cluster supplies that contract
with the pinned NVIDIA GPU Operator release in `dependencies.lock.json`; local
k3d supplies the same contract through its pinned device-plugin manifest.

## Dependency policy

The image builds only from the immutable pins in `dependencies.lock.json`.
Cesium release bytes are checked before extraction. Pegasus and PX4 are checked
out by commit SHA. The Isaac base uses the platform-specific manifest digest.
Python wheels and their transitive dependencies are exact build-time pins.
Isaac supplies its coupled NumPy and Pillow builds, while Cesium supplies its
release-bundled lxml wheel. Package installation at container startup is
forbidden.

Pegasus 5.1.0 is the newest upstream release but targets Isaac 5.1.0. The
showcase carries a focused source patch for Isaac 6.0.1. Its compatibility test
must pass before an image can be published. Failure blocks the release; it does
not select an older Isaac image.

PX4 1.17.0 and pymavlink use MAVLink 2 exclusively. A render-free physics
bootstrap completes the simulator and commander handshakes before the first
RTX/Cesium render can compile shaders. The runtime rebinds Pegasus's state,
sensor, dynamics, and MAVLink callbacks after each Isaac physics reset because
Isaac 6 recreates the underlying subscription interface.

The stage authors a deterministic dome light and distant sun for headless RTX
rendering. The canonical UAV sensor is a nadir camera at `camera/down`, with
image up aligned to vehicle forward. Helm values carry its resolution, frame
rate, focal length, clipping range, translation, and unit quaternion through
validated runtime fields. Camera readiness is based on the exact RGB8 bytes
sent to H.264: three consecutive frames must contain measurable luma and
non-black pixels. This operational gate permits takeoff from the nearly uniform
launch surface. The live scenario separately requires scene detail after the
configured climb. Frames without visible detail are withheld from the video
stream, and a camera that remains black for 30 seconds after Google tiles
become resident fails readiness instead of producing an apparently successful
recording.

## Verification layers

The central Rust smoke harness owns orchestration and assertions. The Justfile
contains only short dispatch recipes.

- Contract tests run against a deterministic fake simulator adapter.
- Image tests verify dependency revisions, extension discovery, the Pegasus
  patch, and PX4 startup.
- Helm tests render interactive and batch workloads and reject plaintext token
  values.
- Live acceptance loads Google Photorealistic 3D Tiles inside Isaac, reads its
  climb, camera thresholds, waypoint, and perception capture from
  `scenarios/bioma-aerial.json`, retains its Rerun recording, and runs
  Perception while View remains healthy.

The live test is the release proof. Fixture tiles exercise offline code paths
but cannot replace it.

## Build and deploy

Put both provider credentials in the main worktree `.env` without copying them
into an overlay:

```dotenv
CESIUM_ION_ACCESS_TOKEN=
GOOGLE_MAPS_API_KEY=
VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM=
VEOVEO_RECORDING_PRODUCER_KEY_ID=recording-producer-2026
```

The first credential belongs to Isaac/Cesium. The second remains owned by View
MCP. Build and validate the UAV images with:

```bash
just showcase-uav-sim-test
just showcase-uav-sim-build
just helm-check
```

The normal Bioma flow provisions the Secret, imports the smaller platform
images, publishes the UAV dependency base plus commit-addressed runtime, MCP,
and recording-forwarder images to the k3d-managed OCI registry, and installs
the two charts:

```bash
just bioma-resources
just bioma-uav-sim-publish
just bioma-platform-up
```

Interactive mode creates one `uav-sim` Deployment containing the Isaac runtime
and UAV MCP sidecar. Batch mode creates an Isaac-only Job. The workloads use
separate persistent runtime-cache claims, and `cache.version` places every
Isaac, shader, and Cesium cache generation beneath its own directory. Changing
the cache version starts clean without allowing interactive and batch writers
to share files. Runtime data and shared memory remain ephemeral.

`publish-images.py` refuses a dirty worktree, tags the thin runtime and MCP
images with the full Git commit, and pushes through OCI. The stable dependency
base tag contains Isaac, Cesium, Pegasus, PX4, and Python wheels. The runtime
Docker stage starts from that published base reference, which lets a clean
builder pull the immutable layers instead of rebuilding the dependency stage.
A runtime-only change rebuilds the final source-copy layer and the registry
transfers only missing blobs. Edit `scenarios/bioma-aerial.json` and rerun
`just bioma-uav-sim-verify`; a mission-only change performs no image build,
push, or Helm rollout.

Development deployments derive the image tag from the same Git commit. A
production render sets
`global.production=true` and must provide `images.runtime.digest`,
`images.mcp.digest`, and `images.forwarder.digest`; Helm fails before producing
a manifest when a digest is absent. CI records the published digests in the
deployment values rather than treating a tag as immutable.

Run the credentialed acceptance after all three GPU deployments are available:

```bash
just bioma-uav-sim-verify
```

That command verifies resident Google tiles inside Isaac, a PX4 mission,
canonical Frames and Recording Hub identities, Perception over the Isaac camera
stream, and continued availability of View and Perception.
