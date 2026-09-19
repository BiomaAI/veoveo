# Agent Authoring Browser Edge

## Standards And Protocols

The edge projects Veoveo's typed agent-management HTTP JSON and contentless SSE
contracts through the existing OAuth/PKCE, application cookie and CSRF boundaries.
Canonical DTOs live in `mcp/contract/src/agent_management`. This is an application
projection, not an MCP server or a provider credential endpoint.

## Authority And Forwarding

Console and Workspace mount the same closed handlers with separate application
state. The upstream origin and profile come from installation configuration.
Typed definition IDs, bounded page cursors and closed request DTOs determine the
route. Browser-supplied authorization and destination headers never pass upstream.
Each application's existing CSRF middleware protects mutations.

Responses decode into the expected canonical type within a byte and time bound.
Validation findings survive a rejected publication. Session rotation settles on
success and failure. SSE uses bounded chunks and idle timeouts; the gateway owns
current authority checks and reconnect reconciliation. No observer invokes a model.
