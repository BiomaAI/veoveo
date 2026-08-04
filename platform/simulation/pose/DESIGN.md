# Simulation View Pose Protocol

This crate owns the renderer-neutral latest-pose protocol between a simulation
producer and Simulation View. It carries complete visual snapshots at render
cadence. Physics controls, forces, solver buffers, and backpressure into a
simulation loop are outside the contract.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/simulation-view-pose/v1` | length-delimited binary snapshots with one fixed canonical coordinate convention |
| `veoveo.io/simulation-view-pose-ingress-control/v2` | private typed producer-binding declarations and status with monotonic authorization revisions and revocation tombstones |
| POSIX shared memory and Unix datagrams | bounded ordered snapshot ring with acquire/release publication and best-effort renderer wake edges |
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

The shared-memory implementation uses a fixed-capacity ordered ring. Its slot
count covers the admitted cadence across the complete stale window, with two
additional samples for the active interpolation bracket. A producer publishes
the payload and length under a per-slot generation marker, then advances the
latest generation with release ordering. A reader drains every retained
generation in order and accepts a slot only when its marker remains stable.
An overrun remains visible as a source sequence gap instead of being hidden.
After advancing the shared generation, ingress sends its value to a private
per-session Unix datagram socket. The renderer blocks on that edge and drains
every retained generation when it wakes. A missing or saturated socket never
blocks or rejects publication because the shared ring remains authoritative;
the next delivered edge drains the accumulated history.

The streaming implementation prefixes the exact same snapshot with a
big-endian 32-bit length. `simulation-view-pose` is the canonical ingress
process. It accepts TLS 1.3 only, requires a client certificate chained to the
installation trust root, extracts exactly one SPIFFE URI SAN, and compares
that identity with the active producer binding before publishing a snapshot.

The installation-secret control endpoint binds, inspects, and revokes
sessions under `/v1/bindings/{session_id}`. A binding fixes the epoch, Frames
revision, ordered entity table, producer identity, authorization revision,
expiry, and all admission limits. A higher authorization revision with the
same immutable identity renews the active declaration in place. The latest
pose store, sequence, heartbeat, shared-memory inode, ordered history, and
reader mapping remain intact.

Revocation is a revisioned binding rather than an unversioned expiry change.
Ingress records the revoked revision as a floor, removes the active binding,
and removes its shared-memory name. Any active or revoked declaration at or
below that floor is rejected. This makes delayed retries fail closed while a
later explicit authorization can deliberately establish a higher revision.

`/readyz` succeeds only after the mTLS listener has bound and reports the exact
pose schema with mutual authentication enabled. The shared-memory directory
is private to the workload, each generated file is mode `0600`, and neither
paths nor credentials enter MCP resources.
