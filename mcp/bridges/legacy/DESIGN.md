# Legacy MCP Bridge Design

## Standards And Protocols

| Standard or protocol | Boundary |
|---|---|
| MCP `2026-07-28` | Sole Veoveo-facing protocol, with Discover and stateless Streamable HTTP |
| MCP `2025-11-25` | Explicit external connector profile only, established through Initialize |
| JSON-RPC 2.0 | Request and response envelope on both MCP connections |
| MCP Streamable HTTP | Optional remote external connector and the stateless internal endpoint |
| MCP stdio | Optional locally owned external child connector |

## Purpose

The optional bridge admits a configured third-party server that has not migrated to
MCP `2026-07-28`. Configuration selects either one local stdio child or one remote
Streamable HTTP endpoint. The connector never probes or downgrades.

The bridge observes the legacy Initialize result and exposes the supported tools,
resources, resource templates, prompts, and completions through a final-profile
server. Catalog results are sorted and receive explicit private cache policy. Legacy
resource-not-found errors are translated to final `Invalid Params`.

The bridge does not fabricate Tasks, multi-round requests, subscriptions, Roots,
Sampling, Logging, or any extension capability. Loss of the owned child or remote
connection terminates the process and fails in-flight work without replay.

## Security Boundary

The HTTP listener must remain on loopback or an installation-internal network. A
remote bearer token is read from the named environment variable and never accepted
as a command-line value. Installation policy still decides whether the bridge's
observed surface is exposed by the gateway.
