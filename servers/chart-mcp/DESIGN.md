# Chart MCP Server

The chart server retains the pinned upstream `flint-chart-mcp` rendering and
Flint domain implementation. Veoveo owns its MCP registration and network
launcher because the upstream release still targets an older MCP SDK.

## Standards And Protocols

Model Context Protocol `2026-07-28` over JSON-RPC 2.0 and stateless Streamable
HTTP, JSON terminal responses, request-scoped subscription streams, JSON
Schema 2020-12, and MCP Apps per
[`mcp/apps-extension/DESIGN.md`](../../mcp/apps-extension/DESIGN.md). The
official TypeScript server and Node packages are pinned to `2.0.0`.
The [knowledge-source extension](../../mcp/knowledge-extension/DESIGN.md) declares
the `charts.docs` collection using SHA-256 document revisions and RFC 9110
conditional-read semantics. Documents are immutable for the running image.

## Packaging Contract

- The Dockerfile pins the upstream version (`flint-chart-mcp@0.5.1`) and the
  Node base image; upgrades are explicit digest and version changes reviewed
  like any dependency bump.
- The container runs as an unprivileged system user with a fixed uid and
  serves on port 8795.
- The server keeps no domain data in a private database
  (`platformStore: false`). Every MCP request uses a fresh protocol instance;
  there is no protocol session or sticky-replica state.
- The canonical endpoint is `/charts/mcp`. Health is `/charts/healthz`, and
  the authenticated read-only document projection is under
  `/charts/admin/docs`. The launcher verifies the gateway's Ed25519 internal
  token against the installation trust bundle with audience `charts`.
- The gateway entry in the installation control plane owns identity, routes,
  policy, and audit, the same as every Rust server.

## Upstream Surface

Chart validation, compilation, and static rendering use the upstream domain
and render exports. The direct-launch `ui://charts/composer.html` App owns a
session-local authoring draft, validates and compiles through canonical tools,
and renders through the same upstream backend. `flint-v2.mjs` owns their
final-protocol registration and schemas.

The server owns `charts://chart-types`, `charts://agent-skill`, and
`charts://theme-skill`; prompts reference those same addresses. Its tool, resource
and prompt lists are fixed at startup and advertise no list-change notifications.
The Composer App remains at `ui://charts/composer.html`.

## Knowledge Documents

`build-docs.mjs` hashes the original UTF-8 document bytes during image construction.
`documents.mjs` requires the resulting `_documents.json` manifest at startup and
rejects missing, altered, empty, oversized or invalid UTF-8 documents. The document
budget reserves 1 KiB for provenance within the kernel's 64 KiB item limit.

`knowledge.mjs` declares `charts.docs` on `charts://docs/{doc_id}` and pages its
index by document ID. It uses the SDK's URI templates and the WHATWG URL API
for concrete addresses. Knowledge negotiation reads the current request's capability
envelope; the read condition comes from ordinary request metadata. Closed schemas
reject unsupported settings and malformed validators. Observation values come only
from image documents and server time.

The HTTP launcher verifies the gateway token before dispatching every read.
A matching validator returns empty contents and `notModified` only after that check.
Readers that omit the extension receive document contents without an observation.
Resource reads have private, zero-TTL cache hints so a conditional request reaches
the server's authorization check. Chart exposes no knowledge search or change
subscription because its only collection contains image documents.

The native `knowledge.test.mjs` fixture exercises the pinned SDK over loopback HTTP
with synthetic signed tokens. It checks negotiation, cursor rejection, original byte
hashes, conditional delivery, expired and wrong-audience identity denial, forged
observation rejection and manifest tampering. The fixture does not render charts;
visual qualification uses the installed hardware workflow.
