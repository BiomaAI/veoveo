# Workspace Client

## Standards And Protocols

Workspace consumes the Veoveo same-origin HTTP JSON API at `/workspace/api` and
Server-Sent Events for chat changes. OAuth authorization code with PKCE runs through
the existing Rust browser edge. Generated JSON Schema and TypeScript come from
`mcp/contract/src/workspace.rs`. The browser never implements MCP Tasks transport;
the Rust platform owns that boundary. Embedded capability views retain MCP Apps
`2026-01-26` and Veoveo's declared App extensions.

React, TypeScript and Vite retain the repository's qualified frontend pins.
assistant-ui `0.15.20` is the first presentation candidate, verified against the
[upstream package registry](https://www.npmjs.com/package/@assistant-ui/react).
It must pass the multi-participant qualification before being accepted as the chat
renderer. Its runtime cannot own chat authorization, persistence or agent execution.

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

Canonical generated contracts are being introduced. The interactive client,
assistant-ui qualification, capability views and installed acceptance remain active
implementation work. No local fixture counts as production model execution.
