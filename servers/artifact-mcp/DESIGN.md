# Artifact MCP Server

The artifact server is the typed MCP surface over the artifact plane:
discovery, metadata, access grants, release state, and revocable sharing.
It projects artifact-service state through MCP; bytes never flow through this
server. Byte policy enforcement and client streaming remain in Artifact service,
and SurrealDB remains authoritative for occurrences, identity, grants, release
state, shares, policy, and audit.

## Standards And Protocols

Model Context Protocol over JSON-RPC 2.0 and Streamable HTTP; JSON Schema
2020-12 tool contracts; platform artifact identities per
[`docs/WORK_CONTEXT_GOVERNANCE.md`](../../docs/WORK_CONTEXT_GOVERNANCE.md).
MCP Apps uses SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26`; the
server-owned `ui://artifact/library.html` resource is the canonical Library UI.
Artifact bytes use HTTP GET, HEAD, and single byte-range semantics through the
installation origin. The private S3-compatible API is not an MCP or client
contract.

## Protocol Surface

The server owns the `artifact://` scheme:

| Surface | Identity |
|---|---|
| index resource | `artifact://index` |
| Library App | `ui://artifact/library.html` |
| occurrence template | `artifact://{artifact_id}` |
| metadata template | `artifact://metadata/{artifact_id}` |
| grants template | `artifact://grants/{artifact_id}` |

Metadata, grant, release, and share operations are tools with declared input
and output schemas generated through the shared `tool` macro. Domain types
`ArtifactId`, `ArtifactMetadata`, and `ArtifactReleaseState` come from the
domain-owned [`veoveo-artifact-contract`](../../platform/artifacts/contract/DESIGN.md).
`Grant`, `ArtifactShareLink`, and its distinct ID also come from the Artifact plane
contract. Authentication, policy evaluation and service request interfaces stay in
`veoveo_mcp_contract`.

## Boundaries

- Artifact identities are opaque `artifact://{uuidv7}` occurrences; hashes
  serve integrity and deduplication within a tenant and are never public
  addresses.
- Every call presents the forwarded gateway internal identity; the server
  verifies it with the shared gateway token verifier.
- Share links are expiring, revocable, and read only, with optional download
  limits, exactly as the governance model states.
- The server keeps no private control database and serves no bytes.

## Library Features And Addresses

Clients select `default-features = false, features = ["contract"]` on the server
library. Its public `contract` module contains tool DTOs and `ArtifactResource`.
The resolved dependencies contain identity, URI, date/time, serialization and schema
libraries. The `runtime` feature supplies service clients and persistence dependencies;
`mcp` adds the hosted executable and transport. Default builds select `mcp`.

`ArtifactResource` covers occurrences, metadata, grants, document roots and the
contract resource. Constructors take `ArtifactId` or the closed `ArtifactDocument`
enum, and use the foundation URI builder. Parsers reject query strings, fragments,
encoded aliases, unrelated schemes and extra segments. A URI locates an occurrence;
current service authorization still decides whether the caller can read it.
Prompt inputs decode the same Artifact ID and use these builders for references.

The independent [contract consumer](../../testing/fixtures/server-contract-consumer/DESIGN.md)
checks the public library without runtime feature unification. Native service tests
own authorization and transport qualification.

<!-- TODO(foundations): Qualify installed Artifact reads/sharing and the Computers/Speech
hosted feature builds at the next Phase 3 integration checkpoint; native contract gates
and direct consumers pass, and the reference cluster stays stopped during development. -->
