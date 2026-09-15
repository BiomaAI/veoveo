# Workspace Browser Edge

## Standards And Protocols

The browser edge exposes `/workspace/api` as same-origin HTTP JSON. It uses the
existing OAuth authorization-code/PKCE flow, encrypted session cookie and CSRF
header. DTOs come from `mcp/contract/src/workspace.rs`; the browser never receives
an access token. Workspace is a separate application using the existing edge
deployment.

## Admission And Forwarding

Routes fix the gateway profile and upstream origin through installation config.
UUID path parameters and closed request DTOs determine the forwarded operation.
Caller Authorization, Cookie, Host and destination headers are never forwarded.
People-search input is length bounded and URL-encoded. Mutations pass the shared
CSRF middleware before execution.

Responses are decoded to the expected Rust DTO within a byte and time bound.
Redirects fail closed. Session rotation is settled even if the subsequent operation
fails; upstream 401 clears the invalid session. Responses are `no-store`.
OAuth return-path validation admits `/workspace/` under the same origin.

Client assets, shared-chat streaming and the installed release remain delivery
work. This edge does not itself execute agents or confer capability authority.
