# Governed Recording Reader

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Veoveo recording identity | Canonical `recording://recordings/{UUIDv7}` resources |
| Gateway internal identity | Typed actor, tenant and data-label authority; no retained bearer |
| Artifact plane | Existing caller or bounded task-read capability, immutable occurrence UUID, expected length and SHA-256 |
| Rerun RRD | Existing repository profile 0.36.3, canonical dataset/recording Store IDs |
| Local filesystem | Confined complete live parts and a bounded, verified, pinned Artifact cache |
| `RrdIdentityValidator` | Trusted internal Rust extension for server-owned RRD kinds; no public wire protocol |

## Ownership

`RecordingReader` composes the platform Store, a canonical spool root, and a required
`LayerCache`. It owns authorized analysis plans and task-local snapshots. It has no
Recording Hub or Recording MCP dependency. Service startup, producer ingest, sealing,
Redap, projection and playback lifecycle stay in their owning services.

`access.rs` holds the shared visibility and path-confinement rules. `read.rs` resolves
the actor's tenant, checks recording labels, loads a bounded catalog layer set, and
materializes committed layers through the cache. Complete acknowledged live parts are
copied into task-local storage and rechecked against their captured length and digest.
The plan holds cache leases for its lifetime.

`materialize_analysis_snapshot` requires explicit Artifact read authority and a
positive source-byte limit. A caller must match the recording identity and labels.
A task capability is checked through Artifact's current scope endpoint before any
catalog access, including snapshots containing only live ingest parts. Its verified
principal, tenant and labels must match the durable task owner. The tighter of the
capability's byte ceiling and the requested source limit applies before live copies.
Committed layers also pass the Artifact occurrence checks during materialization.

Stream and Reason persist bounded read capabilities alongside their existing output
capabilities. They recover the same task-bound credential after a worker restart;
the submitted gateway bearer is never stored. Expiry, revocation, authority outages
and Work Context policy changes reject further materialization.

## Cache And Identity

`cache.rs` retains bounded reservations, partial-file publication, full byte and RRD
identity validation, atomic installation, and pin-aware eviction. Cache hits first
request current Artifact metadata with the presented caller or task credential. That endpoint
requires Read access. A revoked grant, expired credential, missing occurrence, or
unavailable authority rejects reuse before local byte validation or pinning. The
returned occurrence ID, canonical URI, and byte length must match the requested layer.
Successful hits then run the identity validator again without downloading bytes.
Cache misses retain the download client's authorization path.
The default recording-layer validator binds both canonical
Store UUIDs, length and SHA-256. Recording MCP supplies its Blueprint validator from
`blueprint_cache.rs`, which binds application, Blueprint ID and message count with the
existing Hub Blueprint parser. Blueprint interpretation does not enter analysis readers.

A validator is trusted repository code, never caller-supplied executable behavior. It
must verify the full expected byte identity and its typed domain identity on every
call. Adding a new kind requires its own rejection tests. Network downloads additionally
verify the Artifact occurrence, declared length, streamed length and streamed digest
before validation and installation.

The native HTTP regression in `cache/authorization_tests.rs` opens an existing cache
entry and changes the Artifact endpoint's decision between requests. It verifies
caller isolation, revocation, expiry, missing occurrences, mismatched metadata, and
authority outage. Denied requests do not validate or pin local bytes. The fixture
accepts only metadata requests and serves no artifact body.

## Build Boundary

Stream, Reason and the video materializer depend on this crate. Recording MCP composes
the same cache and visibility rules for playback. A service-internal Hub or Recording
MCP edit therefore has no production Cargo dependency edge into Stream or Reason.
All shared Cargo manifests remain available to image planning. Runtime input contexts
must still follow the production dependency closure to realize this source isolation.

Cancellation drops a download reservation, removes its partial file, and releases
managed capacity. A completed cache entry becomes pinned only after full validation.
Startup removes abandoned partial files and rejects unmanaged entries. Reader
readiness includes the cache's free-space and managed-byte checks.
