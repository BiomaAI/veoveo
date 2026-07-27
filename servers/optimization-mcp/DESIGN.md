# Optimization MCP Design

This document is the canonical design and operational contract for the
`optimization-mcp` crate.

Optimization accepts compact spatial work declarations and produces governed
multi-agent plans. It expands assignment candidates inside the service, solves
the bounded model, and records complete assignment evidence. Callers do not
construct a static option matrix.

## Status

The compact spatial assignment contract is implemented. The hosted server owns
the `optimization` slug, the `optimization://` URI scheme, and the
`/optimization/mcp` endpoint.

The public planning surface is a hard cut. Static `PlanningOption` input,
`inline` and `duck_db_options` modes, and source-URI option loading are not
supported.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP. The server exposes tools, resources, resource templates, tasks, and structured results. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Generated schemas describe compact agents, groups, tasks, constraints, objectives, assignments, findings, and artifacts. Controlled fields use Rust structs and enums. |
| [Veoveo MCP server contract](../../mcp/contract/DESIGN.md) | Hosted-server contract revision 2, including gateway-signed invocation authority, canonical pagination, artifact-plane access, and usage evidence. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`. Planning always uses the shared durable task runtime. |
| `good_lp` 1.15.2 and `microlp` 0.4.0 | Pure-Rust binary linear assignment solver. The only public backend value is `micro_lp`. |
| [Rerun](https://rerun.io/) 0.35.0 RRD | Optional immutable plan evidence containing the governed plan, assignments, and summary metrics. |
| DuckDB SQL | Optional immutable analytical projection containing assignment, requirement, and governed-plan tables. |
| SHA-256 | Request and governed-plan digests. |
| UUID version 5 | Stable plan and assignment identifiers derived from governed inputs. |
| Map and Frames repository contracts | Optimization retains immutable Map releases, Map resource references, mobility-profile revisions, and one Frames world revision. It performs no coordinate conversion or route construction. |
| OAuth bearer and signed JWT identity | Gateway policy fixes principal, tenant, profile, labels, Work Context, and invocation authority before task creation. |

DuckDB and RRD are evidence projections. The mandatory canonical plan is the
typed JSON artifact and the corresponding `optimization://plan/{plan_id}`
resource.

## Responsibilities

Optimization owns:

- bounded validation of compact planning declarations;
- candidate expansion for agents and declared groups;
- capability and mobility-profile admission;
- shared and per-agent resource constraints;
- lane and resource-band assignment;
- fixed-window collision constraints;
- task dependency and mutual-exclusion constraints;
- weighted objective compilation;
- deterministic tie breaking;
- complete assignment and requirement results;
- plan provenance, digests, artifact publication, and usage records.

Map owns source features, spatial derivations, route construction, terrain,
restriction checks, projected CRS behavior, geodesics, and mobility-envelope
validation. Frames owns world revisions and coordinate conversion.

The consuming extension owns plan admission, session binding, final dynamic
validation, bounded runtime-buffer compilation, and execution.

## Non-Goals

- No physics-rate stepping or waypoint advancement.
- No controller, actuator, or collision authority.
- No route generation, CRS conversion, geodesic calculation, or terrain query.
- No dynamic feasibility claim.
- No raw LP or MILP request model.
- No provider job protocol or provider status polling.
- No client REST, gRPC, or WebSocket job surface.
- No autonomous plan execution.

## MCP Surface

The internal endpoint is:

```text
/optimization/mcp
```

The gateway exposes the local `plan` tool under the configured server
namespace, normally `optimization__plan`.

The server advertises tools, resources, resource templates, and final tasks.
Resources are not subscription-enabled.

### Tool

```text
plan(PlanRequest) -> PlanOutput
```

The tool always creates a durable task. A client that negotiates the final task
extension receives the task handle immediately. A direct call waits on the same
durable execution and returns its result.

Planning reserves artifact-plane authority before task creation. One write is
mandatory for the plan JSON. Optional DuckDB and RRD outputs reserve one
additional write each.

### Resources

```text
optimization://plans
optimization://plan/{plan_id}
optimization://artifact/{artifact_id}
optimization://usage
optimization://usage/task/{task_id}
```

`optimization://plans` lists at most 100 visible completed plans and reports
whether the result was truncated. The exact plan resource returns `PlanOutput`,
including the governed plan and every artifact identity.

Plan visibility is reconstructed from durable completed task results. A caller
must match the recorded principal, profile, tenant, data-label authority, and
Work Context. The gateway independently authorizes the resource operation
under the caller's current profile.

