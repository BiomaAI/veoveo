# Optimization MCP Design

This document describes the hosted Veoveo MCP server for high-level
optimization planning. The first implementation is intentionally in the middle:
it is not a raw LP/MILP interface, and it is not drone-specific. It models one
or many agents selecting options to complete one or more tasks under typed
constraints.

The central design choice is that Rerun `.rrd` recordings are the canonical
mission worldline: the append-only, temporal record of observations, fused
state, plans, predictions, validation, and simulation. DuckDB indexes and
summarizes that worldline, but it is not the source of truth for how the world
evolves over time.

## Status

Initial implementation exists in this workspace.

The target crate name is `veoveo-optimization-mcp`, with a concise folder name:

```text
crates/optimization-mcp
```

The hosted server slug and URI scheme are both `optimization`. The canonical
local tool name is `plan`; through the gateway it is exposed under the mounted
server namespace.

## Goals

- Expose high-level agent/task/option planning as a task-required MCP tool.
- Keep transform fusion, scenario analysis, validation, and simulation as future
  extensions over the same RRD/DuckDB artifact model.
- Keep agents lightweight by centralizing solver execution, state management,
  audit evidence, and RRD-backed temporal context.
- Preserve the current Veoveo gateway model: external clients talk to gateway
  profiles, and the gateway talks to hosted Veoveo MCP servers over internal MCP.
- Use strong Rust domain types for mission state, poses, assignments,
  constraints, solver options, plans, and artifacts.
- Make long-running compute task-based by default so clients get immediate task
  ids, progress notifications, cancellation, durable results, and resource links.
- Make each mission session a Rerun recording that agents can scrub, search,
  slice, summarize, and use as prediction context.
- Provide deterministic smoke fixtures before adding real solver complexity.

## Non-Goals

- No client-facing REST API for optimization calls.
- No public gRPC API in the first version.
- No separate WebSocket job protocol.
- No ad hoc JSON blob contract for controlled mission data.
- No second canonical world-state database competing with RRD.
- No treating Rerun as visualization-only output.
- No autonomous execution path. This server produces plans, evidence,
  confidence, and validation output for agents and operators.
- No provider status polling fallback if a future external provider is added.
  Provider-backed completion must remain webhook-only.

## Fit With Veoveo

The existing platform already has the shape this server needs:

```text
MCP client
  |
  | MCP over streamable HTTP
  v
mcp-gateway profile (/mcp/{profile})
  |-- media-mcp
  |-- optimization-mcp
```

The optimization server should be mounted internally at:

```text
/optimization/mcp
```

The gateway should expose it through profiles with a namespaced tool name:

```text
optimization__plan
```

Resource URIs remain server-owned and are not renamed by the gateway:

```text
optimization://artifact/{sha256}
optimization://usage/task/{task_id}
```

## Server Surface

The server binds one Axum listener. MCP is the client contract. HTTP routes below
the mount path are limited to protocol, health, and artifact plumbing.

```text
/optimization/mcp                 internal MCP over streamable HTTP
/optimization/healthz             ops health
/optimization/artifacts/{sha256}  immutable DuckDB or Rerun RRD bytes
```

`/optimization/mcp` requires the same gateway-signed internal identity assertion
used by `media-mcp`. External clients normally access this server only through a
gateway profile such as `/mcp/default`, `/mcp/research`, or `/mcp/ops`.

## MCP Capabilities

The first production version advertises:

- tools
- resources
- resource templates
- tasks
- notifications

Resource subscriptions, completions, and prompts are not part of v1.

## Tool Model

All compute tools should be task-required in the first version. This gives one
canonical execution model and avoids guessing which optimization will finish
inside a transport timeout.

| Tool | Purpose | Result |
|---|---|---|
| `plan` | Select task-completion options for one or many agents under typed constraints. | `PlanOutput` plus optional `optimization://artifact/{sha256}` DuckDB and Rerun RRD artifacts. |

`PlanRequest` supports two input modes:

- `inline`: typed agents, tasks, options, and constraints.
- `duck_db_options`: typed agents/tasks/constraints plus option rows loaded from
  the shared `DuckDbSource` contract.

The v1 solver uses `good_lp` with the pure-Rust `microlp` backend. Each
`PlanningOption` becomes a binary decision variable. Constraints cover task
requirements, resource limits, mutual exclusion, dependencies, explicit min/max
groups, agent max options, capability feasibility, and fixed-window no-overlap.

The server may later add read-only tools only if MCP resources cannot represent
the workflow cleanly. Discovery and state reads should default to resources.

## Canonical Resource Model

