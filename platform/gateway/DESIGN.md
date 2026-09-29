# Gateway MCP Forwarding

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP | The repository's hosted MCP 2026-07-28 profile, with typed protocol envelopes, resource discovery, tools, Tasks and subscriptions; the [server contract](../../mcp/contract/DESIGN.md) defines requirements |
| MCP Apps | Extension `io.modelcontextprotocol/ui`, release 2026-01-26; `_meta.ui.resourceUri` links a tool to its App document |
| Resource addresses | Repository-owned server schemes and `ui://` App routes validated through foundational URI types; installation manifests select identity or server-owned projection |
| Installation authority | Typed server manifests, profile exposure and policy from the MCP gateway contract; [authentication](src/auth/DESIGN.md) resolves request identity |

## Ownership

The gateway authenticates and authorizes a request before forwarding it to a selected
server. Its catalog composes server manifests and exposed capabilities. Domain servers
own their tool schemas, result values, resource content and identifiers. The gateway
does not enumerate domain payload types or require a new producer to change core.

`mcp_support.rs` owns the transformation of protocol resource addresses when an
installation selects server-owned projection. Resource discovery, templates, embedded
resource envelope addresses and resource links use the server's registered namespace.
The MCP Apps `_meta.ui.resourceUri` field receives the corresponding App address.
`referenced_resource_schemes` preserves protocol links owned by another registered
server; installation validation checks that declaration against the registry.

## Domain Payload Preservation

Structured tool results, resource content and unknown extension metadata keep their
upstream values. A URI-shaped string inside them does not identify a gateway routing
field. For example, a Frames world can reference an independent pose producer through
its own scheme. Changing that reference changes the world and invalidates its digest.
Payload preservation applies to every producer without a scheme allowlist.

The gateway changes only the App link within extensible metadata. CSP origins,
visibility, domain references and other metadata fields keep their values. A malformed
App link produces a protocol error. This transformation grants no resource access;
the selected profile and caller policy still govern subsequent requests.

An adapter that changes a domain's public namespace must construct coherent domain
payloads itself. The gateway cannot repair arbitrary result strings, recompute domain
digests or infer foreign-resource ownership. Installation upgrades use one gateway
behavior; no compatibility mode rewrites domain payloads.

## Qualification

Native forwarding cases cover identity projection, explicit resource links, App link
projection, independent producer references, unknown metadata, malformed App links and
profile-scoped App dependencies. Domain contracts qualify their own serialization and
digest checks. Installed acceptance compares a published Frames world with its resource
readback through the public gateway before configuring a simulator.