`optimization://artifact/{artifact_id}` is the Optimization presentation URI
for bytes in the shared artifact plane. The neutral
`artifact://{artifact_id}` identity remains the cross-server form.

## Compact Request Contract

`PlanRequest.schema_version` is `1`. Every request declares at least one
immutable Map release, one Frames world revision, one agent, and one spatial
task.

### Agents And Groups

An agent declares:

- a controlled agent identifier;
- exactly one immutable Map mobility-profile URI;
- a capability set;
- resource capacities;
- a positive maximum assignment count;
- assignment cost, risk, and confidence values.

A group declares its controlled identifier, member agents, and group
capabilities. Group candidate cost and risk are the sums of member values.
Group confidence is the least member confidence. The group consumes the
assignment capacity and overlapping time of every member.

### Spatial Tasks

A task declares:

- minimum and desired quantity;
- required or optional admission;
- agent, group, or agent-or-group assignment;
- positive priority;
- a Map source-feature or spatial-derivation target;
- a Map route, Map spatial derivation, or artifact trajectory execution
  reference;
- required capabilities and allowed mobility profiles;
- optional eligible agents and groups;
- dependencies;
- shared-resource and per-agent demand;
- allowed lanes and resource bands;
- an unscheduled or fixed time window;
- once, loop, or periodic recurrence;
- assignment cost and risk.

Every source-feature target must belong to a release identifier declared in
`source_map_releases`. References are identities only. Optimization never
dereferences them to duplicate Map or Frames computation.

### Global Policies

Requests may declare:

- shared resources with positive capacities;
- lanes with positive assignment capacities and optional Map geometry;
- resource bands with positive assignment capacities;
- mutual-exclusion sets with a maximum active-task count;
- weights for priority, cost, risk, confidence, and resource use;
- a deterministic seed;
- a maximum generated-candidate count;
- optional DuckDB and RRD evidence.

All identifiers are unique within their declared kind. Every reference must
resolve inside the request, and task dependencies must form an acyclic graph.

## Bounds

The schema and runtime enforce these service limits:

| Input | Maximum |
|---|---:|
| Agents | 512 |
| Groups | 128 |
| Tasks | 512 |
| Shared resources | 256 |
| Lanes | 256 |
| Resource bands | 256 |
| Mutual-exclusion sets | 256 |
| Source Map releases | 64 |
| Generated candidates | 50,000 |

The request may lower the candidate limit. Expansion fails before solver
construction when the compact declaration exceeds that bound.

## Candidate Expansion

The service builds individual units from agents and collective units from
declared groups. It removes units that fail capability, mobility-profile, or
explicit eligibility checks.

Each eligible unit expands across the task's allowed lane and resource-band
choices. An empty lane or band set creates one unassigned choice. Candidate
keys contain the task, unit, lane, and band identities, which gives a stable
ordering independent of map iteration order.

The deterministic seed feeds a SHA-256 tie break smaller than
`1e-9`. The tie break chooses consistently among otherwise equivalent
candidates without materially changing declared objective weights.

## Solver Model

Each generated candidate has one binary variable. Each task has one binary
active variable.

The model enforces:

- assigned quantity no greater than desired quantity;
- active tasks meeting their minimum quantity;
- required tasks remaining active;
- one lane-band variant for a task and unit;
- per-agent maximum assignments;
- per-agent resource capacities;
- no simultaneous fixed-window assignments for one agent;
- shared-resource capacities;
- lane and resource-band capacities;
- an active dependent task requiring its prerequisite to reach desired
  quantity;
- mutual-exclusion active-task limits.

The objective maximizes weighted priority and confidence while penalizing
cost, risk, and declared resource demand. `good_lp` expresses the model and
the pinned pure-Rust `microlp` backend solves it.

Solver infeasibility is a governed result, not a transport failure. Contract
validation errors fail the tool result before a model is solved.

## Governed Plan

`GovernedPlan` records:

- schema version, stable plan identifier, resource URI, and status;
- ordered complete assignments;
- complete, partial, unmet, or inactive requirement results;
- typed findings;
- exact source Map releases and Frames world revision;
- every declared mobility-profile revision;
- objective components and aggregate metrics;
- solver backend, seed, variable count, constraint count, candidate count, and
  termination;
- algorithm revision;
- request and plan SHA-256 digests;
- submitting principal, Work Context, policy revision, and time.

