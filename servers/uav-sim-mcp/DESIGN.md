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
| Model Context Protocol | Version `2026-07-28` over the repository stateless Streamable HTTP profile, including Discover, tools, resources, templates, `subscriptions/listen`, official Tasks, and one MCP App. |
| JSON Schema | Draft 2020-12 strict request, result, camera, product, capacity, and health schemas. |
| `veoveo.io/live-view/v2` | Repository-owned provider-neutral profile for authoritative cameras, stable encoded products, ephemeral viewer leases, capacity, endpoints, and redacted state. |
| `veoveo.io/uav-runtime-event/v2` | Private authenticated HTTP/1.1 NDJSON stream carrying an `adapter_ready` edge before world admission and a final `ready` edge after authoritative visual admission. It is an internal adapter event, not a public MCP resource or a simulation control protocol. |
| WebRTC and H.264 | One direct NVIDIA NVENC H.264 product, native WebRTC peer, and SRTP state for each active viewer lease. Shared-bitstream fan-out and media relays are outside this profile. |
| OpenUSD and RTX Hydra | Isaac Sim `6.0.1` stage and render products inside the authoritative runtime. These are implementation details, not MCP wire types. |
| Native sensor video | Isaac Sim `6.0.1` packages `omni.kit.livestream.aov` `10.2.0` and `omni.kit.livestream.rtsp` `10.2.3`. The private adapter consumes the loopback RTSP/RTP H.264 stream without decoding or re-encoding. This is not an MCP wire type. |
| RTSP, RTP, and H.264 | RTSP 1.0 over loopback TCP with interleaved RTP/RTCP. The adapter supports the RFC 6184 single-NAL, STAP-A, and FU-A packetization modes and emits decoder-reentrant Annex B access units. |
| OGC 3D Tiles | Cesium Omniverse `0.29.0` with pinned Cesium Native commit `ca0311f25c412b74ad1af9a3636924122cc76156`, one simulator-owned world, and one cache. The repository extension adds private redacted lifecycle events; it does not add an MCP wire protocol. |
| WGS 84, ECEF, ENU, NED, and FLU | Explicit world, physics, entity, rig, and camera coordinate boundaries. |
| `veoveo.io/map-route-handoff/v1` | Map MCP-owned, execution-neutral route projection with exact route, digest, mobility-profile, snapshot, release, restriction, and validation provenance. |
| `frames://world/{world_id}/revision/{revision_id}` | Frames MCP-owned immutable world revision identity consumed by session configuration and mission admission. |
| MAVLink 2 and ROS 2 Jazzy | Private simulator integrations. Neither protocol is projected as high-rate MCP traffic. |
| Rerun RRD | Version `0.35.0` recording data and producer-authored Blueprint stores sent independently to Recording Hub. |
| NVIDIA Container Runtime | One Kubernetes GPU allocation with compute, graphics, utility, and video driver capabilities. CPU rendering and encoding are unsupported. |

## Authority Boundary

The simulation runtime is authoritative for physics, entity transforms, the OpenUSD
stage, Cesium georeference, domain sensors, operator cameras, Hydra render products,
and NVENC products. The Rust server owns caller authorization, principal-to-vehicle
grants, mission admission, exclusive command leases, MCP state projection, ephemeral
viewer leases, signaling authorization, and access audit. Map MCP owns operational
geography, place resolution, mobility profiles, restrictions, routing, and the route
handoff. Frames MCP owns world trees and immutable revisions. The UAV server consumes
those exact products and never reimplements either vertical.

The cluster-private adapter is the only boundary between those responsibilities. It exposes
typed configuration, command, state, and live-product activation operations. It does
not carry a visualization pose stream. No MCP request participates in the physics or
render loop.

