# UAV Simulation MCP Server

The UAV Simulation server owns the governed control surface for one authoritative
simulation runtime. The same runtime advances physics, owns the stage and streamed
world, derives operator cameras from current entity transforms, renders those cameras,
and produces their encoded video. No second process mirrors the scene or replays poses
for visualization.

> A simulation MCP server exposes governed logical cameras rendered by its authoritative
> simulation. All streamable cameras occupy typed regions in one continuous tiled RTX
> render and NVIDIA NVENC product. Every browser decodes that atlas once and presents
> each region as an independent camera canvas. Camera smoothing operates once on each logical-camera
> transform and never changes or delays authoritative simulation state.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | Version `2026-07-28` over the repository stateless Streamable HTTP profile, including Discover, tools, resources, templates, `subscriptions/listen`, official Tasks, and one MCP App. |
| SurrealDB / SurrealQL `3.2.4` | Tenant and Work Context grant/plan queries, transactional command-lease, plan and Task-link transitions, caller-owned Task pages, SQL completion, and shared LIVE/changefeed invalidation. |
| UAV mission admission | Repository-owned Store relationship linking each admitted plan to one native Task and its vehicle command lease. Public plan JSON exposes domain state. |
| RFC 6570 URI Templates | iri-string `0.7.14` through foundational template admission and scalar expansion; every advertised UAV template is checked against the owning address builder. |
| Concrete resource URIs | URL `2.5.8` and percent-encoding `2.3.2` through foundational components; typed UAV routes and collection-bound hexadecimal JSON cursors, version 1. |
| JSON Schema | Draft 2020-12 strict request, result, camera, tiled-product, region, and health schemas. |
| `veoveo.ai/live-view/v4` | Repository-owned provider-neutral profile for authoritative cameras, typed regions in shared encoded products, viewer authorizations, WebSocket H.264 endpoints, and redacted state. |
| `veoveo.ai/uav-runtime-event/v2` | Private authenticated HTTP/1.1 NDJSON stream carrying an `adapter_ready` edge before world admission and a final `ready` edge after authoritative visual admission. It is an internal adapter event, not a public MCP resource or a simulation control protocol. |
| WebSocket and H.264 | RFC 6455 binary messages under subprotocol `veoveo.h264.annexb.v1`; each message carries one decoder-reentrant or predicted Annex B H.264 access unit. The encoded atlas is H.264 Main Profile Level 5.2, advertised to WebCodecs with the exact RFC 6381 codec string `avc1.4d4034`. One tiled NVIDIA NVENC atlas fans out unchanged to authenticated viewers. This WebSocket is a media adapter, not a public simulator-control protocol. |
| OpenUSD and RTX Hydra | Isaac Sim `6.1.0` stage and render products inside the authoritative runtime. These are implementation details, not MCP wire types. |
| Native sensor video | Isaac Sim `isaacsim.streaming.rtsp` `0.1.5` supplies `RTSPStreamWriter`. Its CUDA-buffer mode passes resident pixels to `omni.kit.livestream.rtsp` `10.4.1` for one NVIDIA NVENC encode. The private adapter consumes the loopback RTSP/RTP H.264 stream without decoding or re-encoding. This is not an MCP wire type. |
| RTSP, RTP, and H.264 | RTSP 1.0 over loopback TCP with interleaved RTP/RTCP. The adapter supports the RFC 6184 single-NAL, STAP-A, and FU-A packetization modes and emits decoder-reentrant Annex B access units. |
| OGC 3D Tiles | Cesium Omniverse `0.29.0` with pinned Cesium Native commit `ca0311f25c412b74ad1af9a3636924122cc76156`, one simulator-owned world, and one cache. The repository extension adds private redacted lifecycle events; it does not add an MCP wire protocol. |
| WGS 84, ECEF, ENU, NED, and FLU | Explicit world, physics, entity, rig, and camera coordinate boundaries. |
| `veoveo.ai/map-route-handoff/v1` | Map-owned `MapRouteHandoff` consumed through its contract-only library, including `MapMobilityProfileUri` and `ValidationId`; UAV admits validated routes and explicitly granted planning-advisory routes. |
| `frames://world/{world_id}/revision/{revision_id}` | Frames MCP-owned immutable world revision identity consumed by session configuration and mission admission. |
| MAVLink 2 | Private PX4 command, telemetry, actuator, and HIL sensor integration. The protocol is not projected as high-rate MCP traffic. |
| Recording resources | RFC 9562 UUIDv7 identities and `recording://recordings/{id}` addresses from `veoveo-recording-contract`; pending catalog state carries no public identity. |
| Rerun RRD | Version `0.38.1` recording data and producer-authored Blueprint stores sent independently to Recording Hub. |
| NVIDIA Container Runtime | One Kubernetes GPU allocation with compute, graphics, utility, and video driver capabilities. CPU rendering and encoding are unsupported. |

## Library Features

`veoveo-uav-sim-mcp` exposes `contract`, `runtime` and `mcp` features. The default is
`mcp`, and the server binary requires it. Consumers of public types use
`default-features = false, features = ["contract"]`. This feature includes IDs, tool
requests and results, `UavScope`, Map/Frames handoffs, Recording references, and the live-view v4 model. Its dependencies
provide value types, validation, serialization and URI parsing. It excludes the server,
MCP integration, database, async runtime, simulator adapter and GPU libraries. Chrono's
clock support belongs to the runtime feature.

`runtime` adds world binding and the private simulator HTTP adapter. Its completion
receipt has private construction and validates the response against its dispatched
operation before result resolution. This low-level adapter requires installation
credentials; vehicle authorization and mission admission belong to the hosted domain
service. The `mcp` feature adds that service, gateway identity conversion, Tasks,
subscriptions, live streaming and process setup.

