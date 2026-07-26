# Simulation View Pose Protocol

This crate owns the renderer-neutral latest-pose protocol between a simulation
producer and Simulation View. It carries complete visual snapshots at render
cadence. Physics controls, forces, solver buffers, and backpressure into a
simulation loop are outside the contract.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/simulation-view-pose/v1` | length-delimited binary snapshots with one fixed canonical coordinate convention |
| `veoveo.io/simulation-view-pose-ingress-control/v1` | private typed producer-binding declarations and status |
| POSIX shared memory | double-buffered latest-value slot with acquire/release publication |
| TLS 1.3 | mutually authenticated private streaming transport with exactly one SPIFFE URI SAN per producer certificate |
| WGS 84 and local tangent frames | Frames-owned immutable world revision with local ENU positions |
| SI | metres, seconds, metres per second, and radians per second |
| SHA-256 | Frames and ordered entity-table identities |

Certificate issuance, Service exposure, and NetworkPolicy are deployment
concerns. They do not change the public binary schema or freshness rules.

## Snapshot

Every message names one session, epoch, monotonic sequence, simulation
timestamp, Frames revision and digest, ordered entity-table revision and
digest, and complete entity array. Positions are local ENU metres. Entity axes
are FLU. Quaternions use XYZW order.

An entity includes a stable identity, position, orientation, active and
visibility bits, and optional bounded velocity and semantic display state.
Identities are strictly ordered, which makes the entity-table digest and
encoded message deterministic.

## Freshness And Admission

`LatestPoseStore` rejects a wrong session, epoch, Frames revision, entity
table, shape, size, or cadence. It drops stale sequences. A publisher uses a
nonblocking swap and may drop a write when the reader owns the narrow critical
section. Renderer disconnection therefore cannot block a simulation loop.

Changing the epoch clears the accepted snapshot before the new binding
becomes visible. The renderer may hold or interpolate only while the published
staleness policy permits.

## Transports

The shared-memory implementation uses two fixed-capacity slots. A producer
writes the inactive payload, publishes its length, flips the active slot, and
increments a generation with release ordering. A reader copies one slot and
accepts it only when generation and active slot remain stable.

The streaming implementation prefixes the exact same snapshot with a
big-endian 32-bit length. `simulation-view-pose` is the canonical ingress
process. It accepts TLS 1.3 only, requires a client certificate chained to the
installation trust root, extracts exactly one SPIFFE URI SAN, and compares
that identity with the active producer binding before publishing a snapshot.

The installation-secret control endpoint binds, inspects, and revokes
sessions under `/v1/bindings/{session_id}`. A binding fixes the epoch, Frames
revision, ordered entity table, producer identity, expiry, and all admission
limits. Revocation atomically removes the active binding and its shared-memory
name. Rebinding replaces the shared-memory name without truncating an inode
that a reader may still have mapped.

`/readyz` succeeds only after the mTLS listener has bound and reports the exact
pose schema with mutual authentication enabled. The shared-memory directory
is private to the workload, each generated file is mode `0600`, and neither
paths nor credentials enter MCP resources.
