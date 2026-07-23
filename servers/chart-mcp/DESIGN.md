# Chart MCP Server

The chart server is a packaged wrapper, its implementation is the pinned
upstream npm package `flint-chart-mcp` rather than a workspace crate. The
directory owns the packaging contract that turns that upstream into a hosted
Veoveo server: a reproducible image, a stateless deployment, and a governed
gateway registration.

## Packaging Contract

- The Dockerfile pins the upstream version (`flint-chart-mcp@0.2.2`) and the
  Node base image; upgrades are explicit digest and version changes reviewed
  like any dependency bump.
- The container runs as an unprivileged system user with a fixed uid and
  serves on port 8795.
- The deployment is stateless (`platformStore: false`): the server keeps no
  platform state and no private database, satisfying the contract's runtime
  boundary by construction.
- The gateway entry in the installation control plane owns identity, routes,
  policy, and audit, the same as every Rust server.

## Upstream Surface

Chart validation, compilation, static rendering, and the interactive chart
MCP App are upstream behavior. Protocol compliance for that surface is
verified through the conformance client against the running server; source
review of the upstream package is out of scope for this repository.

## Standards And Protocols

Model Context Protocol over JSON-RPC 2.0 and Streamable HTTP as implemented
by the pinned upstream; MCP Apps per
[`mcp/apps-extension/DESIGN.md`](../../mcp/apps-extension/DESIGN.md).