Resources are the stable nouns in the system. The canonical temporal noun is a
Rerun recording. Session, snapshot, plan, scenario, and solve resources are
typed indexes and views over recording data.

```text
optimization://sessions
```

List visible sessions for the principal.

```text
optimization://session/{session_id}
```

Session metadata, owner, retention, active recording id, current time cursor,
current snapshot id, current active plan id, and visible child resources.

```text
optimization://recording/{recording_id}
```

Canonical logical RRD worldline for a session. The resource body contains
recording metadata, application id, recording id, segment list, time ranges,
indexed timelines, entity path prefixes, and query hints. The actual `.rrd`
bytes are exposed through segment resources or artifact links.

```text
optimization://segment/{segment_id}
```

Immutable RRD segment bytes plus metadata. A long mission may have multiple
segments under one logical recording so append, retention, replication, and
random access stay manageable. Segments with the same Rerun recording id and
application id compose into the session worldline.

```text
optimization://context/{context_id}
```

Typed agent context window produced from a worldline query. Contains the source
recording id, timelines, time range, selected entity paths, component filters,
query summary, and compact typed facts suitable for planning or prediction.

```text
optimization://snapshot/{snapshot_id}
```

Typed view of the recording at a specific time or time range. Contains the
query that produced the snapshot, source recording id, time cursor, entity
paths, frames, transforms, covariance, source observations, version, and
consistency score. It is reproducible from the RRD worldline.

```text
optimization://plan/{plan_id}
```

Typed view of plan components appended to the recording. Contains assignments,
trajectories, solver summary, objective values, infeasible constraints,
confidence fields, validation links, and the recording time at which the plan
was produced.

```text
optimization://scenario/{scenario_id}
```

What-if scenario definition, base recording/time range, forked prediction
timeline, deltas, produced plans, and delta summary against the baseline.

```text
optimization://solve/{solve_id}
```

Solver job detail: backend, model kind, status, objective, bounds, timing,
termination reason, and diagnostic summary. This is not a provider status path;
it is local durable solver evidence indexed back to recording events.

```text
optimization://artifact/{sha256}
```

Large immutable bytes. This includes canonical RRD segments when addressed by
content hash, plus derived exports such as CSV diagnostics or compact plan
bundles. The recording and segment resources remain the semantic access path
for worldline data.

```text
optimization://usage/task/{task_id}
```

Usage estimate and actual compute records for the MCP task.

## RRD Worldline

Each mission session owns one logical Rerun recording. That recording is the
canonical worldline: all sensor observations, fused state, derived tracks,
operator annotations, agent hypotheses, solver inputs, solver outputs,
validation reports, and simulation rollouts are logged as time-indexed Rerun
entities and components.

Rerun's data model is a good fit for this because `.rrd` is its native recording
format and current Rerun architecture represents recording data as column
chunks with entity paths, time columns, component columns, and semantic
metadata. The server should pin a Rerun SDK version and maintain a migration
policy because `.rrd` compatibility is versioned by Rerun.

The RRD worldline should use multiple timelines:

```text
mission_time       physical or simulated event time
ingest_time        server ingestion time
solve_time         solver job phase time
prediction_time    forecast horizon for simulated or predicted state
operator_time      approval, annotation, and review time
```

The core entity path conventions should be stable:

```text
/world/frames/{frame_id}
/world/entities/{entity_id}/pose
/world/entities/{entity_id}/track
/world/entities/{entity_id}/classification
/world/observations/{observation_id}
/world/constraints/{constraint_id}
/plans/{plan_id}/assignments/{assignment_id}
/plans/{plan_id}/trajectories/{trajectory_id}
/solves/{solve_id}
/scenarios/{scenario_id}
/predictions/{prediction_id}/entities/{entity_id}
```

Typed Rust structures remain the boundary contract for tools and resource
summaries. The durable temporal payload for those structures is logged into the
RRD recording as Rerun components. DuckDB stores indexes, ownership rows, and
materialized summaries that let MCP handlers find the relevant recording
segments quickly.

## Agent Context

The agent's operational context is not a static JSON snapshot. It is a
time-addressable view over the session RRD worldline.

Agents need four canonical context operations:

- Scrub: move a time cursor across `mission_time`, inspect the world state, and
  compare it to the state known at `ingest_time`.
- Find: query entities, observations, constraints, plans, anomalies, and
  decisions by time range, entity path, component type, label, confidence, or
  solver status.
- Slice: build a compact typed context window from selected entities,
  timelines, and time ranges for a planning or prediction call.
- Predict: append forecast state on `prediction_time` without mutating the
  observed `mission_time` history.

