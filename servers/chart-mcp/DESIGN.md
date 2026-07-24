# Chart MCP Server

The chart server retains the pinned upstream `flint-chart-mcp` domain
implementation. Veoveo owns its network launcher because upstream HTTP mode
is stateless and returns direct JSON. The launcher gives the complete
upstream surface a sessionful Streamable HTTP endpoint with event-stream
responses.

## Packaging Contract

- The Dockerfile pins the upstream version (`flint-chart-mcp@0.3.0`) and the
  Node base image; upgrades are explicit digest and version changes reviewed
  like any dependency bump.
- The container runs as an unprivileged system user with a fixed uid and
  serves on port 8795.
- The server keeps no domain data in a private database
  (`platformStore: false`). MCP sessions remain local to the one active
  launcher process.
- The launcher owns each transport lifecycle. MCP `DELETE` closes the
  transport and removes its session without recursively closing the connected
  protocol server.
- The gateway entry in the installation control plane owns identity, routes,
  policy, and audit, the same as every Rust server.

## Upstream Surface

Chart validation, compilation, static rendering, and the interactive chart
MCP App are upstream behavior. Protocol compliance for that surface is
verified through the conformance client against the running server; source
review of the upstream package is out of scope for this repository.

## Standards And Protocols

Model Context Protocol over JSON-RPC 2.0, sessionful Streamable HTTP with
event-stream responses, and MCP Apps per
[`mcp/apps-extension/DESIGN.md`](../../mcp/apps-extension/DESIGN.md).
