# SUMO traffic-world showcase

This showcase connects a [SUMO](https://eclipse.dev/sumo/) traffic simulation to
Veoveo. A Rust MCP server holds the single TraCI connection and sends every
command through it in order. The server publishes typed Rerun frames to Recording
Hub and exposes traffic reads, traffic control, durable tasks, resources, and
subscriptions through the gateway.

The bundled simulation is the MIT-licensed LuST Luxembourg scenario at a pinned
source revision. TraCI is reachable only inside the Kubernetes cluster. Like every
hosted server, the MCP endpoint accepts only requests carrying a gateway-signed
Ed25519 identity assertion.

The upstream SUMO 1.27.1 image is published for `linux/amd64`. The showcase
images declare that architecture explicitly.

## Capabilities

- `query_state` and `describe_scenario` return typed live-world data.
- `set_signal_phase`, `reroute_vehicle`, `set_edge_speed`, `close_lane`,
  and `open_lane` mutate the serialized simulation.
- `run_batch` is a durable task. Its recovery class is
  `interrupted_indeterminate` because a live simulation advance cannot be
  replayed safely after an uncertain interruption.
- `generate_network`, `compute_routes`, and `optimize_signals` run SUMO
  programs as resumable durable tasks. Outputs enter the shared artifact plane
  through task-bound write capabilities.
- `sumo://state` and `sumo://scenario` are typed resources.
- `sumo://congestion` supports subscriptions and resource-update notifications.
- `/world/sumo/**` is pushed continuously through an authenticated recording
  forwarder and stored by Recording Hub.

Task state lives in the required SurrealDB 3.2.4 platform store. The server uses
official MCP Tasks and Veoveo's shared task runtime.

## Tests

The deterministic driver unit tests need only the pinned Rust toolchain:

```bash
cargo test -p veoveo-sumo-mcp
```

The in-process push smoke writes fake-driver frames through the real Recording Hub
durability boundary and queries the resulting RRD segments:

```bash
cargo xtask smoke sumo-push
```

The live verification targets the active k3d profile. It checks that
unauthenticated requests are rejected, reads the live world, changes an edge speed,
advances a durable batch, and confirms that Recording Hub stored the world:

```bash
cargo xtask smoke sumo-verify --context k3d-veoveo-sumo
```

## Run in k3d

Use the latest versions pinned in `deploy/local/k3d/versions.env`. The cluster
profile requires a working NVIDIA container runtime even though SUMO itself does
not request a GPU, so the same local cluster can also run the GPU simulators and
renderers. Startup applies the profile's NVIDIA device-plugin manifest and
waits for allocatable GPU capacity before deployment.

```bash
PROFILE=showcase/sumo/deploy/deployment.json
LOCK=output/deployments/sumo/deployment.lock.json
REVISION=$(git rev-parse HEAD)
cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output "$LOCK"
cargo xtask smoke profile-up --profile "$PROFILE" --lock "$LOCK" \
  --all-components --receipt-output output/development/installation.receipt.json
```

The profile selects the platform images and publishes the SUMO images through a
separate `workload` source. Publishing configures the managed builder with the
profile's registry address and transport and keeps the builder's existing cache.

Normal clients use the `operator` gateway profile at
`http://localhost:8780/mcp/operator`. They mint a scoped service token through
the configured OAuth token endpoint before inspecting the namespaced SUMO
surface through that gateway. The direct
authenticated verification endpoint is `http://127.0.0.1:8895/sumo/mcp`; it
exists for the Rust acceptance harness.

SUMO's TraCI server accepts one client. The chart has no TCP
readiness probe on port 8813 because a probe would consume that connection and
terminate the simulation. `sumo-mcp` owns connection readiness and retries while
the LuST network loads.

The showcase chart leaves telemetry disabled because the minimal local platform
does not install a collector. Set `telemetry.enabled=true` and configure its
endpoint when a profile installs the collector.

`sumo-mcp` sends native Rerun traffic only to the forwarder on pod loopback.
The forwarder keeps its size-limited on-disk queue on
`sumo-recording-forwarder`, authenticates to the gateway with the
`sumo-recording-producer` private key, and connects through the internal
gateway Service. The public OAuth issuer, protected-resource URI, assertion
audience, and Host header stay `http://localhost:8780`.

## Layout

```text
showcase/sumo/
  deploy/
    gateway.json             # SUMO-owned gateway profile
    platform-values.yaml     # minimal platform selection
    helm/                    # SUMO and sumo-mcp release
  sim/
    Dockerfile               # pinned SUMO and LuST world
    run-sumo.sh
  sumo-mcp/
    Cargo.toml
    Dockerfile
    src/contract.rs          # typed domain and task contracts
    src/driver.rs            # deterministic and TraCI drivers
    src/recording.rs         # typed Recording Hub publisher
    src/server/              # auth, MCP, tasks, artifacts, and HTTP
```

Remove the composed platform and showcase releases with
`cargo xtask smoke profile-down --profile showcase/sumo/deploy/deployment.json`.
The shared registry stays up for other profiles. Local deployment mechanics
are documented in
[`../../docs/LOCAL_DEPLOYMENT_PROFILES.md`](../../docs/LOCAL_DEPLOYMENT_PROFILES.md).
