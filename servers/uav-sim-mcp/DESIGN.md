# UAV Simulation MCP Server

The UAV Simulation server owns the governed control surface for one authoritative
simulation runtime. The same runtime advances physics, owns the stage and streamed
world, derives operator cameras from current entity transforms, renders those cameras,
and produces their encoded video. No second process mirrors the scene or replays poses
for visualization.

> A simulation MCP server exposes governed live cameras rendered by its authoritative
> simulation. Each camera produces at most one encoded stream product, shared by
> authorized viewer leases. Camera smoothing operates only on the operator-camera
> transform and never changes or delays authoritative simulation state.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | Version `2025-11-25` over the repository Streamable HTTP profile, including tools, resources, templates, subscriptions, tasks, and one MCP App. |
| JSON Schema | Draft 2020-12 strict request, result, camera, product, capacity, and health schemas. |
| `veoveo.io/live-view/v2` | Repository-owned provider-neutral profile for authoritative cameras, stable encoded products, ephemeral viewer leases, capacity, endpoints, and redacted state. |
| WebRTC and H.264 | One NVIDIA NVENC H.264 product for each active camera, with independent WebRTC peer and SRTP state for each viewer lease. |
| OpenUSD and RTX Hydra | Isaac Sim `6.0.1` stage and render products inside the authoritative runtime. These are implementation details, not MCP wire types. |
| OGC 3D Tiles | Cesium-backed streamed-world rendering from one simulator-owned world and cache. |
| WGS 84, ECEF, ENU, NED, and FLU | Explicit world, physics, entity, rig, and camera coordinate boundaries. |
| MAVLink 2 and ROS 2 Jazzy | Private simulator integrations. Neither protocol is projected as high-rate MCP traffic. |
| Rerun RRD | Version `0.35.0` recording data and producer-authored Blueprint stores sent independently to Recording Hub. |
| NVIDIA Container Runtime | One Kubernetes GPU allocation with compute, graphics, utility, and video driver capabilities. CPU rendering and encoding are unsupported. |

## Authority Boundary

The simulation runtime is authoritative for physics, entity transforms, the OpenUSD
stage, Cesium georeference, domain sensors, operator cameras, Hydra render products,
and NVENC products. The Rust server owns caller authorization, MCP state projection,
ephemeral viewer leases, signaling authorization, and access audit.

The pod-local adapter is the only boundary between those responsibilities. It exposes
typed configuration, command, state, and live-product activation operations. It does
not carry a visualization pose stream. No MCP request participates in the physics or
render loop.

```text
gateway actor
    |
    v
UAV Simulation MCP server
  governance + ephemeral viewer leases + signaling + audit + App
    |
    | pod-local typed adapter
    v
authoritative Isaac runtime
  physics + USD/Cesium + operator cameras + Hydra + NVENC
    |
    +---- WebRTC viewer A
    +---- WebRTC viewer B
```

Simulation never waits for recording, Rerun playback, audit persistence, a browser, or
an MCP consumer. Those boundaries may report failure, but they cannot stop authoritative
physics.

## MCP Surface

The server owns the `uav-sim://` scheme, slug `uav-sim`, MCP path `/uav-sim/mcp`,
and port `8802`.

The domain tools govern session configuration, simulation execution, missions, dataset
capture, and typed inspection. The live-view profile adds:

- `list_live_cameras`
- `open_live_view`
- `renew_live_view`
- `close_live_view`
- `set_operator_camera`

Live state uses these canonical resources:

- `uav-sim://session/{session_id}/live-cameras`
- `uav-sim://session/{session_id}/live-camera/{camera_id}`
- `uav-sim://session/{session_id}/stream-products`
- `uav-sim://session/{session_id}/stream-product/{product_id}`
- `uav-sim://session/{session_id}/live-views`
- `uav-sim://session/{session_id}/live-view/{live_view_id}`

`ui://uav-sim/live.html` is the only live-view App resource. There are no aliases for
the removed hosted viewer service, scene mirror, or pose protocol.

## World And Session Lifecycle

Each session starts unconfigured. `configure_world` binds the session exactly once to
an immutable Frames world revision and static simulation frame. The adapter derives
the WGS 84, ECEF, ENU, NED, stage, and Cesium mappings from that binding. Runtime
configuration is immutable after admission.