The MCP server should expose these operations through resources first and tools
only when computation is required. For example, reading
`optimization://snapshot/{snapshot_id}` returns a typed summary of one slice,
while `simulate_execution` computes and appends a future prediction timeline.

Prediction output must be distinguishable from observation and fused state. A
forecast is logged under `/predictions/{prediction_id}/...` and linked to the
plan, scenario, source snapshot, and solver job that produced it.

The canonical query shape should be typed:

```text
WorldlineQuery
  session_id: SessionId
  recording_id: RecordingId
  timelines: Vec<TimelineName>
  time_range: TimeRange
  entity_paths: Vec<EntityPathPattern>
  component_kinds: Vec<ComponentKind>
  labels: Vec<DataLabelId>
  confidence: Option<ConfidenceRange>
  limit: Option<NonZeroU32>

ContextWindow
  context_id: ContextId
  recording_id: RecordingId
  query: WorldlineQuery
  facts: Vec<ContextFact>
  source_segments: Vec<SegmentId>
  time_coverage: Vec<TimelineCoverage>
  created_at: DateTime<Utc>
```

## Core Domain Types

The server should introduce typed ids instead of plain strings:

- `SessionId`
- `RecordingId`
- `SegmentId`
- `ContextId`
- `TimelineName`
- `EntityPath`
- `SnapshotId`
- `EntityId`
- `FrameId`
- `ObservationId`
- `PlanId`
- `AssignmentId`
- `TrajectoryId`
- `ScenarioId`
- `SolveId`
- `ConstraintId`

Controlled request shapes should be modeled with Rust structs and enums. Raw
JSON is only acceptable at genuinely open-ended extension points, such as an
opaque solver debug export or externally defined scenario metadata.

### Geometry and Registry

```text
WorldlineEvent
  recording_id: RecordingId
  timeline: TimelineName
  time: TimePoint
  entity_path: EntityPath
  component_kind: ComponentKind
  source: EventSource

Pose3
  translation_m: Vec3
  rotation: UnitQuaternion
  covariance: Option<Covariance6>

TransformEdge
  from_frame: FrameId
  to_frame: FrameId
  pose: Pose3
  observed_at: DateTime<Utc>
  source: ObservationSource
  confidence: Confidence

RegistrySnapshotView
  snapshot_id: SnapshotId
  session_id: SessionId
  recording_id: RecordingId
  time_cursor: TimeCursor
  entities: Vec<EntityState>
  frames: Vec<Frame>
  transforms: Vec<TransformEdge>
  created_at: DateTime<Utc>
  consistency: ConsistencyScore
```

Transforms are logged into the RRD worldline first. A snapshot is an immutable
materialized view over a recording id plus time cursor or time range. A session
may have a mutable current snapshot pointer, but plan results must reference the
source recording id and snapshot id for reproducibility.

### Assignment Planning

```text
OptimizeAssignmentRequest
  session_id: SessionId
  snapshot: SnapshotInput
  context_window: Option<ContextWindowSpec>
  assets: Vec<AssetState>
  targets: Vec<TargetState>
  protected_assets: Vec<ProtectedAsset>
  constraints: Vec<AssignmentConstraint>
  objective: AssignmentObjective
  solver: SolverOptions

AssignmentPlan
  plan_id: PlanId
  recording_id: RecordingId
  snapshot_id: SnapshotId
  assignments: Vec<Assignment>
  unassigned_assets: Vec<EntityId>
  unassigned_targets: Vec<EntityId>
  objective_value: f64
  risk_score: RiskScore
  solver_summary: SolverSummary
  worldline_time: TimePoint
  validation_uri: Option<String>
  recording_uri: String
```

Constraint examples should be typed enum variants:

```text
AssignmentConstraint
  MaxAssignmentsPerAsset { limit: NonZeroU32 }
  MaxTargetsPerSector { sector: SectorId, limit: NonZeroU32 }
  ResourceBudget { resource: ResourceKind, limit: f64 }
  TimeWindow { entity: EntityId, earliest: DateTime<Utc>, latest: DateTime<Utc> }
  Exclusion { asset: EntityId, target: EntityId, reason: String }
  RequiredCoverage { protected_asset: EntityId, minimum_score: f64 }
```

The schema should not use free-form strings like `"max 2 interceptors in sector
A"` as the primary contract. Natural language can be accepted only by a separate
future interpretation tool that emits typed constraints for review.

## Solver Architecture

`good_lp` should be used as the modeling layer for linear and mixed-integer
problems. The solver backend should be explicit configuration.

Recommended initial backend:

```text
highs
```

Reasons:

