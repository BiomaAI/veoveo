# Database Change Observation

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Observation declarations | Typed Rust declarations in `veoveo-modules`; owner libraries supply closed table vocabularies |
| Database transport | SurrealDB Rust SDK 3.3.0 over WebSocket; native LIVE notifications and database changefeeds |
| Replay cursor | Qualified single-node SurrealDB 3.3.0 versionstamp layout, `unix_millis << 16 | logical`; internal storage contract |
| Delivery | Internal invalidation and record streams; public resource reads apply the owning service's current SQL authorization |

## Owner Declarations

`ObservationTable` contains a checked `TableName` and an explicit replay capability.
`ObservationReplay::LiveOnly` permits invalidation without promising replay.
`Changefeed` carries a positive checked retention duration. These declarations
describe the owner's requirements; they do not install a table or prove database
readiness. Native schema checks must establish that the configured feed retains at
least the declared duration.

Optional owners export closed observation enums through their schema features.
Conversions into the shared descriptor keep table spelling with its owner. Store's
`PlatformTable` enumerates kernel tables only. Mixed subscriptions convert their
owner values into `ObservationTable` at the shared observation call. An independent
owner can add a table without editing Store or MCP infrastructure.

The declaration layer has no database or asynchronous-runtime dependency. Owner
enums reuse the foundational `Vocabulary` derive and its Serde and schema support.
Their public contract features remain separate from schema and runtime features.

## Record Delivery And Checkpoints

`observe_changes` rejects LIVE-only sources before opening a subscription. It
captures a cursor from the database clock, registers LIVE hints and reads committed
changes through `SHOW CHANGES FOR DATABASE`. LIVE wakes the reader; the feed supplies
the records. An idle subscriber does not poll the database.

The consumer owns its current-state projection and durable checkpoint. Every source
establishment delivers `Reconcile`, including reconnects with recent cursors, because
a disconnection can overlap feed cleanup. A missing checkpoint or one older than
the shortest declared retention minus one day starts from the new database anchor.
A declaration with no window after that safety margin always reconciles from the
anchor. The consumer acknowledges only after reconciling or applying a complete page.

SurrealDB applies replay limits before table filtering and versionstamp grouping.
Database-wide replay therefore advances through unrelated changes, and a page's
last transaction is reread to obtain its complete tail before advancing the cursor.
A tail that exceeds the supported limit fails instead of silently dropping rows.
Filtering this internal feed selects subscribed tables; it does not authorize a
public read. Unrecognized change-entry shapes fail decoding before a page can be
acknowledged. Owners reject malformed domain records during typed decoding.

The shared decoder exposes upserts, deletes and schema definitions. Owners decode
their domain records. Computers owns `ComputerChange` in its domain library;
Task and Artifact decoding stay in Store. A delete carries its original row only
when the table enables `INCLUDE ORIGINAL`. Consumers needing deleted tenant or
parent fields must qualify that schema property.

## Resource Invalidation

`resource_changes` accepts replayable and LIVE-only declarations. It exposes only
an invalidation reason, never a record or identity. Callers reread their complete
visible resource selection using current policy. Knowledge's coordinator uses
this LIVE-only profile.

A connection establishes its database-clock anchor before registering LIVE. On
reconnect, replay checks the overlap for relevant committed changes. Every successful
connection invalidates the projection even when retained history has expired or a
table has no replay feed. Live writes are coalesced for 100 milliseconds; the clock
checkpoint advances on observed writes rather than an idle timer.

## Queries And Subscription Lifetime

Shared statements live in `platform/store/queries/changefeed`. Table selection in
the projected LIVE query uses `type::table($table)`. Each observer group owns one
authenticated WebSocket connection. Its tables share that connection, with a separate
top-level query and notification stream for each table. Resource hubs share a group
across their consumers. If any source stream ends, the merged stream reports an error
so its consumer can reconnect the complete selection.

Registration runs in an owned task with a 30-second deadline. When its caller stops
waiting, the task finishes receiving the current statement's query ID and registers
no further tables. Dropping the returned stream, including an unpolled stream, starts
explicit `KILL` statements through the registering session. Cleanup attempts all known
IDs despite individual errors, with a 30-second deadline for the whole cleanup. The
observer then releases its connection. Owning the transport permits cleanup of
registrations whose response was lost before their IDs reached the client; retiring
only a logical session on a shared connection does not provide that guarantee in the
pinned SDK. Other Store users keep their connections. This costs one additional
connection per observer group, independent of the number of tables in the group.

The owning Tokio runtime must stay running for asynchronous cleanup. Errors, deadline
expiry and a missing runtime are logged; dropping a stream does not synchronously prove
database cleanup. The SDK notification stream's own drop behavior is insufficient for
this qualified SDK profile, so both projected and full-row observers use this adapter.
The SDK's reconnect worker can outlive released client handles during a database outage;
it exits when connectivity returns or the runtime stops. The 30-second deadlines bound
adapter registration and cleanup work, not that SDK worker's lifetime. Reconnection
restores session state without replaying LIVE registration statements.

The SurrealDB 3.3.0 `SHOW CHANGES` grammar requires literal numeric cursor and limit
slots. `replay.surql` therefore has two fixed placeholders, replaced only with decimal
representations of the checked cursor and a limit between 1 and 1,000. Native
qualification rejects bound parameters in those slots and checks complete-transaction
limit behavior. This is the observation path's dynamic SQL exception; no caller
supplies statement fragments or table text.

## Qualification

Native qualification covers multi-table delivery, unrelated feed entries, complete
transaction tails, reconnect and checkpoint recovery, stale cursors and listener
cleanup against the pinned database. Lifetime cases include unpolled streams, partial
registration, caller cancellation before a delayed response and transport retirement
without a received query ID. The lost-response case checks the server after the delayed
statement could finish and verifies that the parent Store connection still works.
Decoder unit tests cover malformed payloads.
Owner fixtures verify table claims, feed retention and original delete payloads
where required. Computers keeps its grant-change assertions beside its decoder.
Dependency checks build declarations without service runtimes; they do not
substitute for native schema checks.

The [active plan](../../../../docs/CONTRACT_CONSISTENCY_PLAN.md#phase-3-module-ownership-of-persistence-and-queries)
tracks qualification and the remaining production repository and schema-lane moves.
