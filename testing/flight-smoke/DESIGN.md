# Composed Flight Acceptance

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Installation input | Required `--installation` file using `veoveo.ai/installation-target/v1`; identities and endpoints validated against its control-plane document |
| MCP and authentication | Repository conformance CLI over public HTTPS, the hosted MCP 2026-07-28 profile, OAuth token exchange, exact Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.uav-sim-acceptance/v11` JSON with bounded typed mission, world, video and observation parameters |
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
`uav-world-publish`, `uav-route-verify`, `uav-domain-verify`, `uav-showcase-up`, and `uav-showcase-verify`. Their command
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

## Build Acceptance

The resolved Cargo graph is tested for each focused client and for the flight client
combined with its conformance helper. The graph must exclude service implementations,
SurrealDB, DuckDB, and Rerun. A verifier edit must rebuild only the affected client;
an unchanged command must dispatch within the two-second warm budget.

A build check or `--help` timing supplies compilation evidence. It does not certify a
flight, inference run, screenshot, or visual workflow. Those require the unchanged
hardware and authenticated workload assertions against the installation.
