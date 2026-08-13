# Legacy MCP Bridge Instructions

## Contract Compliance

This optional bridge is the repository's only admitted legacy MCP lifecycle
boundary. Its external connector is fixed to MCP `2025-11-25`; its Veoveo-facing
endpoint implements contract revision 3 and MCP `2026-07-28` only.

Never add automatic downgrade, more legacy versions, Tasks synthesis, multi-round
input synthesis, subscriptions, Roots, Sampling, Logging, or elicitation forwarding.
Only expose a method after its translation is exact and directly tested.

The bridge owns its child process or remote connection for its complete lifetime.
An external connection loss fails active calls and terminates the bridge. It never
replays a call.
