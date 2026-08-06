# Simulator-hosted live-view hard cut

  This approved implementation plan supersedes the mirror-oriented Simulation View
  architecture. The target is a reference implementation with multiple authoritative
  cameras, smooth cinematic tracking, and many viewers sharing each camera's single
  encoded product.

  ## Standards And Protocols

  | Standard or protocol | Planned profile |
  |---|---|
  | Model Context Protocol | Version `2025-11-25` through the hosted-server contract, with typed tools, resources, subscriptions, notifications, and an MCP App owned by each simulation server. |
  | JSON Schema | Draft 2020-12 strict camera, product, capacity, health, and viewer-lease schemas. |
  | `veoveo.io/live-view/v2` | Provider-neutral camera, encoded-product, viewer, ownership, signaling, and redaction contract. This is a repository-owned extension rather than a simulator protocol. |
  | WebRTC and H.264 | NVIDIA WebRTC transport over one NVIDIA NVENC H.264 product per active camera. Each viewer receives independent peer and SRTP state without another render or encode. |
  | USD and RTX Hydra | Isaac Sim `6.0.1` authoritative scene and operator-camera render products inside the simulation process. These are implementation dependencies rather than public MCP types. |
  | OGC 3D Tiles | One Cesium-backed streamed world and cache in the authoritative simulator. |
  | WGS 84, ECEF, ENU, FLU, and quaternions | Explicit world, stage, entity, camera-rig, and shortest-arc orientation mappings. |
  | NVIDIA Container Runtime and NVENC | One hardware GPU allocation for the complete reference simulator. CPU rendering and encoding are excluded. |

  The governing rule is:

  > Every simulation MCP implementation renders its operator cameras inside its authoritative simulation. Veoveo standardizes governance, camera descriptions, stream products, viewer leases, signaling, and
  > conformance—not a shared renderer.

  ## 1. Target architecture

                           Veoveo shared contracts
                    governance + live-view + conformance
                                     │
                                     ▼
                      UAV Simulation MCP reference
                       camera/product declarations
                       ownership and access policy
                       ephemeral viewer leases
                       access audit
                       signaling proxy
                       live-view App
                                     │
                          pod-local control only
                                     │
                                     ▼
                       Authoritative Isaac process
                        one USD/Cesium world
                        several operator cameras
                        one HydraTexture per camera
                        one NVENC product per camera
                        one NVIDIA GPU
                                     │
                       ┌─────────────┼─────────────┐
                       ▼             ▼             ▼
                    viewer A      viewer B      viewer C

  Camera count may increase rendering and encoding work. Viewer count must only increase WebRTC peer and network work.

  There will be no:

  - Separate Simulation View Isaac process.
  - Scene mirroring.
  - Visualization pose transport.
  - Pose buffering or interpolation.
  - Duplicate Cesium world.
  - Durable renderer desired state.
  - Renderer reconciliation.
  - Shared simulation MCP server.

  ## 2. Reference implementation contract

  The UAV server remains one reference simulation MCP implementation. Other simulator authors are expected to implement their own MCP server and authoritative camera integration while conforming to the same
  shared profile.

  The shared contract should standardize:

  - Camera identifiers and descriptors.
  - Supported camera rig types.
  - Optics, resolution, cadence, and stream policy.
  - Stable encoded stream-product identity.
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

  The UAV reference should support a bounded configured set of camera products.

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

  Cameras must not be created when a browser connects.

  At session initialization:

  1. The simulator creates each configured USD camera under the authoritative stage.
  2. Each camera receives a stable camera ID.
  3. Each streamable camera receives a stable physical slot.
  4. Each physical slot receives a stable HydraTexture name.
  5. Each streamable camera receives a stable stream-product ID and fixed signaling/media ports.
  6. Viewer leases refer to the existing product.

  Suggested internal paths:

  /World/OperatorCameras/follow
  /World/OperatorCameras/chase
  /World/OperatorCameras/orbit
  /World/OperatorCameras/look_at
  /World/OperatorCameras/stabilized_mount
  /World/OperatorCameras/formation

  Suggested HydraTexture identities:

  uav_operator_follow
  uav_operator_chase
  uav_operator_orbit
  uav_operator_look_at
  uav_operator_stabilized_mount
  uav_operator_formation

  The historical /World/FollowCamera behavior remains the starting reference, but the new implementation should use one canonical organized camera namespace.

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

  The authoritative loop should:

  1. Advance physics at its configured cadence.
  2. Read the latest authoritative entity transforms.
  3. Derive and smooth active operator camera poses.
  4. Update the corresponding USD camera transforms.
  5. Advance Kit at the required operator-render cadence.
  6. Allow each HydraTexture to tick at its declared cadence.
  7. Capture domain sensors only at their declared cadence.

  There is no pose serialization between steps 2 and 4.

  ## 8. Cesium integration

  The authoritative Isaac process remains the only Cesium consumer.

  Each Kit update should submit the complete active authoritative viewport set:

  - Continuous operator cameras.
  - Active on-demand operator cameras.
  - Domain sensor viewport when required.
  - A readiness viewport only if it provides unique evidence.

  This preserves the useful multi-viewport tile-selection work without duplicating:

  - Cesium provider connections.
  - Tile downloads.
  - Tile decoding.
  - Materials.
  - Cache storage.
  - Georeference state.
  - World rendering.

  Camera activation and deactivation must update the submitted viewport set without recreating the Cesium world.

  ## 9. Continuous and on-demand camera policies

  Supporting several cameras does not require rendering every camera continuously.

  ### Continuous

  Use for the primary operator camera when immediate availability is important.

  - Camera transform updates continuously.
  - HydraTexture updates continuously.
  - Stream product remains warm.
  - Viewer connection does not change render state.

  ### On demand

  Use for secondary cameras.

  - USD camera and physical slot already exist.
  - HydraTexture is stable but paused while unused.
  - First viewer lease activates the product.
  - Open remains starting until a visible advancing frame exists.
  - Additional viewers reuse the active product.
  - Closing one viewer does not pause it while other viewers remain.
  - Last viewer close starts a short bounded idle grace period.
  - Product pauses after the grace period.
  - Product is not destroyed or reassigned.

  This is local ephemeral activity management, not durable reconciliation.

  On UAV MCP startup:

  - All prior viewer leases are gone.
  - The server issues one idempotent pod-local deactivate_all_on_demand_products.
  - Continuous products remain governed by simulator configuration.
  - The App opens fresh viewer leases.

  On simulator restart:

  - Existing products become unavailable.
  - Existing viewers disconnect.
  - The App opens fresh leases after simulator readiness returns.
  - No desired/realized replay is performed.

  ## 10. One product, many viewers

  Every active camera has exactly one encoded stream product:

  camera
    → one HydraTexture
    → one LdrColor AOV
    → one NVENC encode
    → encoded/WebRTC fan-out
    → multiple viewer peers

  Each viewer has:

  - Its own actor identity.
  - Its own browser-instance identity.
  - Its own lease ID.
  - Its own token hash.
  - Its own expiry.
  - Its own signaling connection.
  - Its own WebRTC peer/SRTP/network state.

  Viewer count must not affect:

  - USD camera count.
  - HydraTexture count.
  - RTX render count.
  - Cesium viewport count.
  - NVENC session count.

  The pinned NVIDIA WebRTC implementation must be tested to verify that several peers on one product truly share one encode. If it internally starts one NVENC session per peer, the corrective design is encoded
  packet fan-out or an SFU over the one encoded product—not another renderer and not another encode per viewer.

  Do not build that fan-out component unless the hardware test demonstrates it is necessary.

  ## 11. Camera-product capacity

  Capacity should be product-based.

  Count:

  - Configured logical operator cameras.
  - Active rendered cameras.
  - Active continuous products.
  - Active on-demand products.
  - Total active render pixels per second.
  - NVENC sessions.
  - GPU memory reservation.
  - Signaling/media port slots.
  - Viewer leases separately.
  - Aggregate estimated network bitrate separately.

  Do not count every viewer as a rendered or encoded camera.

  The server must not silently reduce:

  - Resolution.
  - Cadence.
  - Optics.
  - Smoothing.
  - Camera type.
  - Codec.

  A rejected activation identifies the exact exhausted dimension.

  The UAV reference should ship a small measured camera profile. Exact camera and NVENC limits must be qualified on the target GPU.

  ## 12. Reusing encoded sensor products

  If an authoritative onboard/nadir camera already produces an encoded H.264 stream for Stream or Recording, the live-view path should consume that same encoded product where technically possible.

  It must not create:

  - One encode for Stream.
  - Another encode for Recording.
  - Another encode for browser viewing.

  The long-term product model is:

  one authoritative camera
    → one render
    → one encode
    → browser viewers
    → Stream processing
    → Recording publication

  Where the current runtime cannot yet expose the encoded product to every consumer, mark the exact duplication point TODO(GPU) and name the intended encoded fan-out replacement. Do not add new CPU capture or
  encoding.

  ## 13. Governance

  Removing renderer reconciliation does not weaken governance.

  ### Camera and product ownership

  Each camera and stream product belongs to:

  - Tenant.
  - Work Context.
  - Output owner.
  - Policy revision.
  - Data-label set.
  - Simulation session.

  ### Viewer ownership

  Each viewer lease additionally belongs to:

  - Gateway actor.
  - Browser-instance identity.
  - Selected camera.
  - Stable stream product.
  - Camera revision.

  ### Authorization

  Open and renew verify:

  - Current gateway assertion.
  - Work Context membership.
  - Output ownership.
  - Policy revision.
  - Data-label access.
  - Camera/product availability.
  - Viewer capacity.

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
  - Product activation rejected.

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
  - Activate an on-demand product on the first viewer.
  - Deactivate it after the last viewer’s idle grace.
  - Proxy authorized signaling to the product’s stable native port.
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
  - Optional bounded grid for several different cameras.
  - One viewer lease per selected camera and browser instance.
  - No duplicate connection for the same camera within one App instance.
  - Automatic lease renewal.
  - Lease closure on tile removal or App teardown.
  - Camera health and smoothing profile display.
  - Requested and decoded resolution/cadence.
  - Frame-age and transport statistics.
  - H.264 hardware/software decode labeling.
  - Configured attribution.
  - No simulation control dashboard.

  A grid consumes one camera product per distinct camera shown, not one new product per tile or user.

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
  - operator_products.py: HydraTexture, AOV, NVENC configuration, stable slots, pause/resume.
  - operator_health.py: CUDA/RTX/NVENC/product/frame evidence.

  Do not restore the historical monolithic live_stream.py. Governance and leases belong in Rust; rendering belongs in Isaac.

  ## 17. Helm and media deployment

  The UAV simulator pod owns:

  - MCP HTTP.
  - Public signaling proxy TCP.
  - Private native signaling TCP range.
  - Public WebRTC media UDP range.
  - The authoritative Isaac GPU.

  Each physical camera slot receives:

  private signaling port = signalingBase + slot
  public media port       = mediaBase + slot

  The Helm chart should validate:

  - Port ranges are large enough for maximum camera products.
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
  - Stable encoded stream product.
  - Camera/product resources.
  - Viewer leases.
  - Signaling endpoint.
  - App declaration.

  Conformance should test:

  - Strict tools and resource schemas.
  - Camera capability discovery.
  - Product identity stability.
  - Owner and viewer isolation.
  - Multiple users sharing one product.
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
  - One stable product ID.
  - One HydraTexture.
  - One NVENC session.
  - Closing one viewer leaves the others live.
  - Revoking one actor does not revoke an unrelated authorized actor.
  - Product pauses only after the last viewer when it is on demand.
  - Viewer count affects network statistics but not render or encoder count.

  ## 21. Performance acceptance

  Proposed initial gates, to be ratified on the designated GPU:

  - One GPU allocation for the complete UAV simulator.
  - Physics real-time factor at least 0.98 with the qualified camera set active.
  - No second Isaac process.
  - No second Cesium world or cache.
  - No visualization pose transport.
  - Camera source-to-render age below 50 ms p95, excluding the explicit smoothing response.
  - Smoothing state uses the current authoritative transform every render tick.
  - Delivered camera cadence at least 95% of configured cadence.
  - Browser motion-to-photon below 200 ms p95 on the acceptance network.
  - Exactly one NVENC session per active camera product.
  - Adding viewers does not add NVENC sessions.
  - On-demand inactive cameras consume no ongoing RTX render cadence.
  - GPU and memory remain inside the measured product profile.
  - No CPU render, encode, or media-copy fallback.

  Smoothing latency must be reported separately from pipeline latency. A 150 ms half-life is a camera behavior choice, not transport delay.

  ## 22. Immediate hard cut

  Delete the old architecture as soon as the minimum simulator-hosted path is in place.
  Broad acceptance does not gate deletion. The minimum cutover threshold is:

  - One authoritative simulator camera renders the current world.
  - One stable HydraTexture and NVIDIA NVENC product advances from that camera.
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

  > A simulation MCP server exposes governed live cameras rendered by its authoritative simulation. Each camera produces at most one encoded stream product, shared by authorized viewer leases. Camera smoothing
  > operates only on the operator-camera transform and never changes or delays authoritative simulation state.

  ## 24. Suggested implementation sequence

  1. Record the corrected simulator-hosted multi-camera architecture.
  2. Generalize the shared live-view contract and define the conformance profile.
  3. Implement and test camera smoothing independently of Isaac.
  4. Establish one authoritative follow camera and its direct HydraTexture/NVENC product.
  5. Move the minimum viewer lease, signaling proxy, audit, and App path into UAV MCP.
  6. Cut Helm and gateway routing to the simulator pod.
  7. Delete the second renderer, pose and scene mirror, reconciliation, durability, and old deployment surfaces immediately.
  8. Restore a coherent build and deploy closure exclusively on the new architecture.
  9. Add chase, orbit, look-at, stabilized mounted, formation, and fixed rig computation.
  10. Add stable camera slots and continuous/on-demand product lifecycle.
  11. Add camera selection and bounded grid behavior.
  12. Replace the external mirror fixture with simulator-hosted conformance.
  13. Add multi-user, multi-camera, GPU, browser, smoothing, restart, and security acceptance.
  14. Run the complete one-GPU acceptance and publish evidence from the new architecture.

  The implementation uses several coherent commits. The runtime cutover is an early
  atomic hard cut. No repository state after that cut advertises, builds, deploys, or
  tests both the mirror and authoritative camera paths.
