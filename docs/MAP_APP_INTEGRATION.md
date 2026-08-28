# Map MCP Integration

Map MCP is the authority for Veoveo geography, routes, layers, publications,
compositions, and spatial products. Other MCP servers may use that capability
without owning a second copy of Map data.

## Choose the integration surface

Use Map MCP tools and resources when the server needs geographic facts or a
domain operation. Reads use canonical `map://` resources. Writes and routing
actions use the typed Map tools and their declared scopes.

Open `ui://map/workspace.html` when the user needs the standard Map experience.
The host discovers and authorizes this App; consumers do not hardcode a Console
route or private Map endpoint.

Ship a separate App when the product needs a domain-specific workspace. Declare
the exact Map resource dependency in the server manifest, then read those
resources through the App bridge. A logistics App might depend on
`map://feature-layer/.../features` and `map://composition/...`; a UAV App might
depend on a published route and a bounded feature layer.

## Rules for consumers

- Keep Map data authority in Map MCP.
- Request the narrowest non-root URI prefix and required scope.
- Preserve Map resource identities, attribution, provenance, and valid-time or
  revision semantics in the rendered view.
- Treat resource-update notifications as wakes, then reread current state.
- Degrade to read-only or show an authorization error when the dependency is not
  admitted.
- Use linked Map tools for mutations; an App resource dependency is read-only.
- Do not access Map storage, private HTTP routes, renderer internals, arbitrary
  URLs, or credentials.

## Dependency shape

The server-owned App resource declares the dependency in the gateway fragment or
manifest. The installation still decides exposure and policy:

```json
{
  "app_resource": "ui://logistics/dispatch.html",
  "server": "map",
  "scheme": "map",
  "uri_prefix": "map://feature-layer/",
  "required_scope": "map:feature:read",
  "operations": ["read"],
  "data_labels": ["operations"]
}
```

The gateway validates the declaration and projects only dependencies admitted
by the active profile, caller scopes, and labels. The App must use that
projection as its allowlist; browser input cannot enlarge it.

## Rendering guidance

The reusable Map workspace is the canonical shared Map experience. A custom App
may render Map resources in its own layout, but it should reuse Map's governed
composition and publication identities whenever possible. It should not
reimplement Map release selection, coordinate authority, attribution, or
provenance rules. If the custom App only needs the standard map, navigate to the
Map App instead of creating a second renderer.

The App remains a normal MCP App: it uses `ui://` discovery, the sandboxed host
bridge, scoped resource reads, and notification-driven refresh. See
[`mcp/apps-extension/DESIGN.md`](../mcp/apps-extension/DESIGN.md) for the full
host and dependency contract and [`servers/map-mcp/DESIGN.md`](../servers/map-mcp/DESIGN.md)
for Map's resource catalog.
