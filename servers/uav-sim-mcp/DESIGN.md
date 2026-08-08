# UAV Simulation MCP Server

The UAV Simulation server owns the governed control surface for one authoritative
simulation runtime. The same runtime advances physics, owns the stage and streamed
world, derives operator cameras from current entity transforms, renders those cameras,
and produces their encoded video. No second process mirrors the scene or replays poses
for visualization.

> A simulation MCP server exposes governed logical cameras rendered by its authoritative
> simulation. Every active viewer lease reserves one bounded direct stream product. In
> the UAV reference, that product owns a camera clone, RTX render, NVIDIA NVENC encode,
> and native Omniverse WebRTC peer. Camera smoothing operates once on the logical-camera
> transform and never changes or delays authoritative simulation state.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | Version `2025-11-25` over the repository Streamable HTTP profile, including tools, resources, templates, subscriptions, tasks, and one MCP App. |
| JSON Schema | Draft 2020-12 strict request, result, camera, product, capacity, and health schemas. |
| `veoveo.io/live-view/v2` | Repository-owned provider-neutral profile for authoritative cameras, stable encoded products, ephemeral viewer leases, capacity, endpoints, and redacted state. |
| WebRTC and H.264 | One direct NVIDIA NVENC H.264 product, native WebRTC peer, and SRTP state for each active viewer lease. Shared-bitstream fan-out and media relays are outside this profile. |
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

`configure_world` binds a session exactly once to an immutable Frames world revision and
static simulation frame. The adapter derives the WGS 84, ECEF, ENU, NED, stage, and
Cesium mappings from that binding. Runtime configuration is immutable after admission.

Always-on installations mount the already-admitted binding from an installation-owned
ConfigMap. The MCP companion parses the strict document and applies it once during
startup. It does not begin serving when the file is absent, malformed, cross-revision,
or rejected by the simulator. This is startup configuration, not durable renderer state:
there is no poller, retry scheduler, desired-versus-realized model, or periodic replay.
Installations that intentionally begin unconfigured omit the mount and use the ordinary
tool once.

Durable task tools use `interrupted_indeterminate` recovery. An unclean interruption
never replays physical work. The default fleet controller keeps the configured fleet
on its admitted loop until a later mission command takes authority.

Restart behavior is intentionally simple. Simulator objects are runtime state. A pod
restart recreates the configured world, cameras, and products through the one-shot
installation binding. Viewer leases disappear when the MCP server restarts, and the App
opens new leases. Native WebRTC stop and signaling-failure events start a bounded
fresh-lease reconnect sequence for selected cameras. A subscribed live-camera resource
update immediately retries cameras waiting for simulator readiness. Closing a tile or
tearing down the App cancels its reconnect state, and exhausting the bounded connection
attempts waits for the next resource notification without polling. There is no
desired-versus-realized renderer deployment or periodic replay controller.

## Authoritative Operator Cameras

Configured logical cameras are created under `/World/OperatorCameras`. Each camera has a
stable logical ID, revision, and final smoothed pose. A browser lease never mutates that
definition. It reserves one preallocated physical viewer slot whose camera clone follows
the logical pose for the lifetime of that lease.

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

Each physical viewer slot owns one stable camera clone, Hydra texture, LdrColor AOV,
native signaling port, UDP media port, and H.264 product identity. The runtime copies
the selected logical camera pose into every assigned clone and submits every active
viewer viewport to Cesium in the same Kit frame. Headless operation disables the pinned
Cesium extension's interactive viewport-window update subscription, leaving one
authoritative viewport writer instead of racing an empty GUI inventory. The runtime
does not create another Cesium world, provider connection, georeference, material set,
or cache.

Viewer products have stable resources but pause their render and encode cadence while
unassigned. Assignment completes only after the first assigned RTX frame exists and the
slot's exact native signaling socket is listening. Hydra drawable events drive both
observations. One bounded activation wait protects the adapter request; it does not poll
or connect to the listener. Failure releases that exact slot before any public lease or
signaling endpoint is returned. Close, expiry, revocation, and signaling loss pause the
slot immediately.

The runtime timestamps each active clone when the current authoritative entity snapshot
becomes its USD camera pose. The corresponding Hydra drawable event closes that
source-to-render interval. Each slot retains the latest 256 event-derived samples and
publishes their nearest-rank p95 in integer microseconds. Assignment resets the window.
The runtime never uses a wall-clock sampler or health poll to produce latency evidence.

Camera capacity and viewer capacity are separate. Camera capacity accounts for physical
slots, active pixels per second, NVENC sessions, GPU memory reservation, and port slots.
Viewer capacity accounts for ephemeral leases and aggregate network bitrate. The server
rejects an exhausted dimension without reducing resolution, cadence, optics, rig,
smoothing, or codec.

