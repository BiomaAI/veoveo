# Console Bootstrap Edge

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Same-origin `GET /console/api/session`, closed JSON, `no-store` |
| Console session cookie | Existing encrypted cookie and silent OAuth rotation; current CSRF material returned with every settled session |
| Veoveo Console bootstrap | Generated Rust DTO, fixed configured profile and gateway destination |

The BFF rejects queries and nonempty request bodies before refreshing a session.
It forwards the cookie's access token and configured public Host to the gateway's
`/console-api/{profile}/session`. Caller Authorization, Cookie and destination input
are excluded. Redirects fail, response bodies are capped at 256 KiB, and forwarding
plus body admission completes within ten seconds after session renewal.

The decoded response must name the configured profile. A completed refresh returns
its new cookie and CSRF material even when the following request fails. An upstream
401 follows the existing session-clear behavior. Upstream cookies never reach the
browser. Local tests exercise the real BFF router against an HTTP fixture, including
ordinary-user bootstrap, anonymous rejection, forged bearer exclusion and rotation.