Durable task tools use `interrupted_indeterminate` recovery. An unclean interruption
never replays physical work. The default fleet controller keeps the configured fleet
on its admitted loop until a later mission command takes authority.

Restart behavior is intentionally simple. Simulator objects are runtime state. A
simulator restart recreates its configured cameras and products during normal startup.
Viewer leases disappear when the MCP server restarts, and the App opens new leases.
There is no desired-versus-realized renderer deployment or periodic replay controller.

## Authoritative Operator Cameras

Configured cameras are created under `/World/OperatorCameras`. Each camera has a stable
logical ID, revision, physical slot, Hydra texture identity, and stream-product ID. A
browser connection never creates or reassigns a camera.

The admitted rig set is:

| Rig | Behavior |
|---|---|
| `fixed` | Applies an exact world pose without smoothing state. |
| `look_at` | Keeps the configured eye and follows a world point or entity through orientation. |
| `orbit` | Holds a configured radius, elevation, and azimuth around one authoritative entity. |
| `follow_entity` | Applies FLU eye and target offsets to the current entity transform. |
| `chase_entity` | Derives a trailing eye and target from the same current entity transform. |
| `stabilized_mounted_entity` | Composes an entity and mount transform, then smooths the operator view only. |
| `formation_overview` | Frames the current centroid and bounds of a configured entity set. |

Every rig claimed by the contract has deterministic desired-pose tests. Operator-camera
transforms never feed back into an entity, sensor, mission, or physics state.

## Camera Smoothing

Smoothing uses a frame-rate-independent exponential filter over the final desired camera
pose. Translation uses linear interpolation. Orientation uses normalized shortest-arc
quaternion SLERP.

```text
alpha = 1 - 2^(-dt / half_life)
```

The typed profile contains `translationHalfLifeMs`, `rotationHalfLifeMs`,
`teleportDistanceMillimetres`, and `resetAfterGapMs`. Zero half-life snaps the relevant
component. A target change, camera revision, simulation generation, long render gap, or
teleport resets the filter. The filter stores one previous camera pose and no entity-pose
history.

Chase eye and target calculations consume the same authoritative transform at one
physics step. Formation cameras consume one snapshot of all selected entity transforms.
This prevents camera-target disagreement without delaying simulation.

## Render Products

Each physical slot owns one stable camera, Hydra texture, LdrColor AOV, native signaling
port, UDP media port, and H.264 product identity. The runtime submits every active camera
viewport to Cesium in the same Kit frame. It does not create another Cesium world,
provider connection, georeference, material set, or cache.

Continuous products stay warm. On-demand products have stable resources but pause their
render and encode cadence while unused. The first viewer activates an on-demand product.
The last close or expiry starts a bounded idle grace, after which the product pauses if
no new viewer exists. Startup deactivates every stale on-demand product once.

Camera capacity and viewer capacity are separate. Camera capacity accounts for physical
slots, active pixels per second, NVENC sessions, GPU memory reservation, and port slots.
Viewer capacity accounts for ephemeral leases and aggregate network bitrate. The server
rejects an exhausted dimension without reducing resolution, cadence, optics, rig,
smoothing, or codec.

The domain nadir sensor and operator cameras have independent cadence. The current
sensor recording path contains a `TODO(GPU)` at its CPU readback boundary; the intended
replacement is direct CUDA/NVENC packet fan-out once Recording Hub accepts the canonical
encoded product. That debt is not a fallback and is not acceptance evidence for live
operator rendering.

## Viewer Leases And Fan-Out

A logical camera and encoded product belong to the Work Context output owner. Every
viewer lease additionally binds the gateway actor and one browser-instance identity.
Two users, tabs, or browser profiles receive different lease IDs and tokens while
sharing the same camera and product.

Only `open_live_view` and `renew_live_view` return the token. Resources contain redacted
lease state. The server retains a SHA-256 token hash in memory and compares it in constant
time. Renew rotates only that lease. Close, expiry, and teardown invalidate only that
lease. Closing one viewer cannot stop another viewer or rotate another actor's token.

