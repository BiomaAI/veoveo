# Recording Persistence

## Standards And Protocols

This library uses the SurrealDB Rust SDK `3.3.0` over the shared Store WebSocket
connection. Private records use typed `SurrealValue` encoding. The internal
`veoveo-modules` declaration API owns schema claims and lane identity; it is separate
from Recording's public MCP, resource and playback contracts.

## Ownership

`RecordingRepository` holds a shared authenticated `PlatformStore` connection.
It owns Recording persistence identities, driver records, dataset and layer lifecycle,
read grants, projection receipts, ingest streams and Blueprint revisions. Queries
live in `src/queries`; callers bind admitted values and retain transaction decoding.
Native fixture statements live in `tests/queries/`, with complete static files for
finite grant and Recording corruption cases.
`PlatformStore` supplies the connection and kernel owner APIs, and does not define
Recording's domain records. The Recording service owns current caller admission,
Artifact transport, playback and native Redap integration.

## Schema And Features

The `schema` feature exports the optional `recordings` module without a database
driver or runtime. It claims `recording_*` and `recording`, and requires Tasks and
its earlier kernel requirements. Migration zero installs the established tables from
`src/schema/migrations/0000_current.surql`; migration one declares the immutable
`recording_manifest_publication` table. Composition supplies execution;
repository construction and connection startup apply no schema.

The default `persistence` feature exposes repositories and activates the database
client. The optional lightweight Recording and Artifact contract dependencies activate
only under `persistence`; schema-only consumers disable defaults. Owner observation declarations
identify native LIVE and changefeed sources without granting read authority.

## Projection Receipt Lifetime

Reservation requires a grant whose expiry reaches the receipt's expiry. Reads and
transitions check that grant, its scope, recordings and catalog revision again.
The authority cleanup transaction removes expired receipts before expired grants.
A missing grant denies access to its receipt. This joint expiry owns cleanup without
a reference cascade; retained recordings and their published products have separate
lifetimes.

Layer staging accepts the shared SHA-256 type for content and optional schema digests.
The repository converts to bare lowercase hexadecimal at query binding, preserving the
database profile, compare-and-set transaction and full idempotent readback comparison.
Native recording revisions permit zero; sealed properties admission uses the same
nonnegative profile.

## Manifest Publication Intent

`manifest_publication.rs` owns version-1 intent records under the existing Recording
lane. Native columns identify tenant, initiating actor, full invocation authority,
selected source owner and Work Context, dataset and Recording epochs, seal time,
and descriptor hash. Typed driver adapters store the current checked RecordingManifest
and canonical StreamArtifactRequest with complete SCHEMAFULL child declarations.
The Artifact descriptor preserves classification, labels, retention, MIME, filename,
reserved occurrence, body length, SHA and closed Recording provenance. The lane
introduces no backfill or compatibility defaults.

Reservation checks the Recording's Sealing state, current revision, tenant and source
owner/Work Context, the selected dataset epoch, every committed layer revision, and the current
optional Blueprint absence or ID, revision and Artifact occurrence in one transaction before inserting the intent. The intent does not advance the
Recording revision. A conflict leaves the fence in place; a transport failure has
an unresolved outcome. Replay admits the stored descriptor/body and original source
snapshot before effects. Local manifest files derive from that intent and do not
authorize recovery independently.

The descriptor hash covers the owner's request serialization, including its closed
Recording metadata with recursively sorted object keys. The producer sorts metadata
before freezing the hash. Reservation rejects a different metadata key order before
persistence. Native objects pass through the complete typed metadata decoder and the
same sorting profile on read, independent of serde_json's map-order feature.
Unknown or malformed metadata refuses admission; the adapter checks the recorded
hash rather than calculating a replacement identity.

The reservation compares the complete unique selected layer set. It registers named
Recording, dataset, layer and selected Blueprint records with SurrealDB 3.3
`FOR UPDATE`. Every source-selection transaction registers the deterministic intent
ID even when absent. A genuine layer or Blueprint selection advances the existing
Recording revision in that transaction. Intent-first commits invalidate a concurrent
source writer through the absent-intent dependency; source-first commits invalidate
a concurrent reservation through its Recording dependency. Commit conflicts refuse
the whole transaction. Exact duplicate no-ops and intent reservation advance no
revision and invoke no external retry.

After seal, the catalog permits Derived layers whose IDs are absent from the
publication's selected members. Their open, stage, commit and failure transactions
keep the parent/intent dependencies and check this exclusion. A Derived member
selected before publication stays immutable. Later catalog rows do not change the
original manifest, Properties preimage or publication intent.
The retained receiver selects the bounded intent ID list with tenant and Recording
predicates before pagination. The same SQL scans the whole parent for disallowed
unselected rows; Rust checks the complete selected count and body relationships.
General catalog page limits do not constrain immutable manifest admission.

`properties_preparation.rs` stores version 1 with the canonical checked
RecordingProperties body in the existing properties layer row. Capture and derived
layers have no preparation. Reservation captures the original source epoch, seal
time and complete properties preimage before file or Artifact effects. Writing and
Staged recovery use that admitted body, not the mutable Recording row. Migration
one declares each known child and the two honestly open string revision dictionaries;
migration zero stays unchanged. Missing or unsupported preparation refuses admission.
Properties reservation compares the complete non-Properties source set, requires every
member to be committed, and registers each selected record with `FOR UPDATE` before
persisting the body. A missing, added, pending or changed member refuses the transaction.
Sealing prevents new Capture and Derived opens; committed source mutations admit only
exact no-ops. This stabilizes the preparation through subsequent publication.
The native optional Blueprint object stores absence as NONE; its focused body adapter
restores public null before strict current decoding. Other native NONE values refuse.

Current service permission is separate from recorded invocation facts. Settling an
already confirmed occurrence requires current Seal scope, SQL visibility and fresh
Artifact-read permission. Original policy revision and membership confer no present
authority. Effectful replay uses the original occurrence and descriptor with current
publisher authentication; Artifact's create-only retry predicate decides whether the
current producer authority agrees with an existing outcome.

The coordinated current-format deployment drains writers and selects migration one
before enabling this producer. Retained outcomes without a current intent refuse
admission. Rollback preserves intent, native journals and occurrences; an older
producer cannot consume this current intent/body cohort. No destructive conversion
or historical reader belongs to the repository. Native lane execution, cross-replica
replacement and installed recovery require their owning qualification controls.

## Qualification

The `recording_catalog`, `recording_grants` and `recording_projections` integration
suites use isolated native fixtures and current selected lanes. They check database
behavior rather than playback, public authorization or installed image/Job operation.
Those acceptance requirements belong to the
[Recording service design](../../../servers/recording-mcp/DESIGN.md).
