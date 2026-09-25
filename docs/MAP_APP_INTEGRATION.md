# Map MCP Integration

Map MCP is the authority for Veoveo geography, routes, layers, publications,
compositions, and spatial products. Other MCP servers may use that capability
without owning a second copy of Map data.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Authorized tools, resource reads and request-scoped subscriptions under the [server contract](../mcp/contract/DESIGN.md) |
| MCP Apps `2026-01-26` | Server-owned `ui://` resources and the sandboxed host bridge |
| Veoveo App resource dependencies | Repository-owned declarations for exact source server, URI prefix, scope and read operations; defined by the [Apps contract](../mcp/apps-extension/DESIGN.md) |
| `map://` resources | Map-owned geographic identity, provenance and revision semantics; individual data formats are specified by the [Map design](../servers/map-mcp/DESIGN.md) |

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

The server-owned App resource declares the dependency in the server's entry in the
gateway control plane. The installation still decides exposure and policy:

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

## Upload, Download And Live Changes

Upload a file through the artifact upload service, then pass its authorized artifact
identity to `import_feature_layer`. Map accepts GeoJSON FeatureCollections, RFC 8142
sequences and the supported GeoPackage vector profile. GeoPackage users first call
`inspect_geopackage` and choose the table and column mapping explicitly. Import runs
as an MCP Task and commits at most 10,000 features in one transaction.

Small edits use Map's layer and feature tools. Open views subscribe to mutable
layer, publication and composition indexes, then query the current viewport after
an update. Notifications signal a change; they do not contain geometry or provide
a replayable feature stream. Map Explorer coalesces notifications and refreshes only
affected metadata. It preserves the previous view when a read fails.

To download data, publish a layer and call `export_feature_layer`. Its task produces
an immutable GeoJSON sequence, GeoParquet 1.0 or GeoPackage artifact. Transfer the
artifact through the authorized artifact download service. Large files belong on
that byte-transfer path; MCP calls carry identities, task progress and results.

The remaining client and streaming opportunities are:

- Map Explorer's import drawer takes an artifact ID. A file picker connected to the
  existing upload queue would remove that manual handoff; an export-and-download
  action could use the same artifact service.
- Source registration and acquisition-status indexes need authorized update
  notifications before clients can subscribe to them. Existing layer subscriptions
  do not imply coverage of these indexes.
- Imports beyond the transaction limit need checkpointed batches with cancellation,
  recovery and progress. The present import task is a bounded batch operation.
- Dense, continuously changing maps need revision-aware feature deltas or a qualified
  tile data path. Current viewport queries are paginated and capped at 5,000 features
  per layer; notifications trigger queries rather than streaming geometry.
