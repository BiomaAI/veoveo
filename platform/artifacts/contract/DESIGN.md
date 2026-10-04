# Artifact Plane Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| UUID, RFC 9562 | UUID 1.25.0 parses occurrence identities; admission requires version 7 and the RFC variant. Serialization uses lowercase hyphenated form. `ArtifactId::new` generates a v7 identity using the system clock and entropy. |
| JSON | Serde string identities and structured metadata; constructors validate nested identity fields during deserialization |
| JSON Schema 2020-12 | Schemars schemas for public identity, metadata, compliance, provenance, and release-state values; provenance uses mode-specific alternatives with required attribution identities |
| RFC 3339 timestamps | Chrono 0.4.45 date/time values with Serde support; this crate excludes Chrono's clock feature |
| Veoveo Artifact addresses | Neutral `artifact://{id}` occurrences and server presentations such as `media://artifact/{id}` |
| [Veoveo concrete resource profile](../../types/DESIGN.md#concrete-resource-components) | URL 2.5.8 parses components and builds addresses; Artifact addresses exclude escapes, queries, fragments, credentials, ports, templates, and additional path segments |

## Ownership And Dependencies

This library owns the Artifact plane's occurrence identity, metadata, compliance,
provenance, release state, grant records, share-link values and identities, and
in-process byte handoff values. The Artifact service,
HTTP client, MCP server, and other domains import these definitions directly.
The library depends on foundational identity types, UUID, date/time values, and
serialization/schema support. It has no MCP, asynchronous runtime, database, GPU,
provider, or HTTP dependency.

The Artifact plane has its own service and client. Housing their common types in
the MCP server package would create a server-to-client-to-server Cargo cycle when
the server runtime enables the client. This domain-owned crate supplies the shared
model below both adapters. It introduces no process or deployment requirement.
The MCP server exposes the plane model through its isolated `contract` feature,
alongside its tool DTOs and resource families.

Artifact access evaluation, Work Context membership, grant composition, capabilities,
and transport-facing request/response types currently live in `mcp/contract`.
`ledger.rs` owns distinct UUIDv7 identities for access requests, read and write
capabilities, uploads, upload requests and Artifact Tasks. Its `ArtifactLedgerAddress` builder constructs
private `veoveo://artifact-plane/` addresses for audit targets and related ledger
objects. These addresses grant no access and declare no public MCP read route.
The service and gateway import the same builder; MCP transport DTOs re-export its IDs.
This library neither authenticates metadata nor authorizes a read or mutation.
Artifact service continues to enforce current tenant, context, clearance, and grants.
`Grant` implements the foundational `AccessGrant` trait for the shared evaluator;
its persisted and public fields keep their Artifact-specific identity.

## Wire And Construction

`ArtifactId` parses the UUID library's supported textual spellings, checks version and variant,
and emits the lowercase hyphenated identity. A content hash never identifies an
occurrence. `ArtifactIdError` describes the required version without echoing input.

`ArtifactMetadata` carries the occurrence, byte count, MIME and filename presentation,
creation time, release state, compliance, and open producer metadata. Its compliance
model uses specific foundational identity and label types. `ArtifactProvenance::new`
accepts a producer, the foundational `InvocationProvenance` enum, and a policy revision.
The enum requires an initiator for direct invocation and both an initiator and a
delegation identity for delegated invocation. Automated invocation has neither.
Services establish the truth of those claims; typed construction grants no authority.

`ArtifactPut` and `ArtifactObject` are in-process byte handoffs. They expose owned
byte vectors and metadata without transport machinery. A streaming transport may
use its own declared streaming interface.

`ArtifactUri` owns a validated wire reference and its parsed `ArtifactAddress` variant.
`plane(ArtifactId)` constructs a neutral address; `presented(&ResourceScheme, ArtifactId)`
constructs a server presentation. Both use the foundation's typed authority and path
builder. A new server supplies its own scheme without changing this library or MCP core.
`ResourceAddress` parsing returns the same typed model. A presentation identifies an
occurrence; it never directs the Artifact client to fetch from that URI's host.
The server's scheme and metadata confer no permission to read the occurrence.

`ArtifactMetadata::artifact_id()` derives the occurrence from its typed `artifact_uri`.
Serialization emits the public `artifact_id` field, and decoding verifies that it
agrees with the URI. Internally there is one identity source. The HTTP client and
service resolution interfaces require `ArtifactUri`; strings enter at HTTP and JSON
decoding. `TryFrom<Uuid>` checks version and variant when a driver provides a UUID.
Optional `download_url` typing and remaining access/service contracts are work in the
[consolidated plan](../../../docs/CONTRACT_CONSISTENCY_PLAN.md#modular-types-and-server-contracts).

## Metadata Snapshots

`ArtifactMetadataSnapshot` carries service-owned neutral metadata, read-grant subjects
and their expiry, and the stored metadata update time. Its constructor validates the
grant occurrence and tenant before reducing administrative grants to read information.
The wire decoder requires stored tenant, owner, Work Context and provenance. It rejects
transfer URLs, repeated subjects and update times preceding creation. Grants serialize
in subject order. The timestamp describes metadata; access revisions also include the
read grants and their deadlines.

The Artifact service admits the caller before returning a snapshot. The type confers
no authority by itself and introduces no knowledge-extension dependency. Consumers
can map it into their own protocol observations while keeping Artifact policy in the
service. `tests/snapshot.rs` qualifies construction, decoding and schema output.

## Address And Identity Profile

Generated URIs use lowercase hyphenated UUIDs. The parser preserves accepted input
spelling, including uppercase and simple UUID forms in neutral or presented addresses
and `urn:uuid:` inside a presentation's path segment. Standalone occurrence IDs also
accept the UUID library's braced and URN spellings. Those spellings are not valid
neutral URI authorities. Encoded aliases and braced URI templates are rejected.

Readers compare occurrence identities through `artifact_id()` and resource spelling
through `ArtifactUri` equality. The download client additionally requires the Artifact
service's neutral metadata address in its generated spelling. JSON schemas describe
string references; domain decoding enforces route, UUID and cross-field requirements.
Malformed input, non-RFC UUID variants and contradictory ID/URI pairs fail admission.
The private metadata wire type exposes the repeated ID and checks its agreement with
the parsed URI before constructing the model.

## Attribution Wire Profile

Artifact JSON carries `producer`, `invocation_mode`, `initiator`, `delegation_id` and
`policy_revision` as flat fields. A private wire type maps those fields to the
foundational invocation enum. Authorization uses the service's verified invocation authority.

| Mode | Required attribution | Other attribution fields |
|---|---|---|
| `direct` | `initiator` | `delegation_id` must be omitted or null |
| `delegated` | `initiator`, `delegation_id` | Both identities must be non-null |
| `automated` | None | `initiator` and `delegation_id` must be omitted or null |

Serialization omits absent fields. Decoding ignores unknown extra JSON fields. The
private wire type uses an optional uninhabited type for inapplicable identities,
which rejects values including empty objects. Contradictory attribution and incomplete
delegated or direct claims fail decoding. External producers supply the identities
their claimed mode requires.

The Artifact ledger stores invocation authority separately and validates it while
reconstructing `InvocationProvenance`. Publication and repository reads build metadata
from that checked authority. Metadata alone cannot establish who initiated the work.

## Qualification

Tests compare the five public schemas with the captured contract fixture, check
UUID admission and canonical serialization, round-trip nested metadata, and reject
invalid nested identities. URI tests cover independent and standard schemes, accepted
UUID spellings, version/variant admission, malformed components, input-free errors,
and metadata identity agreement. Compile-fail cases reject raw or unrelated IDs and
unvalidated schemes. Generated schemas qualify current metadata output.
An independently resolved consumer must exclude service and adapter dependencies.
MCP and service tests qualify authorization and transport behavior separately.
Attribution checks cover schema/decoder agreement for all mode and identity-presence
combinations, compile-time rejection of an incomplete delegated invocation, and
publication/readback through memory and independently connected database repositories.
Schema qualification uses the workspace's pinned JSON Schema validator, also used by
gateway and MCP conformance; its next upgrade is qualified across those consumers.

## Sharing Values

`access.rs` owns the Artifact `Grant`, `ArtifactShareLink`, and `ArtifactShareLinkId`
values used by the service, MCP, gateway and Console. The grant retains its occurrence,
subject, access level, tenant, labels and optional retention deadline. It contains no
policy evaluator. The service applies current policy before storing or returning it.

Share identities are distinct RFC UUIDv7 values. Their decoder rejects other versions
and non-RFC variants without echoing the input. Share links keep the existing JSON
fields and redact bearer URLs from Debug output. Transport adapters map invalid
share identities to their existing invalid-request response.

## Identity Declaration Mechanics

Occurrence and private ledger identities use `Id` with Artifact-owned RFC UUIDv7 admission. Parser aliases remain valid input and display emits the normalized UUID. Owner Serde declarations keep their String wire representation and existing unconstrained string schema; generation and typed accessors stay separate from authority.
