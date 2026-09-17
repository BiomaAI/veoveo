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
- Catalog changes now invalidate the gateway's authoritative cache. In-flight
  discovery uses exact claims, preventing a late response from restoring an
  invalidated entry. Five-second cache expiry and a 30-second client reconciliation
  recover missed notifications without invoking models or querying providers.
- Ended native catalog subscriptions retire the browser-edge MCP client and end
  its event feed. Reconnection creates a fresh authorized client. The real RMCP
  fixtures qualify EOF, partial acknowledgements, changed contents and cache expiry.
- The catalog checkpoint passed 89 gateway library tests and 99 BFF tests. Gateway
  and BFF Clippy passed. Warm gateway tests took under two seconds; BFF compilation
  took eleven seconds and its tests took under one second. Clippy took about 26
  seconds per package. The recorder does not yet admit the BFF Clippy command for
  scoped reuse, which produces an unnecessarily broad receipt; narrow that
  descriptor in the build follow-up. Avoid concurrent Cargo qualification commands
  because package locks serialize them.

- Run feedback now records preparation, response and tool execution, including the
  count of admitted private operations. Shared chat carries no tool content.
  Native request progress binds the SDK-assigned token, coalesces four writes per
  second, and clears at settlement or continuation. A real HTTP fixture verifies
  measured progress before the receipt. Store tests cover stale fences and regression.
- The personal feed shares two projected database LIVE sources and coalesces client
  invalidation. Exact native Tasks retain current owner authority across replicas.
  Real-store tests cover invitations, disabled principals, private results and
  reconnect without tool replay. The client shows attention outside the open chat.
- Stable [RMCP 3.4.0](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.4.0)
  is published in the maintained fork at `8728fef3d3f18b5f95afb801d6f28064bda94b9a`.
  [Rig 0.42.0](https://github.com/0xPlaygrounds/rig/releases/tag/rig-v0.42.0)
  uses that dependency at `6a92dacd7802d9105f106344a8e85a8a19ec88b2`.
  Qualification retains exact Task filters and covers transport concurrency,
  cancellation, protocol headers, disconnect and 48 Rig RMCP tests.
- Public Task subscriptions authorize first, select exact baseline records and
  replay only selected aggregates in bounded pages. Runtime clones share a projected
  wake source. Two real-store listeners receive another replica's completion and
  reconnect to current state while excluding an unrelated malformed Task envelope.
- Time and Recording now share database resource invalidations across replicas.
  Broadcast overflow requests reconciliation instead of losing changes silently.
  Idle resource sources emit no periodic synthetic changes; this preserves event
  driven agent execution. Media already projects committed provider outbox events,
  and Artifact and Computers retain their domain-specific authorized durable feeds.
- The headed client fixture passed in 47.3 seconds using NVIDIA RTX 4090 WebGL.
  WebGPU exposed a software fallback, which was not used as hardware evidence.
  Measured and indeterminate progress, private Task inputs, cancellation, reload,
  reconnect and revoked access passed. Client build took 2.1 seconds after TypeScript.

## Remaining Performance Experiments

Installed acceptance found a cold-catalog race after the first rollout. Expired
entries started background discovery while returning an empty successful snapshot;
model preparation could consequently omit every configured tool. The gateway now
shares one 500 ms settlement budget across parallel discoveries. Required tool
surfaces must also be complete before Workspace submits a model request. The new
regressions cover expiry, a hung optional source, failed discovery, and required
versus unrelated server degradation.

Load tests with thousands of Tasks and many concurrent people remain separate from
functional acceptance. Measure database query work and event rates before changing
subscription caps or retention. Resource recovery currently uses source reconnection
and client reads; a retained per-domain resource cursor would support replay through
silent notification loss without emitting synthetic changes while idle.

Build observations: the SDK change invalidates many Rust dependents; sequential
Cargo qualification avoids lock contention. Native resource-family checks took 80
seconds, and the isolated new subscription tests took 2.6 seconds after compilation.
The broad gateway test command is not yet classified for scoped receipt reuse,
causing a 2,531-file fingerprint. Add reviewed descriptors for this command and BFF
Clippy in the build follow-up. Record image solve and GitOps timings after rollout.
