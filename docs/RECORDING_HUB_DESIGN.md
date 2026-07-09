# Recording Hub Design

One container-group that turns Rerun's transport into the platform's durable
time-and-space record: every sensor, estimator, and agent streams into one
gRPC ingest point; a spooler persists every message as segment files; the OSS
Rerun catalog server makes the same bytes queryable over the wire (latest-at,
range, dataframe) for agents, pipelines, analysts, and viewers alike.

Files are the record. The proxy is the bus. The catalog is the reading room.

## Build status (2026-07-08)

**Hub H0–H4 built and tested** (`crates/recording-hub`): embedded-proxy spooler
+ `sensor-sim` + `hub-query`; 10 unit + 2 integration Rust tests; process
smokes `hub_spool` (kill -9 + `.rN` resume + QueryEngine counts),
`hub_catalog` (freeze→optimize→serve→**real redap query** cross-check, segment
id == recording id, exact counts), `hub_agent_world` (routing), `hub_bench`
(lossless at **~225k msgs/s**, burst 100). Docker image + `hub` compose profile
(`docker compose config` valid). Justfile: `smoke-hub*`, `bench-hub`.

**Showcase S0–S4 built and tested** (`showcase/sumo/`): Python `sumo-mcp`
(task-native, `mcp==1.28.x`, pydantic, sync + task tools + congestion
resource); **24 pytest** incl. the full `call_tool_as_task → poll →
get_task_result` lifecycle and the subscribe→`resources/updated` wake; push
spine proven against the real Rust hub (`sumo_push_smoke`, 40 frames durable).
SUMO + sumo-mcp Dockerfiles, `showcase/sumo/compose.showcase.yaml` (config valid), README.
Justfile: `test-sumo-mcp`, `smoke-sumo-push`, `showcase-up`, `showcase-capstone`.

**Full live stack proven end to end on a real city** (2026-07-08): the SUMO
container runs **LuST — Luxembourg SUMO Traffic** (a validated OpenStreetMap
network, 5779 edges, 201 signals, geo-referenced), started at the morning ramp.
`sumo-mcp` calibrates cartesian→lon/lat once from the network's own projection,
reads all vehicles per frame in a single TraCI subscription round-trip, and
pushes them as one speed-coloured GeoPoints layer into the hub — **962 live
vehicles at (49.55, 6.03) Luxembourg**, streamed to a native Rerun viewer on the
real map via the hub's published proxy. The served MCP endpoint drives it:
`query_state`/`describe_scenario`, `run_batch` as a detached task, and the full
control surface on real objects — `set_signal_phase`, `set_edge_speed`,
`close_lane`/`open_lane`. Offline task tools (`generate_network`,
`compute_routes`) shell out to the real bundled SUMO CLIs. An interim
fake-driver run (`compose.interim.yaml`) proves the container runtime without
the SUMO image.

Deferred (recorded): `append_transport_without_footer` fast path (unreachable
through `spawn_with_recv`, and target already met); heavy compose e2e of the
agent tee with real gateway+Cloudflare; the Autonomy Harness HTML gaining the hub.

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
4. Our own kernel facts: agents already tee their decision logs to a proxy URI
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
- **H5 — record & docs.** Autonomy Harness document gains the hub; memory notes
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

---

# Showcase — a SUMO traffic world

