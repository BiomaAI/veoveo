# Workspace Browser Edge

## Standards And Protocols

The browser edge exposes `/workspace/api` as same-origin HTTP JSON. It uses the
shared OAuth authorization-code/PKCE implementation with a dedicated Workspace
client/profile, encrypted session cookie and CSRF header. DTOs come from `mcp/contract/src/workspace.rs`; the browser never receives
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
OAuth routes live under `/workspace/auth/`. Return-path validation admits only
`/workspace/` paths for this application. Workspace never uses the Console cookie.
The shared browser authentication boundary is specified in [`../../DESIGN.md`](../../DESIGN.md).

The edge streams contentless chat notifications with bounded chunks and preserves
session rotation headers. The gateway owns ongoing session and membership checks.
The static entry is `no-store`; content-addressed assets use immutable caching and
missing bundles return 404. Client assets have their own Docker stage outside Rust
compiler inputs. The installed release remains delivery work. This edge does not
itself execute agents or confer capability authority.

Operation routes forward closed Task projections, input answers and cancellation
to the fixed Workspace gateway profile. Input DTOs exclude `requestState`, Task
replacement IDs and authority fields. The same session/CSRF boundary applies to
these mutations. A bounded comma-separated list of operation UUIDs selects the
contentless native Task wake stream; the gateway resolves and authorizes Task IDs.
