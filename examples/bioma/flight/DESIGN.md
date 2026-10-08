# Composed Flight Acceptance

The client selects MCP Contract’s transport-free `configuration` feature for
control-plane decoding and catalog validation. Domain service libraries remain
contract-only; hosting, token signing and analytics are outside this package.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Installation input | Required `--installation` file using `veoveo.ai/installation-target/v1`; identities and endpoints validated against its control-plane document |
| MCP and authentication | Maintained workspace Rust MCP SDK and conformance CLI over public HTTPS; hosted MCP 2026-07-28, OAuth token exchange, Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.ai/uav-sim-acceptance/v13` JSON with bounded typed mission, world, video and observation parameters |
| Recording analysis | Stream and Reason contract features, Recording catalog and checked live-part snapshot types; `veoveo.ai/uav-recording-acceptance/v1` JSON result |
| Stream live sessions | Server-owned live-session types imported through the Stream library's isolated `contract` feature |
| Stream notifications | Official workspace Rust MCP SDK; public resource subscriptions, uncached typed reads, cancellation and reconnected baselines; `veoveo.ai/stream-notification-acceptance/v1` result |
| UAV control grants | UAV-owned grant, permission and collection types through its isolated contract feature; a 60-second and 100-page traversal limit; Map owns mobility-profile references |
| Route admission | Map-owned route request, position, policy and result types through its isolated contract feature; a current aviation route Task runs before flight commands |
| World readiness | UAV-owned simulation, tile and camera state types; the scenario timeout includes reads and warmup waits; invalid bindings, failed resources and unsupported encoders fail immediately |
| Browser automation | Headed Chrome DevTools Protocol, hardware-backed WebGPU or WebGL, shared browser assertions owned by `testing/browser-smoke` with composed browser assertions in `examples/bioma/acceptance/src/browser` |
| GPU workload evidence | Existing NVIDIA resource identity, NVENC source and concurrent workload assertions; software rendering is rejected |
| World publication | Frames-owned immutable revisions and typed frame URIs; UAV-owned validated installation binding; `veoveo.ai/uav-world-publication/v2` JSON receipt with the output file SHA-256 |
| Evidence | `veoveo.ai/uav-showcase-acceptance-evidence/v6` JSON and revision-qualified captures; `veoveo.ai/uav-showcase-phase-outcomes/v3` records domain and visual outcomes and preserves successful visual measurements even when the domain phase fails; both mark Reason `not_run` because its acceptance runs separately |
| Focused restart stages | `veoveo.ai/uav-live-view-restart-stage/v1` records each accepted container restart and its headed hardware browser observations before the next restart begins |

## Ownership

`cargo xtask smoke` builds this harness and its conformance executable for
`uav-world-publish`, `uav-route-verify`, `uav-stream-verify`, `uav-recording-verify`,
`uav-domain-verify`,
`uav-showcase-up`, and `uav-showcase-verify`. Their command
arguments and assertions stay in Rust. `src/main.rs` installs TLS, enters the shared cancellation scope and dispatches
parsed commands. `src/cli.rs` owns those arguments.

`src/domain.rs` sequences the composed workflow. Its modules separate scenario validation,
authenticated MCP calls, world and vehicle state, Stream session ownership, governed
Artifact checks, and visual flight checkpoints. The harness reads and commands a
running installation through its admitted interfaces. It does not compile or start a
platform database, task runtime, recording service, or unrelated MCP server.

`src/domain/world_publication.rs` publishes the scenario through Frames, reads back its
revision and simulation frame, and constructs startup input through the UAV contract
builder. `uav-world-publish` writes that input to an explicit output path and prints its
SHA-256. It requires only operator credentials and does not configure the simulator.
The installation deploys the reviewed file and digest together. Repeating publication
of the same tree uses Frames' existing immutable publication semantics. The composed
flight path requires the installed revision and frame to resolve through Frames,
including after a database reset.

Scenario and grant decoders admit Map profile references through the same owning type.
The route request serializes its typed profile ID and version without reparsing text.
`src/domain/route.rs` sends the same typed route request for prerequisites and the mission.
`uav-route-verify` needs only the configured operator and Map endpoint. It tests the
scenario origin at takeoff altitude and its nearby destination, accepts a usable
aviation route with release provenance, and issues no flight commands. Map performs
current tenant, source, release, family and departure-time admission in SQL. The flight
path repeats the prerequisite before world and control operations, then requests the
mission route from the observed airborne position. A prerequisite pass does not freeze
Map policy or make the later route request optional.
The mission keeps Map's `RoutePlan` and `RouteCost` types through handoff and timeout
calculation; JSON serialization occurs at the MCP call.
UAV-owned preparation, execution and result types preserve the selected mission,
vehicle and world revision through execution. The client checks the prepared plan
against its request before dispatching the Task and correlates the completed result.

A successful flight returns through a second Map-admitted route to the scenario's
world origin before descending. The return holds the aircraft's current ellipsoid
altitude. The reference launch surface is a 40 m square centered on that origin;
the harness checks a 20 m horizontal radius on arrival and after landing. Google
tiles provide visual geometry, so an arbitrary destination elsewhere in the city
does not establish a valid landing site. Failure cleanup still requests landing
without dispatching another mission.

`uav-stream-verify` requires operator credentials and an already running UAV camera
and Stream service. It reads the typed NVENC camera state, starts or reuses the
admitted live session, and checks fresh inference results and its encoded H.264 preview.
It sends no vehicle commands and does not require Recording replay, Reason or a browser.
The full flight path uses the same check before obtaining control and repeats it after
mission completion. Session, result and preview identities must agree. One scenario
timeout covers all result reads and waits. The preview admits AVC presentation
reordering while checking contiguous decode sequence and distinct timestamps.

The focused live command also subscribes to the session, results and preview through
the public Gateway using the selected operator. It requires the exact acknowledged
filter and an initial invalidation for each address. After the baseline read, fresh
notifications must accompany advancing inference-frame and encoded-preview counters.
It cancels that request, reconnects, checks that the new baseline does not move backwards,
and requires another advance. Explicit SDK requests bypass its response cache. The
notification case has a sixty-second deadline and prints a JSON result with both
cycles, counters, cancellation and any failure. The owner still cleans up only a
session it created. The SDK uses the existing qualified workspace pin; this client
does not import a service implementation.

Both commands report a failure before cleanup and stop only sessions they created.
Flight cleanup lands the vehicle only after this run began issuing flight commands.
An early Stream failure therefore needs no landing cycle. Existing sessions keep their
owner, and an independent Stream pass does not establish composed flight or visual acceptance.

`src/domain/recording.rs` owns Stream replay and grounded Reason acceptance. The composed
flight calls its replay check and requires UAV, View and Stream to run concurrently.
`uav-recording-verify` adds grounded Reason over that admitted replay selection.
Reason has its own GPU acceptance and need not run during flight acceptance.
The focused command needs operator
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
the shared contract. Scenario v13 carries the current Frames camelCase tree while
its other owner fields follow their declared profiles. Receivers refuse earlier tags
and retired Frames spellings. Independent replay acceptance does not establish flight
or headed visual acceptance.

`src/domain/readiness.rs` distinguishes startup from invalid state. Runtime, terrain,
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
`testing/browser-smoke/src/source_timeline.rs` aligns the final Rerun observation
with the simulator's published timestamps in both focused and composed acceptance.
The sampler keeps the closest earlier sample and waits up to ten seconds for a sample
at or after the observation. That deadline includes source reads and 100 ms waits;
unchanged cached samples can wait, while backward clocks, malformed samples and a
non-running simulation fail. Interpolation requires both clocks to advance and the
observation to fall inside the sample interval. Playback must trail the aligned source
by zero to one second. Both samples and the interpolation fraction enter the report.
Takeoff, live Stream and moving Recording holds require an explicit successful capture
acknowledgement. A closed capture channel or expired deadline ends domain progression
and enters owned cleanup. The domain branch cannot infer capture success from the
browser branch exiting.
Flight preflight uses the conformance client's paged `tools` command to require its
domain and live-view tools. Unrelated prompt and resource-template catalogs do not
establish flight readiness. The scenario still reads every resource it actually needs
and checks the required GPU workloads before issuing commands.

The shared browser sampler reads the displayed camera canvas at 64 by 36 pixels and
reports luminance statistics for the frame and a four-by-four grid. The content check
requires a luminance standard deviation of at least five and at least four regions
with a luminance range of sixteen and standard deviation of five. Mean brightness is
diagnostic; scene exposure does not determine whether a frame contains detail. Uniform
images, a small isolated bright patch and failed or incomplete samples fail this check.
The sampler's native JavaScript cases and the Rust content-policy cases qualify these
measurements and decisions. They establish no GPU or installed visual acceptance.

Initial and restarted video may produce uniform frames while the world loads. The
browser harness waits within its existing startup or recovery deadline for a frame
that meets every content threshold above. Invalid samples and software-renderer
warnings fail immediately. Restart acceptance also requires the original document
and viewer identities, a fresh stream authorization, and the unchanged cadence and
latency checks. A uniform stream that exhausts its deadline fails. The focused
restart harness waits for a declared warming camera within the scenario's view
timeout; stale, failed, unknown or missing cameras fail immediately. It saves each
accepted restart stage before proceeding, so a later failure preserves the earlier
stage's container and browser observations.

The domain harness releases visual checkpoints after this run's takeoff and mission
complete. Takeoff requires the selected vehicle to be flying at the scenario's minimum
altitude within its takeoff deadline; the flight-state transition alone cannot release
the capture. Each observation must belong to the selected session and world revision.
It waits for the takeoff capture before dispatching the mission. Existing
simulator flight and preflight recovery cannot satisfy these checkpoints. The live
Stream session stays open through its capture, and landing waits for the moving
Recording capture. Scenario deadlines bound these holds. A failed visual branch
releases its holds so the domain checks and owned cleanup can finish; the composed
command still fails acceptance.
The harness writes create-only domain and visual phase outcomes after both branches
settle, before propagating either error. A domain failure therefore preserves the
visual status, and the native log includes its diagnostic. Successful runs also
write the complete capture and timing report.

## Build Acceptance

The resolved Cargo graph is tested for each focused client and for the flight client
combined with its conformance helper. The graph must exclude service implementations,
SurrealDB, DuckDB, and Rerun. A verifier edit must rebuild only the affected client;
an unchanged command must dispatch within the two-second warm budget.

A build check or `--help` timing supplies compilation evidence. It does not certify a
flight, inference run, screenshot, or visual workflow. Those require the unchanged
hardware and authenticated workload assertions against the installation.

## Source And Executable Delivery

The existing `veoveo-flight-smoke` package lives beside the Bioma installation composition. Its `flight-smoke` binary requires the nondefault `smoke` feature. The normal and build graph stays client-only: owner contract features, Gateway catalog declarations, the pure browser transport library and domain-neutral test support. Service implementations, Store, DuckDB and Rerun runtime crates are excluded.

`src/support.rs` compiles the single composition-owned installed identity, OAuth exchange and NVIDIA identity helper sources through explicit modules. Their source owner is Gateway composition; the executable owner is this focused client. Installation validation still uses the complete Gateway catalog. OAuth exchange invokes the separately declared Gateway support executable through the recorded artifact manifest. The browser assertion source belongs to Bioma acceptance and is compiled here through its explicit source module; the CDP library stays domain-neutral. Shared upload reports select acceptance's isolated `reports` feature, and conversation values select Agent Runtime's `contract` feature. These selections reuse the owner declarations without importing their persistence or server implementations.

The dispatcher declares each selected native executable and its prerequisites from Cargo metadata. Relocation preserves the CLI arguments, stdout receipts, private credential environment and hardware checks.

## Interrupted Remote Operations

Flight registers Stream start and vehicle takeoff intent before dispatch. An accepted
owner acknowledgement binds the observed identity to the selected parent. Reused
Stream sessions carry no cleanup ownership. Task calls use the maintained SDK helper,
which retains the returned Task ID before awaiting completion.

A caller error or inner Task timeout returns to the owner before ordinary
postflight effects. Successful normal postflight verifies landing and then stops
the owned Stream session. Cancellation stops the main operation and executes the
registered actions in reverse order within the original grace interval. Retained
Tasks are therefore reconciled before physical vehicle and Stream cleanup. Cleanup uses the captured authenticated SDK peer
without admitting new ordinary effects. Stream stop and vehicle landing use the same
domain result and state checks as normal completion. Unknown dispatch outcomes or
refused cleanup keep their identities for reconciliation; they cannot justify another
start. Subprocess reads preserve the caller timeout and the owner deadline caps it.

The vehicle owner shares one landing-dispatch record between normal postflight
and interrupted cleanup. It marks that record before awaiting the command and
admits the accepted acknowledgement against the selected session and aircraft.
Refused, malformed, foreign or unknown acknowledgements cannot authorize another
land command. An admitted parent state can establish that the aircraft is landed;
a cancelled Task alone cannot establish physical mission abort. A still-flying
state is reconciled within the available interval without resending the command.
