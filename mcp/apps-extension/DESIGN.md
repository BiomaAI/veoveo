# MCP Apps Contract Design

This document is the canonical contract for how domain MCP servers ship
user-facing views and administrative surfaces to Veoveo hosts (the Console
today, any MCP Apps host tomorrow). The crate `veoveo-mcp-apps-extension`
owns the pinned protocol constants and helpers; this document owns the rules.

## Status

Implemented in this workspace.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | App discovery and invocation remain ordinary MCP resource, tool, and task traffic. The hosting path uses JSON-RPC 2.0 over Streamable HTTP. |
| MCP Apps SEP-1865 / `ext-apps` | Version `2026-01-26`, with `ui://` resources, `text/html;profile=mcp-app`, tool-to-app metadata, host context, lifecycle notifications, and the `postMessage` bridge. |
| Veoveo reactive App resource adapter | Repository-owned extension over MCP Apps `2026-01-26`. A sandboxed App may request ordinary MCP `resources/subscribe` and `resources/unsubscribe`; the Console projects contentless `ui/notifications/resource-updated` wakes from the authenticated pooled MCP session. This adapter is not claimed as part of SEP-1865. |
| [Veoveo final task extension](../task-extension) | Version `2026-06-30`; app-started durable work retains the same task lifecycle and ownership rules as a normal MCP client. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Linked tool arguments and structured results use the same canonical schemas exposed outside the app. |
| HTML iframe sandbox and Content Security Policy | HTML runs in an opaque-origin `sandbox="allow-scripts"` frame. The default CSP denies remote network access while permitting local `data:` fetches; a live-data App may declare exact origins through `_meta.ui.csp`, which the host validates before adding them. Cookies, storage, and same-origin privilege remain absent. |

## The rule

A domain server's entire operational surface crosses exactly one protocol
boundary: MCP.

- **Reads are resources.** Domain state an operator or app needs is a
  `{slug}://…` resource (plus templates for addressable entities).
- **Mutations are tools.** Administrative writes are ordinary MCP tools,
  scope-gated inside the server and policy-gated at the gateway.
- **Views are app resources.** Interactive UI ships as a self-contained HTML
  document at `ui://{slug}/{page}` with MIME `text/html;profile=mcp-app`
  (SEP-1865 / ext-apps "2026-01-26").

Domain servers must not expose bespoke admin REST routers, and hosts must not
hardcode domain pages, domain nav entries, or domain proxy routes. If a
domain needs UI, it ships an app; if it needs new operations, it ships tools.
The Console's platform-plane views (overview, work, artifacts, agents,
recordings, MCP, apps, access, audit, cluster) are the only compiled-in
views: they render installation-generic state, never one server's domain
vocabulary.

## Server obligations

A server shipping a view (see `servers/timeseries-mcp` and
`servers/map-mcp` for reference implementations):

1. Declare the extension: `extend_capabilities(&mut caps)` in `get_info`.
2. List the view: `app_resource(uri, name)` in `list_resources`, with
   `.with_title(...)`, `.with_description(...)`, and optionally
   `.with_icons(...)` (data: URIs — hosts render nav/catalog entries from
   these fields, so they are the server-owned menu contribution).
3. Serve the view: `app_html_contents(uri, include_str!(...))` from
   `read_resource`. The document stays self-contained unless its function
   requires a declared live-data connection. Such a view uses
   `app_resource_with_meta` and lists exact installation-owned origins in
   `_meta.ui.csp`; it does not name wildcards, paths, credentials, queries, or
   fragments.
4. Link tools: `link_tool_to_app(tool, uri, &[Model, App])` in `list_tools`
   for every tool the view may invoke. Tools without an app link are never
   app-callable.
5. Gate in-server: admin tools call `require_scope` with the domain admin
   scope (e.g. `map:admin`) exactly like any other scoped tool; resources
  carry the scope their data warrants.

## Host obligations

The hosting core (gateway + console BFF + console web) stays fully generic:

- **Gateway** — the server's catalog entry lists its tools in the manifest,
  exposes them per profile, applies `tools_call` policy rules, and registers
  `resource_projection: server_owned` so `ui://…` URIs project as
  `ui://{mounted-slug}/{page}`.
- **Catalog** — the BFF discovers apps dynamically from `resources/list`
  (`is_app_resource`), derives ownership from the `ui://{server}/…` prefix,
  and attaches only that server's app-visible linked tools. There is no
  manual registration step anywhere.
