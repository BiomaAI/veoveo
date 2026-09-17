# Reactive Execution And Client Feedback

Status: approved for implementation on 2026-09-17. The delivery target is Veoveo on
`veoveo.bioma.ai`, using the Bioma installation configuration.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Discover, stateless Streamable HTTP, request-scoped subscriptions, independent catalog changes, and request progress |
| MCP Tasks, SEP-2663 | Authorized Task-ID subscriptions, authoritative `tasks/get`, input updates, cancellation and retained results |
| MCP multi-round requests, SEP-2322 | Protected continuation state and explicit input responses |
| Server-sent events | Authorized browser invalidations and typed execution presentation; independent of MCP protocol sessions |
| Veoveo Workspace contracts | Rust-owned DTOs, generated client types, actor-private operations and owner-controlled shared chat |
| SurrealDB LIVE and durable outbox | Committed changes wake readers; bounded authoritative reads recover lost delivery |
| RMCP | Upgrade the maintained `3.1.4` fork to stable `3.4.0`, retaining Task subscription support and qualified shutdown behavior |
| Rig | Retain the qualified `0.42.0` fork and update its exact RMCP dependency; unreleased upstream architecture changes are outside this delivery |

## Outcome

People see when an agent starts, calls a capability, waits for work or input, and
finishes. A new operation appears without waiting for an inventory polling interval.
Invitations and private Task attention remain visible outside the originating chat.
Catalog changes converge after source changes, stream loss and replica changes.

Feedback comes from execution facts. Numeric progress requires a real measurement;
unknown progress remains indeterminate. Private tool arguments, results and model
reasoning never become shared-chat status. UI observation does not invoke a model.
Provider completion remains webhook-only. Task reconciliation reads Veoveo's native
Task state and cannot redispatch an uncertain operation.

## Delivery Sequence

1. Repair gateway catalog invalidation and App catalog listener recovery. Qualify
   changed catalog contents, concurrent discovery and source interruption.
2. Project typed execution phases and request progress through the existing run and
   operation boundaries. Preserve admission, cancellation and private-result rules.
3. Add a bounded personal event feed for operation creation, Task attention and
   invitations. Current authority governs every read and event wake. Reconnect and
   visibility recovery perform bounded reconciliation without resubmission.
4. Merge stable RMCP `3.4.0` into the maintained SDK fork, update the Rig fork's pin,
   qualify affected transport behavior and pin both exact revisions in Veoveo.
5. Bound Task subscription baselines to requested identities and improve event-source
   sharing. Repair resource publishers that cannot deliver across ordinary replicas.
6. Record focused test evidence, publish affected images, deploy through Bioma GitOps,
   and verify the client in a headed hardware-backed browser. Record phase timings
   and remaining nonblocking performance experiments.

Each completed concern lands as a coherent commit. Owning component designs describe
the implemented contracts. This plan records delivery status and unresolved work.

## Acceptance

- An upstream catalog mutation changes subsequent gateway and App catalog contents.
  Interruption recovers without waiting for access-token replacement. Concurrent
  discovery cannot reinstall a snapshot invalidated while its request was running.
- Tool execution presents truthful progress before the final agent text. Operation
  results and input requests retain their existing actor-private authority.
- Two authorized browser sessions receive operation and invitation changes. Revoked
  or unrelated people receive neither content nor continued observation authority.
- Task notification loss, reconnect and replica change converge on current state;
  none replays a tool call. Idle observation produces no model request.
- RMCP concurrency and cancellation tests pass with Veoveo's final protocol profile;
  exact Task-ID subscriptions survive the upgrade.
- Subscription work is bounded by admitted filters and shared retained state.
- Relevant recorded tests pass, GitHub presents current evidence, and the installed
  Workspace and Console pass focused headed hardware-browser verification.

## Investigation Baseline

The September 17 source audit found that gateway subscription notifications bypass
the callback that invalidates discovery caches. App catalog listeners can terminate
without retiring their cached client. Workspace consumes text and final Rig stream
items while ignoring other execution events. Activity inventory refetches every ten
seconds, and invitations refresh on navigation or focus. Native Task subscriptions
materialize a server-wide baseline before filtering requested IDs. Several domain
resource publishers use process-local hubs; their durable Task channel does not by
itself make resource notifications replica-safe.

Upstream RMCP `3.4.0` still omits the fork's Task-ID subscription filter. Its transport
concurrency, cancellation and HTTP validation fixes justify the upgrade while retaining
that patch. Rig `0.42.0` remains the latest stable release verified on September 17.

## Delivery Record

- Goal created; main synchronized at `634c9cf0`. The working tree was clean.
- Initial disk check: 279 GiB available on the workspace filesystem. No cleanup or
  unrelated workload restart was required.
