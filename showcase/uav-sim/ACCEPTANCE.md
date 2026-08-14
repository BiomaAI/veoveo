# UAV Showcase Acceptance

This runbook defines evidence-bearing acceptance cases for the deployed UAV showcase.
The cases exercise the public product surfaces and the owning MCP servers without moving
UAV concepts into the agent kernel, gateway, Console, or shared conversation contract.

## Test Catalog

| Test ID | Name | Classification | Purpose |
|---|---|---|---|
| `UAV-E2E-001` | Per-Agent Named-Location Mission E2E | Deployed system end-to-end acceptance | Prove that one operator can prompt one pilot, Map MCP can resolve and route the named destination, UAV Simulation MCP can enforce the pilot-to-vehicle grant and execute the mission, and the durable result can be read through both Console and headless interfaces. |

`UAV-E2E-001` is a functional acceptance test. It is not a unit test, component
integration test, performance benchmark, soak test, or live-camera acceptance test. It
does not apply the 85 ms source-to-render gate. A run may record camera observations,
but those observations do not qualify the visual data plane.

## UAV-E2E-001: Per-Agent Named-Location Mission E2E

### System Boundary

The test crosses these deployed boundaries:

```text
authenticated operator
  -> Console or headless agent-message API
  -> durable agent wake and pilot episode
  -> Map MCP place resolution and route task
  -> Map route handoff
  -> UAV Simulation MCP mission admission
  -> principal-to-vehicle grant and exclusive command lease
  -> PX4-backed simulator execution
  -> durable task-result wake and conversation projection
  -> Console UI and headless conversation API
```

Map MCP owns place, route, mobility-profile, and route-handoff data. UAV Simulation MCP
owns control grants, mission admission, command leases, execution, and vehicle telemetry.
The agent runtime owns durable episodes and task resumption. The Console and gateway
expose generic agent-message and conversation contracts.

### Prerequisites

Record every prerequisite in the evidence before sending the instruction:

- the source revision and GitOps configuration revision are known;
- the `uav-1-pilot`, Map MCP, UAV Simulation MCP, gateway, Console, and simulator
  workloads are ready;
- the simulator reports NVIDIA GPU execution, PX4 connectivity, and an authoritative
  Frames world revision;
- a headed signed-in browser is available for the Console portion, with hardware-backed
  WebGPU or WebGL proven according to repository policy;
- the authenticated `uav-1-pilot` principal has one active grant for session
  `uav-showcase`, vehicle `uav-1`, and an exact Map mobility-profile version;
- no unreleased `uav-1` command lease protects an executing mission;
- the selected named destination exists in the installed Map catalog and differs enough
  from the current vehicle position to produce an observable route.

Do not clear authentication state, copy a Console token, edit a grant, release a lease,
or change simulator state to make the test pass. Diagnose a failed prerequisite through
the normal product and administrative contracts.

### Canonical Instruction

Submit this message to the exact agent id `uav-1-pilot`, replacing
`<named-location>` with a place from the installed Map catalog:

```text
Fly uav-1 to <named-location> now. Read your active UAV control grant, ask Map MCP to
resolve and route this named location from current telemetry, then use UAV MCP to admit
and execute the mission only for your bound vehicle. Report the terminal result.
```

Times Square and the Statue of Liberty form a useful pair in the New York showcase. Pick
the destination that is not the vehicle's current location.

### Console Procedure

1. Open the signed-in Console Agents page and locate `uav-1-pilot`.
2. Record the current position, pilot state, active deployment revision, and wall-clock
   time.
3. Submit the canonical instruction through **Message this agent**.
4. Record the returned request id and wake id. The operator entry must appear as
   `accepted` with the authenticated human actor, never as the pilot principal.
5. Observe the conversation until the pilot reports that it dispatched the Map route
   task, then the UAV mission task. The pilot may finish those episodes while each
   durable task continues in the background.
6. Wait for the terminal task-result episode. Refreshing the conversation is permitted;
   resubmitting the instruction is not.
7. Capture the completed conversation entry in the signed-in Console.

### Expected MCP And Runtime Sequence

Audit evidence must show this semantic order. Additional read-only calls are allowed.