- **Frame** — app HTML is served same-origin with `default-src 'none'` into an
  `<iframe sandbox="allow-scripts">`. The BFF validates every declared CSP
  origin, sorts and deduplicates the result, and adds only those exact sources
  to the relevant directive. Local `data:` fetches do not add a remote network
  origin. The opaque origin has no cookies, storage, or same-origin privilege.
- **Bridge** — the host declares `serverTools` and `serverResources`
  capabilities. `tools/call` from a view is proxied only to app-visible
  tools linked to that exact view on that view's server. Own-server resource
  reads remain implicit. A foreign `resources/read` must match one exact
  gateway-projected App dependency that names the owning App URI, target
  server, URI scheme and prefix, required scope, operation, and optional data
  labels. Gateway policy remains the authoritative second wall.
- **Tasks** — task-based tools stay task-based inside apps. A view may send
  `tools/call` with a `task` augmentation plus `tasks/get`, `tasks/result`,
  and `tasks/cancel`; the console host intercepts these ahead of the
  AppBridge (which rejects task traffic) and proxies them through the BFF,
  which records task ownership per app view and forwards spec task requests
  on its gateway session. The same allowlists apply: only app-visible linked
  tools may start tasks, and a view may only poll tasks it started
  (`servers/view-mcp/assets/preview-app.template.html` is the reference
  task-driving view).
- **Reactive resources** — the Console intercepts App `resources/subscribe`
  and `resources/unsubscribe` requests because MCP Apps `2026-01-26` does not
  include them. The BFF registers one UUID-bound reference on the pooled MCP
  session and returns a contentless authenticated SSE wake stream. Upstream
  `notifications/resources/updated` becomes
  `ui/notifications/resource-updated` inside the opaque frame. Reconnecting
  the SSE stream reuses its UUID, and multiple frames retain independent
  references to the one upstream subscription. Authorization expiry closes
  the stream. Domain state never travels in the wake; the App reads the
  current resource through the ordinary bridge.
- **Navigation** — the host's menu merges its static platform views with one
  entry per discovered app (label from the resource title, icon from the
  resource icons). Catalog failures degrade the menu to platform views only;
  they never block the shell.

## Governed Cross-Server Resources

Cross-server requirements are typed control-plane declarations, not
App-authored metadata. An extension may declare them in
`ServerManifest.app_resource_dependencies`; installation bindings still
decide whether the owner and target are exposed and which policy applies.
Each declaration binds one projected `ui://{owner}/...` App resource to one
registered target server and one non-root URI prefix under that server's
canonical scheme. The initial operation profile is `read`.

Control-plane validation rejects an unknown target server, a scheme not owned
by that server, an invalid App owner, a root or mismatched prefix, an unknown
data label, an empty operation set, and duplicate declarations. The gateway
filters valid declarations against the active profile, actor scopes, and actor
data labels before adding
`_meta["io.veoveo/app-resource-dependencies"]` to the listed App resource.
Dependencies are sorted for deterministic projection.

The Console trusts only that gateway projection. It re-lists the exact App
resource before each cross-server read and accepts the URI only when it matches
the projected scheme, prefix, and `read` operation. A browser-supplied server,
App URI, or resource URI cannot enlarge the declaration. The eventual
`resources/read` runs through the gateway under the same session and Work
Context, where profile exposure and resource policy authorize the exact
target.

## View obligations

The view side of the postMessage protocol (see
`servers/timeseries-mcp/assets/forecast-app.html` for the reference bridge):

- `ui/initialize` → apply `hostContext` (theme, display mode), then
  `ui/notifications/initialized`.
- Self-driving views load their own data through `resources/read` after
  initialize — a view must render meaningful content without waiting for a
  `tool-result` push.
- Report height via `ui/notifications/size-changed`; respond to
  `ui/resource-teardown`.
- Handle tool/resource failures inline (authorization errors surface as
  ordinary failed requests — degrade to read-only or show the error).
- A reactive view subscribes only to resources owned by its server, treats
  each update as a wake to read current state, and unsubscribes during
  teardown. It does not use a timer as a substitute for resource updates.

## Why not admin REST

The previous pattern (server-local admin REST + gateway
`/admin/{profile}/servers/{server}/{*path}` proxy + a hand-written console
page per domain) required four hardcoded integration points per domain:
console view, console nav entry, BFF proxy route, and server router. The
apps contract replaces all four with discovery. The generic gateway server
admin proxy remains for platform infrastructure, but domain servers must not
grow new REST surfaces behind it.