`contract/live_view.rs` owns the provider-neutral live-view v4 Rust model. Camera rigs,
optics, health, regions, NVIDIA NVENC metadata, endpoints and connection tokens keep
their published wire shapes. The authenticated adapter derives viewer ownership from
the gateway's resolved output policy, including classification and data labels. The
public model depends on foundational identity types and takes no gateway identity.
MCP core has no dependency on this library. Schema publication can select the live-view
models independently of the vehicle tool schemas.

The flight acceptance client imports this contract for camera products, grants and
100-item collection pages. Scenario session and vehicle IDs use the owning types;
invalid values fail decoding before the client starts work. It retains its existing
60-second, 100-page traversal budget.

The feature combinations share live-view v4, private adapter JSON and persisted formats.
Internal Rust imports use one owner without compatibility exports. This library boundary
requires no data conversion or deployment drain. Contract schema snapshots and
an independently resolved consumer qualify the library surface; installation behavior
and hardware execution require their separate acceptance runs.

## Recording References

The Recording domain contract owns public recording IDs and URIs. UAV imports those
types directly and owns only its producer `RecordingKey`. The adapter carries that
key through state handling to the catalog query. Completion receipts keep unresolved
producer keys separate from admitted keys; their validation runs during result
resolution, after the worker settles the correlated physical completion. Catalog admission
requires the Recording table and a native RFC UUIDv7 key before the shared URI builder
constructs the public address. SQL still selects the tenant, application and producer
key before the adapter decodes the selected row.

`contract/recordings.rs` owns `RecordingCatalog` and `RecordingState`. Catalog readiness
carries one typed Recording URI; serialization derives `recording_id` from that address.
Pending, unavailable and invalid states have no public recording identity. The private
wire representation validates status, ID, URI and diagnostic agreement on decoding.
It preserves the public JSON fields, and the schema inlines the owner's string identity.
Mission, scenario and capture results carry vectors of the same URI type. Catalog
resolution still follows settlement of physical completion, so catalog failure cannot
turn a completed physical operation into permission to repeat it.

Flight and browser acceptance clients decode this public model and keep the Recording
ID typed through capture and result serialization. Replay smoke constructs source URIs
through the owner builder. CLI recording arguments use the owner's parser before
network or browser work begins. Native contract and adapter tests qualify identity
admission; these tests establish no simulation or visual GPU acceptance.

## Checked MCP Setup

`server/setup.rs` implements `McpServerContract` with `UavScope` and `UavResource`.
The MCP feature owns this association; the public contract feature has no MCP
integration dependency. Startup evaluates `McpServerSetup` before Store recovery or
adapter calls. Service construction also evaluates it for library consumers. The
shared checker verifies implementation/document identity, resource round trips,
required documents, resource capabilities and unique scope/resource/template declarations.
The server declares its own vocabulary without changing MCP core.

Initialization, document reads and discovery consume this setup. Templates have
RFC 6570 admission and lexicographic ordering. Native checks expand every template,
including optional collection cursors, through the foundational URI library and compare
the result with UAV's builder. Domain parsing rejects invalid IDs and cursor parents.

Caller scope checks use the setup's typed scope membership. Domain SQL still selects
visible grants, plans, missions and Tasks. Discovery filters checked static roots by
scope. The live App attaches its installation CSP and caller-selected agent targets
through a checked descriptor constructor, preserving its typed address. Agent-target
changes keep their declared resource-list invalidation source. This wiring checks
declarations and types; installed authorization, recovery and GPU behavior have
separate qualification requirements.

## Resource Addresses And Cursors

The contract feature owns `UavResource`, which implements foundational
`ResourceAddress`. Each variant takes the corresponding session, vehicle, mission,
grant, plan or live-view ID. `uris` exposes typed constructors and fixed discovery
roots and templates. The foundational builder encodes components and the parser
rejects normalization, credentials, fragments, duplicate queries and unsupported
routes. Both resource reads and subscription admission dispatch the parsed variant.
They share scope requirements and check the requested session and child identity.
Persisted catalogs reach their SQL visibility queries without fetching simulator state.

Each collection owns its cursor type. Version 1 preserves hexadecimal JSON field
order (`version`, `collection`, `position`). Grant tool cursors bind their active
session; live-view cursors bind their session path. Cursors contain positions and
establish no authority. Every page checks the current caller. UAV usage cursors and
addresses require native UUIDv7 Task identities. The runtime converts their typed
positions into TaskRuntime inputs at the service boundary. `server/index.rs` assembles
100-item pages from at most 101 selected rows and serializes owner-produced cursors.

Public ID constructors and Serde decoding reject relative path segments `.` and `..`.
The provider-neutral `LiveViewUri` also uses the foundational parser, preserves other
provider schemes, and rejects normalized or escaped spellings outside its ASCII ID
profile. Existing accepted route and cursor bytes keep their representation.

This admission tightening requires a coordinated upgrade of UAV replicas and producers.
Before replacement, drain mutations and export retained UAV grants, plans, Tasks,
world bindings and provider configuration. Check their UAV identity fields with the
new contract decoder and check stored resource references with their owning parser.
An invalid record or provider ID stops the upgrade with its storage key or configuration
location. Correct the producer configuration or perform an explicit, reviewed rekey
of the record and all references before retrying. Keep the original export; this
change performs no automatic rewriting or destructive conversion. Ephemeral live views
close during the drain and clients reopen them after replacement. Resume work only
after native and installed contract checks pass. Rollback uses the retained export
and a compatible prior build that preserves the Task scope guard and the new ID
admission rules; a build accepting relative IDs cannot overlap with this profile.
The owner must qualify this preflight and rollback on the installation before rollout.

