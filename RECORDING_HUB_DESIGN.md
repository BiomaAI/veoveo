# Recording Hub Design

One container-group that turns Rerun's transport into the platform's durable
time-and-space record: every sensor, estimator, and agent streams into one
gRPC ingest point; a spooler persists every message as segment files; the OSS
Rerun catalog server makes the same bytes queryable over the wire (latest-at,
range, dataframe) for agents, pipelines, analysts, and viewers alike.

Files are the record. The proxy is the bus. The catalog is the reading room.

## Verified constraints this design is built on (Rerun 0.34)

1. The gRPC message proxy is a relay with a bounded in-memory queue
   (`--server-memory-limit`, default 1 GiB): oldest messages drop on
   overflow, late joiners see only the buffer. It is multi-producer and
   sessioned by recording id. It persists nothing and authenticates nobody.
2. The OSS catalog server (`rerun server`, redap, :51234) serves directories
   of `.rrd` files as datasets whose **segment id = the file's recording
   id**. It answers latest-at/range/dataframe queries with server-side chunk
   pruning, lazy-loads via manifests embedded in optimized RRDs, holds all
   registration state in memory (restart = re-register from `-d` flags), has
   **no write path to disk and no authentication**.
3. `re_grpc_server`, `re_grpc_client`, and `re_log_encoding` are published
   crates: the proxy is embeddable, the read stream is consumable, and the
   encoder appends `LogMsg`s to RRD framing (footers optional; a raw
   transport pass-through exists behind an explicit `unsafe` contract).
4. Our own kernel facts: agents already tee their flight logs to a proxy URI
   (`--viewer-tee`); segment files with `write_footer: false` are the proven
   long-lived sink discipline; `rerun rrd optimize` adds the manifests lazy
   loading wants.

Consequences: durable ingest is ours to build (small), the catalog is free,
the proxy is embeddable, and governance must come from network placement plus
the artifact plane — never from the hub itself.

## Architecture

```
producers (sensors, estimators, agents' tees)
   │  gRPC WriteMessages, sessioned by recording id
   ▼
┌─ hub-spooler ────────────────────────────────────────────────┐
│  EMBEDS re_grpc_server: one process is the proxy AND the     │
│  writer. Every accepted message is (a) offered to the        │
│  in-memory queue for live consumers (viewers) and (b)        │
│  appended durably to the spool before ack-equivalent drop.   │
│                                                              │
│  demux: StoreId → SegmentWriter                              │
│  spool: /spool/{dataset}/{YYYY-MM-DD}/{recording_id}.rrd     │
│  freeze: size/age → finalize → verify → optimize → publish   │
└──────────────────────────────────────────────────────────────┘
   │ shared volume (spooler: rw · everyone else: ro)
   ▼
┌─ hub-catalog ────────────────┐   ┌─ viewers ────────────────┐
│  rerun server :51234         │   │  connect to spooler's    │
│  -d per dataset/day dirs     │   │  proxy for live tail, or │
│  latest-at / range / frames  │   │  to the catalog for      │
│  over redap                  │   │  history                 │
└──────────────────────────────┘   └──────────────────────────┘
```

**The embedding decision is the core of the design.** A standalone proxy plus
a subscribing spooler has three structural flaws: the ring buffer can drop
data the spooler never saw, a spooler reconnect replays buffered history
(duplicate chunks), and every byte crosses the wire twice. Embedding
`re_grpc_server` in the spooler makes the durable write the first-class path:
data is spooled on receipt, the in-memory queue survives only as the live-tail
convenience for viewers, and there is no reconnect window at all. Fallback if
embedding fights us (API surface, runtime coupling): standalone
`rerun --serve-grpc` + subscribing spooler with a bounded chunk-id dedup LRU —
kept as milestone insurance, deleted after H1.

### Spool layout and the catalog's session model

The catalog overwrites a segment when a second file carries the same
recording id in one dataset. Day-partitioned datasets resolve this cleanly
and match how recording fleets are actually operated:

```
/spool/world/2026-07-08/{recording_id}.rrd      ← live, footer-less
/spool/world/2026-07-08/{recording_id}.rrd.part ← mid-freeze scratch
```

- One growing file per `(dataset, day, recording_id)`. A recording id is a
  session; a session spanning midnight becomes one segment per daily
  dataset — cross-day queries address multiple datasets by name
  (`world_20260708`, `world_20260709`).
- **Freeze pass** (size/age threshold, and at day rollover): finish the
  encoder, `rerun rrd verify`, `rerun rrd optimize` into `.part`, atomic
  rename over the original. Optimized files carry manifests, so the catalog
  lazy-loads them; the live file of the current day is served eagerly (it is
  at most one day / one size-threshold big).
- Dataset routing: the producer's application id maps to the dataset
  (`veoveo-agent-pilot` → `agents`; sensor sims → `world`), with a typed
  routing table in spooler config; unknown application ids land in a
  `quarantine` dataset rather than being dropped.

### Strong typing

- `crates/recording-hub`, package `veoveo-recording-hub`, lib
  `veoveo_recording_hub`, bins `spooler` and `sensor-sim`.
- Typed config throughout (clap + env, `hide_env_values` on anything
  secret-adjacent): `SpoolerConfig { bind, spool_dir, datasets:
  Vec<DatasetRoute>, segment_max_bytes, segment_max_age, flush_interval,
  fsync_interval, live_queue_limit, rerun_bin: Option<PathBuf> }` with
  fail-closed validation (routes must be unambiguous, spool dir writable,
  thresholds sane).
- `DatasetRoute { dataset: DatasetName, application_id_prefix: String }` with
  `DatasetName` as a validated newtype (lowercase, path-safe).
- Spool state is typed, never stringly: `SegmentKey { dataset: DatasetName,
  day: NaiveDate, recording: StoreId }`, `SegmentWriter` owns its `Encoder`,
  byte/row counters, and freeze state machine
  (`Live → Freezing → Published`).

### Performance

Budget: a spooler that cannot outrun its producers is a data-loss machine, so
the write path is engineered and *measured*, not hoped:

- Hot path per message: demux (hash on `StoreId`) → `Encoder::append` into a
  `BufWriter` (1 MiB) → periodic flush (default 250 ms) → periodic fsync
  (default 2 s, configurable to 0 for every-flush durability). Zero
  allocation beyond what encoding requires; no serde on the hot path.
