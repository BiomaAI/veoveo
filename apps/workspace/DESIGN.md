# Workspace Client

## Standards And Protocols

Workspace consumes the Veoveo same-origin HTTP JSON API at `/workspace/api` and
Server-Sent Events for chat changes. OAuth authorization code with PKCE runs through
the existing Rust browser edge at `/workspace/auth/`, using Workspace's own
OAuth client and encrypted cookie. Generated JSON Schema and TypeScript come from
`mcp/contract/src/workspace.rs`. The browser never implements MCP Tasks transport;
the Rust platform owns that boundary. Embedded capability views retain MCP Apps
`2026-01-26` and Veoveo's declared App extensions.

React, TypeScript and Vite retain the repository's qualified frontend pins.
assistant-ui `0.15.20` is the first presentation candidate, verified against the
[upstream package registry](https://www.npmjs.com/package/@assistant-ui/react).
It must pass the multi-participant qualification before being accepted as the chat
renderer. Its runtime cannot own chat authorization, persistence or agent execution.
The adapter qualification passes with two human and two agent authors, concurrent
agent statuses, subsequent human messages and replay. Playwright `1.63.0`, verified
from the upstream registry, runs the client-owned browser acceptance fixture.

## Product And State

The application serves `/workspace/` independently of the administrative Console.
Its source and asset bundle are separate; both applications share the existing Rust
browser-edge deployment. The complete accepted product and delivery gates are in
[`WORKSPACE_PLAN.md`](../../docs/WORKSPACE_PLAN.md).

Chat IDs select authoritative server state. Messages retain their author and stable
ID, and replay follows committed sequence order. The client may retain unsent text
in memory while a request is pending. It must not persist message bodies, tokens or
capabilities to local storage implicitly. An ambiguous send retains its request ID
for reconciliation; it never manufactures a fresh ID and repeats execution.

The assistant-ui adapter preserves human and agent metadata with
`joinStrategy: "none"`. Runs and their controls belong to individual messages;
streaming must not disable the whole room's composer. Veoveo owns participation,
invitation, attachment access and Task presentation. The qualification fixture has
two humans and two agents, concurrent outputs and a browser reconnect.

Computers, Apps, uploads and activity use existing governed platform operations.
Heavy viewers load on demand. Reuse of a Console component requires an audit of its
authentication and inventory assumptions; Workspace does not grant administrative
access merely to render a reused view.

## Delivery Status

The interactive client implements human chats, latest-first history loading,
invitations, participant controls, ownership transfer and archiving. It renders
plain text without executing HTML. A contentless SSE wake requests incremental
authorized state. A failed ambiguous send retains its original ID and text for
retry; refreshing restores committed messages without repeating a mutation.

The gateway and browser edge serve the live stream and assets. A local headed
RTX 4090 WebGL browser fixture exercises two humans, an interrupted response,
idempotent retry, owner controls, reload and the mobile layout. Its screenshots
are local fixture evidence. Agent admission and explicit recipient selection now
render separate response streams and stop controls; a two-human/two-agent fixture
keeps the composer usable and preserves run identities after reload. The Rust
runner has a separate HTTP-model fixture. Task activity now has private in-chat and
personal views, current status, input forms, cancellation acknowledgement, result
text and canonical resource references. Native subscriptions wake authorized reads;
visible active tasks reconcile at no less than five seconds or the server's longer
poll interval. Completed tasks stop that interval. Unknown progress is indeterminate.
Reload restores receipts and Task references without submitting work. The gateway
has native protocol/runtime acceptance; capability views and installed acceptance
remain required implementation work.

## Development And Acceptance

Run `npm ci` and `npm run dev` in this directory. Vite uses port 4174 and proxies
the browser edge on 8786. The edge accepts `VEOVEO_WORKSPACE_ASSET_DIR` for its
built entry directory; the production image uses `/app/workspace`. `npm run build`
builds only the client. Docker assembles Console and Workspace assets in separate
frontend stages while reusing the compiled Rust browser edge.

`npm test` runs generated-contract and assistant-ui adapter tests. `npm run
test:browser` owns a temporary Vite listener, isolated browser context and HTTP
fixtures. It attaches to headed Chrome at `VEOVEO_BROWSER_CDP` (default localhost
9222), probes both graphics APIs, and requires hardware before interacting or
capturing. Its context and listener close on success or failure. No local fixture
counts as production model execution.
