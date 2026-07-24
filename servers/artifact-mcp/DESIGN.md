# Artifact MCP Server

The artifact server is the typed MCP surface over the artifact plane:
discovery, metadata, access grants, release state, and revocable sharing.
It projects artifact-service state through MCP; bytes never flow through this
server. Byte policy enforcement and presigned delivery remain in
artifact-service, and SurrealDB remains authoritative for occurrences,
identity, grants, release state, shares, policy, and audit.

## Protocol Surface

The server owns the `artifact://` scheme:

| Surface | Identity |
|---|---|
| index resource | `artifact://index` |
| occurrence template | `artifact://{artifact_id}` |
| metadata template | `artifact://metadata/{artifact_id}` |
| grants template | `artifact://grants/{artifact_id}` |

Metadata, grant, release, and share operations are tools with declared input
and output schemas generated through the shared `tool` macro. Domain types
(`ArtifactId`, `ArtifactMetadata`, `Grant`, `ArtifactReleaseState`,
`ArtifactShareLink`) come from `veoveo_mcp_contract`.

## Boundaries

- Artifact identities are opaque `artifact://{uuidv7}` occurrences; hashes
  serve integrity and deduplication within a tenant and are never public
  addresses.
- Every call presents the forwarded gateway internal identity; the server
  verifies it with the shared gateway token verifier.
- Share links are expiring, revocable, and read only, with optional download
  limits, exactly as the governance model states.
- The server keeps no private control database and serves no bytes.

## Standards And Protocols

Model Context Protocol over JSON-RPC 2.0 and Streamable HTTP; JSON Schema
2020-12 tool contracts; platform artifact identities per
[`docs/WORK_CONTEXT_GOVERNANCE.md`](../../docs/WORK_CONTEXT_GOVERNANCE.md).