The signaling proxy authorizes the lease before opening the stable native product
endpoint, then strips the credential before forwarding. Each viewer receives separate
peer and SRTP state. Viewer count must not increase camera, Hydra texture, render, Cesium
viewport, or NVENC-session counts.

Lease expiry uses one exact deadline task created with the lease. Product idle shutdown
uses one deadline created by the last close. Neither path polls. Runtime health is read
on demand and announced through MCP subscriptions.

## Audit And Failure Isolation

Open, denial, close, expiry, camera mutation, and product-activation rejection produce
typed access events. The platform store appends each accepted audit record and outbox
projection atomically. Tokens, native endpoints, media, and provider credentials never
enter audit data.

An audit-store outage is logged with the typed action and lease identity. It cannot roll
back an already-issued lease, interrupt a viewer, or stop simulation. A product transition
failure returns `product_transition_failed`; camera and capacity failures retain their
own codes.

Gateway authority is evaluated for every MCP open, renew, close, and resource read. A
lease is bounded and cannot renew after its actor loses authority. Expiry invalidates
the signaling connection even when the browser disappears without teardown.

## Live App

The App discovers the authoritative camera collection and maintains at most one lease
per selected camera in one browser instance. It supports one primary view and a bounded
multi-camera grid. Renewals rotate only that tile's lease. Removing a tile or tearing
down the App closes its lease.

Each tile reports requested and decoded dimensions, cadence, frame age, transport state,
camera health, smoothing profile, and attribution. Media Capabilities labels H.264
decode as hardware only when the browser reports `powerEfficient`; supported smooth
software decode is labeled explicitly. Browser acceptance still requires a headed,
hardware-backed WebGPU or WebGL context.

## Deployment

The UAV chart deploys one pod with exactly one GPU request. The Isaac container receives
`compute,graphics,utility,video` NVIDIA capabilities. The companion Rust MCP container
does not request another GPU. Stable signaling and media port ranges are derived from
physical camera slots and validated by the chart.

The same pod owns MCP HTTP, the public signaling proxy, private native signaling ports,
public UDP media ports, one Cesium cache, and the authoritative runtime. Network policy
admits gateway traffic, signaling, bounded media, DNS, and the configured public TLS
world provider. Provider credentials come from installation-owned Secrets and never
appear in Helm-rendered ConfigMaps or MCP state.

The platform chart contains no generic simulation renderer, pose ingress, mirror cache,
or live-view GPU workload. External simulation implementations package the same
domain-owned boundary in their own release.

## Recording Isolation

Recording publication is asynchronous and bounded. Queue pressure may drop recording
events according to the recording policy, but it never delays physics, camera transforms,
or operator rendering. A Recording Hub, Rerun, object-store, or browser failure changes
recording health only.

The recording producer emits one recording store plus an associated producer Blueprint.
The Blueprint selects the fleet, leader camera, and map views. Viewer-local layout changes
remain separate from that producer default.

## Conformance And Acceptance

Deterministic coverage proves strict camera parsing, all rig computations, half-life
behavior at several frame rates, shortest-arc normalization, reset rules, stable product
identity, exact capacity, lease isolation, token rotation, expiry, on-demand activation,
signaling redaction, and App teardown.

The external fixture proves that another simulation server can own its camera/product,
viewer, signaling, and App contract without importing an Isaac-specific renderer or a
pose-mirroring protocol. First-party visual certification remains the GPU UAV showcase.

Hardware acceptance requires one NVIDIA GPU, a headed hardware-backed browser, RTX
rendering, NVIDIA NVENC, advancing H.264 frames, several authoritative cameras, correct
Cesium alignment, shared-product multi-viewer evidence, and no software renderer or
encoder. The smoke entry points are:

```sh
cargo xtask smoke uav-showcase-up --context <context> --public-base-url <url>
cargo xtask smoke uav-showcase-verify --context <context> --public-base-url <url>
```

The Python camera and runtime suite runs with:

```sh
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.5.1 --with pymavlink==2.4.49 --python python3 \
  python -m unittest discover -s showcase/uav-sim/runtime/tests -v
```

## Contract Compliance

The server implements MCP contract revision 2. Its control-plane registration declares
that revision and the complete tool, resource, subscription, task, and App capabilities.
Compliance gaps are not hidden in deployment values or fixture behavior.
