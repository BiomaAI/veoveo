# Composed Flight Acceptance

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Installation input | Required `--installation` file using `veoveo.ai/installation-target/v1`; identities and endpoints validated against its control-plane document |
| MCP and authentication | Repository conformance CLI over public HTTPS, the hosted MCP 2026-07-28 profile, OAuth token exchange, exact Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.ai/uav-sim-acceptance/v12` JSON with bounded typed mission, world, video and observation parameters |
| Recording analysis | Stream and Reason contract features, Recording catalog and checked live-part snapshot types; `veoveo.ai/uav-recording-acceptance/v1` JSON result |
| Stream live sessions | Server-owned live-session types imported through the Stream library's isolated `contract` feature |
| UAV control grants | UAV-owned grant, permission and collection types through its isolated contract feature; a 60-second and 100-page traversal limit; Map owns mobility-profile references |
| Route admission | Map-owned route request, position, policy and result types through its isolated contract feature; a current aviation route Task runs before flight commands |
| World readiness | UAV-owned simulation, tile and camera state types; the scenario timeout includes reads and warmup waits; invalid bindings, failed resources and unsupported encoders fail immediately |
| Browser automation | Headed Chrome DevTools Protocol, hardware-backed WebGPU or WebGL, shared browser assertions owned by `testing/browser-smoke` |
| GPU workload evidence | Existing NVIDIA resource identity, NVENC source and concurrent workload assertions; software rendering is rejected |
| World publication | Frames-owned immutable revisions and typed frame URIs; UAV-owned validated installation binding; `veoveo.ai/uav-world-publication/v1` JSON receipt with the output file SHA-256 |
| Evidence | Existing `veoveo.ai/uav-showcase-acceptance-evidence/v4` JSON and revision-qualified captures |

## Ownership

`cargo xtask smoke` builds this harness and its conformance executable for
`uav-world-publish`, `uav-route-verify`, `uav-stream-verify`, `uav-recording-verify`,
`uav-domain-verify`,
`uav-showcase-up`, and `uav-showcase-verify`. Their command
arguments and assertions stay in Rust. `main.rs` only installs TLS and dispatches
parsed commands. `cli.rs` owns those arguments.

`domain.rs` sequences the composed workflow. Its modules separate scenario validation,
authenticated MCP calls, world and vehicle state, Stream session ownership, governed
Artifact checks, and visual flight checkpoints. The harness reads and commands a
running installation through its admitted interfaces. It does not compile or start a
platform database, task runtime, recording service, or unrelated MCP server.

`domain/world_publication.rs` publishes the scenario through Frames, reads back its
revision and simulation frame, and constructs startup input through the UAV contract
builder. `uav-world-publish` writes that input to an explicit output path and prints its
SHA-256. It requires only operator credentials and does not configure the simulator.
The installation deploys the reviewed file and digest together. Repeating publication
of the same tree uses Frames' existing immutable publication semantics. The composed
flight path requires the installed revision and frame to resolve through Frames,
including after a database reset.

Scenario and grant decoders admit Map profile references through the same owning type.
The route request serializes its typed profile ID and version without reparsing text.
`domain/route.rs` sends the same typed route request for prerequisites and the mission.
`uav-route-verify` needs only the configured operator and Map endpoint. It tests the
scenario origin at takeoff altitude and its nearby destination, accepts a usable
aviation route with release provenance, and issues no flight commands. Map performs
current tenant, source, release, family and departure-time admission in SQL. The flight
path repeats the prerequisite before world and control operations, then requests the
mission route from the observed airborne position. A prerequisite pass does not freeze
Map policy or make the later route request optional.
The mission keeps Map's `RoutePlan` and `RouteCost` types through handoff and timeout
calculation; JSON serialization occurs at the MCP call.

`uav-stream-verify` requires operator credentials and an already running UAV camera
and Stream service. It reads the typed NVENC camera state, starts or reuses the
admitted live session, and checks fresh inference results and its encoded H.264 preview.
It sends no vehicle commands and does not require Recording replay, Reason or a browser.
The full flight path uses the same check before obtaining control and repeats it after
mission completion. Session, result and preview identities must agree. One scenario
timeout covers all result reads and waits. The preview admits AVC presentation
reordering while checking contiguous decode sequence and distinct timestamps.

Both commands report a failure before cleanup and stop only sessions they created.
Flight cleanup lands the vehicle only after this run began issuing flight commands.
An early Stream failure therefore needs no landing cycle. Existing sessions keep their
owner, and an independent Stream pass does not establish composed flight or visual acceptance.

`domain/recording.rs` owns replay and grounded Reason acceptance for both
`uav-recording-verify` and the composed flight. The focused command needs operator
credentials, a running NVENC camera with at least the scenario's recording history,
Recording, Stream and Reason services, and their NVIDIA workers. It sends no vehicle
commands and does not start a live Stream session. Operators stop only sessions they
own when freeing GPU capacity before this check.

The harness selects the active recording for the scenario's vehicle camera, reads its
live catalog entry, and submits one bounded Stream replay Task followed by one grounded
Reason Task over the same range. The services capture acknowledged live parts through
the shared Recording reader. Each completion must agree with its typed result resource
and downloaded Artifact. Dataset, recording, entity, timeline and range must match the
request; the checked source snapshot must include a live ingest part. The command
prints a JSON result with the selection, Artifact identities and frame counts. Catalog
readiness has a 30-second deadline; Tasks use the scenario deadlines and are not retried.
An unresolved Task needs its existing identity reconciled before another run. Task
workers own their scratch cleanup; successful result Artifacts remain inspectable.

The client imports Reason through its contract feature and Artifact identities through
the shared contract. Internal scenario v12 removes the archive-projection probe fields;
obsolete shapes fail decoding. Independent replay acceptance does not establish flight
or headed visual acceptance.

`domain/readiness.rs` distinguishes startup from invalid state. Runtime, terrain,
PX4 connection and camera warmup share the scenario's single timeout. A running
session can still be loading terrain. The waiter checks world identity and hardware
contracts during warmup; a failed runtime, degraded terrain or camera, stale operator
camera, invalid tiled product, or unsupported encoder ends the wait immediately.
The client imports UAV's public grant and camera-product types. Scenario session and
vehicle IDs decode through UAV's owning types before any external work.
The installation target selects every requested scope. Flight admission checks its
UAV requirements using `UavScope` from the owning library; it never adds scopes to a
request. The shared loader resolves client and profile IDs, principal, tenant, Work
Contexts, issuer and token endpoint from the selected control plane before execution.
Cluster checks use the target's context and namespace. Artifact assertions compare
provenance and output ownership with that resolved identity and its output policy.

Flight verification requires an explicit `administrator` client selection and an
operator `comparisonContext` for artifact isolation. Operator credentials use
`VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE` and `VEOVEO_SERVICE_CLIENT_KEY_ID`.
Administrator credentials use `VEOVEO_ADMIN_SERVICE_CLIENT_PRIVATE_KEY_FILE` and
`VEOVEO_ADMIN_SERVICE_CLIENT_KEY_ID`. Token exchange maps the selected credentials
only into its child process. Private keys and issued tokens stay outside target files.
`uav-showcase-up` needs only the operator because it does not administer grants or
run artifact isolation acceptance.

Browser attachment, GPU rejection and visual assertions have one source owner in
`testing/browser-smoke/src/browser.rs`. Both focused clients compile that source.
Small process, GPU identity and token-exchange helpers likewise keep one Rust source.
No new test-support framework or upstream dependency is introduced.

The domain harness releases visual checkpoints after this run's takeoff and mission
complete. It waits for the takeoff capture before dispatching the mission. Existing
simulator flight and preflight recovery cannot satisfy these checkpoints. The live
Stream session stays open through its capture, and landing waits for the moving
Recording capture. Scenario deadlines bound these holds. A failed visual branch
releases its holds so the domain checks and owned cleanup can finish; the composed
command still fails acceptance.

## Build Acceptance

The resolved Cargo graph is tested for each focused client and for the flight client
combined with its conformance helper. The graph must exclude service implementations,
SurrealDB, DuckDB, and Rerun. A verifier edit must rebuild only the affected client;
an unchanged command must dispatch within the two-second warm budget.

A build check or `--help` timing supplies compilation evidence. It does not certify a
flight, inference run, screenshot, or visual workflow. Those require the unchanged
hardware and authenticated workload assertions against the installation.