| Order | Owner | Expected operation | Required evidence |
|---:|---|---|---|
| 1 | UAV Simulation MCP | Read active vehicle-control grants | The authenticated principal resolves to `uav-1-pilot`, session `uav-showcase`, and vehicle `uav-1`. |
| 2 | UAV Simulation MCP | Read current telemetry | The route origin comes from authoritative `uav-1` telemetry. |
| 3 | Map MCP | Search and inspect the named location | The selected place has a canonical identity and WGS 84 position. |
| 4 | Map MCP | Execute the route task | The result carries a validated Map route URI, exact Frames revision, operational snapshot, and mobility-profile version. |
| 5 | Map MCP | Prepare the route handoff | The handoff retains the route provenance needed by mission admission. |
| 6 | UAV Simulation MCP | Prepare the vehicle mission | The admitted plan binds the authenticated principal, `uav-showcase`, `uav-1`, the Map handoff, and the current world revision. |
| 7 | UAV Simulation MCP | Execute the mission plan | One exclusive `uav-1` command lease exists while the plan is `executing`. |
| 8 | UAV Simulation MCP | Finalize execution | The task and plan become `succeeded`/`completed`, and the exact command lease receives `released_at`. |
| 9 | Agent runtime | Deliver the task-result wake | A later episode consumes the result and reports the terminal mission state. |

The authorization audit should name the installation's policies for UAV pilot vehicle
control, Map read, and Map route operations. Policy names may vary by installation; the
authorized principal, Work Context, profile, target server, and operation must be
explicit.

### Binding Proof

The successful mission is accepted only when all persisted UAV records name the same
principal, session, and vehicle:

| Field | Expected value |
|---|---|
| Pilot principal | the deployed OAuth principal for `uav-1-pilot` |
| Session | `uav-showcase` |
| Vehicle | `uav-1` |
| Mobility profile | the exact version named by the active control grant |
| Frames revision | the simulator's current immutable world revision |

No mission plan or command lease may be created for another vehicle by this request.
Other pilots and vehicles must retain their existing autonomous state. This test does not
mutate grants or issue a deliberately unauthorized flight; server contract tests own the
negative authorization matrix.

### Headless Procedure

A headless client uses the same generic contracts as the Console. It authenticates
through the installation's normal OAuth flow and needs policy permission for agent
message and conversation operations. It must not reuse a token copied from the browser.

Submit a client-generated UUIDv7 as the durable retry identity:

```http
POST /admin/<profile>/agents/uav-1-pilot/messages
Authorization: Bearer <normally-acquired-access-token>
Content-Type: application/json

{
  "request_id": "<uuid-v7>",
  "message": "<canonical-instruction>"
}
```

Read the durable conversation:

```http
GET /admin/<profile>/agents/uav-1-pilot/conversation
Authorization: Bearer <normally-acquired-access-token>
Accept: application/json
```

The response is an `AgentConversationView`. Match entries by `request_id`, `wake_id`,
`episode_id`, and `in_reply_to_request_ids`; do not use display text as the only join.
The terminal agent entry must be `completed` and must refer back to the submitted request.

Signed-in Console automation may also verify the BFF projection with
`GET /console/api/agents/uav-1-pilot/conversation`. That request proves the JSON surface
used by the UI under the current user session. It does not replace independent headless
OAuth acceptance when the installation exposes a machine client.

### Timing

Mission duration is route-dependent. Derive the physical lower bound from admitted route
distance and mission speed before choosing a timeout:

```text
minimum flight seconds = route distance metres / admitted speed metres per second
```

Allow time beyond that lower bound for Map task completion, mission admission, simulator
scheduling, finalization, and the terminal agent episode. A 9.227 km route admitted at
12 m/s has a physical lower bound of about 12 minutes 49 seconds. The reference flight
took 13 minutes 10 seconds to execute and about 17 minutes from operator submission to
the final conversation entry. A fixed short API timeout is therefore invalid evidence of
failure.

### Pass Criteria

The test passes only when all of these claims hold:

- one authenticated operator message is durably accepted for `uav-1-pilot`;
- Map MCP resolves the requested name and returns a validated route from current
  authoritative telemetry;
- the route handoff preserves the exact Map mobility profile and Frames revision;
- UAV Simulation MCP admits the plan only under the `uav-1-pilot` to `uav-1` grant;
- one command lease protects execution and is released after terminal finalization;
- every admitted waypoint completes and final telemetry is within the route endpoint's
  declared tolerance;
- PX4 remains connected, collisions do not increase, and unrelated vehicles are not
  commanded by the request;
- the durable execution task succeeds and is consumed by a task-result wake without a
  second execution call;
- Recording Hub durably accepts and materializes the recording batches that span the
  mission execution interval;
- the Console displays the terminal completed entry;
- the headless conversation API returns the same request lineage and terminal result.

