# Recording MCP design

`recording-mcp` is the governed catalog and read boundary for Recording Hub
data. The repository-wide ingest, storage, and playback contract is normative
in [`docs/RECORDINGS.md`](../../docs/RECORDINGS.md).

The MCP surface owns recording discovery, bounded queries, subscriptions, and
artifact publication. HTTP routes beside the MCP endpoint carry RRD bytes
because the embedded Rerun viewer consumes byte streams rather than MCP content
blocks. Gateway policy and audit still target the canonical
`recording://recordings/{id}` resource before either route reaches this server.

Archive playback exposes immutable frozen or sealed shards individually. A
manifest identifies each shard by catalog UUIDv7, ordinal, capture bounds,
length, and digest. The data route reauthorizes the shard and preserves HTTP
range semantics. It never decodes, merges, or rewrites an archive during a
request.

Live playback is a generated stream over the current writing shard. It emits
store information and static context, retains a bounded row-ID history window,
then follows newly durable data. The live URL is bound to one writing segment
identity and ends at rollover. While the recording remains live, Console
refreshes the manifest every five seconds and restarts Rerun only when that
identity changes.

`contract.rs` owns the typed manifest. `service/read.rs` resolves a fresh
authorized filesystem plan from durable identities. `live_playback.rs` owns the
bounded follow projection. `bin/server.rs` owns HTTP framing and byte-range
forwarding; authorization remains in `service.rs`.
