# Governed Recording Reader

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Veoveo recording identity and RFC 9562 | Typed Store IDs backed by native RFC UUIDv7 record keys; adapters require the declared table. Public URI admission belongs to the Recording contract. |
| Gateway internal identity | Typed actor, tenant and data-label authority; no retained bearer |
| Artifact plane | Existing caller or bounded task-read capability, immutable occurrence UUID, expected length and typed SHA-256; snapshot fields and cache filenames use bare lowercase hexadecimal |
| Rerun RRD | Existing repository profile 0.38.1, canonical dataset/recording Store IDs |
| Local filesystem | Confined complete live parts and a bounded, verified, pinned Artifact cache |
| `RrdIdentityValidator` | Trusted internal Rust extension for server-owned RRD kinds; no public wire protocol |

## Ownership

`RecordingReader` composes the platform Store, a canonical spool root, and a required
`LayerCache`. It owns authorized analysis plans and task-local snapshots. It has no
Recording Hub or Recording MCP dependency. Service startup, producer ingest, sealing,
Redap, projection and playback lifecycle stay in their owning services.

`access.rs` holds path-confinement and record-identity checks. `read.rs` resolves
the actor's tenant and calls the Store's shared Recording read query. SQL requires
that tenant and all recording labels in the actor's clearance before returning a row.
The reader then loads a bounded catalog layer set and
materializes committed layers through the cache. Complete acknowledged live parts are
opened once and copied into task-local storage through that file handle. Each copy
is bounded by the admitted file length, inspected for its producer identity and hashed.
Only one live part handle stays open at a time; large recordings do not require a
file descriptor for every part.
The reader then normalizes those copies to the catalog's dataset and Recording IDs.
Committed codec metadata and live video samples therefore join into one Rerun store
after segment rollover. The snapshot records the original source bytes and hashes;
normalization changes only the disposable copies. Analysis consumers derive Rerun
Store IDs from the plan's typed catalog IDs. Producer names stay inside source validation.
The plan holds cache leases for its lifetime.

`RecordingReadSourceKind`, `RecordingReadSource` and `RecordingReadSnapshot` are
native runtime values. Video's checked `TryFrom` adapters admit their facts into
the public `RecordingSourceSnapshot` contract before serialization. That contract
owns wire names, source ordering and the snapshot digest, and excludes local paths.

Acknowledged parts remain readable while a layer is Writing or Staged, including
the interval when its final file exists and upload is in progress. The reader checks
parts-directory confinement even when that final file is present. Hub removes the
parts after catalog commit; a committed read requires the verified Artifact cache
lease. The materialized staging file does not replace the acknowledged parts as an
analysis source. After copying, the reader rechecks every selected live layer's catalog
state. A commit can race directory enumeration and produce an incomplete list without
an I/O error. If a live layer left Writing/Staged, the reader discards that attempt
and obtains a fresh authorized plan, up to three attempts per request. Missing-file
errors permit that refresh only when the catalog confirms the transition. Identity,
byte-limit and other errors fail immediately. Cache leases and temporary copies are
released between attempts. An unlink after a part is opened cannot interrupt its copy.

`materialize_analysis_snapshot` requires explicit Artifact read authority and a
positive source-byte limit. A caller must match the recording identity and labels.
A task capability is checked through Artifact's current scope endpoint before any
catalog access, including snapshots containing only live ingest parts. Its verified
principal, tenant and labels must match the durable task owner. The tighter of the
capability's byte ceiling and the requested source limit bounds the cumulative source
bytes; each part's length is admitted before copying it.
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
Cache APIs, validators, read plans and snapshots carry `Sha256Digest`. Store and RRD
inspection adapters admit text before those values enter the reader. Cache filenames
serialize the digest as bare lowercase hex. Video's public snapshot fields use the
shared `veoveo_types::sha256_hex` adapter.
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

The canonical cache validator takes distinct public dataset and recording identities
and a typed SHA-256 digest. It verifies the complete Store ID, byte length and digest on
materialization and cache reuse. Task-local live-part normalization converts native
repository identities explicitly. Upstream producer names remain open in source
inspection. These controls check bytes and relationships, not playback or GPU execution.

## Build Boundary

Stream, Reason and the video materializer depend on this crate. Recording MCP composes
the same cache and visibility rules for playback. Stream, Reason and Video import
the shared Recording domain types; server consumers may use the MCP library’s
isolated contract feature. That Cargo edge excludes
Hub and Recording service implementations. Public URI parsing belongs to Recording;
the reader accepts Store identities.
All shared Cargo manifests remain available to image planning. Runtime input contexts
must still follow the production dependency closure to realize this source isolation.

Cancellation drops a download reservation, removes its partial file, and releases
managed capacity. A completed cache entry becomes pinned only after full validation.
Startup removes abandoned partial files and rejects unmanaged entries. Reader
readiness includes the cache's free-space and managed-byte checks.
