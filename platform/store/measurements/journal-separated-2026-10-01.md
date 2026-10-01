# Computer Journal Cost With Separate Request Payloads

The source base is `07f349f2`, with payload separation and the adjusted harness
committed beside this record. Command and file admissions commit their immutable
encrypted request into a private payload row. The journal carries a read-only link.
Payload tables have no changefeed, and parent deletion cascades to the payload.
The [Computer storage design](../../computers/DESIGN.md#private-request-storage)
defines the storage and recovery rules.

## Method

The [journal harness](../../computers/tests/journal_cost.rs) repeats the
[inline-payload experiment](native-after-2026-10-01.md#encrypted-command-journals):
real command admission, 1 KiB/64 KiB/1 MiB stdin, 16 metadata-only updates per
fixture, native-feed and feed-disabled profiles, and three rotated rounds.
Both profiles use the new storage layout. All 288 updates commit on their first
client attempt, and the 18 owned RocksDB containers are removed.

The qualified SurrealDB 3.3.0 image, Docker limits of two CPUs and 2 GiB RAM,
64 MiB block cache and two 16 MiB write buffers are unchanged. The cluster and
BuildKit are stopped, and no build or cleanup runs during measurement. Fixture
administration requests compaction before and after the workload; kernel writeback
settles before the counters are read. Those steps are outside the latency window.

Write bytes measure Linux cgroup-v2 attribution on device `259:0`, including requested
compaction. They do not measure SSD NAND writes or steady-state amplification.
Layer growth measures Docker's logical writable-layer size. The feed byte count
serializes normalized rows after the actual shared Store replay reader returns;
it excludes WebSocket framing and the SDK's binary wire representation. Replay time
excludes that JSON-size calculation.

The earlier experiment and this one are separate observations on the same host.
The before record includes a slow native-feed run; all samples from both runs are
retained. The comparison establishes the cost of the controlled metadata-update
workload and makes no installed throughput claim.

## Results

The table reports median totals for 16 updates across three runs. Feed-disabled
controls still persist the encrypted request and the journal metadata.

| Stdin | Profile | Before elapsed ms | After elapsed ms | Before writes MiB | After writes MiB | After layer growth bytes |
|---|---|---:|---:|---:|---:|---:|
| 1 KiB | Feed disabled | 19.924 | 18.406 | 0.422 | 0.371 | 51,636 |
| 1 KiB | Native feed | 18.254 | 19.452 | 0.633 | 0.469 | 102,833 |
| 64 KiB | Feed disabled | 30.729 | 18.145 | 2.391 | 0.367 | 51,550 |
| 64 KiB | Native feed | 40.395 | 23.839 | 8.004 | 0.465 | 103,268 |
| 1 MiB | Feed disabled | 274.964 | 21.189 | 34.195 | 0.371 | 51,616 |
| 1 MiB | Native feed | 317.705 | 19.379 | 132.195 | 0.465 | 103,245 |

At 1 MiB stdin, native-feed metadata updates use 99.6% fewer attributed write
bytes. Median elapsed time drops by 93.9%. After separation, pooled operation p50
and p95 are 1.165 and 1.920 ms; the inline-payload values were 18.754 and 355.665 ms.
The small-input latency results do not establish a benefit from the split.

All nine native-feed runs contain 39,232 normalized JSON bytes and zero request
ciphertext bytes, independent of stdin size. At 1 MiB, the inline profile contained
29,869,632 JSON bytes, of which 29,828,608 were repeated ciphertext. Median replay
time drops from 256.426 to 7.108 ms. The initial encrypted input remains available
through the private payload row; it is not discarded to obtain these results.

[Per-run results](journal-separated-2026-10-01.csv) and
[all operation/device samples](journal-separated-2026-10-01.jsonl) preserve the
measurements.

## Qualification

The replay checks require exactly 16 journal updates for each native-feed run and
none for a feed-disabled run. Every updated row carries the expected typed payload
record link. No journal row contains the request envelope, and no payload-table
change appears in replay. All native-feed rows therefore contain zero request
ciphertext bytes. Input size changes the stored payload's size without changing
the metadata feed's row size.

The grouped Store, Computers and Computers MCP qualification passes 296 distinct
functional tests. New native cases prove read-only payloads, rollback after payload
creation fails, parent-owned cleanup and missing-input recovery that keeps the
execution fence. Existing cases cover concurrent retries, changed input, authority,
task recovery, containment, completion and public HTTP admission. Strict all-target
Clippy passes for the three packages. Installed provider execution is not part of
this experiment.

## Reproduction

Use a local Linux Docker engine with cgroup-v2 accounting and the qualified native
build environment. Stop the cluster and builder:

```sh
cargo test -p veoveo-computers --test journal_cost -- \
  --ignored --exact compare_encrypted_journal_feed_cost --nocapture --test-threads=1
```

The complete experiment has a fifteen-minute deadline. Its `JOURNAL_COST` lines
contain numeric measurements and profile names; they contain no request bodies,
ciphertext or credentials.
