# Console Installation Stream

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP and Server-Sent Events | Authenticated installation stream, named entity events, `Last-Event-ID`, reconnect and reset |
| SurrealDB 3.2.4 changefeeds | Database-wide versionstamp replay with `INCLUDE ORIGINAL`; LIVE queries accelerate replay |
| Veoveo installation projection | Tenant-filtered Console summaries; current administrator authority admits each connection |

The snapshot anchors a cursor before reading rows. The stream replays database
changes from that cursor and emits authorized summaries. LIVE notifications carry
no browser data. They wake durable replay, which also reconciles every fifteen
seconds after missed notifications.

One database cursor advances across every committed transaction, including changes
to tables outside the Console projection. SurrealDB applies its page limit before
filtering tables, so a per-table cursor can stall behind unrelated traffic. The
shared store adapter reads database pages and completes the final transaction
before advancing. It rejects a transaction at the upstream thousand-table bound
instead of silently losing its tail.

Each replay round processes at most four pages before yielding browser events.
Within a transaction, parent projections apply before dependent records. The last
emitted event in each versionstamp group carries the resume ID. All row mutations
pass existing tenant and Artifact-access projection checks; unrelated database
records never become browser events. A replay failure emits `reset`, which asks
the client to reload its authorized snapshot.

Agent lease renewals use this stream without model work or browser polling. The
client expires a lease locally only when no newer renewal reaches its projection.
Native store tests cover sparse agent updates behind busy tables and a transaction
split across a physical page boundary. Installed acceptance also observes agent
cards across multiple lease intervals without a page reload.
