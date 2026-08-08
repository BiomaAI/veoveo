# Simulator-hosted live-view hard cut

  This approved implementation plan supersedes the mirror-oriented Simulation View
  architecture. The target is a reference implementation with multiple authoritative
  cameras, smooth cinematic tracking, and a bounded native NVIDIA stream product for
  each active viewer.

  ## Execution Scope And Completion

  This plan owns the simulator-hosted live-view path from an authoritative logical
  camera through one lease-bound native NVIDIA WebRTC product to a headed hardware-GPU
  browser. It also owns the shared live-view contract, UAV MCP governance and signaling,
  preallocated viewer-slot runtime, deployment surface, removal of the mirror stack, and
  focused conformance and acceptance evidence described below.

  Physical sensor capture remains in scope only at the separation boundary. Live-view
  acceptance proves that opening and closing viewer slots does not change physics time,
  sensor cadence, simulation authority, or sensor product identity. The independent
  health of Stream MCP sessions, Recording ingest and archive playback, mission control,
  unrelated applications, and installation-wide composed verification is not a
  completion gate for this plan. A failure in one of those systems blocks this plan only
  when focused evidence attributes the failure to the live-view implementation.

  The execution gates are:

  | Gate | Required evidence | Blocking boundary |
  |---|---|---|
  | Contract and governance | Focused Rust schema, ownership, lease, redaction, audit, capacity, and signaling tests | Failures in the shared live-view or UAV MCP surfaces block completion. |
  | Authoritative runtime | Mathematical and synthetic camera tests plus pod-local runtime contract tests | Failures in logical cameras, smoothing, slot allocation, native product lifecycle, or Cesium viewport ownership block completion. |
  | Native GPU media | Runtime evidence for RTX, one NVENC session and native Omniverse WebRTC peer per assigned slot, with no software or relay path | Any fallback, relay, duplicate simulation process, or unproven native product blocks completion. |
  | Browser behavior | Focused headed hardware-GPU acceptance for camera selection, distinct viewers, teardown, restart recovery, cadence, and latency | Browser or media failures caused by this path block completion. |
  | Isolation | Focused proof that simulation remains authoritative and viewer activity does not alter physics or sensor cadence | Independent downstream sensor consumers do not block completion. |
  | Hard cut and documentation | Repository search, deployment closure, owning design documents, and removal of this completed plan | Any remaining advertised, built, deployed, or tested mirror or relay path blocks completion. |

  Broad showcase verification is useful follow-on evidence, but it is not a substitute
  for these focused gates. When it fails, the failure must first be attributed to an
  owning component. Unrelated defects are recorded in a committed owning document and do
  not redirect this implementation.

  Completion does not mean retaining this plan as a second design authority. Once every
  gate above and every requirement below is implemented and proven, the final contract
  belongs in the component design documents listed in section 23. This plan is then
  deleted in the same commit that removes its entry from `docs/CODEMAP.md`.

  ## Standards And Protocols

  | Standard or protocol | Planned profile |
  |---|---|
  | Model Context Protocol | Version `2025-11-25` through the hosted-server contract, with typed tools, resources, subscriptions, notifications, and an MCP App owned by each simulation server. |
  | JSON Schema | Draft 2020-12 strict camera, product, capacity, health, and viewer-lease schemas. |
  | `veoveo.io/live-view/v2` | Provider-neutral logical-camera, viewer-product, ownership, signaling, capacity, and redaction contract. This is a repository-owned extension rather than a simulator protocol. |
  | `veoveo.io/uav-runtime-event/v1` | Private pod-local Unix datagram carrying one simulator-ready edge to the companion MCP server. It remains an internal reference-adapter protocol. |
  | WebRTC 1.0, RTP/SRTP, and H.264 | The public profile exposes one direct WebRTC product for each active viewer lease. The UAV reference implements each product with native Omniverse WebRTC and NVIDIA NVENC H.264. RTSP relays, WHEP, media routers, shared-bitstream fanout, and software transcoding are excluded from that reference path. |
  | USD and RTX Hydra | Isaac Sim `6.0.1` authoritative scene and operator-camera render products inside the simulation process. These are implementation dependencies rather than public MCP types. |
  | OGC 3D Tiles | One Cesium-backed streamed world and cache in the authoritative simulator. |
  | WGS 84, ECEF, ENU, FLU, and quaternions | Explicit world, stage, entity, camera-rig, and shortest-arc orientation mappings. |
  | NVIDIA Container Runtime and NVENC | One hardware GPU allocation for the complete reference simulator. CPU rendering and encoding are excluded. |

  The governing rule is:

  > Every simulation MCP implementation renders its operator cameras inside its authoritative simulation. Veoveo standardizes governance, logical camera descriptions, lease-bound stream products, viewer
  > leases, signaling, and conformance—not a shared renderer or media relay.

  ## 1. Target architecture

                           Veoveo shared contracts
                    governance + live-view + conformance
                                     │
                                     ▼
                      UAV Simulation MCP reference
                       logical-camera declarations
                       ownership and access policy
                       ephemeral viewer leases
                       bounded slot allocation
                       access audit
                       signaling proxy
                       live-view App
                                     │
                          pod-local control only
                                     │
                                     ▼
                       Authoritative Isaac process
                        one USD/Cesium world
                        logical operator cameras
                        preallocated viewer slots
                        one NVIDIA GPU
                                     │
                  ┌──────────────────┼──────────────────┐
                  ▼                  ▼                  ▼
           viewer slot A      viewer slot B      viewer slot C
           camera clone       camera clone       camera clone
           HydraTexture       HydraTexture       HydraTexture
           NVENC + WebRTC     NVENC + WebRTC     NVENC + WebRTC
                  │                  │                  │
                  ▼                  ▼                  ▼
              viewer A           viewer B           viewer C

  Logical camera count increases pose and Cesium-view selection work. Active viewer count
  intentionally increases RTX rendering, NVENC sessions, native WebRTC peers, and network
  work within a measured installation limit.

  There will be no:

  - Separate Simulation View Isaac process.
  - Scene mirroring.
  - Visualization pose transport.
  - Pose buffering or interpolation.
  - Duplicate Cesium world.
  - Durable renderer desired state.
  - Renderer reconciliation.
  - Shared simulation MCP server.
  - RTSP or WHEP transport.
  - Encoded media relay, SFU, or packet-fanout sidecar.
  - Dynamically created GPU resources on a browser request.

  ## 2. Reference implementation contract

  The UAV server remains one reference simulation MCP implementation. Other simulator authors are expected to implement their own MCP server and authoritative camera integration while conforming to the same
  shared profile.

  The shared contract should standardize:

  - Camera identifiers and descriptors.
  - Supported camera rig types.
  - Optics, resolution, cadence, and stream policy.
  - Stable logical-camera identity.
  - Stable capacity-slot and lease-bound stream-product identity.
  - Codec, color, encoder, and transport metadata.
  - Work Context and data-label ownership.
  - Actor and browser-instance viewer leases.
  - Lease open, renew, close, expiry, and revocation.
  - Product health and camera health.
  - Capacity reporting.
  - Signaling authorization.
  - MCP App metadata.
  - Access audit requirements.

  It must not prescribe:

  - Isaac Sim.
  - USD paths.
  - A scene mirror.
  - Pose transport.
  - A common renderer.
  - A common camera implementation language.
  - A hosted Simulation View service.

  ## 3. Public MCP surface

  Each simulation MCP implementation should expose equivalent domain-owned tools:

  list_live_cameras
  open_live_view
  renew_live_view
  close_live_view

  Optional camera mutation tools are domain-specific. The UAV reference may expose:

  set_operator_camera

  only if interactive rig or target changes are required.

  Recommended UAV resources:

  uav-sim://session/{session_id}/live-cameras
  uav-sim://session/{session_id}/live-camera/{camera_id}
  uav-sim://session/{session_id}/stream-products
  uav-sim://session/{session_id}/stream-product/{product_id}
  uav-sim://session/{session_id}/live-views
  uav-sim://session/{session_id}/live-view/{live_view_id}

  The existing shared live-view contract should become provider-neutral. It should retain the useful live-view v2 identities from current main while removing the hardcoded simulation-view:// resource
  assumption.

  No compatibility aliases should remain.

  ## 4. Authoritative camera collection

  The UAV reference should support a bounded configured set of logical cameras.

  ### Follow camera

  Tracks one UAV from an offset expressed in the target’s FLU frame.

  Use cases:

  - General spectator camera.
  - Side or rear-quarter following view.
  - High trailing overview.

  ### Chase camera

  Follows behind the UAV using its current authoritative orientation or flight direction.

  Use cases:

  - Close third-person view.
  - Mission-following view.
  - Low-altitude cinematic view.

  ### Orbit camera

  Maintains a configurable radius, azimuth, and elevation around a target UAV.

  Use cases:

  - Inspection.
  - Hover observation.
  - Operator-controlled perspective.

  ### Look-at camera

  Keeps a fixed or independently controlled eye position while tracking a UAV or world point.

  Use cases:

  - Ground observer.
  - Tower or rooftop viewpoint.
  - Mission checkpoint camera.

  ### Stabilized mounted operator camera

  Uses a mount relative to a UAV but smooths the resulting operator view.

  This is distinct from a physical sensor camera. A real onboard sensor must retain its exact mount and motion because it is evidence. A stabilized mounted operator camera is explicitly cinematic.

  ### Formation overview camera

  Tracks the centroid and spatial extent of several UAVs.

  Use cases:

  - Fleet supervision.
  - Formation verification.
  - Multi-UAV mission overview.

  ### Fixed camera

  Uses a static pose and requires no follow smoothing. It remains useful for world observation and regression tests.

  The reference does not need to expose every camera immediately in the App, but every camera type claimed by its schema must have an authoritative-stage implementation and acceptance coverage.

  ## 5. Camera creation and lifecycle

  GPU camera products must not be created synchronously when a browser connects. The
  simulator preallocates a bounded viewer-slot pool during startup.

  At session initialization:

  1. The simulator creates each configured logical USD camera under the authoritative stage.
  2. Each logical camera receives a stable camera ID and owns one final smoothed pose.
  3. The simulator creates the configured maximum number of viewer-camera clones.
  4. Each viewer slot receives a stable camera prim, HydraTexture, stream-product ID, and native signaling/media ports.
  5. Every viewer product remains inactive and unassigned until a lease reserves its slot.
  6. A lease binds one slot to one logical camera and activates one RTX render and one NVENC/WebRTC product.
  7. Assignment completes only after a drawable event proves the first assigned GPU frame and the exact native signaling listener is ready. A bounded activation failure releases the slot before returning a lease or public endpoint.
  8. Close, expiry, revocation, or signaling loss deactivates and releases only the assigned slot.

  Suggested internal paths:

  /World/OperatorCameras/follow
  /World/OperatorCameras/chase
  /World/OperatorCameras/orbit
  /World/OperatorCameras/look_at
  /World/OperatorCameras/stabilized_mount
  /World/OperatorCameras/formation
  /World/OperatorCameras/fixed
  /World/OperatorViewerCameras/slot_0
  /World/OperatorViewerCameras/slot_1

  Suggested HydraTexture identities:

  uav_operator_follow
  uav_operator_chase
  uav_operator_orbit
  uav_operator_look_at
  uav_operator_stabilized_mount
  uav_operator_formation
  uav_operator_fixed
  uav_viewer_slot_0
  uav_viewer_slot_1

  The historical /World/FollowCamera behavior remains the starting reference, but the new
  implementation should use one canonical organized camera namespace. Active viewer clones
  copy the selected logical camera's final pose and optics. They do not independently smooth
  the same logical view.

  ## 6. Smooth camera tracking

  The current mirror implementation only smooths the camera eye and then points it at the raw target platform/simulation/view-isaac/veoveo_simulation_view/camera.py:598. That still allows target-position and
  orientation jerk into the final view. It should not be ported unchanged.

  ### Smoothing model

  Use one simple, explicit, frame-rate-independent exponential filter over the final desired camera pose.

  For translation:

  alpha = 1 - 2^(-dt / translation_half_life)
  position = lerp(position, desired_position, alpha)

  For orientation:

  alpha = 1 - 2^(-dt / rotation_half_life)
  orientation = shortest_arc_slerp(orientation, desired_orientation, alpha)

  Half-life has a clear meaning: after one configured half-life, the remaining error is halved.

  This model is preferable to hidden pose buffering because it:

  - Consumes the current authoritative target transform.
  - Requires only O(1) state per camera.
  - Does not delay simulation state by a fixed number of frames.
  - Is stable across 30, 60, and 120 Hz updates.
  - Does not overshoot.
  - Has explicit and testable latency.
  - Smooths both camera translation and view orientation.

  ### Per-camera smoothing state

  Each camera maintains:

  - Previous smoothed position.
  - Previous smoothed orientation.
  - Last monotonic update time.
  - Current target identity.
  - Current camera revision.
  - Last authoritative physics step.
  - Whether the filter is initialized.

  It must not maintain a history of target poses.

  ### Strongly typed smoothing contract

  Replace ambiguous smoothingSeconds with:

  translationHalfLifeMs
  rotationHalfLifeMs
  teleportDistanceM
  resetAfterGapMs

  Rules:

  - Half-life 0 disables smoothing for that component.
  - Values are finite and bounded.
  - A target change resets the filter.
  - A camera-definition revision resets the filter.
  - A simulation reset resets the filter.
  - A render gap beyond resetAfterGapMs resets the filter.
  - A target displacement beyond teleportDistanceM snaps to the new pose rather than flying across the world.
  - Quaternion interpolation always uses the normalized shortest arc.

  Defaults should be selected from GPU visual acceptance, not copied blindly from the mirror renderer. A reasonable starting profile is approximately 100–200 ms, but the final values must be evidence-backed.

  ### Rig-specific behavior

  Follow and chase:

  - Compute the desired eye and orientation from the current authoritative UAV transform.
  - Smooth the final camera pose.
  - Never smooth or alter the UAV itself.

  Orbit:

  - Compute the current desired orbit pose from the authoritative target.
  - Smooth both target-follow movement and commanded angular changes.
  - Handle azimuth wrap through quaternion shortest-arc interpolation.

  Look-at:

  - Keep the configured eye fixed when appropriate.
  - Smooth the desired view orientation toward a moving target.
  - If the eye is also movable, smooth its translation independently.

  Formation overview:

  - Compute the current authoritative centroid and bounds.
  - Derive the desired camera pose.
  - Smooth the final camera pose, preventing one UAV’s sudden correction from jerking the entire formation view.

  Stabilized mounted operator:

  - Derive the desired transform from authoritative entity transform × mount transform.
  - Smooth the final operator-camera pose.
  - Never reuse this smoothed pose as sensor evidence.

  Fixed:

  - Apply the configured pose exactly.
  - No smoothing state is necessary.

  ## 7. Render scheduling

  The simulator should retain separate cadence concepts:

  - Physics cadence.
  - Kit/operator render cadence.
  - Per-camera AOV cadence.
  - Domain sensor cadence.
  - Recording cadence.

  A 30 FPS operator camera must not cause a 2 FPS nadir sensor to emit 30 FPS recordings.

  Timing has one canonical authority. Physics advances from the runtime's measured native
  update interval. A viewer product's cadence gate selects render opportunities without
  changing physics time, replaying a physics step, or delaying the final vehicle transform.
  The Kit rate-limiter policy is fixed by the qualified runtime profile. Visual debugging
  must not toggle it or introduce an alternate timing branch.

  The authoritative loop should:

  1. Advance physics at its configured cadence.
  2. Read the latest authoritative entity transforms.
  3. Derive and smooth active operator camera poses.
  4. Update the corresponding USD camera transforms.
  5. Copy each selected logical camera's final pose and optics into its assigned viewer slots.
  6. Advance Kit at the required operator-render cadence.
  7. Allow each active viewer-slot HydraTexture to tick at its declared cadence.
  8. Capture domain sensors only at their declared cadence.

  There is no pose serialization between steps 2 and 4.

  ## 8. Cesium integration

  The authoritative Isaac process remains the only Cesium consumer.

  The application is the sole Cesium viewport writer. It derives every submitted frustum
  from the authoritative USD camera pose and matching camera apertures. Hydra projection
  matrices, transpose guessing, extension-owned fallback writers, and competing native
  viewport authority are excluded.

  Each Kit update should submit the complete active authoritative viewport set:

  - Unique logical cameras selected by active viewer slots.
  - Domain sensor viewport when required.
  - A readiness viewport only if it provides unique evidence.

  This preserves the useful multi-viewport tile-selection work without duplicating:

  - Cesium provider connections.
  - Tile downloads.
  - Tile decoding.
  - Materials.
  - Cache storage.
  - Georeference state.
  - World-stage state.

  Several viewer slots bound to the same logical camera share one Cesium tile-selection
  viewport even though they render and encode independently. Slot assignment and release
  update the unique submitted viewport set without recreating the Cesium world.

  ## 9. Logical-camera and viewer-slot policies

  Supporting several logical cameras does not require rendering or encoding them without
  viewers.

  ### Logical cameras

  A logical camera owns its rig, target, optics, smoothing state, health, and final pose.

  - A continuous logical camera may keep its pose current before a viewer arrives.
  - An on-demand logical camera begins pose work when its first viewer slot is assigned.
  - Neither policy creates a shared encoded stream product.
  - Multiple slots selecting one logical camera consume the same final pose and optics.

  ### Viewer slots

  The configured slot pool is created once during simulator startup.

  - Every slot has a stable camera clone, HydraTexture, native WebRTC product, and port pair.
  - An unassigned slot has rendering and encoding disabled.
  - One viewer lease atomically reserves one free slot.
  - Open remains starting until that slot produces a visible advancing frame.
  - A second viewer receives another slot, even when selecting the same logical camera.
  - Closing one viewer affects no other slot.
  - Close, expiry, revocation, or signaling loss returns the slot to the pool immediately.
  - A released slot may be assigned to another logical camera and viewer.

  This is local ephemeral allocation, not durable reconciliation. There is no idle grace,
  first-viewer sharing state, or last-viewer product lifecycle.

  On UAV MCP startup:

  - All prior viewer leases are gone.
  - The server issues one idempotent pod-local release_all_viewer_slots.
  - All native products remain inactive until assigned by a new lease.
  - The App opens fresh viewer leases.

  On simulator restart:

  - Existing slot assignments disappear.
  - Existing viewers disconnect.
  - Native WebRTC stop or signaling failure starts a bounded reconnect sequence for each
    selected camera. This is connection recovery, not status polling.
  - A subscribed live-camera resource update immediately retries selected cameras that
    are waiting for simulator readiness.
  - The App opens a fresh lease against a recreated native slot product after simulator
    readiness returns. The stable physical slot and stream-product ID may be reused; the
    prior viewer lease may not.
  - The App stops reconnecting immediately when the camera tile closes or the App tears
    down. Exhausted reconnect attempts remain visibly waiting for the next readiness
    notification instead of polling forever.
  - No desired/realized replay is performed.

  ## 10. One native product per viewer

  Every active viewer receives a separate native Omniverse stream product:

  logical camera final pose
    → assigned viewer camera clone
    → viewer HydraTexture
    → viewer LdrColor AOV
    → viewer NVENC encode
    → viewer native NVIDIA WebRTC peer

  Each viewer has:

  - Its own actor identity.
  - Its own browser-instance identity.
  - Its own lease ID.
  - Its own token hash.
  - Its own expiry.
  - Its own physical viewer slot.
  - Its own stream-product identity.
  - Its own camera clone and HydraTexture.
  - Its own RTX render and NVENC session.
  - Its own signaling connection.
  - Its own WebRTC peer/SRTP/network state.

  Viewer count intentionally affects:

  - Active viewer-camera clones.
  - Active HydraTextures and RTX renders.
  - Active NVENC sessions.
  - Native WebRTC peer and network work.

  Viewer count must not create another simulation process, Cesium world, tile cache, physics
  loop, logical-camera smoothing state, or visualization pose transport.

  Native Omniverse WebRTC is the only media implementation in the UAV reference. Do not
  introduce MediaMTX, RTSP, WHEP, an SFU, a packet relay, shared-bitstream fanout, or a
  software transport fallback. If the bounded native slot capacity is exhausted,
  admission fails directly.

  This deliberately spends one bounded GPU render and encode per viewer to retain the
  native CUDA-to-NVENC-to-WebRTC path and a smaller operational topology. It prefers
  explicit capacity rejection over a general-purpose media distribution layer.

  ## 11. Viewer-slot capacity

  Capacity is defined by preallocated native viewer slots and their measured GPU cost.

  Count:

  - Configured logical operator cameras.
  - Configured viewer slots.
  - Assigned and available viewer slots.
  - Active viewer camera clones and HydraTextures.
  - Total active render pixels per second.
  - NVENC sessions.
  - GPU memory reservation.
  - Native signaling/media port pairs.
  - Viewer leases.
  - Aggregate estimated network bitrate separately.

  Every active viewer counts as one rendered and encoded product. Admission reserves the
  slot before activating GPU work. It never steals another viewer's slot or partially
  creates a product.

  The server must not silently reduce:

  - Resolution.
  - Cadence.
  - Optics.
  - Smoothing.
  - Camera type.
  - Codec.

  A rejected activation identifies the exact exhausted dimension.

  The UAV reference should ship a small measured viewer-slot profile. Exact simultaneous
  RTX, NVENC, GPU-memory, cadence, and real-time-factor limits must be qualified on the
  target GPU.

  ## 12. Sensor and viewer products remain separate

  A physical sensor product remains exact evidence. A cinematic viewer slot remains an
  ephemeral presentation product. Even when they use equivalent optics, they have separate
  render and encode lifecycles.

  - Stream and Recording consume the authoritative sensor product.
  - A browser consumes its assigned native viewer product.
  - A viewer product never becomes sensor or mission evidence.
  - No relay or fanout component couples those lifecycles.
  - No CPU render or software encode path is permitted.

  This deliberate duplication keeps native NVIDIA transport end to end and bounds the
  cost through viewer-slot admission.

  ## 13. Governance

  Removing renderer reconciliation does not weaken governance.

  ### Camera and product ownership

  Each logical camera belongs to:

  - Tenant.
  - Work Context.
  - Output owner.
  - Policy revision.
  - Data-label set.
  - Simulation session.

  Each assigned viewer product inherits that boundary and additionally records its
  physical slot and owning live-view lease.

  ### Viewer ownership

  Each viewer lease additionally belongs to:

  - Gateway actor.
  - Browser-instance identity.
  - Selected camera.
  - Assigned viewer slot and stream product.
  - Camera revision.

  ### Authorization

  Open and renew verify:

  - Current gateway assertion.
  - Work Context membership.
  - Output ownership.
  - Policy revision.
  - Data-label access.
  - Logical-camera availability.

  Open additionally requires one free native viewer slot. Renew requires the caller's
  existing slot assignment and never allocates or rotates another viewer's product.

  ### Secrets

  - Tokens appear only in open and renew results.
  - Stored resources contain no token.
  - Only SHA-256 token hashes are retained in memory.
  - Comparisons are constant-time.
  - Signaling removes credentials before proxying to NVIDIA.
  - Private native signaling addresses remain private.

  ### Audit

  Durable audit records should cover:

  - Viewer lease opened.
  - Viewer lease denied.
  - Viewer lease closed.
  - Viewer lease expired.
  - Viewer authority revoked.
  - Camera definition changed.
  - Viewer-slot allocation rejected.

  Audit records contain identity and policy context but never tokens or media.

  Viewer leases themselves remain ephemeral and never enter durable desired state.

  ## 14. UAV MCP implementation

  Add focused modules:

  servers/uav-sim-mcp/src/server/live_view.rs
  servers/uav-sim-mcp/src/server/signaling.rs
  servers/uav-sim-mcp/src/server/live_view_audit.rs
  servers/uav-sim-mcp/src/live_app.rs
  servers/uav-sim-mcp/assets/live-app.html

  The server should:

  - Read camera/product health from the pod-local simulator adapter.
  - Maintain ephemeral viewer leases.
  - Atomically reserve one free native viewer slot for each new lease.
  - Bind the slot to the selected logical camera and activate its GPU product.
  - Proxy authorized signaling only to that slot's stable native port.
  - Release the exact slot on close, expiry, revocation, or signaling loss.
  - Publish camera, product, and redacted viewer resources.
  - Emit access audits.
  - Serve the live-view App.

  It should not:

  - Persist renderer state.
  - Persist viewer leases.
  - Reconcile scenes or cameras.
  - Renew pose authorization.
  - Materialize artifacts.
  - Know anything about a mirrored stage.

  ## 15. Live-view App

  The App should support:

  - Camera selector.
  - One primary selected view.
  - Optional bounded grid for several cameras.
  - One viewer lease per selected camera and browser instance.
  - No duplicate connection for the same camera within one App instance.
  - Automatic lease renewal.
  - Bounded, event-triggered fresh-lease recovery after native stream interruption.
  - Lease closure on tile removal or App teardown.
  - Camera health and smoothing profile display.
  - Requested and decoded resolution/cadence.
  - Frame-age and transport statistics.
  - H.264 hardware/software decode labeling.
  - Configured attribution.
  - No simulation control dashboard.

  Every open tile consumes one native viewer slot. The App prevents duplicate connections
  for the same camera inside one browser instance, but another actor, tab, or browser
  receives its own product.

  ## 16. Simulator runtime modules

  Suggested focused structure:

  showcase/uav-sim/runtime/veoveo_uav_sim/
    operator_camera.py
    operator_camera_rigs.py
    operator_camera_smoothing.py
    operator_products.py
    operator_health.py

  Responsibilities:

  - operator_camera.py: USD cameras and authoritative entity sampling.
  - operator_camera_rigs.py: desired-pose computation for each rig.
  - operator_camera_smoothing.py: half-life translation/quaternion filtering and reset rules.
  - operator_products.py: preallocated viewer-camera clones, HydraTextures, AOVs, native WebRTC/NVENC products, slot assignment, and release.
  - operator_health.py: CUDA/RTX/NVENC/product/frame evidence.

  Do not restore the historical monolithic live_stream.py. Governance and leases belong in Rust; rendering belongs in Isaac.

  ## 17. Helm and media deployment

  The UAV simulator pod owns:

  - MCP HTTP.
  - Public signaling proxy TCP.
  - Private native signaling TCP range.
  - Public WebRTC media UDP range.
  - The authoritative Isaac GPU.

  Each physical viewer slot receives:

  private signaling port = signalingBase + slot
  public media port       = mediaBase + slot

  The slot count is the maximum simultaneous viewer count. It is independent of the
  logical-camera count. The chart creates no RTSP port, WHEP endpoint, relay container, or
  media-router configuration.

  The Helm chart should validate:

  - Port ranges are large enough for maximum viewer slots.
  - Product activation has one explicit bounded deadline.
  - No collision with other pod ports.
  - Public media IP is numeric where required.
  - WSS origin is credential-free.
  - NVIDIA container capabilities include compute, graphics, utility, and video.
  - Exactly one GPU is requested for the simulator.
  - No Simulation View GPU or pod exists.
  - Only one Cesium/runtime cache exists.

  ## 18. Shared conformance for other simulation MCP servers

  Replace the mirror-oriented external fixture with a simulator-hosted live-view fixture.

  The fixture should implement its own:

  - Simulation MCP server.
  - Authoritative camera source.
  - Bounded one-viewer stream-product slots.
  - Camera/product resources.
  - Viewer leases.
  - Signaling endpoint.
  - App declaration.

  Conformance should test:

  - Strict tools and resource schemas.
  - Camera capability discovery.
  - Capacity-slot identity stability while assignments remain ephemeral.
  - Owner and viewer isolation.
  - Multiple users selecting one logical camera through distinct products.
  - Token rotation and redaction.
  - Revocation and expiry.
  - App teardown.
  - Signaling security.
  - Capacity rejection.
  - No dependency on Simulation View scene or pose protocols.

  Real GPU certification remains implementation-owned. The UAV reference is the first-party NVIDIA acceptance implementation.

  ## 19. Smoothing tests

  ### Mathematical unit tests

  For every smoothed rig:

  - Error halves after one configured half-life within tolerance.
  - Results are equivalent across 30, 60, and 120 Hz updates.
  - Quaternion interpolation takes the shortest arc.
  - Orientations remain normalized.
  - No overshoot occurs.
  - 0 half-life snaps exactly.
  - Teleport threshold resets exactly.
  - Render-gap threshold resets exactly.
  - Target replacement resets exactly.
  - Camera revision resets exactly.

  ### Synthetic motion tests

  Feed:

  - Position steps.
  - Velocity steps.
  - Alternating noisy headings.
  - High-frequency target jitter.
  - Sharp turns.
  - Hover noise.
  - Formation member correction.
  - Session reset.
  - Teleport.

  Measure:

  - Camera translation continuity.
  - Angular continuity.
  - Camera acceleration and jerk.
  - Tracking error.
  - Recovery time.
  - Absence of stale historical state.

  ### Visual GPU tests

  Capture follow, chase, orbit, stabilized mounted, and formation views during a real mission.

  Require:

  - No visible high-frequency target jerk.
  - No sudden orientation reversal.
  - No long sweep after reset or teleport.
  - Correct target framing.
  - Stable horizon where appropriate.
  - Correct Cesium alignment.
  - No frame stalls introduced by smoothing.

  ## 20. Multi-user tests

  Open the same camera from:

  - Two different users.
  - Two browser instances for one user.
  - Multiple tabs.
  - A selected view and grid.

  Prove:

  - Distinct viewer IDs and tokens.
  - One stable camera ID.
  - Distinct stream-product IDs and physical slots.
  - One viewer-camera clone and HydraTexture per active viewer.
  - One native NVIDIA WebRTC peer and NVENC session per active viewer.
  - Closing one viewer leaves the others live.
  - Revoking one actor does not revoke an unrelated authorized actor.
  - Closing or revoking a viewer releases only its assigned slot.
  - Released slots can be reassigned without retaining prior ownership or camera state.
  - Exhausted slot capacity fails directly without changing existing viewers.

  ## 21. Performance acceptance

  Proposed initial gates, to be ratified on the designated GPU:

  - One GPU allocation for the complete UAV simulator.
  - Physics real-time factor at least 0.98 with the qualified camera set active.
  - No second Isaac process.
  - No second Cesium world or cache.
  - No visualization pose transport.
  - Camera source-to-render age below 85 ms p95, excluding the explicit smoothing response.
  - Smoothing state uses the current authoritative transform every render tick.
  - The reference profile targets 16 FPS and delivers at least 12 FPS under maximum two-viewer admission.
  - Browser motion-to-photon below 250 ms p95 on the acceptance network.
  - Exactly one NVENC session per assigned viewer slot.
  - Adding one viewer adds exactly one RTX render product and one NVENC session.
  - Unassigned viewer slots consume no ongoing RTX render or NVENC cadence.
  - GPU and memory remain inside the measured viewer-slot profile at maximum admission.
  - No CPU render, encode, or media-copy fallback.
  - No RTSP, WHEP, media relay, protocol conversion, or second media process exists.

  Smoothing latency must be reported separately from pipeline latency. A 150 ms half-life is a camera behavior choice, not transport delay.

  ## 22. Immediate hard cut

  Delete the old architecture as soon as the minimum simulator-hosted path is in place.
  Broad acceptance does not gate deletion. The minimum cutover threshold is:

  - One authoritative simulator camera renders the current world.
  - One preallocated viewer slot binds to that camera and advances one native HydraTexture/NVIDIA NVENC product.
  - The simulation MCP server authorizes an ephemeral viewer lease.
  - Its signaling proxy connects a browser to the simulator-owned product.
  - Helm and gateway routing select that simulation-owned App and media path.

  Once this threshold is met, delete:

  - servers/simulation-view-mcp/
  - platform/simulation/view-isaac/
  - platform/simulation/pose-ingress/
  - Visualization-only pose and scene contracts with no remaining consumer.
  - UAV view-scene publication and assets.
  - UAV visualization pose publisher.
  - Simulation View Python SDK modules.
  - Mirror-oriented fixtures.
  - Mirror-vs-native visual comparison.
  - Simulation View images and offline locks.
  - Renderer readiness and reconciliation orchestration.
  - Store adapters and models for Simulation View desired state.
  - Gateway registration for the hosted Simulation View server.

  Add a forward store migration that removes the obsolete live Simulation View state table while retaining historical migration-ledger integrity.

  No disabled mirror profile, fallback deployment, compatibility tool, old environment
  variable, or URI alias remains. Development then proceeds only on the authoritative
  simulator-hosted path. A failure in that path is fixed there and never routed back to
  the mirror implementation.

  The abandoned relay branch is also removed in the same hard cut. No MediaMTX image,
  RTSP extension selection, WHEP route, WHEP contract value, relay service, relay health
  path, or dormant fanout implementation remains.

  ## 23. Documentation

  Update:

  - UAV MCP design.
  - UAV showcase README.
  - Shared MCP live-view design.
  - MCP conformance design.
  - Runtime GPU design.
  - Deployment contract.
  - Reference installation.
  - Architecture decisions.
  - Software component catalog.
  - Diagrams and generated publications.
  - docs/CODEMAP.md:44.

  The normative statement should be:

  > A simulation MCP server exposes governed logical cameras rendered by its authoritative simulation. Every active viewer lease reserves one bounded direct stream product. In the UAV reference, that product
  > owns a camera clone, RTX render, NVIDIA NVENC encode, and native Omniverse WebRTC peer. Camera smoothing operates once on the logical camera transform and never changes or delays authoritative simulation state.

  ## 24. Suggested implementation sequence

  1. Record the corrected simulator-hosted multi-camera and per-viewer-product architecture.
  2. Remove every MediaMTX, RTSP, and WHEP change and restore the native Omniverse WebRTC surfaces as the only UAV media path.
  3. Generalize the shared live-view contract around logical cameras, viewer products, and bounded capacity.
  4. Preserve the one surviving physics-timing path, sole Cesium viewport writer, authoritative camera poses, and matching apertures; delete alternate heuristics.
  5. Keep camera smoothing independently tested outside Isaac.
  6. Establish one authoritative follow camera and one preallocated native viewer slot.
  7. Make the UAV MCP lease service atomically assign and release native slots through the pod-local simulator adapter.
  8. Retain the authenticated native signaling proxy and NVIDIA browser client.
  9. Cut Helm and gateway routing to the simulator pod and expose one fixed signaling/media port pair per viewer slot.
  10. Delete the second renderer, pose and scene mirror, reconciliation, durability, old deployment surfaces, and the abandoned relay branch immediately.
  11. Restore a coherent build and deploy closure exclusively on the revised architecture.
  12. Keep chase, orbit, look-at, stabilized mounted, formation, and fixed rig computation bound to logical cameras.
  13. Update camera selection and bounded grid behavior to consume one viewer slot per open tile.
  14. Replace the external mirror fixture with simulator-hosted one-viewer-product conformance.
  15. Add multi-user, multi-camera, GPU, browser, smoothing, restart, capacity, and security acceptance.
  16. Run the complete one-GPU acceptance and publish evidence from the revised architecture.

  The implementation uses several coherent commits. The runtime cutover is an early
  atomic hard cut. No repository state after that cut advertises, builds, deploys, or
  tests both the mirror and authoritative camera paths.
