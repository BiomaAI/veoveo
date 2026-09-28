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
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Protocol version `2026-07-28`; JSON-RPC 2.0 over stateless Streamable HTTP with per-request metadata, Discover, tools, resources and templates, prompts, completions, subscriptions, ordered notifications, and structured content. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server owns separate `ui://optimization/routes.html` and `ui://optimization/models.html` applications over the same canonical resources and task-required tools. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; all five tools require durable task invocation. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Self-contained schemas generated from strong Rust request and response types through the shared MCP contract machinery. |
| [NVIDIA cuOpt](https://github.com/NVIDIA/cuOpt) | Stable release `26.08`, running from `nvidia/cuopt:26.8.0-cuda13.3-py3.14` at Linux amd64 manifest digest `sha256:81441d50797ffaf6352552d94bd14560c53df28370bd0c9bf413fd7eeebbf178`. |
| CUDA | CUDA 13.3 runtime supplied by the pinned cuOpt image. A hardware NVIDIA GPU is mandatory. |
| GNU OpenMP runtime | Ubuntu Jammy `libgomp1` `12.3.0-1ubuntu1~22.04.3` satisfies the cuDSS `libcudss_mtlayer_gomp.so.0` runtime dependency omitted by the cuOpt 26.08 image. The executor image build verifies that the threading layer has no unresolved shared libraries. |
| `veoveo.ai/optimization/v1` | Repository-owned Optimization resource and result profile. |
| `veoveo.ai/routing-problem/v1` | Repository-owned routing problem profile for service or pickup-delivery orders. |
| `veoveo.ai/convex-problem/v1` | Repository-owned continuous LP, QP, QCQP, and quadratic SOCP representation. |
| `veoveo.ai/milp-problem/v1` | Repository-owned linear MILP profile with continuous, integer, and semi-continuous variables. |
| `veoveo.ai/travel-model-artifact/v1` | Immutable Map-to-Optimization matrix exchange with location order, vehicle types, units, unavailable cells, and Map resource attestation. |
| `veoveo.ai/cuopt-executor/v1` | Private control-to-executor protocol over a Unix-domain socket. Each JSON message has an unsigned 64-bit big-endian length prefix and a configured byte bound. It is not a public contract. |
| SHA-256 and UUID version 7 | Canonical problem and solution digests use SHA-256. Problem, run, solution, and verification identities use UUIDv7-derived controlled identifiers. |
| Veoveo usage resource profile | Native UUIDv7 Task addresses built through the shared URI component profile, 100-entry pages, and version 1 URL-safe unpadded Base64 cursors as specified under Usage Reads. |
| SurrealDB 3.2.4 | Domain-owned parameterized SQL for current owner, Work Context, Task metadata and completed-result selection. The runtime dependency matches the Store driver and installed server. |
| Veoveo Optimization catalog profile | Collection-bound version 1 Base64 cursors over creation time and native UUIDv7 Task identity; concrete addresses use the shared URI component builder. |
| RFC 9562 and the Veoveo concrete URI profile | Output identities use lowercase hyphenated RFC-variant UUIDv7 values with domain prefixes. Resource constructors and parsers use the foundation's URL 2.5.8 component implementation. Profile names use bounded ASCII unreserved characters and exclude relative path segments. |
| RFC 6570 | Discovery templates use checked declarations; iri-string 0.7.14 expansion is qualified against each typed resource family. |
| Veoveo MCP server contract | Revision 3, including canonical result handoff, bounded discovery, the 8 MiB final serialized-response cap, the hosted runtime, artifact plane, platform store, documentation resources, and gateway registration. |

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
  |-- NVIDIA cuOpt 26.08
  |-- CUDA 13.3 and RMM device pool
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

## Library Features

The `contract` feature exposes the public model through `contract` with default
features disabled. It gates dependencies as well as modules, excluding the MCP
adapter, Store, TaskRuntime, Artifact client, compiler and executor. Map imports
this feature to qualify its travel-model exchange. The `runtime` feature adds
compilation, execution clients, verification, Artifact access and the domain readers.
The default `mcp` feature adds the hosted binary and transport integration.

The contract owns typed addresses for every hosted resource family. Constructors retain
domain IDs, and parsing rejects aliases, wrong parents, fragments and unsupported query
parameters. `OptimizationResource` provides the exhaustive dispatch vocabulary.
`OptimizationScope` is empty because this server declares no additional domain OAuth
scopes. Gateway operation policy and current owner, Work Context and label checks
authorize requests. The public Map travel-model reference comes from Map's library with
its isolated `contract` feature. MCP core has no Map or Optimization dependency.

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

Each successful solve returns typed `structuredContent` with one top-level
`result_uri` for its canonical `optimization://solution/{solution_id}`. Human
content contains an identity-free status and one resource link for that
solution. Problem, run, artifact, provenance, and verification detail remains
in structured content. `verify_solution` creates a report and artifact but no
new addressable domain product, so it does not fabricate `result_uri`. No
structured result embeds an ungoverned download URL.

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

Quadratic equality constraints are outside the cuOpt 26.08 profile. A
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
barrier method because cuOpt 26.08 requires barrier for quadratic objectives
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
| `optimization://problems` | First bounded page of visible immutable problem discriminators. |
| `optimization://runs` | First bounded page of visible durable-run discriminators. |
| `optimization://solutions` | First bounded page of visible completed-solution discriminators. |
| `optimization://usage` | First bounded page of visible task usage identities. |
| `optimization://docs` | Embedded server documents. |
| `optimization://contract` | Machine-readable revision-3 compliance declaration and capability inventory. |

Resource templates provide:

```text
optimization://profile/{profile_id}
optimization://problems{?cursor}
optimization://problem/{problem_id}
optimization://runs{?cursor}
optimization://run/{run_id}
optimization://run/{run_id}/incumbents
optimization://solutions{?cursor}
optimization://solution/{solution_id}
optimization://solution/{solution_id}/routes
optimization://solution/{solution_id}/variables
optimization://solution/{solution_id}/verification
optimization://artifact/{artifact_id}
optimization://usage{?cursor}
optimization://usage/task/{task_id}
optimization://docs/{doc_id}
```

Problem, run, and solution identities are deliberately disjoint. A problem
states what was solved. A run records when, where, and under which policy it
was solved. A solution records the returned decision and verification. This
prevents retries or alternative profiles from overwriting decision evidence.

`resources/list` advertises stable roots and templates. It does not enumerate
dynamic problems, runs, solutions, or usage records. Their resource reads
return at most 100 compact discriminators in stable creation and task order,
with a versioned opaque cursor bound to the selected collection. Exact problem,
run, and solution reads use indexed domain identities under the full caller
authority and never load the current collection. Resource reads repeat
authorization. Usage reads return the same not-found response for absent and inaccessible Tasks.

### Catalog Selection And Recovery

`OptimizationReads` owns problem, run and solution queries in the runtime library.
Its caller passes a `TaskOwner`, typed domain identities and a collection address.
SQL compares indexed owner, profile and tenant with their owner-envelope values,
preserves absent versus explicit installation tenants, and checks the indexed Work
Context against both retained authority representations. Labels and domain Task kinds
are selected before limits and completion grouping. Solution queries additionally
require a successful Task and a non-error structured result.

Selected rows must decode as the declared solve family. A completed result must agree
with its problem and run parents. Invalid visible records fail the read; adapters do
not silently remove them from a page. Completion returns typed domain IDs and reports
malformed selected identities. Missing and denied exact identities both return no row.

`OptimizationIndexCursor` preserves the version 1 fields `version`, `collection`,
`created_at` and `task_id`, including emitted Base64 bytes. It requires a native RFC
UUIDv7 identity. `OptimizationCollectionUri` binds that cursor to its collection,
uses the shared component builder and rejects aliases, fragments and extra query
parameters. A saved position grants no access; each continuation rechecks SQL policy.

Replace all control replicas together after active solves settle. Before the upgrade,
inspect retained Tasks for disagreements among indexed ownership, envelope ownership,
Work Context fields, Task kind, family and output parents. Preserve rejected records
for operator review; the reader performs no conversion or deletion. The wire profiles
and solver protocol are unchanged. A rollback restores the prior control/executor
pair against the same Store and its earlier read policy; require operator acceptance
of that policy difference before reopening traffic. Installation acceptance must
exercise retained records, permission changes and reverse/forward replacement.

`tests/reads.rs` uses separate Store connections and disposable fixtures for catalog
continuations, exact lookup, completion, optional tenants, changed clearance and
malformed denied records. `tests/index_contract.rs` qualifies cursor wire preservation,
typed construction and URI rejection. These are database and contract checks; GPU
solver and installed acceptance remain separate requirements.

### Resource Admission And Setup

`McpServerSetup<OptimizationContract>` checks the server identity, documents, typed
resource descriptors and RFC 6570 templates before startup opens the listener. Discovery
uses that checked inventory, preserving the App metadata and fixed solver profiles.
Handlers parse a concrete address once and dispatch through `OptimizationResource`.
Problem and solution loaders retain typed addresses through SQL selection and Artifact
access. Verification locates its prepared problem through the same SQL reader.

Problem, run, solution and verification IDs require their own prefix followed by a
lowercase hyphenated RFC-variant UUIDv7. Solver profile IDs contain 1–128 ASCII letters,
digits, hyphens, underscores or dots, with `.` and `..` excluded. The installed profiles
use this spelling. URI builders preserve their published strings, and public schemas
keep string representations. The parser rejects escaped aliases and undeclared children.

Use the coordinated control/executor replacement described above. Preflight retained
Task inputs, results, prepared problems and solution Artifacts for output IDs and URI
parents that fail this profile. Compare them with the domain identities, including the
run and problem referenced by each solution. Preserve rejected bytes for review; readers
do not normalize, rewrite, or delete them. Rollback restores the prior pair against the
same stored bytes. Native URI, schema, setup and database checks qualify admission;
installed retained-data recovery, reverse/forward replacement and GPU acceptance are
separate gates.

## Prompts, Completions, And Notifications

Each replica feeds its resource hub from committed Store task and domain-usage
changes. The shared LIVE source replays changefeeds after a reconnect and invalidates
the visible resource set before resuming. Reads recheck the caller's authority.
The observer stops with the server's cancellation token.

The server provides three prompts:

- `formulate_routing_problem`;
- `compare_route_scenarios`;
- `formulate_mathematical_model`.

They direct agents to stable IDs, explicit units, the correct problem family,
the Map travel-model boundary, bounded profiles, durable task invocation, and
verification.

Completions discover visible profile, problem, run, and solution identifiers.
Dynamic completion search is authorization-scoped and limited to 101 distinct
store values, which yields at most 100 results plus an exact `hasMore` signal.
Subscriptions apply to the mutable problem, run, and solution collection
resources. Individual problem, run, and solution snapshots are immutable and
not subscribable. A completed task emits resource-list-change and
subscribed-resource notifications in protocol order through the owning
session.

The shared transport rejects a final serialized JSON response larger than
8 MiB and sends the canonical response-budget diagnostic without a partial
result.

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

Model and travel-manifest inputs use the Artifact owner's
[`ArtifactUri`](../../platform/artifacts/contract/DESIGN.md#wire-and-construction)
through resolution. The owner validates occurrence identity and URI components;
Optimization owns model validation and current caller access. Public fields serialize
as strings under the Artifact contract's declared compatibility profile.

Usage records capture measured solve work against the durable task.

## Usage Reads

`OptimizationUsage` binds reads to the Optimization Task runtime and selects
`TaskUsageAccess::WorkContext`. SQL checks principal, profile, optional tenant
spelling and the complete label clearance. Both usage and Task records must match
the runtime's server and tenant. The Task's indexed Work Context, retained authority
context and owner-envelope context must match the caller; the authority tenant must
also agree. Selection precedes grouping, ordering and the 101-ID lookahead for a
100-entry page. Exact reads use the same predicates in one query. Missing parents
and conflicting stored identities grant no access. Rust decodes only selected rows.

The isolated contract owns `OptimizationTaskUsageUri`, `OptimizationUsageIndexUri`,
`OptimizationUsageCursor`, entries and pages. Builders require native UUIDv7 Task
identities and use the foundational [URI component profile](../../platform/types/DESIGN.md).
Parsers reject aliases, fragments and duplicate or unsupported query parameters.
Entries derive their Task identity from the typed URI. Pages require ascending unique
IDs, a limit of 100, and a continuation matching the last entry of a full page.

The cursor preserves the published version 1 JSON envelope
`{"version":1,"task_id":"<uuid>"}` encoded as URL-safe unpadded Base64.
It grants no access: every read checks the current caller and stored authority.
The response fields are `usage`, `limit` and optional `next_cursor`; terminal pages
omit the cursor. Exact usage report fields are unchanged.

### Usage Deployment And Qualification

Replace all Optimization control replicas together when adopting SQL-filtered pages.
Old replicas can return short nonterminal pages that the checked consumer rejects.
Pause new solves and let active solver Tasks settle before replacing the control
container and its executor pair. This transition changes no retained rows or solver
protocol. Emitted addresses and cursor bytes remain valid; callers must refresh
hand-written noncanonical addresses. Rollback uses the previous service/client pair
with the same Store and restores its earlier pagination behavior. Installed acceptance
must traverse multiple pages behind denied rows and check direct reads under changed
labels and Work Contexts before the transition is accepted.

`tests/usage.rs` exercises the library reader through separate connections to the
isolated pinned Store. TaskRuntime's native suite owns the shared owner/context and
retained-metadata matrix. `tests/usage_contract.rs` qualifies wire forms, URI admission,
page relationships and typed construction. `tests/contract_schema.rs` preserves eleven
public solver schemas. An independent contract consumer and a runtime-only build
qualify dependency gates. These checks do not execute the GPU solver.

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
| `src/contract/` | Public problem, profile, solution, verification, IDs, typed addresses, resource variants and discovery URI declarations. |
| `src/usage.rs` | Owner and Work Context usage reads through TaskRuntime SQL. |
| `src/compiler/` | Deterministic routing and sparse mathematical compilation. |
| `src/verification/` | Independent route and mathematical checks. |
| `src/executor/` | Private protocol types and bounded Unix-socket client. |
| `src/problem_store.rs` | Digest-verified prepared-problem staging. |
| `src/profiles.rs` | Curated immutable solver profiles. |
| `src/solution_builder.rs` | Typed solution construction, provenance, digest, and initial verification. |
| `src/bin/server/` | Thin HTTP/MCP wiring, tasks, identity, artifacts, resources, prompts, and output publication. |
| `src/bin/server/setup.rs` | Checked MCP setup, documents, fixed resource descriptors and templates. |
| `src/reads.rs` | SQL-scoped exact lookup, collection pages, completion and retained-result consistency. |
| `src/task_records.rs` | Retained solve and verification Task requests shared by readers and the MCP Task adapter. |
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

Contract revision: 3.

C09 has remaining DTO relationship admission work.
Checked MCP setup is implemented; installed readiness qualification for C31 is pending. The knowledge-source extension
(C32) is planned. C06 is satisfied by the single canonical surface. The gateway
registration states revision 3 and the cuOpt 26.08 engine. Documentation and
contract resources are embedded at build time and served through MCP and the
canonical administrative mount.
