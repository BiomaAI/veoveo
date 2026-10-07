# UAV Sim MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 4.

## Purpose

Governs interactive and durable work against UAV simulation sessions without
exposing simulator native control ports through the gateway. The server owns
the typed simulation protocol, caller ownership, task state, resource
identities, subscriptions, and recording references; the simulator adapter
owns Isaac stage mutation, Cesium tiles, Newton rigid bodies, the Warp UAV
plant, PX4 transport, and authoritative operator cameras and encoded products.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `UavSimMcp` implements
  `DomainServer` and is its own `ResourceSubscriptions` source, reusing the read
  path's checks. `serve_with_shutdown` runs it beside the live-stream gate, and
  either stopping ends both. Do not add a `ServerHandler`, router, host check or
  authentication middleware here.
- Declare MCP scopes, resource descriptors, templates, capabilities and documents in
  `server/setup.rs`. Startup and handlers consume `McpServerSetup<UavContract>`.
  Keep checked declaration metadata separate from caller authority. Every added
  template must expand to its typed domain builder in the setup qualification.

- Use `UavResource` and the typed `uris` constructors for resource addresses.
  Collection cursors belong to the contract: identity positions use revision 1, while usage positions use revision 2.
  Decode each resource once; reads and subscriptions share scopes and parent checks.
  Relative IDs fail construction and retained JSON admission. Apply the design's
  retained-data and provider preflight before a coordinated upgrade.

- Public UAV and live-view v4 types belong in this library's `contract` module.
  Keep the `contract` feature free of MCP, async, database, adapter and GPU dependencies.
  Other crates import it with default features disabled. Gateway-to-domain ownership
  conversion belongs in the authenticated server adapter; MCP core exports no live-view types.

- Use `UavScope` in domain permission checks and Task admission. Ordinary and Task
  routes apply the same scope requirement before starting work: scenarios and captures
  require `Admin`, and mission-plan execution requires `Control` plus its vehicle grant.
  Unrelated validated grants remain admissible. Clients import scope spellings from
  this contract library; installation configuration retains validated wire names.

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
  restrictions, routing, and `veoveo.ai/map-route-handoff/v2`. Frames MCP owns
  immutable world revisions. This server owns principal-to-vehicle grants,
  mission admission, exclusive command leases, execution, telemetry, and its
  domain App. Do not move Map or Frames behavior into UAV code.
- Import Map handoffs and profile addresses from its contract-only library. UAV admits
  only validated or explicitly granted planning-advisory routes. Profile and advisory
  grant predicates run in SQL before selection and during execution admission. Check
  selected mission documents against their indexed metadata before use.
- A gateway scope, agent manifest, message target, or requested vehicle ID never
  grants vehicle authority. Every vehicle mutation requires a current UAV-owned
  principal grant; mission execution additionally requires the exact admitted
  plan revision and an exclusive vehicle command lease. Acquire that lease and admit
  the plan with its exact retained Task link in one transaction. Create and pin the
  queued Task first; compose admission through TaskRuntime's queued-snapshot guard.
  Finalize the plan and release its admitting token together.
  Lease expiry cannot displace an executing plan. Preserve uncertain outcomes for
  reconciliation, and reject exhausted revisions without modifying retained records.
- Dispatch consumes the admission guard. Only a correlated simulator completion can
  release a dispatched mission's authority. Task cancellation, lease loss, HTTP rejection
  or timeout, and invalid responses preserve the vehicle fence. Resolve recording
  references after settling physical completion. Queued mission recovery never replays
  a public plan request as a private simulator command. Keep unresolved mission Tasks
  pinned. Release only this domain's pin after settlement or proven non-admission;
  select startup retention candidates in SQL before paging.
- Every session starts `unconfigured`. `configure_world` binds it exactly once
  to an immutable Frames world revision and a static simulation frame from that
  revision. The adapter derives Cesium and Newton fleet georeferencing from that
  binding, converts ENU/NED locally, and makes no MCP calls in the physics loop.
