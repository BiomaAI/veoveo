# Store Write And Journal Cost After Native Feed Adoption

The source base is `82b47a82`, with the measurement helpers committed beside this
record. Production writers use native table feeds. The harness recreates the old
transaction shapes in isolated measurement tables to compare their costs under
the same conditions. These controls do not restore the removed outbox API.

## Commit Latency

The workload and latency window match the
[before measurement](2026-10-01.md): 64 transactions per writer, one or eight
writers sharing a multiplexed WebSocket client, three rotated profile orders,
and a fresh RocksDB fixture for each run. All 5,184 transactions commit in 18
fixtures. Every final domain revision and event count matches the workload.
Both measurement tables keep `INCLUDE ORIGINAL` to preserve the baseline workload;
the production schema enables it only for consumers that need deleted parent fields.

Elapsed time is the median of three runs; percentiles use nearest ranks over all
operation samples for the profile and writer count. Times include the client round
trip. Both measurements use SurrealDB 3.3.0 on the same host with the cluster and
BuildKit stopped. Docker allows two CPUs and 2 GiB RAM, a 64 MiB block cache and
two 16 MiB write buffers. The runs are separate observations, without a claim that
host scheduling was identical.

| Writers | Profile | Before elapsed ms | After elapsed ms | After p50 ms | After p95 ms |
|---:|---|---:|---:|---:|---:|
| 1 | Shared sequence | 177.261 | 178.992 | 2.215 | 4.654 |
| 1 | Independent event | 73.751 | 112.714 | 1.290 | 1.787 |
| 1 | Native feed only | 100.521 | 68.309 | 1.040 | 1.165 |
| 8 | Shared sequence | 999.344 | 759.294 | 11.123 | 13.372 |
| 8 | Independent event | 314.987 | 184.857 | 2.564 | 4.985 |
| 8 | Native feed only | 242.954 | 126.527 | 1.848 | 2.831 |

In the current eight-writer comparison, removing sequence allocation reduces
median elapsed time by 75.7%; removing the extra event row and its indexes reduces
it by another 31.6%. Native-feed-only transactions take 83.3% less time than the
shared-sequence control in this workload. Neither observation reports a
client-visible conflict or retry. Database-internal retries are not observable here.
These measurements do not establish installed throughput or complete MCP latency.

The [per-run CSV](native-after-2026-10-01.csv) and
[operation samples](native-after-2026-10-01.jsonl) preserve the results.

## Storage Method

A separate run measures eight writers and the same three transaction shapes in
three rotated orders: 4,608 transactions across nine fresh fixtures. The fixture
admin requests database compaction before and after the workload. The harness
flushes kernel writeback and waits for two seconds of stable cgroup-v2 counters
before each observation. Those steps and Docker inspection are outside the timed
write window. Measured writes still use database-editor credentials.

