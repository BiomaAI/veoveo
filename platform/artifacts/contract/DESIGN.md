# Artifact Plane Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| UUID, RFC 9562 | UUID 1.25.0 parses occurrence identities; admission checks the version field equals 7 without a separate variant check. Serialization uses lowercase hyphenated form. `ArtifactId::new` generates a v7 identity using the system clock and entropy. |
| JSON | Serde string identities and structured metadata; constructors validate nested identity fields during deserialization |
| JSON Schema 2020-12 | Schemars schemas for public identity, metadata, compliance, provenance, and release-state values; provenance uses mode-specific alternatives with required attribution identities |
| RFC 3339 timestamps | Chrono 0.4.45 date/time values with Serde support; this crate excludes Chrono's clock feature |
| Veoveo Artifact addresses | Neutral `artifact://{id}` occurrences and server presentations such as `media://artifact/{id}` |

## Ownership And Dependencies

This library owns the Artifact plane's occurrence identity, metadata, compliance,
provenance, release state, and in-process byte handoff values. The Artifact service,
HTTP client, MCP server, and other domains import these definitions directly.
The library depends on foundational identity types, UUID, date/time values, and
serialization/schema support. It has no MCP, asynchronous runtime, database, GPU,
provider, or HTTP dependency.

The Artifact plane has its own service and client. Housing their common types in
the MCP server package would create a server-to-client-to-server Cargo cycle when
the server runtime enables the client. This domain-owned crate supplies the shared
model below both adapters. It introduces no process or deployment requirement.
The MCP server's public library can expose the plane model with its own domain
contract; that feature's remaining dependencies are tracked in the foundations plan.

Artifact access evaluation, Work Context membership, grant composition, capabilities,
and transport-facing request/response types currently live in `mcp/contract`.
This library neither authenticates metadata nor authorizes a read or mutation.
Artifact service continues to enforce current tenant, context, clearance, and grants.

## Wire And Construction

`ArtifactId` parses the UUID library's supported textual spellings, checks the version field,
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

URI presentation helpers currently return strings. Their parser recognizes the
neutral occurrence and server-presented forms; it is not a general URI validator.
`artifact_uri` and optional `download_url` remain string fields. Typed address and
builder adoption, including admission of existing UUID spellings and variant validation, is explicit work
in the [foundations plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).
The server's scheme and metadata confer no permission to read the occurrence.

## Attribution Wire Compatibility

The Artifact 0.1.x JSON profile carries `producer`, `invocation_mode`, `initiator`,
`delegation_id`, and `policy_revision` as flat fields. A private wire adapter maps
that profile to the foundational invocation enum. It is a serialization interface;
authorization uses the service's verified invocation authority.

| Mode | Required attribution | Other attribution fields |
|---|---|---|
| `direct` | `initiator` | `delegation_id` must be omitted or null |
| `delegated` | `initiator`, `delegation_id` | Both identities must be non-null |
| `automated` | None | `initiator` and `delegation_id` must be omitted or null |

Serialization omits absent fields. Unknown extra JSON fields retain the 0.1.x
admission behavior. The private wire type uses an optional uninhabited type for
inapplicable identities, so neither JSON decoding nor the schema admits a value there.
This also rejects empty objects, which a Serde unit field can otherwise accept inside
an internally tagged enum.

Older and current readers consume the same valid 0.1.x representations. Golden cases
qualify every mode against the previous schema and the typed decoder. This permits mixed-version
service/client operation without a drain or wire conversion. Contradictory attribution
and incomplete delegated/direct claims fail decoding; no default actor or mode repairs
them. External producers must supply the identities their claimed mode requires.

The Artifact ledger stores invocation authority separately and already rejects those
invalid combinations while reconstructing `InvocationProvenance`. Metadata is built
from that checked authority when an occurrence is published or read. This change
requires no persistent rewrite. Existing records with contradictory authority remain
corrupt and require operator investigation; changing metadata cannot establish who
initiated the work. Rollback reads the unchanged valid representation. The Rust library
uses one internal model, with no old field aliases. The 0.1.x wire adapter stays while
that public profile is supported; removal requires a versioned metadata protocol.

## Qualification

Tests compare the five public schemas with the captured contract fixture, check
UUID admission and canonical serialization, round-trip nested metadata, and reject
invalid nested identities. URI and presentation tests belong here with their types.
An independently resolved consumer must exclude service and adapter dependencies.
MCP and service tests qualify authorization and transport behavior separately.
Attribution checks cover schema/decoder agreement for all mode and identity-presence
combinations, compile-time rejection of an incomplete delegated invocation, and
publication/readback through memory and independently connected database repositories.
Schema qualification uses the workspace's pinned JSON Schema validator, also used by
gateway and MCP conformance; its next upgrade is qualified across those consumers.
