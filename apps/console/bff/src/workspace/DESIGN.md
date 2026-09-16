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

Artifact result preview and download use the existing streaming handlers under
`/workspace/api/artifacts/{id}`. Every read requires the Workspace cookie and fixes
the Workspace profile; a receipt or chat membership supplies no read grant. Range
and conditional reads retain the governed download boundary. Preview documents
receive a sandbox CSP without scripts or same-origin authority, including direct
navigation to HTML or SVG content. No Artifact body is buffered in the edge.

The edge composes the shared resumable upload and Computers control routers beneath
the Workspace API root. These handlers select the Workspace profile and cookie
through their application state, including terminal ticket endpoints. Upload parts
retain their streaming body path outside the chat DTO size limit. Computer commands,
receipts, file transfers, named grants and terminal relays retain the existing domain
and transport invariants. CSRF and exact terminal Origin checks apply unchanged.
The dedicated stock CLI pairing page remains a Console surface until its client
bootstrap is adapted; this router does not add a Workspace CLI entry page.

## App Hosting

`apps.rs` composes the shared caller-scoped catalog, sandboxed frame, resource-read
and resource-notification handlers with Workspace state. Tool invocation and Task
get/update/cancel go through the gateway's durable Workspace operation journal;
Workspace never uses the Console's in-memory App Task registry. Closed App DTOs
include the exact origin URI. All mutations retain Workspace cookie authority and
CSRF enforcement. Native responses deserialize through RMCP types at the edge.
