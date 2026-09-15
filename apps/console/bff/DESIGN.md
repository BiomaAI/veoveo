# First-Party Browser Edge

## Standards And Protocols

The edge implements OAuth 2.0 authorization code with PKCE S256, refresh tokens,
HTTP cookies and same-origin CSRF headers. Encrypted session envelopes use
XChaCha20-Poly1305. The gateway owns identity-provider federation and token
authority. This component owns the browser session and fixed upstream routing.
Its JSON APIs are Veoveo application contracts. MCP clients use the repository's
qualified MCP `2026-07-28` profile; they do not expose tokens to JavaScript.

## Application Authority

Console and Workspace share one edge deployment. `BrowserApp` selects an explicit
OAuth client/resource, cookie namespace, callback and permitted return-path set.
Workspace's installation profile requires `operator:use`; its client cannot request
`admin:manage` and has no client-credentials grant. Console retains its configured
administrative profile. Identity-provider SSO may serve both authorizations.

| Application | Authorization routes | Session cookie | Return paths |
|---|---|---|---|
| Console | `/auth/*` | `veoveo_console` | `/console/`, `/apps/` |
| Workspace | `/workspace/auth/*` | `veoveo_workspace` | `/workspace/` |

Each application's pending-authorization cookie has the same separation. Session
encryption authenticates the application as additional data, so copying a Console
envelope into the Workspace cookie cannot authenticate it. Logout and failed
refresh clear only the application's cookies. Mutating routes check the CSRF token
from that application's session before forwarding. HttpOnly cookies retain the
existing Secure/SameSite settings and refresh semantics.

Both first-party clients execute under the same origin. Cookie names and encryption
domains prevent credential substitution; they do not isolate a same-origin script
compromise. Untrusted MCP Apps retain the separate opaque-origin sandbox specified
by their host contract. Chat bodies render as text without executable HTML.

## Routing And Configuration

The typed configuration validates absolute HTTP(S) URLs without embedded credentials,
queries or fragments, and requires the transport profile to match the OAuth resource.
Workspace configuration lives under `consoleBff.workspace` in Helm and maps to
`VEOVEO_WORKSPACE_OAUTH_CLIENT_ID`, `VEOVEO_WORKSPACE_OAUTH_RESOURCE`,
`VEOVEO_WORKSPACE_MCP_TRANSPORT_URL` and `VEOVEO_WORKSPACE_OAUTH_SCOPES`.
The default Workspace scopes are `operator:use artifact:upload`. Registration's
allowed scope set describes available capability consent, not an automatic grant.

Application routers receive separate state before CSRF middleware is attached.
Workspace has no Kubernetes inventory client. Shared HTTP and MCP transports remain
credential-scoped; a browser cannot supply a destination or choose the other's
profile. Responses settle refreshed cookies even when the requested operation fails.

The Workspace API and asset contracts are owned by
[`src/workspace/DESIGN.md`](src/workspace/DESIGN.md). Console bootstrap, Computers,
uploads and App hosting keep their existing component contracts. Static bundles have
independent frontend build stages and do not become Rust compiler inputs.

## Qualification

Unit tests cover cross-application envelope rejection and return-path constraints.
Browser-edge HTTP tests cover Workspace PKCE metadata, callback, scopes and cookie
names, fixed destinations, CSRF and expired-session settlement. The Bioma acceptance
crate validates the non-admin client, native Tasks surface and resource registration.
Helm smoke validates the rendered installation and complete configuration digest.
These tests do not establish deployed sign-in; public browser acceptance remains
part of the Workspace delivery gate.
