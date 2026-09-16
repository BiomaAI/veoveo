# Workspace Application API

## Standards And Protocols

`/workspace-api/{profile}` is a Veoveo HTTP JSON application contract. It uses the
gateway's current OAuth access-token, session-family and Work Context admission.
Rust DTOs are defined in `mcp/contract/src/workspace.rs`. Change notifications use
Server-Sent Events with committed chat sequence cursors. The endpoint is not MCP;
capability execution continues through the platform's existing MCP profile.

## Authority

Handlers admit direct human sessions. The authenticated actor determines the
author; requests cannot supply an author or arbitrary tenant. Current stored Work
Context rules determine membership using authenticated principal, group, role and
OAuth-client selectors. The store binds that decision to its context digest and
checks chat membership in every operation. Installation administration is not
required to participate.

The browser edge fixes the profile and upstream origin, retains tokens in its
encrypted cookie, and enforces CSRF on mutations. JSON responses are bounded and
marked `no-store`. Stream lifetimes must be bounded by session/token validity and
recheck authority before each wake. A wake carries the committed sequence only;
clients then fetch their authorized state.

People search is tenant-scoped and bounded. An invitation is addressed to an
installation-local person ID. Acceptance requires that person's current Work
Context authority; an invitation cannot create it. No global principal inventory,
policy catalog, credential or arbitrary proxy destination is exposed.

## Change Stream And History

One process-wide database LIVE subscription carries chat IDs to bounded local
subscribers. Each stream reads its authorized committed head before emission.
Database reconciliation runs every 15 seconds, including after LIVE loss; it does
not query any provider. JWT revocation, session-family authority, current Work
Context rules and chat membership are rechecked before a wake. Changed gateway
configuration closes the stream for fresh HTTP admission. Token expiry and a
five-minute maximum bound its lifetime. Admission permits four streams per person
and 128 per process. Lag causes durable reconciliation, never mutation replay.

The authorized head read also settles expired response-worker leases through the
store's shared recovery query. This commits one interrupted-run event and wakes
connected clients even when the worker can no longer publish and nobody opens
Activity. Recovery reads bounded run metadata; it neither loads response bodies
into the watch nor contacts a model provider. Concurrent watchers cannot duplicate
the terminal event or reopen the execution fence.

Initial history reads return the latest 100 messages in ascending display order.
`before` loads older history; `after` reads incremental messages. Supplying both
is invalid. Clients advance catch-up through the last actual message until the
page is exhausted. Invitation inbox summaries disclose only the named invitee's
chat title, inviter name and invitation metadata before acceptance.

Domain and browser-edge stream tests pass. Installed acceptance also qualifies
Workspace authentication, real concurrent agents, independent cancellation, stream
reconnection and worker-loss recovery. The current release evidence and remaining
second-person and native Task-input gates are tracked in
[`WORKSPACE_PLAN.md`](../../../../../../docs/WORKSPACE_PLAN.md).
