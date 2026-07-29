# Optimization MCP Design

This document is the canonical design and operational contract for the
`optimization-mcp` crate.

Optimization owns bounded decision models and verified solver results. Its
public contract follows NVIDIA cuOpt's two strongest domains: vehicle routing
and GPU mathematical optimization. Agents submit typed routing, route-scenario,
continuous convex, or mixed-integer linear problems. Every invocation becomes
a durable MCP task, and every completed solve publishes separate immutable
problem, run, and solution identities.

## Status

Implemented in this workspace.

The canonical service identity is:

```text
crate       veoveo-optimization-mcp
folder      servers/optimization-mcp
slug        optimization
URI scheme  optimization
MCP         /optimization/mcp
health      /optimization/healthz
```

Gateway-mounted tools use the `optimization__` prefix. Resource identities
retain the `optimization://` scheme.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Protocol version `2025-11-25`; JSON-RPC 2.0 over sessionful Streamable HTTP with tools, resources and templates, prompts, completions, subscriptions, ordered notifications, and structured content. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; all five tools require durable task invocation. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Self-contained schemas generated from strong Rust request and response types through the shared MCP contract machinery. |
| [NVIDIA cuOpt](https://github.com/NVIDIA/cuOpt) | Stable release `26.06`, running from `nvidia/cuopt:26.6.0-cuda13.2-py3.14` at manifest-list digest `sha256:0faac7182b32f5be747e30f081427e80e8dfeccd8f49613814d3d217167fe1ad`. |
| CUDA | CUDA 13.2 runtime supplied by the pinned cuOpt image. A hardware NVIDIA GPU is mandatory. |
| `veoveo.io/optimization/v1` | Repository-owned Optimization resource and result profile. |
| `veoveo.io/routing-problem/v1` | Repository-owned routing problem profile for service or pickup-delivery orders. |
| `veoveo.io/convex-problem/v1` | Repository-owned continuous LP, QP, QCQP, and quadratic SOCP representation. |
| `veoveo.io/milp-problem/v1` | Repository-owned linear MILP profile with continuous, integer, and semi-continuous variables. |
| `veoveo.io/travel-model-artifact/v1` | Immutable Map-to-Optimization matrix exchange with location order, vehicle types, units, unavailable cells, and Map resource attestation. |
| `veoveo.io/cuopt-executor/v1` | Private control-to-executor protocol over a Unix-domain socket. Each JSON message has an unsigned 64-bit big-endian length prefix and a configured byte bound. It is not a public contract. |
| SHA-256 and UUID version 7 | Canonical problem and solution digests use SHA-256. Problem, run, solution, and verification identities use UUIDv7-derived controlled identifiers. |
| Veoveo MCP server contract | Revision 2, including the canonical hosted runtime, artifact plane, platform store, documentation resources, and gateway registration. |

## Design Position

The server is a decision engine, not a general-purpose modeling language and
not an execution controller.

Optimization owns:

- routing order, fleet, objective, policy, problem, run, solution, and
  verification types;
- deterministic validation and compilation into cuOpt-native dense routing
  matrices and sparse mathematical structures;
- durable solve lifecycle, cancellation, provenance, usage, and immutable
  evidence;
- independent feasibility and objective verification after cuOpt returns.

Map owns locations, mobility profiles, immutable releases, travel feasibility,
route-cost construction, and travel-model artifacts. The artifact plane owns
bytes and access control. SurrealDB owns durable task, ownership, and usage
records. The gateway owns external authentication, profile exposure, policy,
and the signed internal identity presented to this server.

The server never actuates a route or mathematical decision. A verified
solution is advisory input to a separately authorized operational workflow.

## Architecture

```text
agent
  |
  | MCP Streamable HTTP
  v
mcp-gateway
  |
  | signed invocation identity
  v
optimization-mcp control container
  |-- typed MCP contract and durable tasks
  |-- problem materialization and deterministic compilation
  |-- independent solution verification
  |-- SurrealDB task and usage state
  |-- shared artifact plane
  |
  | private length-prefixed JSON over shared Unix socket
  v
cuOpt executor sidecar
  |-- NVIDIA cuOpt 26.06
  |-- CUDA 13.2 and RMM device pool
  `-- one required NVIDIA GPU
```

The Rust control container never imports cuOpt and never needs a GPU device.
The Python executor is the only container with `nvidia.com/gpu: 1`. This split
keeps public protocol, identity, artifact, and verification logic in the
strongly typed control plane while cuOpt runs in its supported Python and CUDA
environment.

The socket transport avoids an additional network endpoint and keeps
solver-private compiled structures out of the public MCP contract. The
configured frame limit defaults to 256 MiB. Prepared problems are staged under
the Optimization workspace with a recorded byte length and SHA-256 digest.

## Public MCP Contract

The server exposes five tools. They remain separate because each one has
different formulation rules, solver semantics, result shapes, and agent
guidance.

| Tool | Purpose | Canonical output |
|---|---|---|
| `optimize_routes` | Solve one heterogeneous vehicle-routing problem. | One routing solution with case summary, vehicle routes, verification, and optional CSV route table. |
| `optimize_route_scenarios` | Submit two through 64 independent routing cases to cuOpt BatchSolve. | Case-addressed summaries and routes under one problem, run, and solution identity. |
| `solve_convex` | Solve a continuous LP, QP, QCQP, or quadratic representation of an SOCP. | Variable values, constraint activities, quality metrics, and independent verification. |
| `solve_milp` | Solve a linear mixed-integer model. | Variable values, constraint activities, bound and gap metrics, optional incumbents, and independent verification. |
| `verify_solution` | Re-run server-owned checks with caller-selected finite tolerances. | A new verification report and immutable verification artifact for an existing solution. |

All tools declare task support as `required`. A direct non-task call is
rejected with instructions to use the MCP Task API. Task subscriptions carry
progress and terminal wakeups; agents do not poll the solver.

Tool outputs contain short text for conversation, typed `structuredContent`,
resource links for the problem, run, and solution, and artifact links for
canonical evidence. The structured result never embeds an ungoverned download
URL.

## Problem Sources

Each solver family accepts bounded inline JSON. It also accepts an immutable
`optimization://problem/{problem_id}` of the same family or a governed
`artifact://` JSON model. The only mathematical artifact format is
`optimization_json_v1`.

Routing has one additional boundary. Its travel model can be:

- inline dense matrices;
- an immutable `artifact://` travel-model manifest;
- a `map://travel-model/{travel_model_id}` paired with its exact
  `artifact://` manifest.

The Map form is attested. Optimization rejects it unless the artifact declares
the requested Map resource URI. Materialization preserves Map's location
order, vehicle-type identities, objective costs, transit times, and
unavailable cells before the public problem is normalized and hashed.

Problem resources are immutable snapshots. Reusing one creates a new run and
solution identity without mutating the source problem.

## Routing Contract

A routing problem contains:

- stable location, order, vehicle, vehicle-type, and capacity-dimension IDs;
- an absolute UTC time origin and a second or minute unit;
- service orders or pickup-delivery pairs;
- mandatory or optional service with positive drop penalties for optional
  orders;
- heterogeneous vehicle depots, availability windows, breaks, capacities,
  fixed costs, maximum cost, and maximum time;
- order-to-vehicle restrictions;
- separate cost and transit-time matrices per vehicle type;
- unavailable travel arcs;
- minimum or exact vehicle counts;
- weighted cost, travel-time, route-size variance, service-time variance,
  prize, and fixed-vehicle-cost objectives;
- an optional immutable routing solution as a warm start.

One problem uses service orders or pickup-delivery orders, never both.
Pickup and delivery demand signs are derived by the compiler. Client input
uses non-negative demand quantities.

The compiler turns controlled IDs into stable cuOpt indices, checks every
cross-reference, converts route restrictions into cuOpt arrays, and rejects
numeric narrowing that cannot be represented safely. Duplicate objective
metrics and zero-weight objective sets are invalid.

Route scenarios are complete independent problems. A scenario batch contains
two through 64 uniquely identified cases and one solver policy. BatchSolve is
the reason for a dedicated tool: the executor can keep the comparison on the
GPU path while the result preserves an exact case identity.

## Mathematical Contract

Mathematical variables and constraints use stable controlled IDs. Coefficients
are finite `f64` values. Omitted variable or constraint bounds represent
negative or positive infinity as appropriate.

`solve_convex` accepts continuous variables only:

| Declared kind | Required structure |
|---|---|
| `linear_program` | Linear objective and linear constraints. |
| `quadratic_program` | Quadratic objective and no quadratic constraints. |
| `quadratically_constrained_program` | One or more quadratic constraints. |
| `second_order_cone_program` | A quadratic-constraint representation accepted by cuOpt. |

The server validates the declared shape, references, dimensions, bounds, and
finite coefficients. It does not prove that an arbitrary submitted quadratic
matrix is positive semidefinite or that a quadratic constraint is convex.
The formulator remains responsible for the declared convexity. cuOpt
termination and the independent feasibility report do not constitute a proof
of global optimality for a misdeclared non-convex model.

Quadratic equality constraints are outside the cuOpt 26.06 profile. A
two-sided quadratic inequality is compiled into separate lower and upper
constraints.

`solve_milp` accepts a linear objective and linear constraints. At least one
variable must be integer or semi-continuous. The request can provide an inline
MIP start or reference a prior mathematical solution whose variable IDs match.
An output policy controls retained warm-start values and incumbent history.

The compiler merges duplicate terms into deterministic sparse rows. Linear
constraint matrices and quadratic objectives use CSR. The controlled model is
bounded at 16,384 nonzero input terms. Artifact input follows the same bound;
it changes transport and governance, not the accepted mathematical profile.

## Solver Profiles

Clients select an immutable `optimization://profile/{profile_id}`. A request
may shorten the deadline. It cannot exceed the selected profile maximum.
MILP requests may also tighten the relative or absolute gap.

| Profile | Maximum | Routing default | Convex default | MILP default | Convex tolerance | MILP relative gap |
|---|---:|---:|---:|---:|---:|---:|
| `interactive` | 30 s | 5 s | 5 s | 10 s | `1e-4` | `0.05` |
| `balanced` | 300 s | 30 s | 30 s | 60 s | `1e-6` | `0.01` |
| `thorough` | 3,600 s | 300 s | 300 s | 300 s | `1e-8` | `0.001` |

Linear programs use PDLP with presolve. QP, QCQP, and SOCP forms use cuOpt's
barrier method because cuOpt 26.06 requires barrier for quadratic objectives
and constraints. MILP uses presolve and an integrality tolerance of `1e-5`.
Balanced and thorough retain MILP incumbents by default. The profile resource
is the authority; this table documents the current compiled values.

## Durable Execution

Submission performs the following bounded sequence:

1. Verify gateway identity, Work Context, labels, profile, and artifact
   authority.
2. Materialize the problem source and any Map travel model.
3. Validate and compile the public model.
4. Create immutable problem and run IDs, stage prepared bytes, and record their
   length and SHA-256 digest.
5. Enqueue the shared final-extension task.
6. Send the compiled operation and profile to the executor.
7. Rebuild a typed solution from cuOpt output and independently verify it.
8. Publish canonical artifacts, task result, run provenance, usage, and
   resource notifications.

The executor handles one solve at a time. The shared task runtime owns queueing.
Cancellation sends a private cancel operation for the active run. cuOpt does
not expose one uniform safe interruption mechanism across every solver, so the
executor terminates itself when it must cancel an active solve. Kubernetes
restarts the sidecar, and the startup probe requires a fresh CUDA health check
before further work.

Provider-style status polling does not exist. Task state changes come from the
owned runtime and executor call.

## Independent Verification

Solver output is never accepted only because cuOpt labels it feasible.

Routing verification checks:

- route endpoints, vehicle identity, duplicate routes, and node identity;
- mandatory service, pickup-delivery completeness and precedence;
- order-to-vehicle restrictions;
- order and vehicle time windows;
- capacity trajectories;
- maximum vehicle cost and time;
- unavailable travel arcs and arrival sequence consistency.

Mathematical verification checks:

- missing, duplicate, and unknown variables;
- variable bounds and MILP integrality;
- linear and quadratic constraint activities;
- the objective recalculated from canonical problem terms;
- maximum constraint, integrality, and bound violations.

The initial report uses server-owned finite absolute and relative tolerances.
`verify_solution` creates a fresh report under caller-selected non-negative
tolerances. Verification establishes consistency and feasibility against the
published problem. It does not independently reproduce cuOpt's optimality
proof or certify model convexity.

## Resources

The stable roots are:

| Resource | Meaning |
|---|---|
| `optimization://capabilities` | Live GPU identity, cuOpt version and digest, supported families, limits, and verification inventory. |
| `optimization://profiles` | Solver profile catalog. |
| `optimization://problems` | Visible immutable problem records. |
| `optimization://runs` | Visible durable execution records. |
| `optimization://solutions` | Visible completed solution records. |
| `optimization://usage` | Visible task usage index. |
| `optimization://docs` | Embedded server documents. |
| `optimization://contract` | Machine-readable revision-2 compliance declaration and capability inventory. |

Resource templates provide:

```text
optimization://profile/{profile_id}
optimization://problem/{problem_id}
optimization://run/{run_id}
optimization://run/{run_id}/incumbents
optimization://solution/{solution_id}
optimization://solution/{solution_id}/routes
optimization://solution/{solution_id}/variables
optimization://solution/{solution_id}/verification
optimization://artifact/{artifact_id}
optimization://usage/task/{task_id}
optimization://docs/{doc_id}
```

Problem, run, and solution identities are deliberately disjoint. A problem
states what was solved. A run records when, where, and under which policy it
was solved. A solution records the returned decision and verification. This
prevents retries or alternative profiles from overwriting decision evidence.

Visible task-backed lists are capped at 100 entries per response. MCP list
calls use cursor pagination. Resource reads repeat authorization and never
turn a denial into a missing object.

## Prompts, Completions, And Notifications

The server provides three prompts:

- `formulate_routing_problem`;
- `compare_route_scenarios`;
- `formulate_mathematical_model`.

They direct agents to stable IDs, explicit units, the correct problem family,
the Map travel-model boundary, bounded profiles, durable task invocation, and
verification.

Completions discover visible profile, problem, run, and solution identifiers.
Subscriptions apply to the mutable problem, run, and solution collection
resources. Individual problem, run, and solution snapshots are immutable and
not subscribable. A completed task emits resource-list-change and
subscribed-resource notifications in protocol order through the owning
session.

## Artifacts And Usage

Every solve publishes canonical problem JSON and canonical solution JSON.
Optional artifacts are:

| Family | Optional artifact |
|---|---|
| Routing | CSV route table. |
| Convex | JSON warm-start variable values. |
| MILP | JSON warm-start variable values and JSON incumbent history. |
| Verification | JSON verification report. |

Artifact writes use a task-bound capability issued before execution. The
artifact plane stamps tenant and owner from the forwarded identity. Data
labels and Work Context accompany each write. Returned metadata omits download
URLs and uses the `optimization://artifact/{artifact_id}` presentation.

Usage records capture measured solve work against the durable task. They are
read through canonical usage resources and the same owner visibility rules.

## Identity, Visibility, And Storage

Every public request requires a gateway-signed internal assertion scoped to
the Optimization server. The server records principal, profile, tenant,
labels, Work Context, invocation authority, and policy revision with the task.
Problem, run, solution, artifact, and usage reads must match that authority.

SurrealDB holds durable task and usage metadata. The shared artifact plane
holds immutable bytes. The Optimization workspace holds digest-addressed
prepared problem staging needed by durable tasks. It is not an alternate
control database and exposes no byte route.

The default bounds are:

| Boundary | Limit |
|---|---:|
| Routing scenario cases | 64 |
| Inline dense travel matrix | 16,384 cells |
| Inline mathematical terms | 16,384 |
| Routing objectives | 6 |
| Capacity dimensions | 64 |
| Prepared problem | 256 MiB |
| Executor request or response frame | 256 MiB |
| Resolved or published artifact | 512 MiB |

Controlled client IDs are at most 128 ASCII alphanumeric or `-_.:` characters.
Locations, orders, and vehicles are additionally bounded by their cuOpt index
representations.

## GPU Executor

The executor initializes CUDA before opening its socket. Startup fails unless
CuPy can select a hardware device, allocate device memory, and report the
expected cuOpt version. It creates a one-GiB RMM pool by default. Health checks
repeat a CUDA device and memory probe; a lost device returns
`gpu_unavailable`.

The executor supports:

- direct cuOpt routing `Solve`;
- direct cuOpt routing `BatchSolve`;
- low-level cuOpt `DataModel` construction for LP, QP, QCQP, and MILP;
- PDLP or barrier selection for continuous models;
- MIP callbacks for retained incumbents.

There is no CPU solver, software CUDA path, optional GPU mode, or degraded
acceptance profile. A missing socket, wrong protocol version, mismatched run
ID, malformed frame, version mismatch, CUDA failure, or lost GPU fails the
request closed.

## Deployment

Helm deploys one `optimization-mcp` Pod with `runtimeClassName: nvidia` and a
non-overlapping `Recreate` strategy. The Pod contains:

- one Rust control container with CPU and memory resources but no GPU request;
- one cuOpt executor sidecar requesting and limiting exactly one
  `nvidia.com/gpu`;
- a shared `emptyDir` for the Unix socket, staging, and CUDA caches;
- an 8 GiB memory-backed `/dev/shm`;
- a 20 GiB `ReadWriteOnce` Optimization workspace;
- executor startup and liveness probes that perform the CUDA health request.

The image build graph and offline lock include the exact executor image.
Production accepts only a saved, versioned image whose cuOpt base digest
matches the compiled provenance constant.

## Source Layout

| Path | Responsibility |
|---|---|
| `src/domain/` | Public problem, profile, solution, verification, ID, and URI-adjacent types. |
| `src/compiler/` | Deterministic routing and sparse mathematical compilation. |
| `src/verification/` | Independent route and mathematical checks. |
| `src/executor/` | Private protocol types and bounded Unix-socket client. |
| `src/problem_store.rs` | Digest-verified prepared-problem staging. |
| `src/profiles.rs` | Curated immutable solver profiles. |
| `src/solution_builder.rs` | Typed solution construction, provenance, digest, and initial verification. |
| `src/bin/server/` | Thin HTTP/MCP wiring, tasks, identity, artifacts, resources, prompts, and output publication. |
| `executor/veoveo_cuopt_executor/` | Python cuOpt GPU adapter. |
| `tests/cuopt_gpu.rs` | Ignored hardware-GPU acceptance test. |

## Verification And Acceptance

The ordinary Rust suite covers schemas, domain validation, compilation,
independent verification, private protocol framing, resources, prompts, task
behavior, artifacts, and control-server startup checks. Python unit tests cover
framing, health, GPU failure mapping, and executor dispatch.

The ignored `cuopt_gpu` test is acceptance evidence only when run against the
pinned executor image on an NVIDIA GPU. It performs a health request and real
routing, convex LP, and MILP solves through the Rust client. A software solver
or mocked CUDA result cannot satisfy this test.

## Contract Compliance

Contract revision: 2.

All mandatory checks C01 through C30 are met. There are no compatibility
projections, so C06 is satisfied by the single canonical surface. The gateway
registration states revision 2 and the cuOpt 26.06 engine. Documentation and
contract resources are embedded at build time and served through MCP and the
canonical administrative mount.
