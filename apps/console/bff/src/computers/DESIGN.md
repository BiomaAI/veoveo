# Computers Console Transport

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Same-origin Computer control routes and bounded JSON forwarding |
| Existing Console session | Authenticated encrypted cookie, OAuth renewal and constant-time CSRF check for mutations |
| WebSocket, RFC 6455 | HTTP/1.1 upgrade, exact public Origin and cookie authentication |
| Veoveo terminal v2 | One-use first frame, binary terminal, bounded resize, replay fence and upstream authority deadline |
| OpenShell CLI `0.0.116` and gRPC over WebSocket | Custom SSO pairing and binary SSH adapter; private lease controls never reach the stock consumer |
| MCP `2026-07-28` and server-sent events | Auth-scoped collection subscription projected as typed invalidations over a CSRF-protected HTTP POST |

`/console/api/computers` and its exact Computer children are the native Console edge.
The configured Console profile and gateway URL select the upstream. Input cannot
override the profile, actor, owner or destination. The existing gateway and Computers
domain own current authorization. This module has no lifecycle state machine.

GET `/console/api/computers/{id}/operations/{operation_id}` forwards a stored receipt
read under the current cookie session and configured profile. It accepts no query
parameters and does not retry a lifecycle mutation.

GET `/access` and POST `/access/{grant_id}/revoke` below an exact Computer forward its
grant inventory and revocation. The POST retains CSRF enforcement even though the
domain requires only current owner/read authority for this reduction of access.
Neither route accepts query parameters, destination input or provider credentials.

Mutations traverse the existing CSRF middleware. Ticket requests additionally require
one exact public Origin. Control bodies must complete within five seconds and fit
64 KiB before a session refresh can occur. Responses fit 2 MiB. Typed pagination is
allowed only for collection reads; other query strings are rejected. The gateway
validates canonical request and response DTOs. The BFF decodes the ticket again to
bind its Computer and replace its endpoint with the BFF's uncredentialed path.

HTTP forwarding sends the cookie's access token and configured gateway Host. Browser
Authorization, Cookie and arbitrary headers are not forwarded. The gateway issues
the internal assertion. Only Content-Type and the checked ticket Origin accompany
control input. Redirects fail. Responses use `no-store`, and upstream Set-Cookie is
never passed to the browser.

The WebSocket upgrade requires a valid current Console session and exactly one matching
Origin. It accepts no query parameters. Renewal can rotate the encrypted cookie during
the upgrade. The HTTP 101 returns that cookie and the current CSRF token; a later
upstream failure also returns a successfully rotated cookie. Ordinary cookie renewal
keeps the browser grant's session family intact.

The shared transport client receives the Console's existing installation TLS trust,
then selects HTTP/1.1 and disables redirects. Each replica admits at most 128 local
terminal relays. Service-issued deadlines enforce authority through the bounded shared
relay. SIGTERM or Ctrl-C cancels terminal relays before graceful HTTP shutdown.
Disconnecting a relay does not call Stop. Retained Computer execution belongs to the
service and provider.

Behavior tests cover cookie/CSRF admission, fixed profile and headers, origin/query
rejection, body limits, cookie rotation on failure, redirects, WebSocket first-frame
delivery and cancellation. These local tests do not establish installed public
authentication, current domain policy or headed terminal presentation.

`POST /console/api/computers/events` accepts a closed empty body capped at 1 KiB.
It uses the existing authenticated MCP pool and requires the exact collection filter
to be acknowledged. Subscription admission precedes the initial baseline invalidation.
The browser rereads canonical HTTP state after each invalidation. A lagged observer
also invalidates the snapshot. This endpoint performs no provider status polling.

The outgoing queue holds one event. A separate task enforces source loss, source-token
expiry and shutdown even when the browser stops reading. Body drop cancels that task;
all exits release the downstream subscription with a five-second cleanup bound.
Heartbeat comments keep intermediaries active without granting authority or extending
the deadline. Successful session renewal is returned in headers even when subsequent
subscription admission fails.

## Stock CLI Pairing And Relay

The stock CLI `0.0.116` adapter registers
`/console/computers/{id}` as its gateway endpoint. GET
`/console/computers/{id}/auth/connect?callback_port={port}&code={code}` serves the
Console entry document for the dedicated pairing page. Closed query parsing and
the shared code/port validator run before reading that document. This response
alone adds the exact `http://127.0.0.1:{port}` connect source to its restrictive CSP.
It sends no-referrer and no-store. Other Console pages gain no loopback destination.
Existing Console SSO preserves this same-origin return path.

POST `/console/api/computers/{id}/cli-pairings` and its
`/{pairing_id}/confirm` child require CSRF and exact public Origin. They forward
closed requests under the configured profile and current cookie session. A lost
confirmation response cannot be retried to obtain its secret. The browser explicitly
compares the terminal code and sends the one-use response directly to the local CLI.

GET `/console/computers/{id}/_ws_tunnel` and exact GET `/_ws_tunnel` implement the
stock client's two route shapes. They use no Console cookie session or OAuth refresh.
The shared framing validator accepts the stock edge credential and consistent
redundant headers, then forwards one sensitive Bearer header to the configured
gateway profile. Browser Origin and unrelated cookies are rejected. The worker
independently verifies the grant's retained profile, owner, family, Computer and run.
The public relay consumes private Ready/Lease controls and delivers only binary
gRPC bytes. Existing cancellation, buffer and independent deadline bounds apply.

GET `/maintenance` and `/maintenance/{operation_id}` and POST `/update-template` beneath
an exact Computer forward to the corresponding fixed gateway routes. Updates retain
cookie authentication, CSRF and bounded body admission. They cannot select a profile or
destination. A transport retry belongs to the saved browser request and preserves its
request ID; the BFF never automatically repeats a mutation.

POST `/maintenance/{operation_id}/resume` uses the same cookie/CSRF boundary and fixed
Computer/Task route. The BFF forwards the saved recovery request without interpreting
its cancellation consent or creating a new request identity.

The fixed `/files` child admits metadata-only transfers. Its Task child supports status
reads and an empty-body `/cancel` POST. Both mutations retain the existing cookie and
CSRF boundary. The proxy accepts no file bytes, arbitrary task paths or destination
URLs. Public Artifact uploads and downloads continue through their existing transport.