## Authority Boundary

The simulation runtime is authoritative for physics, entity transforms, the OpenUSD
stage, Cesium georeference, domain sensors, operator cameras, Hydra render products,
and NVENC products. The Rust server owns caller authorization, principal-to-vehicle
grants, mission admission, exclusive command leases, MCP state projection, ephemeral
stream authorizations, WebSocket admission, and access audit. Map MCP owns operational
geography, place resolution, mobility profiles, restrictions, routing, and the route
handoff. Frames MCP owns world trees and immutable revisions. The UAV server consumes
those exact products and never reimplements either vertical.

UAV configuration consumes Frames' checked `FrameWorldRevision` from its contract-only
library. Frames validates the complete tree, root, repeated identities and SHA-256 digest
when decoding that value. UAV checks that the requested simulation frame belongs to the
revision and has a static path to a geodetic tangent ancestor. Those domain checks grant
no caller authority; configuration still applies the session's admission policy.

The cluster-private adapter is the only boundary between those responsibilities. It exposes
typed configuration, command, state, and live-stream operations. It does
not carry a visualization pose stream. No MCP request participates in the physics or
render loop.

```text
gateway actor
    |
    v
UAV Simulation MCP server
  grants + mission admission + command leases + telemetry + App
    |
    | authenticated cluster-private typed adapter
    v
authoritative Isaac runtime
  Newton + Warp plant/sensors + PX4 HIL + USD/Cesium + operator cameras + Hydra + NVENC
    |
    +---- one tiled H.264 atlas for every operator camera
             +---- browser A: one decode -> five cropped canvases
             +---- browser B: one decode -> five cropped canvases
             +---- browser N: one decode -> five cropped canvases
```

Simulation never waits for recording, Rerun playback, audit persistence, a browser, or
an MCP consumer. Those boundaries may report failure, but they cannot stop authoritative
physics.

## MCP Surface

The server owns the `uav-sim://` scheme, slug `uav-sim`, MCP path `/uav-sim/mcp`,
and port `8802`.

The domain tools govern session configuration, simulation execution, single-vehicle
mission admission, dataset capture, and typed inspection. Vehicle authority uses:

- `list_active_vehicle_control_grants`
- `grant_vehicle_control`
- `revoke_vehicle_control`
- `prepare_vehicle_mission`
- `execute_vehicle_mission_plan`

`list_active_vehicle_control_grants` is the tool projection of the canonical grant
resources for clients whose execution surface exposes tools but not resource reads. It
applies the same tenant, Work Context, caller-visibility, session, revocation, and
validity filters as the resource surface. Its Map mobility-profile URI is an authority
binding, not a copy of Map work data. Map MCP remains authoritative for the referenced
profile and every route derived from it.

`resources/list` advertises collection roots, documents, and the Live Cameras App.
Templates describe exact records and cursor pages. Discovery uses the session identity
captured at server startup and the existing 32-target agent query; it does not read
simulator state or enumerate grants, plans, Tasks, vehicles, cameras, or viewer sessions.
Agent-target metadata still invalidates discovery when its Store inputs change.

The `control-grants`, `mission-plans`, `missions`, and `usage` roots return
`{items, limit, next_cursor}` with at most 100 items. Their `{?cursor}` templates accept
one versioned opaque cursor. Grant and plan pages order their immutable domain IDs.
Mission pages order distinct mission IDs; usage pages order Task creation time and UUID.
SQL applies caller visibility and cursor predicates before fetching the extra row used
to detect a following page. A cursor carries a position and collection identity, with
no authorization. These are live reads; callers refresh the root to see inserts ahead
of their position and subscribe to the root for invalidation.

Live-view collections use the same page envelope, ordered by live-view ID. The
process-owned viewer map applies owner, viewer actor, session, and cursor selection
before collecting 101 entries. These authorizations expire with the hosting process.

Grant and plan SQL scopes include tenant, Work Context, and principal. The admin scope
admits other principals within that same tenant and context. Direct reads and completion
use those predicates too. Completion searches case-insensitively before its 101-row SQL
limit, returns up to 100 suggestions, and omits a total when more exist. Vehicle
permission queries select a current grant for the requested session, vehicle, and
permission directly. Simulator inspection queries only grants for the current inventory.

`list_active_vehicle_control_grants` accepts `session_id` and optional `cursor` and
returns the same page envelope. It applies session, validity, and revocation predicates
in SQL. Its cursor is bound to that session and cannot be used on the historical grant
collection. Consumers follow all pages before deciding that only one active grant exists.
The flight smoke consumes the typed envelope with a 60-second, 100-page bound.
This response change is a coordinated foundations installation upgrade: deploy the
server, pilot instructions, and consumer harness together.

Mission resource URIs resolve the latest caller-owned admitted Task for that mission
in the current Work Context. SQL follows the plan's exact execution link and verifies
its Task identity, tenant, context and principal before ordering and selecting a row.
A later rejected attempt cannot hide the admitted Task. A tenant/context/principal/mission
index selects matching plans before the Task query uses its server/plan index. Native
`EXPLAIN FULL` qualification checks both access paths. A missing or damaged link supplies no result. Mission pages and completions require
the same link, current parent ownership and Task visibility in SQL before grouping
or applying their limits. A queued request alone does not publish a mission.
Usage resources preserve the shared Task read profile:
server, actor, tenant, gateway profile, and data-label clearance. Their reads can span
the same actor's Work Contexts. Native Store invalidations for grants, plans, and Tasks
refresh subscribed contents across replicas and reconnects. They do not change discovery
descriptors.