- Suitable for linear and mixed-integer optimization.
- Runs in-process, avoiding an external solver binary per request.
- Has parallel solver behavior, so the server must control concurrency at the
  job queue level.

Optional later backends:

- `coin_cbc` for MILP comparison or environments where it is easier to package.
- `clarabel` for convex optimization cases where it matches the model.
- A pure Rust or lightweight fallback only for deterministic test fixtures, not
  for production planning quality.

The solver module should not expose backend-specific raw request JSON. It should
take typed domain requests and compile them into solver variables, constraints,
and objectives internally.

## Job Execution

The server should keep Tokio for I/O and MCP orchestration. CPU-heavy solver and
simulation work should run through a bounded compute queue.

Recommended first version:

- `enqueue_task` records a task and persists ownership.
- A bounded `tokio::sync::Semaphore` limits concurrent solves.
- Solver execution runs via `tokio::task::spawn_blocking`.
- Each task records progress phases:
  - accepted
  - input validated
  - RRD context window resolved
  - snapshot view materialized
  - solver model built
  - solving
  - result appended to RRD
  - indexes updated
  - completed
- Cancellation aborts queued work when possible and marks in-flight work
  cancelled once control returns from the blocking solve.

Do not add `rayon` first. HiGHS already uses parallelism internally, and nested
parallel pools can create poor tail latency. Add a dedicated thread pool only
after measuring actual solver contention.

## Persistence

Use RRD first for mission world state. Use DuckDB for indexes, ownership,
runtime state, materialized summaries, usage analytics, and audit evidence.

Canonical bytes:

- RRD segments are written to the artifact repository or object store.
- Each segment is immutable and content-addressed.
- A session's logical recording is the ordered set of visible segments sharing
  the same Rerun application id and recording id.
- New observations, fused state, plans, validation results, and predictions
  append new RRD data; they do not update old RRD data in place.

DuckDB tables should index the RRD worldline:

```text
sessions
recordings
recording_segments
entity_index
component_index
timeline_index
contexts
snapshots
plans
solves
scenarios
task_owners
artifacts
usage_records
```

DuckDB rows should include enough information to answer lists and policy checks
without reading every segment:

- owner and tenant fields
- data labels
- retention timestamps
- recording id
- segment id
- content hash
- byte length
- time range by timeline
- entity path prefixes
- component kinds
- linked task id
- linked context, plan, solve, scenario, or snapshot id

Typed JSON summaries may be stored in DuckDB for fast MCP resource reads, but
they are materialized views. If a summary conflicts with the RRD segment data,
the RRD segment is canonical and the summary must be rebuilt.

## Rerun Integration

Rerun is both the canonical temporal data format and the visualization/query
substrate for this server.

Recommended first version:

- Create a Rerun recording for each mission session.
- Append all observations, fused transforms, plans, validations, and
  simulations to that recording.
- Rotate the recording into immutable `.rrd` segments for storage and
  retention.
- Store segment bytes as `optimization://segment/{segment_id}` and
  `optimization://artifact/{sha256}`.
- Return recording and time cursor metadata in every plan, snapshot, scenario,
  validation, and simulation result.

The server should treat visualization as one consumer of the recording, not the
reason the recording exists. The same `.rrd` data drives:

- human timeline scrubbing in Rerun Viewer
- agent context slicing
- search/index materialization
- simulation replay
- prediction comparison against later observations
- audit reconstruction

Optional later version:

- Add an operator-only live Rerun stream per session.
- Surface the live stream URL as session metadata when enabled.
- Keep live stream loss non-fatal as long as the server continues writing
  canonical RRD segments.

## Gateway Integration

Add an `optimization` server manifest to gateway control data:

```json
{
  "slug": "optimization",
  "uri_scheme": "optimization",
  "mount_path": "/optimization",
  "mcp_path": "/optimization/mcp",
  "upstream": {
    "transport": "streamable_http",
    "url": "http://optimization-mcp:8792/optimization/mcp",
    "security": "compose_internal_http"
  },
  "capabilities": {
    "tools": true,
    "resources": true,
    "resource_templates": true,
    "resource_subscriptions": false,
    "prompts": false,
    "completions": false,
    "tasks": true,
    "notifications": true
  },
  "tools": [
    "plan"
  ],
  "required_scopes": ["operator:use"],
  "owned_routes": [
    {
      "path": "/optimization/artifacts",
      "purpose": "artifact_bytes"
    }
  ],
  "metadata": {}
}
```

Policy should grant only the actions a profile needs:

- `tools_list`
- `tools_call`
- `resources_list`
- `resources_templates_list`
- `resources_read`
- `tasks_list`
- `tasks_get`
- `tasks_result`
- `tasks_cancel`
- `artifact_read`
- `usage_read`