Credential rotation or process replacement may occur during a long route. The watcher
may reconnect and renew its durable lease, but it must not replay the non-replayable
mission execution call or create a second mission plan.

### Failure Conditions

The test fails on any cross-vehicle admission, unvalidated or locally invented route,
route/world revision mismatch, missing command lease, unreleased terminal lease,
duplicated execution, terminal task error, lost task-result wake, final position outside
tolerance, missing mission-interval data in Recording Hub, missing actor attribution, or
disagreement between Console and headless conversation projections.

An expired user session, absent grant, occupied command lease, unavailable Map route,
unready PX4 connection, or non-hardware browser is a failed prerequisite. Record it as
such instead of altering state behind the product's normal recovery surfaces.

### Evidence Record

Retain these fields together for each run:

```text
test id and result
start and finish timestamps
source and GitOps revisions
deployment readiness evidence
operator actor id
agent id, OAuth principal, Work Context, session, and vehicle id
operator request id and wake id
operator, route-result, mission-result, and terminal episode ids
Map place id, route URI, operational snapshot, mobility profile, and Frames revision
UAV mission id, plan id, server task id, agent task id, and command lease id
route distance, admitted speed, waypoint count, and completion count
start and final telemetry, endpoint error, PX4 state, battery, and collision count
lease release timestamp
authorization audit sequence
Console capture
headless HTTP status and matching terminal conversation entry
recording URI when the showcase recording is active
Recording Hub ingest stream, durable checkpoint, and materialized checkpoint
```

Secrets, browser cookies, OAuth tokens, private keys, and provider credentials never
belong in the evidence.

### Reference Execution: 2026-08-14

The first accepted run used `uav-1-pilot` to fly `uav-1` from the Statue of Liberty area
to Times Square.

| Evidence | Observed value |
|---|---|
| Result | Passed |
| Source revision | `dd3189b5985de47960f4fc02f633aaaef4caa4c0` |
| GitOps revision | `b6e2069f0816e8190a3aafa4ae67eda8762e84d5` |
| Operator wake | `01a0019f-bbec-7b46-a01d-8f232b574f1d` |
| Map route | `map://route/route-01a001a0-2fab-7c22-8478-2cfef553a70b` |
| Route distance | 9,227.027 m |
| Waypoints | 4 of 4 completed |
| UAV mission | `mission-01a001a0-2fab-7c22-8478-2cfef553a70c` |
| UAV plan | `plan-01a001a1-45df-79e3-b4aa-2c822c2906ec` |
| Agent execution task | `gtr_IFoXdbLOhtWa2FvAm1-j0yqw6laI6OcNQXL4-AxNLhE` |
| Execution interval | 18:55:51-19:09:01 UTC |
| Final position | 40.7579989505, -73.9854950592 |
| Final vehicle state | Stationary at destination; PX4 connected; battery 51%; zero collisions |
| Runtime-reported recording | `recording://recordings/01a00156-326d-7893-83f9-0a08733d70cf` |
| Recording Hub ingest stream | `recording_ingest_stream:01a00156-3271-7fb2-bae0-165dc4ee068c` |
| Recording Hub checkpoint | Finished; durable and materialized through batch 12,513 |
| Mission recording samples | Camera-bearing batches 9,720 at 18:55:54, 10,600 at 19:03:29, and 11,290 at 19:09:16 UTC |
| Console evidence | Signed-in terminal entry: `Mission completed successfully.` |
| Headless evidence | Signed same-origin conversation request returned HTTP 200 and the same completed entry |

The gateway rotated the pilot's MCP credential during this flight. The durable watcher
reconnected, resolved the existing task, and produced the terminal wake without replaying
mission execution. The UAV plan completed at revision 2, its server task succeeded, and
the command lease was released at `2026-08-14T19:09:01.769488563Z`.

The UAV camera recorded the mission. SurrealDB was OOM-killed at 18:54:32 UTC while the
Hub was accepting batch 9,557, which left the Hub's materialized view stopped at batch
9,556 even as the recording forwarder continued to capture into its durable queue. The
recovered Hub replayed the missing batches, materialized through batch 12,513, and
finished the affected ingest stream. The archived data now spans the full mission and
can support a separately produced timelapse.

## Automation Boundary

`UAV-E2E-001` is currently an operator-driven deployed acceptance runbook. If it becomes
an automated smoke command, lifecycle, typed assertions, retries, evidence, and cleanup
must live in the repository's Rust smoke harness and be dispatched through `cargo xtask
smoke`. A shell script or browser-only macro is not an acceptable automation owner.