- Operator cameras, RTX rendering, and NVIDIA NVENC products stay inside the
  authoritative simulator. This server owns actor-and-browser stream
  authorization, access audit, WebSocket admission, and `ui://uav-sim/live.html`.
  It never persists renderer desired state or mirrors entity poses.
- Every streamable logical camera owns one continuously active RTX render,
  Cesium viewport, NVIDIA NVENC session, and decoder-reentrant Annex B H.264
  product. Authorized viewers share that exact bitstream. Viewer count never
  changes render-product or NVENC-session count, and no viewer quota is exposed.
- `CESIUM_ION_ACCESS_TOKEN` comes only from the dedicated Kubernetes Secret.
  It is never a tool argument, ConfigMap value, resource field, log field, or
  exported USD content.
- Import public Recording IDs and addresses from `veoveo-recording-contract`.
  `RecordingCatalog` owns readiness and its single URI; derive repeated IDs during
  serialization and reject conflicting wire fields. Keep private producer keys typed
  through catalog lookup and completion handling.
- Recording state publishes its producer key and typed catalog lifecycle
  immediately. The canonical `recording://recordings/{recording_id}` identity
  appears only after catalog resolution; catalog delay or failure never blocks
  simulation, operator rendering, or live-stream delivery. Native Recording Hub
  ports stay private.

## Build And Test

- `cargo check -p veoveo-uav-sim-mcp`
- `cargo test -p veoveo-uav-sim-mcp --no-default-features --features contract --lib --test contract`
- Qualify contract dependencies with a separate consumer workspace; workspace feature
  unification cannot prove isolation. `tests/contract.rs` compares all public schema
  snapshots and can run from that consumer.
  After an intentional schema change, regenerate through the same test with
  `VEOVEO_UPDATE_UAV_CONTRACT_SCHEMA=1 cargo test -p veoveo-uav-sim-mcp --test contract schemas_preserve_the_published_contract`,
  review the snapshot diff, then run the test without the update variable.
- `cargo test -p veoveo-uav-sim-mcp` (deterministic fake adapter, credential
  free)
- The following command runs the Python runtime tests:

  ```sh
  PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
    uv run --with pydantic==2.13.5 --with numpy==2.3.1 --with aiohttp==3.14.1 \
    --with pymavlink==2.4.49 --with fastcrc==0.3.6 --python 3.13 \
    python -m unittest discover -s showcase/uav-sim/runtime/tests -v
  ```

- Helm lint and template checks cover `showcase/uav-sim/deploy/helm`; the
  container builds from `servers/uav-sim-mcp/Dockerfile` (needs Docker).
- The plant's stationary barometer qualification runs separately on NVIDIA CUDA
  with the Warp version pinned by the simulation runtime lock. It fails when CUDA
  is unavailable and does not start Isaac or the cluster:

  ```sh
  PYTHONPATH=showcase/uav-sim/runtime \
    timeout 120s uv run --with warp-lang==1.16.0 --with numpy==2.3.1 --python 3.12 \
    python showcase/uav-sim/runtime/tests_gpu/test_plant.py
  ```

  This checks the CUDA plant kernel, seeded sensor variance and stationary ground
  truth. Installed acceptance still qualifies PX4 re-arming and composed flight.
- Live acceptance is a separately invoked, installation-owned billed test. It
  requires `CESIUM_ION_ACCESS_TOKEN`, NVIDIA registry access, a cluster
  granting `nvidia.com/gpu: 1`, and the Isaac Sim and PX4 runtimes. Unit and
  chart checks never require these.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: met
- C02: met
- C03: met
- C04: met — grant, plan, mission, and usage collections use 100-item SQL pages; direct reads use domain identities
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
- C17: met — local and reference gateway registrations declare contract revision 4
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met — Store LIVE sources with changefeed recovery invalidate durable collection and exact-resource subscriptions
- C28: met — discovery lists roots and templates without simulator reads or dynamic record enumeration; agent-target metadata retains its declared invalidation source
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — native discovery is qualified with an unreachable simulator; installed readiness qualification is pending
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