A `showcase/` that proves the whole platform on a real simulator instead of a
synthetic generator: the [SUMO](https://eclipse.dev/sumo/) traffic simulator
runs a live city; its vehicles, signals, and detectors are **pushed** into the
Recording Hub as typed sensor streams; the Pilot agent perceives that world
through its world model, and acts back on it through a **task-native MCP
server we build ourselves** — so long simulator operations become MCP tasks
the agent detaches from, sleeps through, and wakes on. It is the sleep/wake
loop already proven, now driven by a real simulator doing real work.

The hub never pulls. SUMO is one more push producer; `sumo-mcp` is one more
task-native gateway server. Nothing in the platform learns it is talking to a
traffic simulator.

## Why Python, and why our own server

The showcase's MCP server is **Python by choice, not compromise.** The
overwhelming majority of MCP servers clients build are Python, so a showcase
should demonstrate how to build a *proper* one in the language they will
actually use — task-native, streamable-HTTP, gateway-governed, strongly typed
with pydantic. Python is also where the SUMO ecosystem lives (`traci`,
`sumolib`, the `$SUMO_HOME/tools` scripts), so the language that best drives
the simulator is the same one that best demonstrates the pattern. This is a
feature of the showcase, not a concession.

The public SUMO MCP servers are **inspiration, not dependency**. The most
functional (`XRDS76354/SUMO-MCP-Server`, from the arXiv 2506.03548 paper —
MIT, Python, stdio, FastMCP, no tasks) gives a proven *tool taxonomy* (network
generation, demand generation, route computation, live TraCI queries, signal
optimization, workflows). We take that taxonomy and rebuild it on the official
`mcp` SDK's **lowlevel server** for the features that taxonomy leaves on the
table:

- **The task API.** Network generation, route computation, batch simulation
  runs, and signal optimization are long operations. On the stdio FastMCP
  server they block a request; on ours they are **MCP tasks** — the tool
  handler calls `ctx.experimental.run_task(work)`, returns a
  `CreateTaskResult`, and `tasks/result` blocks until terminal per spec — the
  same 2025-11-25 wire shape our gateway projects and our kernel already
  detaches/sleeps/wakes on. This is the whole point.
- **Gateway projection, auth, policy, audit.** A streamable-HTTP upstream
  projected as `sumo__*` behind the gateway with an INTERNAL token, a tenant,
  and policy checks — governed like the other six servers, not an
  unauthenticated stdio child.
- **Resource subscriptions.** Congestion, arrival, and collision conditions
  become `resources/subscribe` targets that push wakes — the showcase is what
  finally motivates closing the kernel's `resources/subscribe` gap.
- **Strong typing end to end.** pydantic tool params and typed domain ids
  (`ScenarioId`, `VehicleId`, `EdgeId`, `SignalId`) instead of stringly tools
  returning `Error:`-prefixed text.

⚠️ **Never touch `HypaSMarty/SUMO-MCP-Server`.** It is a malware lure — a
copied source tree whose README funnels to a checked-in ZIP payload it tells
you to unzip and run. It is named only so this document can say: not that one.

## The SDK version discipline (load-bearing)

Verified against the official `mcp` SDK: the task lifecycle is complete and
correct — lowlevel `Server.experimental.enable_tasks()`, a `TaskStore`
abstraction with `InMemoryTaskStore`, `run_task()` → `CreateTaskResult`, a
`tasks/result` handler that blocks on `wait_for_update` until terminal, task
status `working|input_required|completed|failed|cancelled`, `tasks/cancel`,
`tasks/list`, `TaskStatusNotification`, and streamable-HTTP transport — but it
lives under an `experimental` namespace, was **deprecated in `mcp==1.28.0`**,
and is **removed from the v2 line** because the spec pulled tasks (SEP-1686)
from core into a not-yet-shipped extension.

Consequences, handled by construction:
- **Pin `mcp==1.28.x`** (last line with working tasks); filter the
  `DeprecationWarning`. A showcase pinning a dependency is normal.
- **Lowlevel server, not FastMCP** — tasks exist only there. Good: explicit is
  the better teaching artifact.
- **Isolate the task-serving code** behind one thin internal module
  (`tasks_compat.py`) so the swap to the forthcoming tasks *extension* is
  localized — and the v1.x `TaskStore`/`TaskResultHandler` is that extension's
  own blueprint, so the concepts transfer intact.
- Our stack's wire protocol is frozen at what our Rust `mcp-contract` + gateway
  + rig fork implement; the Python server only has to match it, and 1.28.x
  speaks exactly that revision.

## Shape — one Python component

Because the MCP server is Python, it needs no separate bridge: one process
owns TraCI, pushes to the hub, and serves the governed tools.

```
┌─ sumo (official Docker) ──────────┐
│  ghcr.io/eclipse-sumo/sumo:v1_27_1│
│  headless, TraCI :8813            │
│  seeded scenario (grid+trips)     │
└───────────┬───────────────────────┘
            │ TraCI (single client, serialized stepping)
┌───────────▼─ sumo-mcp (Python, mcp lowlevel + asyncio) ─────────────┐
│  lowlevel Server over streamable HTTP → gateway → sumo__*           │
│                                                                     │
│  ┌ sim-driver task (owns TraCI, is the sim clock) ───────────────┐ │
│  │  each step: read state → push typed Rerun streams → hub       │ │
│  │             /world/sumo/{vehicle,signal,detector,ego}/**      │ │
│  │  apply queued control commands at step boundaries            │ │
│  │  evaluate watch conditions → resources/updated               │ │
│  └───────────────────────────────────────────────────────────────┘ │
│  sync tools    : query_state, describe_scenario (read cached state) │
│  TASK tools    : generate_network, compute_routes, run_batch,       │
│                  optimize_signals  (run_task → CreateTaskResult)     │
│  actuation     : set_signal_phase, reroute_vehicle, add_vehicle      │
│                  (enqueue → applied at next safe step)               │
│  resources     : sim/congestion, sim/arrival, sim/collision         │
└──────────────┬──────────────────────────────────────────────────────┘
   rerun+http   │ (push)
                ▼
        hub-spooler :9876  →  world dataset
```

**One process, one TraCI owner, one clock.** TraCI is single-client and
stepping is serialized, so exactly one owner is mandatory. An asyncio
sim-driver task holds it, *is* the simulation clock, pushes sensor streams, and
applies control commands only at step boundaries so stepping never corrupts.
Tool handlers never touch TraCI directly — synchronous tools read the driver's
latest cached state; actuation tools enqueue a command; task tools hand the
driver a unit of long work (`run_batch` fast-forwards the sim; the offline
tools shell out to `netgenerate`/`duarouter`/`tlsCoordinator`). The concurrency
model is the same serialization we would have built across the process
boundary, now in-process and simpler.

## Tools (task-native where it counts)

Synchronous (fast TraCI reads, answered inline):
- `sumo__query_state` — vehicle list, per-vehicle speed/pos/lane/route, sim
  time, signal phases. Typed result, not text.
- `sumo__describe_scenario` — loaded network bounds, edges, signals.

Tasks (long; return `CreateTaskResult`, notify on completion):
- `sumo__generate_network` — netgenerate/netconvert (grid/spider/OSM).
- `sumo__compute_routes` — randomTrips + duarouter demand → routes.
- `sumo__run_batch` — advance the sim N steps as fast as possible (libsumo
  in-process mode for speed), returning aggregate outcomes.
- `sumo__optimize_signals` — tlsCycleAdaptation / tlsCoordinator.

Actuation (fast, but state-changing — synchronous with confirmation):
- `sumo__set_signal_phase`, `sumo__reroute_vehicle`, `sumo__add_vehicle` —
  queued to the bridge, applied at the next safe step boundary, ack returned.

Resources (subscribe → push wake):
- `sim/congestion` (mean edge speed below threshold), `sim/arrival` (tracked
  vehicle reaches destination), `sim/collision` (safety event). The bridge
  evaluates each condition per step; crossings become `resources/updated`
  notifications the kernel turns into wakes.

This is the full perceive → decide → act loop against a real simulator:
SUMO streams the world into the hub → the agent reads its world model → calls
a task tool (`run_batch`, `optimize_signals`) → **detaches and sleeps** →
wakes on the task result → acts (`set_signal_phase`, `reroute_vehicle`) → a
congestion resource wakes the next episode. Every arrow is machinery that now
exists; the showcase wires it to traffic.

## Streams and sessions

Vehicles and infrastructure are pushed with the same typed component
discipline the world model uses, so SUMO data is world-model data:

- `/world/sumo/vehicle/{id}` — `GeoPoints` (SUMO geo-projection → lat/lon) +
  scalar `speed`/`accel` + `TextLog` lane/route.
- `/world/sumo/signal/{id}` — phase state over time.
- `/world/sumo/detector/{id}` — induction-loop counts.
- `/world/sumo/ego` — the vehicle the agent tracks/controls, called out.

One recording id per simulation run = one hub session = one catalog segment,
routed by application id `veoveo-sumo` into the `world` dataset — so SUMO
traffic and `sensor-sim` streams share one queryable record. A durable
property worth stating: **SUMO runs are ephemeral; the hub segment is not.**
If the simulator container dies mid-run, the world up to that instant is
already durable in the spool and answerable through the catalog — the record
outlives the simulator.

## Auth and placement

- `sumo-mcp` sits behind the gateway as a streamable-HTTP upstream with an
  INTERNAL token and a tenant, and is projected/policy-checked/audited like
  every hosted server.
- The SUMO TraCI port is **internal-network only** — never published,
  reachable only by `sumo-mcp`, consistent with the hub's placement rule.
- `sumo-mcp`'s push into the hub is the same governed-by-placement ingest as
  any other producer.

## Determinism, strong typing, performance, safety

- **Deterministic scenario.** The smoke's world is the seeded `FakeSimDriver`
  (a pure function of `(seed, step)`), so vehicle counts and positions at step N
  are exact and the smoke asserts on them — not vibes. The live world is the
  real geo-referenced LuST network (Luxembourg), cloned at image build.
- **Typed throughout.** pydantic models (`model_config = ConfigDict(extra=
  "forbid")`) for every tool param and result, `NewType`/`Annotated` domain
  ids, typed traci wrappers, and a typed emit layer mirroring the Rerun
  components — so hub data is world-model data and tool contracts are
  self-documenting. Tool results are typed `CallToolResult`s, never
  `Error:`-prefixed strings.
- **Performance.** The sim-driver's TraCI loop is the sim clock; sensor pushes
  are batched per step; `run_batch` uses libsumo in-process stepping (no TCP
  round-trip per step) for fast-forward runs. Control commands never block
  stepping — they queue and apply at step boundaries. The asyncio server and
  the sim loop share one event loop; long CPU-bound offline tools
  (`netgenerate`, `duarouter`) run in a thread executor so the sim clock and
  the HTTP server stay responsive.
- **Safety.** SUMO container is resource-limited; the scenario is bounded;
  collisions use SUMO's teleport handling; a run is a fresh session so a
  corrupt sim state can't poison history — and the hub already holds what
  happened.

## Test plan

- **Python unit (`sumo-mcp`)** — pytest with a **fake TraCI** (a typed
  in-process stub implementing the traci calls we use), so no simulator is
  needed: tool-schema validation; `run_task` lifecycle (`run_batch` returns a
  `CreateTaskResult`, polls to terminal via the SDK client's
  `poll_until_terminal`, `tasks/result` blocks then returns typed aggregates);
  `tasks/cancel` transitions; resource subscribe/notify round-trip; the
  command-queue-applied-at-step-boundary invariant; a `tasks_compat` contract
  test pinning the wire shape our gateway expects.
- **Python integration** — one short real-SUMO run (the seeded scenario)
  asserting emitted Rerun stream counts equal the scenario's ground truth and
  `query_state` matches libsumo-computed truth.
- **Client-side task test** — the SDK client's
  `experimental.call_tool_as_task` + `get_task_result` against the running
  server, proving the exact detach/poll/result path the kernel uses.
- **Smoke `showcase-sumo`** (capstone e2e, `showcase` profile) — bring up
  SUMO + hub + `sumo-mcp` behind the gateway + the Pilot agent. The
  agent issues `sumo__run_batch`, **detaches and sleeps**, and wakes on the
  task-result notification (the real-deal sleep/wake, now simulator-driven);
  assert the vehicle streams landed in the hub `world` dataset and are
  answerable through the catalog; assert a `sim/congestion` resource update
  woke a follow-up episode that called `sumo__set_signal_phase`.
- **Determinism smoke** — seeded scenario, assert exact vehicle count and ego
  position at a fixed step through `sumo__query_state`.
- **`showcase-live`** (manual recipe) — real Cloudflare Kimi, real SUMO, the
  Pilot managing traffic live, visible in the viewer via the hub tee.

## Compose & layout

```
showcase/
  README.md                     # index of showcases
  sumo/                         # the SUMO showcase (siblings live alongside)
    README.md
    compose.showcase.yaml       # `showcase` profile: sumo, sumo-mcp
    compose.interim.yaml        # `interim` profile: fake-driver runtime, no SUMO
    sim/
      Dockerfile                # FROM ghcr.io/eclipse-sumo/sumo:v1_27_1 + LuST clone
    mcp/                        # Python: lowlevel MCP server, traci owner, push
      Dockerfile                # slim python + mcp==1.28.x + traci + rerun-sdk
      pyproject.toml            # pinned deps; ruff + mypy(strict) + pytest
      src/sumo_mcp/
        server.py               # lowlevel Server, streamable HTTP, tool registry
        sim_driver.py           # asyncio TraCI owner: step, push, apply, watch
        tools.py                # pydantic-typed tool params/results
        tasks_compat.py         # thin task-serving seam (extension-swappable)
        streams.py              # typed Rerun emit layer (/world/sumo/**, map + 3D)
        resources.py            # congestion subscription
      tests/                    # pytest: fake-driver unit, no SUMO needed
    scripts/                    # capstone client + orchestration
```

- `sumo` — official image + baked scenario, headless `sumo -c … --remote-port
  8813`, internal network only.
- `sumo-mcp` — Python image, connects TraCI, pushes to
  `rerun+http://hub-spooler:9876/proxy`, serves streamable HTTP; registered as
  a gateway upstream in `gateway.bioma.json` (`sumo__*`).
- `configs/agents/pilot/` gains a traffic preamble + `sumo` migration tables
  (signals, incidents) so the Pilot can reason about the traffic domain.

## Showcase milestones (depend on hub H1+)

- **S0 — push spine.** SUMO container + seeded scenario + the `sumo-mcp`
  sim-driver pushing vehicle/signal streams into a plain proxy (or the hub),
  viewer shows live traffic. This alone is the "tiny simulator pushing to
  rerun" ask, standalone-provable before the MCP surface is wired.
- **S1 — governed server.** `sumo-mcp` lowlevel streamable-HTTP server with
  synchronous query tools + actuation tools + pydantic typing; gateway
  projection `sumo__*`; pytest unit tests against fake TraCI; `tasks_compat`
  seam in place.
- **S2 — task-native long ops.** `run_batch`/`generate_network`/
  `compute_routes`/`optimize_signals` as MCP tasks via
  `ctx.experimental.run_task`; client-side task test + `showcase-sumo` detach/
  sleep/wake smoke against real SUMO.
- **S3 — event plane.** `sim/congestion|arrival|collision` resources → agent
  wakes; closes the kernel `resources/subscribe` gap; follow-up-episode
  assertion.
- **S4 — capstone.** `showcase` compose profile, capstone + determinism
  smokes, `showcase-live` recipe, Pilot traffic preamble/migrations, docs +
  Autonomy Harness update.

## Showcase open questions

1. **Language** — resolved to Python: it is what clients build, what the SUMO
   ecosystem speaks, and (via `mcp==1.28.x`) a complete task-native
   streamable-HTTP MCP server today. The whole thing collapses to one process,
   no bridge.
2. **Tasks-API longevity** — the SDK's task support is deprecated and headed
   for an extension; handled by pinning `mcp==1.28.x` and isolating the seam in
   `tasks_compat.py`. Revisit when the tasks extension ships (its shape is the
   v1.x `TaskStore`/`TaskResultHandler`, so the port is mechanical).
3. **libsumo vs TraCI for `run_batch`** — libsumo is faster (in-process) but
   cannot attach to the remote container; batch runs may need their own
   short-lived in-process SUMO inside `sumo-mcp` rather than the shared TraCI
   session. If so, `run_batch` spins an in-process libsumo world while the
   sim-driver's live TraCI session continues — S2 spike settles it.
4. **In-process concurrency** — the asyncio server and the TraCI sim loop share
   one event loop; the loop must never be blocked by a synchronous traci call.
   The sim-driver wraps blocking traci in a thread executor (or a dedicated
   thread with an async command channel); the S1 spike confirms the pattern
   holds under load.
5. **Geo-projection fidelity** — SUMO nets may be projected or plain-XY;
   scenario is chosen/authored with a geo-projection so `/world/sumo/**`
   `GeoPoints` are real lat/lon, not net-local meters.
