# Artifact Plane Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| UUID, RFC 9562 | UUID 1.25.0 parses occurrence identities; admission checks the version field equals 7 without a separate variant check. Serialization uses lowercase hyphenated form. `ArtifactId::new` generates a v7 identity using the system clock and entropy. |
| JSON | Serde string identities and structured metadata; constructors validate nested identity fields during deserialization |
| JSON Schema 2020-12 | Schemars schemas for public identity, metadata, compliance, provenance, and release-state values |
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
model uses specific foundational identity and label types. Artifact provenance
preserves the producer, invocation mode, optional initiator/delegation, and policy
revision. Services establish the truth of those claims.

`ArtifactPut` and `ArtifactObject` are in-process byte handoffs. They expose owned
byte vectors and metadata without transport machinery. A streaming transport may
use its own declared streaming interface.

URI presentation helpers currently return strings. Their parser recognizes the
neutral occurrence and server-presented forms; it is not a general URI validator.
`artifact_uri` and optional `download_url` remain string fields. Typed address and
builder adoption, including admission of existing UUID spellings and variant validation, is explicit work
in the [foundations plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).
The server's scheme and metadata confer no permission to read the occurrence.

## Qualification

Tests compare the five public schemas with the captured contract fixture, check
UUID admission and canonical serialization, round-trip nested metadata, and reject
invalid nested identities. URI and presentation tests belong here with their types.
An independently resolved consumer must exclude service and adapter dependencies.
MCP and service tests qualify authorization and transport behavior separately.