The old public multi-vehicle `execute_mission` surface does not exist. The simulator's
typed multi-vehicle adapter request is cluster-private and cannot establish principal
authority. The live-view profile adds:

- `list_live_cameras`
- `open_live_view`
- `renew_live_view`
- `close_live_view`

Live state uses these canonical resources:

- `uav-sim://session/{session_id}/live-cameras`
- `uav-sim://session/{session_id}/live-camera/{camera_id}`
- `uav-sim://session/{session_id}/stream-products`
- `uav-sim://session/{session_id}/stream-product/{product_id}`
- `uav-sim://session/{session_id}/live-views`
- `uav-sim://session/{session_id}/live-view/{live_view_id}`
- `uav-sim://control-grants`
- `uav-sim://control-grant/{grant_id}`
- `uav-sim://mission-plans`
- `uav-sim://mission-plan/{plan_id}`

`ui://uav-sim/live.html` is the only live-view App resource. There are no aliases for
the removed hosted viewer service, scene mirror, or pose protocol. The App resource discovers up to 32 managed agent IDs in the caller's tenant and Work
Context, ordered by key. Each target is Ready or Paused with an active managed generation, current service
identity, an enabled definition admitted to that context, and a current vehicle grant
for this simulation session. A requested template vehicle parameter does not qualify
an agent. The query discloses message targets, never private instructions or grant
contents. The gateway independently authorizes each human-message request.

The Apps extension carries those IDs into the authenticated host context without
giving the iframe agent credentials. Shared database LIVE subscriptions invalidate
resource discovery when lifecycle, definition, grant or identity authority changes.
Reconnect also invalidates the catalog. These notifications do not create model
wakes; ordinary domain resource subscriptions retain their existing behavior.

## World And Session Lifecycle

`configure_world` binds a session exactly once to an immutable Frames world revision and
static simulation frame. The adapter derives the WGS 84, ECEF, ENU, NED, stage, and
Cesium mappings from that binding. Runtime configuration is immutable after admission.

Always-on installations mount the already-admitted binding from an installation-owned
ConfigMap. The MCP companion parses the strict document and applies it once during
startup. It does not begin serving when the file is absent, malformed, cross-revision,
or rejected by the simulator. This is startup configuration, not durable renderer state:
there is no poller, retry scheduler, desired-versus-realized model, or periodic replay.
Installations that intentionally begin unconfigured omit the mount and use the ordinary
tool once.

Durable task tools use `interrupted_indeterminate` recovery. An unclean interruption
never replays physical work. The default fleet controller keeps the configured fleet
on its admitted loop until a later mission command takes authority.

The adapter's completion profile is one cluster-private authenticated HTTP request
and a definitive synchronous response. Its request client disables automatic retries
and redirects. A mission
receipt must name the dispatched mission, report `completed`, account for every requested
waypoint, and have ordered start/finish timestamps. The worker also correlates the
session and single vehicle against its command guard. Other operation receipts must
match the requested result kind and session. Recording catalog lookup follows this
physical observation and can fail independently.

Task cancellation stops waiting for that response. The Python runtime executes its
mission in a thread, and closing HTTP does not stop the PX4 command. Cancellation,
request timeout, HTTP rejection, malformed or mismatched completion, and Task lease loss
before a confirmed completion leave the plan executing and its vehicle fenced. An interrupted Task reports
`interrupted_indeterminate` when its worker still holds the Task lease. A stale worker
cannot settle the Task; TaskRuntime recovery applies the declared interruption profile.
An executing plan means admission is unsettled; it does not assert current vehicle motion.
Queued mission recovery fails the interrupted Task without decoding its public plan
request as a simulator command or dispatching it again.

The worker renews its 120-second Task lease every 40 seconds. Simulator requests use
the configured operation timeout; scenario and capture requests extend it to at least
twenty times their requested duration plus 120 seconds. Recording resolution permits
100 reads per key at 100-millisecond intervals. There is no operation-status poller,
resumable completion receipt, or qualified remote abort in this profile. An operator
must reconcile an unknown simulator outcome before releasing its vehicle fence.

## Scope Authorization

The contract library owns `UavScope` and the four published scope spellings:
`uav-sim:read`, `uav-sim:control`, `uav-sim:admin`, and `uav-sim:stream`.
Tool, resource, completion and subscription checks take enum values. The shared
authentication adapter compares their validated names with the caller's grants;
unrelated installation or external scopes may coexist in that set. Administrative
authority does not imply the control or stream scope. The flight and browser clients
import the same vocabulary and convert it to text when preparing token-exchange inputs.

Ordinary and Task calls use the same scope guards. Scenario execution and dataset
capture require `Admin`; mission-plan execution requires `Control` before admission
checks the plan and vehicle grant. Task checks run before request decoding, persistence
or simulator dispatch. Task ownership still governs subsequent status, cancellation
and result access. Read-only document HTTP routes require authenticated gateway identity
and establish no domain administration API.

Installation qualification requires replacing every UAV replica with this Task guard.
Rollback builds must preserve it. Scope spellings and retained Task formats are unchanged;
already admitted Tasks keep their recorded owner and recovery profile.

## Vehicle Authority And Mission Admission

An authenticated gateway principal controls a vehicle only through a UAV-owned grant.
The grant binds the exact principal key, Work Context, session, vehicle, permissions,
validity interval, and one versioned Map mobility-profile URI. Initial showcase policy
uses one principal per vehicle. This is packaging policy rather than a platform concept;
the UAV contract also supports bounded many-to-one grants when a later use case admits
them explicitly.

