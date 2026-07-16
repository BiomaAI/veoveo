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
- `sim/` owns the immutable Isaac, Cesium, Pegasus, and PX4 image.
- `recording-adapter/` owns sensor and state conversion into Rerun.
- `deploy/helm/` owns interactive and batch Kubernetes workloads.
- `dependencies.lock.json` records every upstream version, commit, digest, and
  verification source used by the image and chart.

The simulator chart installs beside the normal Veoveo chart. It references the
existing SurrealDB, artifact, recording, gateway-trust, and installation Secret
contracts rather than creating alternate platform services.

## Credentials

Local Bioma provisioning reads `CESIUM_ION_ACCESS_TOKEN` from the main
worktree's `.env` and writes it to the existing installation Secret as
`cesium-ion-access-token`. Helm accepts only the Secret name and key. It never
accepts the token value.

NVIDIA registry credentials use a normal `imagePullSecret`. `ACCEPT_EULA=Y` is
set explicitly for the Isaac container. Privacy consent remains a separate
operator decision.

## Concurrency

Isaac Sim, View, and Perception run concurrently. Each workload declares its
GPU request, while the cluster provides enough schedulable capacity for the
complete profile. No chart mode suspends an existing GPU workload to admit a
simulation session.

## Dependency policy

The image builds only from the immutable pins in `dependencies.lock.json`.
Cesium release bytes are checked before extraction. Pegasus and PX4 are checked
out by commit SHA. The Isaac base uses the platform-specific manifest digest.
Runtime package installation is forbidden.

Pegasus 5.1.0 is the newest upstream release but targets Isaac 5.1.0. The
showcase carries a focused source patch for Isaac 6.0.1. Its compatibility test
must pass before an image can be published. Failure blocks the release; it does
not select an older Isaac image.

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