Use the existing `operator:use` scope for normal agent and operator planning.
Introduce optimization-specific scopes only when there are real admin
operations.

## Crate Layout

The server should start modular. Avoid growing one large `server.rs`.

```text
crates/optimization-mcp/
  Cargo.toml
  src/
    lib.rs
    artifacts.rs
    state.rs
    uris.rs
    planning/
      plan model, DuckDBSource materialization, good_lp solve, artifacts
    bin/
      server.rs
      server/
        app_state.rs
        config.rs
        internal_auth.rs
        ownership.rs
```

Binary entrypoint responsibilities:

- parse CLI/config
- initialize telemetry
- open state
- initialize artifact repositories
- build shared `AppState`
- wire routes
- start graceful shutdown

Real behavior belongs in modules.

## Testing Strategy

All smoke behavior should be in Rust, not shell-heavy Justfile recipes.

First tests:

- Unit tests for URI parsing.
- Unit tests for typed planning validation.
- Unit tests for inline and DuckDBSource option materialization.
- Unit tests for `good_lp` selection behavior.
- Contract schema export includes optimization request/result types.
- Direct hosted MCP smoke: internal auth required, task lifecycle works.
- Gateway smoke: `optimization__plan` routes through gateway,
  task id is projected, resources read through `optimization://...`, and a
  principal without `operator:use` is denied.
- Artifact smoke: DuckDB and RRD outputs can be read only by the owning
  principal.

The first implementation uses the pure-Rust `microlp` backend so local checks do
not depend on native solver libraries.

## Security and Safety

The gateway remains the public auth boundary. The server still enforces durable
task, recording, segment, plan, snapshot, usage, and artifact ownership from
gateway-signed internal identity.

Audit records should include:

- principal
- tenant
- profile
- action
- server
- tool or resource target
- task id
- recording id, segment id, plan id, or snapshot id where applicable
- policy decision
- solver backend
- termination reason
- timing
- high-level objective and constraint counts

Audit records must not include:

- raw mission payloads
- bearer tokens
- secrets
- raw RRD or artifact bytes
- full solver models unless explicitly stored as controlled artifacts

Planning output is advisory. Execution should require an external, explicit
operator or system approval path outside this server.

## Phased Implementation

### Phase 1: Task Option Planner

- Add `optimization-mcp` crate.
- Add `PlanRequest` / `PlanOutput` contract types.
- Add typed agents, tasks, options, objectives, constraints, and DuckDB option
  row mapping.
- Implement `plan` with `good_lp` and the pure-Rust `microlp` backend.
- Emit structured output plus optional DuckDB and Rerun RRD artifacts.
- Wire Compose, Caddy, gateway server manifest, profiles, and policies.

### Phase 2: Solver Hardening

- Add configurable solver backends if native CBC/HiGHS is available.
- Add solver time limits, concurrency limits, infeasible diagnostics, and
  objective summaries.
- Keep deterministic fixture mode for tests.

### Phase 3: Transform Fusion

- Add typed registry snapshots, observation deltas, and consistency scoring.
- Implement `fuse_transforms`.
- Append fused transforms to the RRD worldline.
- Add snapshot resources, subscriptions, and rebuild-from-RRD tests.

### Phase 4: Validation and Simulation

- Implement `validate_plan`.
- Implement lightweight forward `simulate_execution`.
- Append validation reports and simulation rollouts to the recording.
- Keep prediction timeline data separate from observed mission history.

### Phase 5: RRD Query and Viewer Workflows

- Add context slicing over entity path, component kind, label, confidence, and
  time range.
- Add Rerun Viewer links or live streams for sessions where operators need
  timeline scrubbing.
- Add prediction-vs-observation comparison views.

### Phase 6: What-If Scenarios

- Implement typed scenario deltas.
- Fork a prediction timeline from a base recording slice.
- Re-run selected planning tools against that forked context.
- Return baseline-vs-scenario deltas and recording cursors.

## Open Decisions

- Exact first solver backend feature flags and packaging path for local Compose
  and production images.
- Whether trajectory planning is MILP-based in version one or starts with a
  simpler bounded waypoint validator.
- Whether plan approvals belong in this server as resources or in a separate
  control-plane service.
- RRD segment size, rotation policy, and retention policy.
- Rerun SDK/version pinning and `.rrd` migration policy.
- Exact agent context query surface for find/slice operations.
- Whether live Rerun streams are needed in addition to stored RRD segments.
- Whether `optimization` belongs in the default gateway profile or only in an
  `ops` or `research` profile initially.