`inspect`, `plan`, `execute`, and `abort` are separate permissions. A caller with only
`uav-sim:control` sees telemetry for vehicles covered by a current `inspect` grant. The
`uav-sim:read` and `uav-sim:admin` scopes retain domain-wide operator visibility. Tool
metadata, an agent manifest, a chat target, or a claimed vehicle ID never creates
vehicle authority.

Mission admission has one vertical handoff:

```text
operator prompt
  -> Map MCP resolves places, applies active data, and routes
  -> Map MCP prepares veoveo.ai/map-route-handoff/v1
  -> UAV MCP verifies grant, profile, provenance, freshness, and Frames revision
  -> UAV MCP persists one principal-bound single-vehicle plan
  -> UAV MCP creates and pins the queued Task
  -> UAV MCP admits that Task, plan and exclusive vehicle command lease atomically
  -> private simulator adapter executes the admitted waypoints
```

The handoff contains WGS 84 geometry and Map provenance, but no UAV command semantics.
The UAV server converts its admitted path into the private simulator request, applies
the granted speed and destination hold, and never asks Map to execute a vehicle. Mission
plans expire after 15 minutes. Route validation may be at most five minutes old when a
plan is admitted. Every executable position requires ellipsoidal height. A
planning-advisory route is accepted only when the vehicle grant says so.

UAV imports the handoff and mobility-profile address from Map's `contract` feature.
Grant requests, returned grants, persisted plan JSON and the flight client keep that
address typed. Map owns profile-ID and version parsing. Its WGS84 position decoder
rejects undeclared fields, and UAV requires finite ellipsoidal height in every waypoint.
UAV rejects stale, invalidated and unavailable route statuses explicitly.

Grant selection applies tenant, Work Context, principal, session, vehicle, permission,
validity, profile and advisory approval in SQL before selecting one grant. Execution
repeats the profile-aware check. Its transaction also requires a current matching grant,
an unexpired prepared plan, and an unchanged retained record. TaskRuntime checks and
writes the queued, unclaimed Task in the same transaction, rejecting cancellation or
changes to its owner, request or required pins. Admission writes the vehicle lease,
moves the plan to executing and creates a unique Task-to-plan execution link containing
the admitting lease token. Rejection rolls back all these writes. Task creation precedes
admission, so an executing plan always has its retained Task identity. An uncertain
database outcome keeps the retained state fenced for reconciliation.
SQL-selected plan documents must agree with indexed identity, ownership, profile, route digest, lifecycle and timestamps
before they are returned or used for execution. These checks reject corrupt selected
records; they do not remove records from a page after SQL selection.

Execution serializes contenders through a write to the same Work Context/session/vehicle
lease record. SurrealDB 3.2.4 transaction conflicts reject concurrent decisions from the
same retained state. Admission rejects any executing plan for that vehicle, across
principals and mission IDs. A composite tenant/context/session/vehicle/state index serves
that lookup. Lease expiry does not establish that simulator work stopped and never permits
replacement while such a plan exists. Lease revisions increase across
replacement and release; exhausted counters reject mutation.

Finalization checks the exact Task link, admitting token and retained plan metadata.
It settles the plan and releases that lease in one transaction. An obsolete token cannot
settle another execution or release its lease. Repeating the same terminal outcome with
the same retained token is a no-op. Admission can replace an orphaned or terminal lease
only when no executing plan exists for the vehicle. The server preserves unknown database
outcomes for reconciliation and never replays physical work from an uncertain result.

Dispatch consumes the local admission guard. Only an undispatched guard can release
authority after a setup error; a dispatched guard requires a correlated completion receipt.
The worker commits physical completion before resolving recordings or publishing the Task
result. A recording failure produces `uav_sim_result_unavailable` while the plan stays
completed. Cancellation concurrent with confirmed completion produces
`completed_after_cancellation` when the Task cancellation transition has already won.
Neither Task delivery failure changes the settled physical outcome.

The `uav-sim:mission-execution` retention pin protects the Task through an unresolved
mission, including after its interruption failure and ordinary result TTL. A terminal
Task can release this pin when its plan is settled or when the plan is still prepared
and has no link to that Task. Other consumers' pins are preserved. At startup, SQL
selects terminal pinned Tasks whose plans are prepared or settled before its 100-row
page limit. UUID cursors and a fixed creation cutoff bound traversal of the starting
set. Exact reads validate each selected plan document before releasing the pin. The
pass has a 30-second budget; unfinished or invalid records stay pinned. This repairs
lost acknowledgements without querying the simulator or replaying a command.
An existing execution link that disagrees with Task input or ownership keeps its pin
and reports the mismatch for repair.

### Execution Storage And Recovery

Every admitted plan has one Task link, created in the same transaction as its vehicle
command lease and executing state. Reads require that link and validate its Task,
plan and caller identities. A terminal Task with an executing plan keeps its retention
pin and the vehicle fence until the physical outcome is settled. A later request for
the same plan cannot replace its admitted Task.

Native control-plane qualification covers exact mission selection across Store
connections, unadmitted and damaged-link exclusion from reads, pages and completions,
current-format interruption recovery, retained Task pins, and atomic admission failure.
The catalog fixture admits and settles current plans through the domain API before
qualifying pagination and query plans. Installed command recovery and physical simulator
behavior require their separate acceptance runs.

