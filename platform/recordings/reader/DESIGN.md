# Governed Recording Reader

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Veoveo recording identity | Canonical `recording://recordings/{UUIDv7}` resources |
| Gateway internal identity | Typed actor, tenant and data-label authority; no retained bearer |
| Artifact plane | Fresh caller credential, immutable occurrence UUID, expected length and SHA-256 |
| Rerun RRD | Existing repository profile 0.36.3, canonical dataset/recording Store IDs |
| Local filesystem | Confined complete live parts and a bounded, verified, pinned Artifact cache |
| `RrdIdentityValidator` | Trusted internal Rust extension for server-owned RRD kinds; no public wire protocol |

## Ownership

`RecordingReader` composes the platform Store, a canonical spool root, and an optional
`LayerCache`. It owns authorized analysis plans and task-local snapshots. It has no
Recording Hub or Recording MCP dependency. Service startup, producer ingest, sealing,
Redap, projection and playback lifecycle stay in their owning services.

`access.rs` holds the shared visibility and path-confinement rules. `read.rs` resolves
the actor's tenant, checks recording labels, loads a bounded catalog layer set, and
materializes committed layers through the cache. Complete acknowledged live parts are
copied into task-local storage and rechecked against their captured length and digest.
The plan holds cache leases for its lifetime.

A committed layer requires a fresh Artifact caller. Calling the analysis entrypoint
without one returns the existing explicit credential error. Stream and Reason retain
that fail-closed replay behavior; this dependency extraction does not activate durable
replay or persist a submitted bearer.

## Cache And Identity

`cache.rs` retains bounded reservations, partial-file publication, full byte and RRD
identity validation, atomic installation, and pin-aware eviction. Cache hits run the
identity validator again. The default recording-layer validator binds both canonical
Store UUIDs, length and SHA-256. Recording MCP supplies its Blueprint validator from
`blueprint_cache.rs`, which binds application, Blueprint ID and message count with the
existing Hub Blueprint parser. Blueprint interpretation does not enter analysis readers.

A validator is trusted repository code, never caller-supplied executable behavior. It
must verify the full expected byte identity and its typed domain identity on every
call. Adding a new kind requires its own rejection tests. Network downloads additionally
verify the Artifact occurrence, declared length, streamed length and streamed digest
before validation and installation.

## Build Boundary

Stream, Reason and the video materializer depend on this crate. Recording MCP composes
the same cache and visibility rules for playback. A service-internal Hub or Recording
MCP edit therefore has no production Cargo dependency edge into Stream or Reason.
All shared Cargo manifests remain available to image planning. Runtime input contexts
must still follow the production dependency closure to realize this source isolation.