```text
gateway actor
    |
    v
UAV Simulation MCP server
  grants + mission admission + command leases + telemetry + App
    |
    | authenticated cluster-private typed adapter
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

The domain tools govern session configuration, simulation execution, single-vehicle
mission admission, dataset capture, and typed inspection. Vehicle authority uses:

- `grant_vehicle_control`
- `revoke_vehicle_control`
- `prepare_vehicle_mission`
- `execute_vehicle_mission_plan`

The old public multi-vehicle `execute_mission` surface does not exist. The simulator's
typed multi-vehicle adapter request is cluster-private and cannot establish principal
authority. The live-view profile adds:

- `list_live_cameras`
- `open_live_view`
- `renew_live_view`
- `close_live_view`

Live state uses these canonical resources:

- `uav-sim://session/{session_id}/live-cameras`
- `uav-sim://session/{session_id}/live-camera/{camera_id}`
- `uav-sim://session/{session_id}/stream-products`
- `uav-sim://session/{session_id}/stream-product/{product_id}`
- `uav-sim://session/{session_id}/live-views`
- `uav-sim://session/{session_id}/live-view/{live_view_id}`
- `uav-sim://control-grants`
- `uav-sim://control-grant/{grant_id}`
- `uav-sim://mission-plans`
- `uav-sim://mission-plan/{plan_id}`

`ui://uav-sim/live.html` is the only live-view App resource. There are no aliases for
the removed hosted viewer service, scene mirror, or pose protocol. An installation may
declare exact generic agent ids with `UAV_SIM_AGENT_MESSAGE_TARGETS`. The App resource
publishes that closed list through the Apps extension, and the Console's authenticated
human-message bridge projects it into App host context. This gives the domain App a
prompt surface without granting the iframe agent credentials or adding UAV concepts to
the Console.

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

## Vehicle Authority And Mission Admission

An authenticated gateway principal controls a vehicle only through a UAV-owned grant.
The grant binds the exact principal key, Work Context, session, vehicle, permissions,
validity interval, and one versioned Map mobility-profile URI. Initial showcase policy
uses one principal per vehicle. This is packaging policy rather than a platform concept;
the UAV contract also supports bounded many-to-one grants when a later use case admits
them explicitly.

`inspect`, `plan`, `execute`, and `abort` are separate permissions. A caller with only
`uav-sim:control` sees telemetry for vehicles covered by a current `inspect` grant. The
`uav-sim:read` and `uav-sim:admin` scopes retain domain-wide operator visibility. Tool
metadata, an agent manifest, a chat target, or a claimed vehicle ID never creates
vehicle authority.

Mission admission has one vertical handoff:

```text
operator prompt
  -> Map MCP resolves places, applies active data, and routes
  -> Map MCP prepares veoveo.io/map-route-handoff/v1
  -> UAV MCP verifies grant, profile, provenance, freshness, and Frames revision
  -> UAV MCP persists one principal-bound single-vehicle plan
  -> UAV MCP acquires the exclusive vehicle command lease
  -> private simulator adapter executes the admitted waypoints
```

The handoff contains WGS 84 geometry and Map provenance, but no UAV command semantics.
The UAV server converts its admitted path into the private simulator request, applies
the granted speed and destination hold, and never asks Map to execute a vehicle. Mission
plans expire after 15 minutes. Route validation may be at most five minutes old when a
plan is admitted. Every executable position requires ellipsoidal height. A
planning-advisory route is accepted only when the vehicle grant says so.

Execution acquires one durable exclusive lease for the Work Context, session, and
vehicle. A concurrent mission for that vehicle fails closed. Completion, failure,
cancellation, task-start failure, and task-lease loss all finalize the mission plan and
release the exact command lease. Physical task interruption remains indeterminate and
is never replayed.

Restart behavior is intentionally simple. Simulator objects are runtime state. A pod
restart recreates the configured world, cameras, and products through the one-shot
installation binding. Viewer leases disappear when the MCP server restarts, and the App
opens new leases. Native WebRTC stop and signaling-failure events start a fresh-lease
reconnect sequence with a five-second maximum backoff for selected cameras. The App
renews its resource subscription before every open attempt, so an MCP companion restart
cannot strand recovery behind a stale session subscription. A subscribed live-camera
resource update immediately retries cameras waiting for simulator readiness. The runtime
emits an
`adapter_ready` edge after its preconfiguration endpoint binds, allowing an existing
companion to reapply the same immutable installation binding after an independent
simulator-container restart. It emits a second `ready` edge after its running lifecycle
and streamed-world readiness are both current; that edge produces the subscribed resource
update. Both use a private Unix datagram. Delivery is best effort and never delays the
simulator. If the companion is absent, its later startup applies the installation binding
directly. Closing a tile or tearing down the App cancels its reconnect state. A selected
camera keeps retrying at the capped backoff until it succeeds, is deselected, or the App
tears down. There is no desired-versus-realized renderer deployment or periodic replay
controller.