Restart behavior is intentionally simple. Simulator objects are runtime state. A pod
restart recreates the configured world, cameras, and products through the one-shot
installation binding. Stream authorizations disappear when the MCP server restarts, and
the App opens new authorizations. WebSocket or decoder failures start a fresh
authorization sequence with a five-second maximum backoff for selected cameras. The App
renews its resource subscription before every open attempt, so an MCP companion restart
cannot strand recovery behind a stale session subscription. A subscribed live-camera
resource update immediately retries cameras waiting for simulator readiness. The runtime
emits an
`adapter_ready` edge after its preconfiguration endpoint binds, allowing an existing
companion to reapply the same immutable installation binding after an independent
simulator-container restart. It emits a second `ready` edge after its running lifecycle
and streamed-world readiness are both current; that edge produces the subscribed resource
update. Both use a private Unix datagram. Delivery is best effort and never delays the
simulator. If the companion is absent, its later startup applies the installation binding
directly. Closing a tile or tearing down the App cancels its reconnect state. A selected
camera keeps retrying at the capped backoff until it succeeds, is deselected, or the App
tears down. There is no desired-versus-realized renderer deployment or periodic replay
controller.

The streamed-world data plane has a smaller reactive lifecycle inside the simulator.
The pinned Cesium extension emits a typed event when the ion endpoint, root tileset, or
tile content request fails. Events contain only the tileset path, load generation, load
type, and HTTP status. Provider URLs, keys, sessions, tokens, headers, and response bodies
never enter the event or projected runtime state.

A rejected tile-content session produces one generation-safe replacement. The runtime
keeps the resident native tileset mounted and preserves Cesium's persistent response
cache. Each native tileset generation bypasses the two endpoint caches for its small ion
bootstrap request, which guarantees a new provider session without deleting cached tile
content. Duplicate failures from the rejected generation collapse into the same action.
Native load completion alone does not promote the replacement. Loaded tiles, prepared
geometry, prepared materials, and rendered geometry must all increase beyond the resident
baseline and remain visible for the configured readiness window. Only then does the
runtime retire the expired tileset. A rejected or unproven replacement is removed after
the bounded 120-second registration-and-coverage window, and the lifecycle becomes
`degraded` without a replacement loop. Credential, quota, asset, and root-provider
failures are typed directly and never masquerade as a provider-session replacement.
An isolated transport or provider failure for child tile content remains observable in
`last_failure`, but it does not withdraw an already proven textured resident generation.
If rendered geometry or loaded materials disappear after that failure, visual readiness
fails closed immediately. Stable textured coverage can then restore readiness without a
cache reset or a speculative replacement.

Render statistics describe current coverage; they do not infer network failure. Zero
visible tiles can be valid while a camera crosses an unavailable footprint or while
refinement is active, so no elapsed-time or visibility threshold triggers provider work.
This lifecycle can change visual readiness, but it cannot change simulation readiness,
physics, pose flow, missions, recording publication, or an already active native camera
product.

## Authoritative Operator Cameras

Configured logical cameras are created under `/World/OperatorCameras`. Each camera has a
stable logical ID, revision, final smoothed pose, and typed atlas region. A
browser authorization never mutates that definition or its product lifecycle.

The admitted rig set is:

| Rig | Behavior |
|---|---|
| `fixed` | Applies an exact world pose without smoothing state. |
| `look_at` | Keeps the configured eye and follows a world point or entity through orientation. |
| `orbit` | Holds a configured radius, elevation, and azimuth around one authoritative entity. |
| `follow_entity` | Applies FLU eye and target offsets to the current entity transform. |
| `chase_entity` | Derives a trailing eye and target from the same current entity transform. |
| `stabilized_mounted_entity` | Composes an entity and mount transform, then smooths the operator view only. |
| `formation_overview` | Frames the current centroid and bounds of a configured entity set. |

Every rig claimed by the contract has deterministic desired-pose tests. Operator-camera
transforms never feed back into an entity, sensor, mission, or physics state.

## Camera Smoothing

Smoothing uses a frame-rate-independent exponential filter over the final desired camera
pose. Translation uses linear interpolation. Orientation uses normalized shortest-arc
quaternion SLERP.

```text
alpha = 1 - 2^(-dt / half_life)
```

The typed profile contains `translationHalfLifeMs`, `rotationHalfLifeMs`,
`teleportDistanceMillimetres`, and `resetAfterGapMs`. Zero half-life snaps the relevant
component. A target change, camera revision, simulation generation, long render gap, or
teleport resets the filter. The filter stores one previous camera pose and no entity-pose
history.

Chase eye and target calculations consume the same authoritative transform at one
physics step. Formation cameras consume one snapshot of all selected entity transforms.
This prevents camera-target disagreement without delaying simulation.

## Render Products

Isaac Sim's Experimental Camera API batches every streamable USD camera with common
optics. One RTX view-tiled Hydra product binds the complete camera relationship into a
stable texture and LdrColor AOV. The 3-column by 2-row product is 3840x1440 for five
1280x720 cameras and owns one pod-loopback RTSP port pair, H.264 identity, and NVIDIA
NVENC session. No Replicator or RGB annotator is loaded, and raw atlas pixels never
enter Python. The runtime submits every camera viewport to Cesium in the same Kit frame.
Headless operation disables the pinned
Cesium extension's interactive viewport-window update subscription, leaving one
authoritative viewport writer instead of racing an empty GUI inventory. The runtime does
not create another Cesium world, provider connection, georeference, material set, or
cache.

The atlas starts with immutable world admission and renders continuously. A native
RTSP receiver depacketizes its exact access units into a 256-entry keyframe-aware ring.
The WebSocket handler gives each viewer its own cursor, begins at the newest
decoder-reentrant keyframe, and advances without decoding or re-encoding. A slow viewer
cannot block the render loop or another connection.

The runtime timestamps the camera batch when the current authoritative entity snapshot becomes
its USD camera poses. The corresponding Hydra drawable event closes that source-to-render
interval without copying pixels to the CPU. The product retains the latest 256 samples and publishes their
nearest-rank p95 in integer microseconds. The runtime never uses a wall-clock sampler or
health poll to produce latency evidence.