The reported bytes are Linux-attributed writes on device `259:0` for the workload
plus requested compaction. They include storage-engine work and do not measure
SSD NAND writes or steady-state write amplification. Retained growth is the change
in Docker's logical writable-layer size, including the engine's files. The original
before run did not collect these counters; the comparison uses contemporaneous
controls. [Fixture methods](../../../testing/fixtures/DESIGN.md#storage-measurements)
define the observation and cleanup requirements.

| Profile | Attributed writes MiB | Writable-layer growth bytes | Client conflicts/retries |
|---|---:|---:|---:|
| Shared sequence | 8.230 | 1,221,278 | 0 |
| Independent event | 3.645 | 1,079,585 | 0 |
| Native feed only | 1.609 | 163,600 | 0 |

The cells report medians across three runs. Native-feed-only writes reduce the
observed block-write volume by 80.4% relative to the shared-sequence control. This
result includes both the event/index removal and sequence removal. The
[per-run CSV](write-io-2026-10-01.csv) and
[operation and device samples](write-io-2026-10-01.jsonl) retain layer sizes,
per-device write deltas and workload timings. All nine fixtures commit their
expected rows and clean up.

## Encrypted Command Journals

The [Computer journal harness](../../computers/tests/journal_cost.rs) admits a real
encrypted command through production policy, grant and queue code, using the full
schema. It dispatches no provider operation. Each fixture performs 16 sequential
changes to `updated_at`, leaving the encrypted input unchanged. Input sizes are
1 KiB, 64 KiB and the supported 1 MiB stdin limit. The two profiles use the current
native feed or disable that one feed as an isolated control. Three rounds rotate
their order.

The same compaction and I/O observation surround each update workload. Replay uses
the production Store reader through the second database-editor connection. The
harness counts matching journal rows and their ciphertext bytes, asserting that
each returned envelope matches the persisted length. JSON size describes the
normalized decoded row; it does not measure WebSocket framing or the SDK's binary
wire encoding. Replay timing excludes that JSON-size calculation.

All logged samples contain sizes, counters and timings. No command bodies,
ciphertext, fixture credentials or installation data enter these artifacts.

| Stdin | Profile | Attributed writes MiB | Layer growth MiB | Feed ciphertext MiB | Replay ms |
|---|---|---:|---:|---:|---:|
| 1 KiB | Feed disabled | 0.422 | 0.082 | 0 | 1.175 |
| 1 KiB | Native feed | 0.633 | 0.226 | 0.030 | 6.520 |
| 64 KiB | Feed disabled | 2.391 | 1.832 | 0 | 1.154 |
| 64 KiB | Native feed | 8.004 | 5.494 | 1.780 | 19.077 |
| 1 MiB | Feed disabled | 34.195 | 28.505 | 0 | 1.165 |
| 1 MiB | Native feed | 132.195 | 85.508 | 28.447 | 256.426 |

Cells report medians of three runs. Every feed-enabled run replays all 16 updated
rows; disabled controls replay none. The 1 MiB stdin becomes a 1,864,288-byte
encrypted envelope. Its 16 unchanged copies contribute 29,828,608 bytes to the
feed, before other row fields and wire encoding. Initial admission is outside the
update window and is excluded from these deltas.

At 1 MiB, the three native-feed update runs take 302.003, 317.705 and 1,678.074 ms;
the controls take 274.964, 226.740 and 286.606 ms. The pooled operation p95 is
355.665 ms with the feed and 25.348 ms without it. All samples are retained; this
experiment does not identify the cause of the slowest run. No update retries are
implemented, and every update succeeds on its first client attempt.

The inline encrypted payload makes metadata changes expensive even without a
feed. Native replay adds a full copy per update, and database-wide readers receive
those copies before selecting their domain's changes. The 16-row case passes
through the actual reader; it does not establish safe memory or response sizes for
a full 512-entry page.

Phase 5 requires separating immutable encrypted request payloads from journal
metadata before deployment. Admission must still commit payload and journal
together, exact retries must compare the accepted input, and recovery must retain
the payload for the journal's lifetime. Metadata feeds must continue to invalidate
authority. Disabling the feed is only the measurement control.
The [separated-payload measurement](journal-separated-2026-10-01.md) records the
qualification of that storage layout.

The [per-run CSV](journal-2026-10-01.csv) and
[operation/device samples](journal-2026-10-01.jsonl) preserve all 288 metadata
updates across 18 fixtures. All fixtures are removed after observation.

## Reproduction

Use the qualified native build environment, a local Linux Docker engine with
cgroup-v2 accounting, and the cached SurrealDB image pinned by the shared fixture.
Commit `07f349f2` contains the harness for this inline-payload observation; the
current journal harness measures the separated layout.
Stop the cluster and builder. Run without concurrent builds or cleanup:

```sh
cargo test -p veoveo-platform-store -p veoveo-computers \
  --test write_cost --test journal_cost -- --ignored --nocapture --test-threads=1
```

The latency and I/O comparisons each have a ten-minute deadline; the encrypted
journal comparison has fifteen minutes. Only the storage observations require
host I/O accounting. The harnesses emit `WRITE_COST`, `WRITE_IO` and `JOURNAL_COST`
JSON records with individual latency samples. The three experiments complete
45 successful fixtures; strict Clippy passes for their test targets.