The streamed-world data plane has a smaller reactive lifecycle inside the simulator.
The pinned Cesium extension emits a typed event when the ion endpoint, root tileset, or
tile content request fails. Events contain only the tileset path, load generation, load
type, and HTTP status. Provider URLs, keys, sessions, tokens, headers, and response bodies
never enter the event or projected runtime state.

A rejected tile-content session produces one generation-safe tileset refresh. Duplicate
failures from the rejected generation collapse into the same action. The replacement
generation must report native load completion and current visible coverage before it is
ready. A rejected replacement becomes `degraded` without a reload loop. Credential,
quota, transport, asset, and provider failures are typed directly and never masquerade
as a provider-session refresh.

Render statistics describe current coverage; they do not infer network failure. Zero
visible tiles can be valid while a camera crosses an unavailable footprint or while
refinement is active, so no elapsed-time or visibility threshold triggers provider work.
This lifecycle can change visual readiness, but it cannot change simulation readiness,
physics, pose flow, missions, recording publication, or an already active native camera
product.

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

Product assignment is transaction-like across the MCP companion and simulator boundary.
After any ambiguous assignment or release result, the companion rereads the exact slot,
requires one unique slot identity, and releases only an assignment whose `liveViewId`
matches the operation. It rereads the slot again and accepts cleanup only after the full
inactive product state is observable. Before physical capacity denial, the companion
compares active products with its active logical leases and reclaims exact untracked
assignments. Missing identity, identity mismatch, duplicate or missing slots, unavailable
runtime state, failed release, and unobserved release fail closed as
`orphan_product_cleanup_failed` with bounded audit details. An individual open, renewal,
close, expiry, or recovery never invokes the runtime's broad product reset.

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
sensor camera receives the exact current body-and-mount transform at render cadence.
Cesium receives that viewport every Kit update. The sensor Hydra product renders at the
declared sensor rate and publishes its CUDA-resident `LdrColor` AOV directly to the
native RTSP extension. No Replicator orchestrator participates in simulation timing.

The manual Isaac loop advances fixed physics from elapsed monotonic time. It preserves
bounded physics debt and coalesces missed visual deadlines into one render of the newest
authoritative state. RTX, Cesium, NVENC, WebRTC, Recording, and Rerun work can reduce
presentation cadence under load, but none of them changes the simulation clock.

The RTSP extension performs one NVIDIA NVENC encode and serves the resulting GOP on a
pod-local loopback transport. The private adapter depacketizes RFC 6184 payloads without
decoding, copying pixels, or encoding again. It qualifies SPS, PPS, IDR, and predicted
access units, then fans each exact encoded access unit to Recording/Rerun and the
optional live RTP publisher. Raw sensor pixels never enter Python or Recording.
The RTSP AOV also receives an explicit internal signal-port reservation adjacent to its
RTSP listener. This prevents the livestream core's otherwise unused `49100` default from
colliding with the first locked operator WebRTC endpoint.

Recording Hub admits ordinary H.264 GOPs and rolls storage only at a decoder-reentrant
IDR boundary. The sensor path exposes its declared rate, monotonic observed-frame count,
last encoded size, and keyframe state. Headed hardware-backed browser acceptance verifies
the actual visual content; runtime health does not require a diagnostic pixel readback.

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

The byte-transparent signaling gate authorizes the lease before opening that slot's
stable native product endpoint, then strips the credential from the forwarded HTTP
upgrade. It never terminates or reframes the upgraded WebSocket. Each viewer receives
separate peer and SRTP state. Every admitted viewer increases active camera-clone,
Hydra-texture, RTX-render, Cesium-viewport, and NVENC-session counts by exactly one up to
the configured slot bound. No shared-bitstream fan-out, SFU, RTSP relay, WHEP adapter, or
software encode path exists.

The native client may resume its signaling socket after the media peer is established.
A credentialed `reconnect=1` request reuses the already-admitted lease and slot without
increasing the connected-viewer count. It cannot create the initial admission, revive a
closed lease, or bypass token and expiry checks.

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

