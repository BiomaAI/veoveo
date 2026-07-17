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
  -> UAV Simulation MCP and Recording Hub
  -> View, Perception, agents, and governed artifacts
```

View MCP retains its direct `GOOGLE_MAPS_API_KEY` source. That path is
complementary and does not replace the tiles loaded inside Isaac.

## Component boundary

- [`../../servers/uav-sim-mcp/DESIGN.md`](../../servers/uav-sim-mcp/DESIGN.md)
  owns the provider-neutral MCP contract.
- `runtime/` owns the immutable Isaac, Cesium, Pegasus, and PX4 image, the
  pod-private typed adapter, and conversion of sensor and world state into
  Rerun.
- `deploy/helm/` owns interactive and batch Kubernetes workloads.
- `dependencies.lock.json` records every upstream version, commit, digest, and
  verification source used by the image and chart.

The simulator chart installs beside the normal Veoveo chart. It references the
existing SurrealDB, artifact, recording, gateway-trust, and installation Secret
contracts rather than creating alternate platform services.

## Credentials

Local Bioma provisioning reads `CESIUM_ION_ACCESS_TOKEN` from the main
worktree's `.env` and writes it to the dedicated `veoveo-uav-sim-secrets`
Secret as `cesium-ion-access-token`. The platform installation Secret remains
the authority for gateway trust. Helm accepts only Secret names and keys. It
never accepts the token value.

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

## Verification layers

The central Rust smoke harness owns orchestration and assertions. The Justfile
contains only short dispatch recipes.

- Contract tests run against a deterministic fake simulator adapter.
- Image tests verify dependency revisions, extension discovery, the Pegasus
  patch, and PX4 startup.
- Helm tests render interactive and batch workloads and reject plaintext token
  values.
- Live acceptance loads Google Photorealistic 3D Tiles inside Isaac, flies a
  bounded PX4 mission, retains its Rerun recording, and runs Perception while
  View remains healthy.

The live test is the release proof. Fixture tiles exercise offline code paths
but cannot replace it.

## Build and deploy

Put both provider credentials in the main worktree `.env` without copying them
into an overlay:

```dotenv
CESIUM_ION_ACCESS_TOKEN=
GOOGLE_MAPS_API_KEY=
```

The first credential belongs to Isaac/Cesium. The second remains owned by View
MCP. Build and validate the UAV images with:

```bash
just showcase-uav-sim-test
just showcase-uav-sim-build
just helm-check
```

The normal Bioma flow provisions the Secret, imports both UAV images, installs
the platform chart, and then installs the UAV chart beside it:

```bash
just bioma-resources
just bioma-platform-up
```

Interactive mode creates one `uav-sim` Deployment containing the Isaac runtime
and UAV MCP sidecar. Batch mode creates an Isaac-only Job. Both use ephemeral
cache, data, and shared-memory volumes. Google tile bytes are not retained on a
PVC.

Development values use the locally built tags. A production render sets
`global.production=true` and must provide `images.runtime.digest` plus
`images.mcp.digest`; Helm fails before producing a manifest when either digest
is absent. CI records the published digests in the deployment values rather
than treating a tag as immutable.

Run the credentialed acceptance after all three GPU deployments are available:

```bash
just bioma-uav-sim-verify
```

That command verifies resident Google tiles inside Isaac, a PX4 mission,
canonical Frames and Recording Hub identities, Perception over the Isaac camera
stream, and continued availability of View and Perception.