- Freeze, verify, and optimize run on a blocking pool, never on the ingest
  task. One ingest task per connection (tonic's model), writers behind a
  sharded map keyed by store id; a single writer is single-threaded by
  construction (one recording = one file = one owner).
- Counters (messages, bytes, per-dataset, queue depth, append p99) kept as
  atomics, logged every 10 s, exported via OTLP when configured — the bench
  harness asserts on these same counters, so the numbers we tune are the
  numbers we test.
- **Fast path (H4, feature-gated)**: raw transport pass-through with
  `append_transport_without_footer` — no decode/re-encode of chunk payloads.
  The `unsafe` contract (encoder compression must match transport encoding)
  is honored by pinning both to the same setting and proving it with a
  corruption test (`rrd verify` over gigabytes of pass-through output).
  Target: fast path sustains ≥ 5× the v1 baseline.
- Bench targets (asserted by `hub-bench`, not the CI suite): v1 append path
  sustains ≥ 100k msgs/s mixed IMU-sized messages on dev hardware with zero
  live-queue overflow warnings and p99 append < 1 ms.

### Safety and failure behavior

- Crash mid-write: footer-less RRD decodes to the last complete message —
  the property the kernel already relies on; the smoke kills the spooler
  mid-stream and proves both decode and (post-restart) continued capture.
- Freeze is atomic: verify + optimize into `.part`, rename to publish; a
  crash mid-freeze leaves the original untouched and a `.part` to sweep.
- Restart: scan the spool, resume each current-day live file by **starting a
  new file** `{recording_id}.r{n}.rrd` (an RRD file is not appendable
  in-place); the freeze pass later merges day files per recording
  (`rerun rrd merge`) so the catalog sees one segment per session per day.
- The spool volume mounts read-only into every other service (catalog,
  viewer); only the spooler holds write.
- The hub is **internal-network only**: OSS redap and the proxy trust every
  caller by design, so nothing publishes past loopback, exactly like the
  control-plane Postgres. Governed, labeled, cross-tenant access to
  recordings continues to flow through the artifact plane (frozen segments
  can be promoted to plane artifacts by existing machinery); policy-checked
  agent queries over the catalog arrive later as a thin hosted MCP tool, not
  as holes in the hub.

## sensor-sim: typed generators, deterministic by construction

One binary that is both the smoke suite's fake fleet and the bench harness's
load cannon. A typed manifest (`sensors.json`, serde `deny_unknown_fields`)
describes a fleet; every generator is seeded and therefore exactly
reproducible — the smoke asserts *counts and final values*, not vibes.

```rust
SensorSpec {
    id: SensorId,                 // validated newtype
    recording: String,            // session identity at the hub
    application_id: String,       // dataset routing
    kind: SensorKind,
    seed: u64,
    duration_s: Option<f64>,
}
SensorKind::Imu     { rate_hz: f64, accel_bias: [f64; 3], gyro_noise: f64 }
SensorKind::Gnss    { rate_hz: f64, origin: LatLon, pattern: TrackPattern } // Orbit{radius_m, period_s} | Line{heading_deg, speed_mps}
SensorKind::Camera  { fps: f64, frame_bytes: usize }                        // synthetic blobs, content = seeded hash
SensorKind::Scalar  { rate_hz: f64, name: String, wave: Wave }              // Sine|Step|RandomWalk
```

- Emission is typed Rerun data on `/world/sim/{id}`: `Points3D`/`GeoPoints` +
  named scalar components for pose and rates, blobs for frames — the same
  component discipline the world model uses, so hub data is world-model data
  from day one.
- Rate-accurate via `tokio::time::interval` with drift correction; `--burst`
  multiplies rates for the bench; `--report` prints the exact emitted counts
  per sensor as JSON (the smoke's ground truth).
- Every generator is a pure function of (seed, tick index) — restartable,
  and final-state assertions are exact.

## Test plan

**Unit (crate)**
- Demux/rotation state machine: synthetic `LogMsg` streams through the
  writer map; assert file-per-(dataset, day, recording), freeze thresholds,
  atomic publish, `.part` sweep.
- Round-trip: spooler pipeline output read back with `QueryEngine`; row
  counts and latest-at values equal the typed input.
- Config validation fail-closed table tests; dataset routing including
  quarantine.

**Smoke `hub-spool`** (deterministic, in-suite)
- Spawn spooler (embedded proxy); run `sensor-sim` with a 3-sensor manifest
  (IMU 200 Hz, GNSS 10 Hz orbit, camera 2 fps) for a fixed duration + seed.
- Assert: segment files exist per session; `QueryEngine` counts equal
  `sensor-sim --report` exactly; GNSS latest-at equals the generator's
  computed final position within 1e-9; **kill -9 the spooler mid-stream**,
  restart, re-run sim: all files decode, post-restart capture resumes into
  `.r1` files, merged freeze yields one segment per session.
- `rerun rrd verify` over the frozen output.

**Smoke `hub-catalog`** (in-suite; requires `rerun` CLI, passed as `--rerun-bin`)
- Freeze + optimize a seeded spool; launch `rerun server -d` over it; query
  through `re_redap_client`: dataset listing, segment ids equal the sim's
  recording ids, latest-at over redap equals local `QueryEngine` answers.
- Restart `rerun server`; identical answers (files are the truth).

**Smoke `hub-agent-world`** (integration)
- The sleep/wake stack plus the hub: the agent's `--viewer-tee` points at
  the spooler; run the standard detach/sleep/wake mission; assert the
  agent's session appears as a hub segment and its `/agent/**` rows are
  queryable from the spool alongside a concurrently-running sensor-sim's
  `/world/**` rows — one unified record, two producers.

**`hub-bench`** (recipe, excluded from the default suite)
- sensor-sim burst fleet (8× IMU at 1 kHz + camera blobs) for 60 s against
  the spooler; assert from the spooler's own counters: sustained ≥ 100k
  msgs/s, zero live-queue drop warnings, p99 append < 1 ms; emit a JSON
  report artifact. Criterion micro-bench on the append hot path lives in the
  crate for regression tracking.
- H4 gate: the same bench with the pass-through feature at ≥ 5× baseline,
  followed by `rrd verify` over everything written.

**`hub-live`** (manual recipe)
- Against the running compose stack: Pilot with tee → hub, one real mission
  (`just agent-pilot-local`), then a redap query listing the Pilot's session
  and a latest-at over its world writes — the live proof that agent memory,
  sensor streams, and the catalog share one record.

## Compose & container

Three services behind an opt-in `hub` profile, one named volume `hub_spool`:

- `hub-spooler`: `crates/recording-hub/Dockerfile` (two-stage, non-root,
  vendors the `rerun` CLI wheel for freeze passes), volume rw, publishes
  `127.0.0.1:9876` replacing the bridge's direct ingest role.
- `hub-catalog`: same image, entrypoint `rerun server` with `-d` flags over
  the volume mounted **ro**, `127.0.0.1:51234`, restart-on-failure (restart
  is cheap and by-design: files are the truth).
- `rerun-bridge` (existing) keeps the viewer + viewer-mcp; the viewer
  connects to the spooler's proxy for live tail — viewing decouples from
  ingestion.
- The Pilot's compose tee retargets to `rerun+http://hub-spooler:9876/proxy`.

## Milestones

- **H0 — scaffold + generators.** Crate, typed configs, `sensor-sim`
  complete with unit tests and `--report`; embeddability spike for
  `re_grpc_server` (the one open API risk) and a `re_redap_client` query
  spike. Exit: sim streams into a plain `rerun --serve-grpc` and a viewer
  shows it; both spikes have running code.
- **H1 — spooler v1.** Embedded proxy + demux + segment writing + freeze +
  restart resume; unit tests; `hub-spool` smoke green including kill -9.
- **H2 — catalog.** Day-partitioned layout finalized, optimize-on-freeze,
  `hub-catalog` smoke green over `re_redap_client`.
- **H3 — platform wiring.** Dockerfile, compose services + volume + `hub`
  profile, Pilot tee retarget, `hub-agent-world` smoke green, Justfile
  recipes (`smoke-hub-spool`, `smoke-hub-catalog`, `smoke-hub-agent-world`,
  `hub-bench`, `hub-live`).
- **H4 — performance.** Bench harness asserting the counter targets;
  BufWriter/flush tuning; feature-gated raw pass-through + corruption gate;
  Criterion regression bench.
- **H5 — record & docs.** Pilot Harness document gains the hub; memory notes
  updated; deferred items recorded (redap MCP tool for policy-checked agent
  queries; plane promotion of frozen segments as an automated flow).

## Open questions, answered by construction where possible

1. **`re_grpc_server` embeddability** — the design's one real API risk;
   H0's spike answers it before anything depends on it, with the
   standalone-proxy + dedup-LRU spooler as the fallback shape.
2. **Catalog rescan of new files** in registered directories — unknown;
   mitigated regardless by the restart-is-cheap stance and the freeze pass
   registering day directories.
3. **Duplicate chunks** — designed out by embedding (no replay window); the
   fallback keeps a bounded chunk-id LRU and the `hub-spool` restart
   assertions catch regressions either way.
4. **redap auth** — none in OSS, by evidence; answered with network
   placement now and a policy-checked MCP query tool later, never by
   trusting the hub.
