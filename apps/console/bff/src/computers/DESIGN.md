# Computers Console Transport

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Same-origin Computer control routes and bounded JSON forwarding |
| Existing Console session | Authenticated encrypted cookie, OAuth renewal and constant-time CSRF check for mutations |
| WebSocket, RFC 6455 | HTTP/1.1 upgrade, exact public Origin and cookie authentication |
| Veoveo terminal v2 | One-use first frame, binary terminal, bounded resize, replay fence and upstream authority deadline |

`/console/api/computers` and its exact Computer children are the native Console edge.
The configured Console profile and gateway URL select the upstream. Input cannot
override the profile, actor, owner or destination. The existing gateway and Computers
domain own current authorization. This module has no lifecycle state machine.

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
