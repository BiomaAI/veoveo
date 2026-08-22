# UAV Sim MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Governs interactive and durable work against UAV simulation sessions without
exposing simulator native control ports through the gateway. The server owns
the typed simulation protocol, caller ownership, task state, resource
identities, subscriptions, and recording references; the simulator adapter
owns Isaac stage mutation, Cesium tiles, Newton rigid bodies, the Warp UAV
plant, PX4 transport, and authoritative operator cameras and encoded products.

## Invariants

- Owns the `uav-sim://` URI scheme. Identity: slug `uav-sim`, endpoint
  `/uav-sim/mcp`, port 8802. Provider names (Isaac, Cesium, Newton, Warp, PX4)
  never enter canonical tool or resource identities.
- MAVLink remains a data plane protocol; it is never projected as high-rate MCP
  tools. The simulator adapter HTTP endpoint stays cluster
  private and accepts only typed requests from this server.
- Durable tools (`run_scenario`, `execute_vehicle_mission_plan`, `capture_dataset`) use
  `interrupted_indeterminate` recovery; live simulator work is never replayed
  after an unclean interruption. Compatibility task tools are not added.
- Map MCP owns place resolution, active operational geography, mobility profiles,
  restrictions, routing, and `veoveo.io/map-route-handoff/v1`. Frames MCP owns
  immutable world revisions. This server owns principal-to-vehicle grants,
  mission admission, exclusive command leases, execution, telemetry, and its
  domain App. Do not move Map or Frames behavior into UAV code.
- A gateway scope, agent manifest, message target, or requested vehicle ID never
  grants vehicle authority. Every vehicle mutation requires a current UAV-owned
  principal grant; mission execution additionally requires the exact admitted
  plan revision and an exclusive vehicle command lease.
- Every session starts `unconfigured`. `configure_world` binds it exactly once
  to an immutable Frames world revision and a static simulation frame from that
  revision. The adapter derives Cesium and Newton fleet georeferencing from that
  binding, converts ENU/NED locally, and makes no MCP calls in the physics loop.
- Operator cameras, RTX rendering, and NVIDIA NVENC products stay inside the
  authoritative simulator. This server owns ephemeral actor-and-browser viewer
  leases, access audit, signaling authorization, and `ui://uav-sim/live.html`.
  It never persists renderer desired state or mirrors entity poses.
- Every active viewer lease owns one exclusively assigned physical slot with a
  camera clone, RTX render, Cesium viewport, NVIDIA NVENC session, and native
  WebRTC peer. Logical cameras and their final poses may be shared; encoded
  products and transport state are never shared between viewers.
- `CESIUM_ION_ACCESS_TOKEN` comes only from the dedicated Kubernetes Secret.
  It is never a tool argument, ConfigMap value, resource field, log field, or
  exported USD content.
- Recording state publishes its producer key and typed catalog lifecycle
  immediately. The canonical `recording://recordings/{recording_id}` identity
  appears only after catalog resolution; catalog delay or failure never blocks
  simulation, operator rendering, or live-view signaling. Native Recording Hub
  ports stay private.

## Build And Test

- `cargo check -p veoveo-uav-sim-mcp`
- `cargo test -p veoveo-uav-sim-mcp` (deterministic fake adapter, credential
  free)
- The following command runs the Python runtime tests:

  ```sh
  PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
    uv run --with numpy==2.5.1 --with pymavlink==2.4.49 --python python3 \
    python -m unittest discover -s showcase/uav-sim/runtime/tests -v
  ```

- Helm lint and template checks cover `showcase/uav-sim/deploy/helm`; the
  container builds from `servers/uav-sim-mcp/Dockerfile` (needs Docker).
- Live acceptance is a separately invoked, installation-owned billed test. It
  requires `CESIUM_ION_ACCESS_TOKEN`, NVIDIA registry access, a cluster
  granting `nvidia.com/gpu: 1`, and the Isaac Sim and PX4 runtimes. Unit and
  chart checks never require these.

## Contract Compliance

Contract revision: 3

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: pending — gateway registration does not state the contract revision
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: met