Live viewing has no lease pool, admission counter, viewer quota, or camera-selection
limit. The persistent atlas is already running. Additional users allocate ordinary
WebSocket delivery and one browser decoder, never another simulator render or encode.

The domain nadir sensor and operator cameras have independent cadence. The physical
sensor camera receives the exact current body-and-mount transform at render cadence.
Cesium receives that viewport every Kit update. The sensor Hydra product renders at the
declared sensor rate and publishes its CUDA-resident `LdrColor` AOV directly to the
native RTSP extension. No Replicator orchestrator participates in simulation timing.

The manual Isaac loop advances fixed physics from elapsed monotonic time. It preserves
bounded physics debt and coalesces missed visual deadlines into one render of the newest
authoritative state. RTX, Cesium, NVENC, H.264 delivery, Recording, and Rerun work can reduce
presentation cadence under load, but none of them changes the simulation clock.

The RTSP extension performs one NVIDIA NVENC encode and serves the resulting GOP on a
pod-local loopback transport. The private adapter depacketizes RFC 6184 payloads without
decoding, copying pixels, or encoding again. It qualifies SPS, PPS, IDR, and predicted
access units, then fans each exact encoded access unit to Recording/Rerun and the
optional live RTP publisher. Raw sensor pixels never enter Python or Recording.
Every RTSP AOV also receives an explicit internal signal-port reservation adjacent to
its RTSP listener. These port pairs remain pod-local and are never viewer endpoints.

Recording Hub admits ordinary H.264 GOPs and rolls storage only at a decoder-reentrant
IDR boundary. The sensor path exposes its declared rate, monotonic observed-frame count,
last encoded size, and keyframe state. Headed hardware-backed browser acceptance verifies
the actual visual content; runtime health does not require a diagnostic pixel readback.

## Viewer Authorization And Shared Products

A logical camera belongs to the Work Context output owner and occupies one immutable
region of the continuous atlas. Opening any camera binds the gateway actor and
browser-instance identity to that camera and returns a secret stream token. Cameras,
users, tabs, and browser profiles all resolve to the same stream-product identity and
exact encoded access units.

Only `open_live_view` and `renew_live_view` return the token. Resources contain redacted
authorization state. The server retains a SHA-256 token hash in memory and compares it in
constant time. Renew rotates only that authorization. Close, expiry, and teardown revoke
only that viewer. None of those actions starts or stops the atlas.

The live-stream gate authenticates the WebSocket upgrade, maps the authorization to its
camera, and injects the private runtime credential. The runtime sends one complete Annex
B access unit per binary message from a bounded keyframe-aware ring. Each connection gets
its own cursor and backpressure, while the renderer and NVIDIA NVENC session remain one
for the entire atlas. There is no viewer quota, media relay, duplicate encode, or GPU-to-CPU pixel
readback.

Authorization expiry uses one exact deadline task. Close, expiry, revocation, and stream
teardown update only connection state. Neither path polls. Runtime health is read on
demand and announced through MCP subscriptions.

## Audit And Failure Isolation

Open, denial, close, expiry, camera mutation, and product-activation rejection produce
typed access events. The platform store appends each accepted audit record and outbox
projection atomically. Tokens, native endpoints, media, and provider credentials never
enter audit data.

An audit-store outage is logged with the typed action and authorization identity. It
cannot roll back an already-issued authorization, interrupt a viewer, or stop simulation.

The GPU Deployment starts the authoritative simulator and recording forwarder together.
The independent MCP Deployment starts whenever the platform database is reachable and
reports unavailable until the authenticated simulator adapter answers. A Gateway, MCP,
or recording outage cannot prevent or restart simulation. The simulator's
preconfiguration API exposes product state but no product mutation route. Immutable world
admission starts the configured camera atlas.

Gateway authority is evaluated for every MCP open, renew, close, and resource read. If
the same actor and browser instance presents a changed output owner, policy revision, or
data-label authority at renewal, the server closes that authorization, records
`viewer_authority_revoked`, and leaves every unrelated viewer attached to the shared
product. Other rejected renewals record `renew_denied`. Expiry invalidates the stream
connection even when the browser disappears without teardown.

## Live App

The App discovers the authoritative camera collection and opens the atlas when its first
camera is selected. Every other selected camera attaches its typed crop to that same
player. Renewal is transparent, and removing one canvas cannot interrupt the remaining
cameras. Tearing down the App closes the browser's atlas authorization.

Each tile reports requested and decoded dimensions, cadence, frame age, transport state,
camera health, smoothing profile, and attribution. Cadence values use fixed-width,
zero-padded integer labels with tabular monospace numerals, so telemetry updates do not
move or wrap the video overlay. Media Capabilities labels H.264
decode as hardware only when the browser reports `powerEfficient`; supported smooth
software decode is labeled explicitly. Browser acceptance still requires a headed,
hardware-backed WebGPU or WebGL context.

The App asks WebCodecs for hardware-preferred decode of each Annex B H.264 atlas frame,
then draws its five typed regions into independent GPU-composited canvases. It claims
hardware decode only when Media Capabilities reports `powerEfficient`; otherwise the UI
uses the documented smooth software H.264 decode label and exception.

Focused browser acceptance combines the runtime source-to-render window with reactive
canvas frame events and browser receive-to-display measurements. It rejects
source-to-render p95 at 85 ms and motion-to-photon p95 at 250 ms. The measured reference
profile schedules Kit presentation at 24 Hz, targets 16 FPS, and rejects delivery below
12 FPS. Smoothing response is reported
by the camera profile and is not counted as transport latency. The same run opens all
five cameras for five concurrent browser users, proves 25 advancing camera canvases use
five browser connections to exactly one simulator product and one NVENC session, then
proves closing every viewer leaves that product ready.

