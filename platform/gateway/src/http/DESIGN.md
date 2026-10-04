# Gateway HTTP Composition

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Axum request admission and streaming response frames, including trailers; interrupted module streams report a body error |
| OAuth bearer, RFC 6750 | Registered profile JWT authentication, current invocation authority and session/revocation checks |
| MCP 2026-07-28 | Native authenticated Streamable HTTP discovery; caller-owned client capabilities and progress observations |
| `http-body` 1.1.0 | The existing Axum frame interface, used directly to preserve data and trailers without buffering |
| Veoveo module names | Checked `ModuleName` keys for deferred factories and authenticated binding reports |

## Context And Admission

`GatewayHttpContext` supplies the typed public deployment, reloadable catalog and
identity-provider HTTP client, current gateway authority, internal assertion issuer
and upstream pool. Owners declare profile-authenticated routers separately from
routers that authenticate their own credentials. The Computers CLI and Recording
producer routes retain their owner credential policies.

Profile capture uses the registered named `{profile}` segment in `MatchedPath`
and its raw spelling in `OriginalUri`. Encoded aliases fail admission before bearer
verification. Nested routers carry their full registered path. Core does not list
owner route prefixes. Catalog reload changes subsequent authentication and policy
checks through the shared handles.

Authentication records credential denials before dispatch. Successful bearer
verification contributes to the owner's action record. The middleware preserves
current principal-directory synchronization and returns an error when its required
state or audit writer is unavailable.

## Module Lifetimes

Registration stores factory declarations without running them. Duplicate bindings
and missing required bindings fail before any factory begins. Optional unbound
entries appear separately from MCP backend health. Startup logs and the authenticated
server-health response report each declared binding's module, state and requirement.

A `ModuleTaskScope` serializes admission with tracker reservation. Closing admission
prevents subsequent requests and workers from registering. An upgrade reserves its
permit before returning HTTP 101, including the interval before its callback starts.
Owners pass the scope to background work, dispatch and stream producers. Request
handlers remain owned if their HTTP caller disappears; transport cancellation does
not assert that a durable operation failed.

The coordinator retains `ModuleCleanupSupervisor` before building routes. Cleanup
slots are reserved before factories start. Factory errors, conflicting routes and
dropped build callers close admission and cancel every created scope. A cleanup
group has one 30-second deadline. Completion records survive cancelled awaiters,
and later observers see unresolved cleanup rather than a new deadline. The supervisor
requires the process's Tokio runtime to remain alive through cleanup.

`BuiltGatewayModules::run` supervises the caller's post-build setup and serving
future. Setup errors and panics await cleanup before returning; a failed cleanup
preserves the original error. Composition drains module work before closing audit
delivery, using the same deadline observed by the cleanup supervisor.

Shutdown closes admission before cancellation. Streaming bodies stop with an
interruption error, release their transport and retain trailers on ordinary
completion. Workers cooperate through their scope cancellation token. A timeout
reports unresolved work; it cannot settle a provider or authorize repeating a mutation.

## Native MCP

The native transport connects to the local listener with the configured public Host,
uses URL component construction and rejects redirects. Its connect and HTTP budgets
are five and eighty seconds; discovery has eight seconds and close has two. The
caller supplies `ClientConfig` and an optional progress observer.

Required-tool readiness subscribes before listing, checks the acknowledged filter,
and waits at most eight seconds for list-change notifications. Pagination permits
sixteen pages and 512 tools. Partial catalog failures reject a required server's
selection. An empty required selection requests the complete authoring picker.

Composition runs post-build setup and serving through `BuiltGatewayModules::run`. The coordinator catches route-merge panics and awaits module cancellation and cleanup before returning a listener, setup, or serving error. Cleanup failure is reported alongside the original error.
