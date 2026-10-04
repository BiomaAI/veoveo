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
reconnection and worker-loss recovery. The
[Workspace design](../../../../apps/workspace/DESIGN.md#qualification-limits)
tracks distinct-person and production Task-input qualification.

## Module Construction And Shutdown

Workspace's gateway feature owns these handlers and their tests. Its deferred
factory creates PersonalHub and chat change observation only after the shared
module builder validates registrations. The provided module scope supplies their
cancellation token. New operations and runs reserve tracked work before committing
admission; descendants also enter that scope. Shutdown closes new admission,
cancels observation and waits for owned work within the gateway's single 30-second
cleanup deadline. Unconfirmed provider outcomes keep the existing persisted fences.

Workspace consumes Agent publication facts through the Agents owner library and
builds its own revision views and chat admission records. Agents do not depend on
Workspace factories or PersonalHub. Both owners reuse the gateway's native MCP
transport and readiness implementation. Workspace supplies its client capabilities
and progress observer without adding Workspace vocabulary to that transport.

The MCP contract still owns the public Workspace HTTP DTOs. This package owns
admission and presentation of those shapes; later contract transfer must preserve
current browser wire forms and schema admission.