Restart acceptance keeps the same headed App document and viewer-instance identity
mounted while independently restarting the MCP pod and the Isaac container. MCP restart
evidence requires the simulator pod UID, container ID, and restart count to remain
unchanged. Each recovery must produce a new authorization, advancing native video, and
unchanged hardware-browser evidence. The atlas product identity remains stable.

The MCP image uses a minimal PID 1 shell that forwards pod termination to one
Rust child and exits with that child. Restart acceptance terminates that sole child
through `kubectl exec`, which lets kubelet restart the MCP container without replacing
its pod or the authoritative simulator Deployment. No restart endpoint or public
control surface exists.

Physical-camera state includes a bounded `render_pose` agreement measurement after the
first rendered frame. It reports the rendered ENU position and forward direction beside
their position and angular error from the authoritative body-and-mount pose. Absence
means that no rendered frame is available yet; it is not synthesized from simulation
state.

## Deployment

The UAV chart deploys one GPU runtime Deployment and one independent MCP Deployment.
Only the Isaac container requests one GPU and receives
`compute,graphics,utility,video` NVIDIA capabilities. Pod-loopback RTSP port pairs are
derived from the configured camera collection and validated by the chart.

The runtime Service exposes the authenticated adapter and private H.264 WebSocket to the
MCP pod. The MCP Service owns MCP HTTP and the public authenticated live-stream gate.
Separate network policies admit
only those edges, platform dependencies, DNS, and the configured public TLS world
provider. Provider and adapter credentials come from distinct installation-owned
Secrets and never appear in Helm-rendered ConfigMaps or MCP state.

The runtime retains its latest typed lifecycle edge and exposes it on the authenticated
NDJSON stream. The MCP consumer reconnects only after transport failure, receives the
latest edge immediately, reapplies the immutable binding after `adapter_ready`, and
projects final `ready` through the subscribed live-camera resource. A missing consumer
never blocks the runtime.

The platform chart contains no generic simulation renderer, pose ingress, mirror cache,
or live-view GPU workload. External simulation implementations package the same
domain-owned boundary in their own release.

## Recording Isolation

Recording publication is asynchronous and bounded. Queue pressure may drop recording
events according to the recording policy, but it never delays physics, camera transforms,
or operator rendering. A Recording Hub, Rerun, object-store, or browser failure changes
recording health only.

The recording producer emits one recording store plus an associated producer Blueprint.
The Blueprint selects the fleet, leader camera, and map views. Viewer-local layout changes
remain separate from that producer default.

## Conformance And Acceptance

Deterministic coverage proves strict camera parsing, all rig computations, half-life
behavior at several frame rates, shortest-arc normalization, reset rules, stable product
identity, 25-viewer fan-out, token rotation, expiry, WebSocket authorization, and App
teardown.

The external fixture proves that another simulation server can own its camera/product,
viewer, stream, and App contract without importing an Isaac-specific renderer or a
pose-mirroring protocol. First-party visual certification remains the GPU UAV showcase.

Hardware acceptance requires one NVIDIA GPU, a headed hardware-backed browser, RTX
rendering, NVIDIA NVENC, advancing H.264 frames, several authoritative cameras, correct
Cesium alignment, shared-product multi-viewer evidence, and no software renderer,
encoder, or media relay. It also proves delivery of at least 12 FPS, source-to-render
p95 below 85 ms, and browser receive-to-display composition below 250 ms from reactive frame
events. The smoke entry points
are:

```sh
cargo xtask smoke uav-showcase-up --context <context> --public-base-url <url>
cargo xtask smoke uav-showcase-browser-verify \
  --public-base-url <url> \
  --chrome-cdp-url http://127.0.0.1:9222
cargo xtask smoke uav-showcase-live-restart-verify \
  --context <context> \
  --public-base-url <url> \
  --chrome-cdp-url http://127.0.0.1:9222
```

The Python camera and runtime suite runs with:

```sh
PYTHONPATH=showcase/uav-sim/runtime:sdk/python/src \
  uv run --with numpy==2.3.1 --with aiohttp==3.14.1 \
  --with pymavlink==2.4.49 --with fastcrc==0.3.6 --python 3.13 \
  python -m unittest discover -s showcase/uav-sim/runtime/tests -v
```

## Contract Compliance

The normative target is MCP contract revision 3. The [agent manual](AGENTS.md#contract-compliance)
records each requirement. Local and reference gateway registrations declare revision 3.
Installed readiness qualification is pending.

Mission admission retains its exact Task identity before dispatch. Unknown outcomes
preserve that plan, lease and Task pin. Retained completion details across process loss
and installed recovery qualification remain work in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md).
The synchronous adapter profile does not provide a remote abort or resumable observation
that could settle such an outcome automatically.

## Resource Query Modules

`server/control_authority/reads.rs` owns SQL grant and plan selection.
`server/control_authority/task_link.rs` owns execution read profiles, typed Task links
and SQL retention selection. `task_link_tests.rs` qualifies migration reapplication,
correlation, rollback and filtering before page limits against an isolated Store.
`server/task_index.rs` owns Task usage and mission correlations. `contract/resources.rs`
owns addresses and `contract/resources/cursors.rs` owns collection cursors.
`server/setup.rs` owns checked protocol declarations. `server/index.rs` assembles pages,
and `server/resources.rs` composes caller discovery, reads,
completion, and subscription admission. `server/bootstrap.rs` constructs the service,
wires HTTP, and owns observer shutdown. `server/catalog_tests.rs` qualifies these
paths against a disposable pinned Store; it performs no simulation or GPU work.
