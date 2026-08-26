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