The domain nadir sensor and operator cameras have independent cadence. The physical
sensor camera receives the exact current body-and-mount transform at render cadence. Its
Hydra product remains warm for streamed-world residency, but evidence capture is
requested by a physics-step cadence gate at the declared sensor rate. In-flight requests
coalesce and no wall-clock capture scheduler exists. The current sensor recording path
exposes the declared sensor frame rate and monotonic observed-frame count. Focused
live-view acceptance compares that count with simulation time while viewer slots are
assigned, which detects accidental coupling to the faster operator-camera cadence.
Its CPU readback boundary contains a `TODO(GPU)`; the intended replacement is direct
CUDA/NVENC packet fan-out once Recording Hub accepts the canonical encoded product. That
debt is not a fallback and is not acceptance evidence for live operator rendering.

## Viewer Leases And Isolation

A logical camera belongs to the Work Context output owner. Every viewer lease
additionally binds the gateway actor, one browser-instance identity, and one exclusively
assigned physical viewer slot. Two users, tabs, or browser profiles selecting the same
logical camera receive different lease IDs, tokens, camera clones, RTX products, NVENC
sessions, native WebRTC peers, and port pairs. They share only the authoritative world,
Cesium cache, and logical camera pose.

Only `open_live_view` and `renew_live_view` return the token. Resources contain redacted
lease state. The server retains a SHA-256 token hash in memory and compares it in constant
time. Renew rotates only that lease. Close, expiry, and teardown invalidate only that
lease. Closing one viewer cannot stop another viewer or rotate another actor's token.

The signaling proxy authorizes the lease before opening that slot's stable native
product endpoint, then strips the credential before forwarding. Each viewer receives
separate peer and SRTP state. Every admitted viewer increases active camera-clone,
Hydra-texture, RTX-render, Cesium-viewport, and NVENC-session counts by exactly one up to
the configured slot bound. No shared-bitstream fan-out, SFU, RTSP relay, WHEP adapter, or
software encode path exists.

Lease expiry uses one exact deadline task created with the lease. Slot release is part
of close, expiry, revocation, and signaling teardown. Neither path polls. Runtime health
is read on demand and announced through MCP subscriptions.

## Audit And Failure Isolation

Open, denial, close, expiry, camera mutation, and product-activation rejection produce
typed access events. The platform store appends each accepted audit record and outbox
projection atomically. Tokens, native endpoints, media, and provider credentials never
enter audit data.

An audit-store outage is logged with the typed action and lease identity. It cannot roll
back an already-issued lease, interrupt a viewer, or stop simulation. A product transition
failure returns `product_transition_failed`; camera and capacity failures retain their
own codes.

The Kubernetes pod starts the authoritative simulator as its first native restartable
init sidecar. Its startup probe admits the recording forwarder and UAV MCP server only
after the pod-local adapter exists. A Gateway or recording outage may delay those
dependent containers, but it cannot prevent or restart simulation. This ordering uses
the stable Kubernetes sidecar-container contract available since Kubernetes 1.29. The
simulator's preconfiguration API accepts an ephemeral-product reset as an idempotent
no-op because no live render product exists before immutable world admission. This
breaks the startup cycle while preserving mandatory cleanup after an MCP-only restart.

Gateway authority is evaluated for every MCP open, renew, close, and resource read. If
the same actor and browser instance presents a changed output owner, policy revision, or
data-label authority at renewal, the server closes that lease, records
`viewer_authority_revoked`, and leaves every unrelated viewer attached to its own
product. Other rejected renewals record `renew_denied`. Expiry invalidates the signaling
connection even when the browser disappears without teardown.

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

Focused browser acceptance combines the runtime source-to-render window with WebRTC
`requestVideoFrameCallback` capture, receive, and expected-display timestamps. It rejects
source-to-render p95 at 50 ms and motion-to-photon p95 at 200 ms. Smoothing response is
reported by the camera profile and is not counted as transport latency. The same run
opens two cameras in one App, proves one browser-instance identity with distinct leases,
products, and physical slots, observes both videos advancing, then proves both slots
return immediately to the inactive pool.

Physical-camera state includes a bounded `render_pose` agreement measurement after the
first rendered frame. It reports the rendered ENU position and forward direction beside
their position and angular error from the authoritative body-and-mount pose. Absence
means that no rendered frame is available yet; it is not synthesized from simulation
state.

## Deployment

The UAV chart deploys one pod with exactly one GPU request. The Isaac container receives
`compute,graphics,utility,video` NVIDIA capabilities. The companion Rust MCP container
does not request another GPU. Stable signaling and media port ranges are derived from
physical viewer slots and validated by the chart. `liveView.activationTimeoutSeconds`
bounds first-frame and native-listener activation without introducing a readiness
poller.

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
Cesium alignment, isolated-product multi-viewer evidence, and no software renderer,
encoder, or media relay. It also proves source-to-render p95 below 50 ms and WebRTC
capture-to-display p95 below 200 ms from reactive frame events. The smoke entry points
are:

```sh
cargo xtask smoke uav-showcase-up --context <context> --public-base-url <url>
cargo xtask smoke uav-showcase-browser-verify \
  --public-base-url <url> \
  --chrome-cdp-url http://127.0.0.1:9222
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