An assignment includes all agents in the unit, the optional group, target,
execution reference, mobility profiles, lane, resource band, timing,
recurrence, shared-resource demand, cost, risk, and confidence.

The status is:

- `optimal` when every active requirement reaches desired quantity;
- `partial` when a solved model leaves a requirement below desired quantity;
- `infeasible` when the hard model has no solution.

Findings distinguish missing eligible units, insufficient eligible units,
unsatisfied desired quantity, unsatisfied hard minima, and solver
infeasibility.

## Identity And Digest Profile

The request digest is lowercase hexadecimal SHA-256 over the UTF-8 JSON bytes
emitted by the typed `PlanRequest` serializer.

The plan identifier is UUIDv5 over:

```text
{durable_task_id}:{request_digest_sha256}
```

Assignment identifiers are UUIDv5 over the plan identifier and stable
candidate key.

The plan digest is lowercase hexadecimal SHA-256 over the typed governed-plan
JSON object with `plan_digest_sha256` omitted. Ordered Rust collections make
maps and sets deterministic. The plan identifier, authority, and submission
time are part of the governed bytes.

## Artifact Profile

The canonical artifact is always:

```text
filename    plan.json
media type  application/vnd.veoveo.optimization-plan+json
```

The optional DuckDB artifact is `plan.duckdb` with media type
`application/vnd.duckdb`. It contains:

- `plan_assignment`;
- `plan_requirement`;
- `governed_plan`.

The optional RRD artifact is `plan.rrd` with media type
`application/vnd.veoveo.rerun-rrd`. It records the canonical plan document,
summary metrics, and one ordered entity per assignment.

Artifact metadata repeats the plan identifier and URI, request and plan
digests, algorithm revision, Map releases, Frames revision, and mobility
profiles. The shared artifact plane stamps ownership and returns immutable
artifact identities. Download URLs are removed from durable task results.

## Durable Execution

The task request persists the compact input, submission time, and issued
artifact-write capability. Recovery class `resume` allows an interrupted
server process to claim and run the same task.

The runtime:

1. validates and solves in a blocking worker;
2. writes the mandatory and selected optional artifacts;
3. records actual usage in generated candidates;
4. stores the structured `PlanOutput` as the durable result.

The worker renews its lease during execution and observes cancellation before
and after artifact publication. Task ownership is stored in the installation
SurrealDB.

## Gateway And Deployment

The gateway catalog registers:

```text
slug        optimization
scheme      optimization
mount       /optimization
MCP         /optimization/mcp
scope       operator:use
```

The catalog declares the `plan` tool, task support, the
`optimization://plan/{plan_id}` and artifact resource families, and usage
resources. The Helm workload receives SurrealDB and artifact-service
configuration through installation-owned values.

No Map or Frames credentials are present because Optimization consumes exact
resource identities and does not fetch cross-server data.

## Source Layout

```text
servers/optimization-mcp/
  DESIGN.md
  AGENTS.md
  src/
    contract.rs
    planning.rs
    plan_artifacts.rs
    state.rs
    uris.rs
    bin/
      server.rs
      server/
        app_state.rs
        config.rs
        host.rs
        internal_auth.rs
        outputs.rs
        ownership.rs
        task_extension.rs
```

`contract.rs` owns wire types and bounds. `planning.rs` validates requests,
expands candidates, builds the solver model, and constructs the governed plan.
`plan_artifacts.rs` owns canonical JSON, DuckDB, and RRD encoding. The binary
modules own task orchestration, authority, artifact publication, and MCP
projection.

## Testing

Unit coverage proves:

- strong URI and identifier validation;
- stable UUIDv5 identities;
- fixed-window overlap semantics;
- internal candidate generation and deterministic selection;
- required-capacity infeasibility;
- optional partial plans;
- dependency and mutual-exclusion behavior;
- canonical JSON round trips;
- DuckDB and RRD evidence encoding;
- plan, artifact, and usage URI parsing;
- canonical MCP tool schemas.

Repository integration tests exercise the hosted task and gateway projection.
The first-party agent-kernel smoke uses the compact request profile.

## Security And Safety

The gateway remains the public authorization boundary. Optimization verifies
gateway-signed internal identity and persists task ownership. Resource reads
must match the recorded principal, profile, tenant, labels, and Work Context.
Artifact reads pass the verified caller and bearer to the shared plane.

Plans are advisory. They contain no access token, credential, arbitrary
network URL, executable content, actuator instruction, or hidden compatibility
behavior.