The GPU Deployment starts the authoritative simulator and recording forwarder together.
The independent MCP Deployment starts whenever the platform database is reachable and
reports unavailable until the authenticated simulator adapter answers. A Gateway, MCP,
or recording outage cannot prevent or restart simulation. The simulator's
preconfiguration API accepts an ephemeral-product reset as an idempotent no-op because
no live render product exists before immutable world admission. This breaks the startup
cycle while preserving mandatory cleanup after an MCP-only restart.

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
camera health, smoothing profile, and attribution. Cadence values use fixed-width,
zero-padded integer labels with tabular monospace numerals, so telemetry updates do not
move or wrap the video overlay. Media Capabilities labels H.264
decode as hardware only when the browser reports `powerEfficient`; supported smooth
software decode is labeled explicitly. Browser acceptance still requires a headed,
hardware-backed WebGPU or WebGL context.

The pinned NVIDIA browser client treats Compute Pressure as optional telemetry. An
opaque-origin App cannot invoke that browser API safely, so the App masks
`PressureObserver` before the client initializes. The client's native capability check
then leaves pressure observation disabled while retaining the same RTX rendering, NVENC
encoding, and WebRTC transport. NVIDIA's local AVIF capability probe uses a CSP-admitted
`data:` fetch and does not add a remote origin.

Focused browser acceptance combines the runtime source-to-render window with WebRTC
`requestVideoFrameCallback` capture, receive, and expected-display timestamps. It rejects
source-to-render p95 at 85 ms and motion-to-photon p95 at 250 ms. The measured
two-viewer reference profile targets 16 FPS and rejects delivery below 12 FPS. NVIDIA's
total frame- and packet-loss counters are evaluated as deltas after the 256-event warm
window. Smoothing response is reported by the camera profile and is not counted as
transport latency. The same run
opens two cameras in one App, proves one browser-instance identity with distinct leases,
products, and physical slots, observes both videos advancing, then proves both slots
return immediately to the inactive pool.

Restart acceptance keeps the same headed App document and viewer-instance identity
mounted while independently restarting the MCP pod and the Isaac container. MCP restart
evidence requires the simulator pod UID, container ID, and restart count to remain
unchanged. Each recovery must produce a new lease, advancing native video, and unchanged
hardware-browser evidence. The stable preallocated product ID may be reused when the
allocator selects the same physical slot.

The MCP image uses a minimal PID 1 shell that forwards pod termination to one
Rust child and exits with that child. Restart acceptance terminates that sole child
through `kubectl exec`, which lets kubelet restart the MCP container without replacing
its pod or the authoritative simulator Deployment. No restart endpoint or public
control surface exists.

Physical-camera state includes a bounded `render_pose` agreement measurement after the
first rendered frame. It reports the rendered ENU position and forward direction beside
their position and angular error from the authoritative body-and-mount pose. Absence
means that no rendered frame is available yet; it is not synthesized from simulation
state.

## Deployment

The UAV chart deploys one GPU runtime Deployment and one independent MCP Deployment.
Only the Isaac container requests one GPU and receives
`compute,graphics,utility,video` NVIDIA capabilities. Stable signaling and media port
ranges are derived from physical viewer slots and validated by the chart.
`liveView.activationTimeoutSeconds` bounds first-frame and native-listener activation
without introducing a readiness poller.

The runtime Service exposes the authenticated adapter and private native signaling to
the MCP pod. Public UDP media selects the runtime pod directly. The MCP Service owns MCP
HTTP and the public byte-transparent signaling gate. Separate network policies admit
only those edges, platform dependencies, DNS, and the configured public TLS world
provider. Provider and adapter credentials come from distinct installation-owned
Secrets and never appear in Helm-rendered ConfigMaps or MCP state.

The runtime retains its latest typed lifecycle edge and exposes it on the authenticated
NDJSON stream. The MCP consumer reconnects only after transport failure, receives the
latest edge immediately, reapplies the immutable binding after `adapter_ready`, and
projects final `ready` through the subscribed live-camera resource. A missing consumer
never blocks the runtime.

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
encoder, or media relay. It also proves delivery of at least 12 FPS, source-to-render
p95 below 85 ms, and WebRTC capture-to-display p95 below 250 ms from reactive frame
events. The smoke entry points
are:

```sh
cargo xtask smoke uav-showcase-up --context <context> --public-base-url <url>
cargo xtask smoke uav-showcase-browser-verify \
  --public-base-url <url> \
  --chrome-cdp-url http://127.0.0.1:9222
cargo xtask smoke uav-showcase-live-restart-verify \
  --context <context> \
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
