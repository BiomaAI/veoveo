# Installed Source Harnesses

## Standards And Protocols

| Contract | Supported profile |
|---|---|
| MCP `2026-07-28` | Official Rust SDK Discover and Streamable HTTP through the installation's public gateway |
| `ai.veoveo/knowledge-source` | Shared K01–K08 checks for one explicitly selected source; K09/K10 require owner review |
| `veoveo.ai/mcp-conformance-report/v1` | Source-only requirement results and the observed endpoint implementation |
| Installation target | Repository-owned typed public origin, Kubernetes coordinates and expected workload names from `veoveo-deploy-contract` |
| Kubernetes Deployment API | One requested rolling restart, native rollout/deletion watches and typed before/after readiness checks |
| `veoveo.ai/time-authority-acceptance/v1` | Private owner journal for isolated authority acquisition, guarded activation, resource invalidations and immutable epoch reads; no public protocol extension |

## Ownership

These helpers serve owner-local installed tests. `knowledge.rs` reads typed fixture
inputs, admits private token files, connects through the official SDK and writes the
source report. `restart.rs` controls one Deployment named by the installation target.
`tools.rs` sends typed owner requests through checked gateway tool names.
They do not choose domain data, grants, service names or expected search results.
Owner tests outside source conformance can select their own fixture environment
variable through `input_from`; the same path and size checks apply.
`knowledge::bearer_header` admits a private token file for owner HTTP requests and
returns a sensitive Authorization header. Malformed header diagnostics omit the token.

Each server's integration test owns those choices and imports its library types.
Artifact owns the grant mutation driver that its metadata test and Reason's finding
test share. Native fixtures under `testing/fixtures` stay independent of installation
identities and credentials.

## Inputs And Commands

Set `VEOVEO_SOURCE_CONFORMANCE_INPUT` to an absolute JSON file path. Files are limited
to 64 KiB and deserialize through the owner's closed input type. Artifact accepts
`artifact` and `grantee` fields; Reason accepts `analysis` and `grantee`. These grant
cases also require an `administrator` object with `endpoint` and `tokenFile` fields.
Every owner requires this `installation` object:

| Field | Value |
|---|---|
| `installationTarget` | Absolute path to a valid installation-target JSON file |
| `endpoint` | Public HTTPS MCP endpoint for the ordinary caller |
| `callerTokenFile` | Absolute path to a private regular file containing that caller's bearer token |
| `deployment` | Expected Deployment name from the target |
| `output` | New absolute report path in an existing output directory |

Caller and administrator endpoints must match the target's public origin. Token files are limited to
64 KiB, must be nonempty and must have no group or other permissions on Unix. Tokens
never enter command arguments or reports. HTTP requests disable redirects and use
ten-second connection and 65-second request timeouts. Supply credentials that remain
valid through the run and cleanup.

The caller needs source read scopes and access to all fixture members. The
administrator needs Artifact grant and revoke permission. Choose a disposable
Artifact or completed Reason analysis, and a subject with no existing grant to its
selected Artifact. The harness refuses an existing grant before it mutates anything.
The selected Deployment must have positive replicas with every requested replica
updated and available. Its selector must name the component fixed by the owner test.
All declared source collections need populated members for read qualification.

Time accepts an `events` array containing two distinct event IDs. Both events must
start scheduled, remain in the future during qualification, belong to the caller
and have names beginning `Source conformance `. The fixture cancels one event before
restart and the other afterwards. Cleanup cancels both and verifies their final state.

Map accepts `layer`, `feature`, two `publicationLayers` IDs, and `releases` containing
`dataset`, `original` and `candidate` IDs. The three
layers must differ and their titles must begin `Source conformance `. The first
layer contains the mutable feature. The publication driver publishes one of the other
layers before restart and the second afterwards. It dispatches each publication once
and returns its typed knowledge URI. The shared checker verifies both immutable
members. Cleanup archives all three layers through the version-checked tools;
Map preserves the publications of archived layers.
The releases must form an active/staged pair from one source in a disposable
non-routing dataset. Its registered source must be an Authority Vector with
`synthetic_test` authority and a name beginning `Source conformance `. Acquisition
supplies the normal digest-based release labels. The probe
activates the candidate and restores the original after restart. Cleanup restores
the original pointer and refuses to overwrite an unrelated release selection.

Map also accepts a typed `search` request, `expected` geography members and a
`restrictedTokenFile`. The restricted caller must be authenticated at the same MCP
endpoint but lack the source's dataset-read authority. The shared checker requires
tool denial and full/conditional denial of every expected hit. Ordinary searches
must return the supplied geography members exactly.

Run one owner case at a time:

```sh
cargo test -p veoveo-artifact-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-reason-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-time-mcp --test gateway_source_conformance -- --ignored --nocapture
cargo test -p veoveo-map-mcp --test gateway_source_conformance -- --ignored --nocapture
```

These tests require the `mcp` feature and are ignored by ordinary native test runs.
They need network access to the public gateway, `kubectl` access to the selected
context and namespace, and ready source dependencies. They perform no inference;
a reused GPU analysis keeps its original execution evidence.

## Time Consumers

`servers/time-mcp/tests/gateway_consumers.rs` selects a separate closed input with
`VEOVEO_TIME_CONSUMERS_INPUT`. Its `installation` uses the fields above and selects
`time-mcp`; this read-only case does not inspect or restart that Deployment. Supply
the expected `authority`, existing versioned `calendar` and `epoch`, two to sixteen
`resolutions` with typed `request`, `expected` instant and `expectedUtc`, and distinct
`zoneIds` and `scales`. Resolution vectors include both subsecond endpoints. The
calendar contains recurrence windows and the epoch and expectations bind the selected
current authority. Keep that authority unchanged throughout qualification.
The `source` is an existing typed HTTPS `TimeSource`. Supply `administrator` with
the installed admin `profile` and private `tokenFile`; the owner derives its read-only
source route under the same public Gateway origin.

Run local admission controls with `cargo test -p veoveo-time-mcp --test gateway_consumers`.
The installed case is ignored and runs through `cargo xtask smoke time-installed-consumers`
only when installation access is authorized. Its create-new private receipt uses
`veoveo.ai/time-consumers-acceptance/v1` and records each completed check, failure
category and separate connection cleanup. Typed request traces retain complete-response
and error digests, available MCP codes and HTTP statuses without response bodies or tokens.
Interrupted body reads record status without a complete-response digest.
MCP response digests cover re-encoded admitted typed values; admin HTTP digests cover
the complete received body bytes.
The separate `time-installed-schedule-task` scenario runs
`schedule::typed_schedule_task_through_public_gateway` in the same owner harness.
Set `VEOVEO_TIME_SCHEDULE_TASK_INPUT` to a closed fixture containing `installation`,
`authority`, a typed `request` (`ExpandScheduleRequest`), independently supplied
`expected` (`ExpandScheduleOutput`) and explicit `mode`: `complete`, `cancel` or
`recover`. The caller profile must expose official Tasks and the
`time__expand_schedule` tool, with `time:read` and `time:schedule` grants.
The selected versioned calendar must already exist and equal `request.calendar`.
The current authority must equal `authority` before dispatch and after successful
completion. The horizon binds that authority and spans at most 31 days. Select one
to eight calendar windows with recurrence count 1–1,000,000 and interval 1–31;
`maximumOccurrences` stays within 1–512. Expected occurrences are nonempty,
untruncated, ordered, sequenced from zero and contained in the horizon. The harness
compares the independent expectation with delivered and current results; it does
not calculate the expected output through the Time engine. A historical UTC daily
recurrence can scan one million dates while returning a small result near the end
of that recurrence. Its execution time depends on the installation and supplies no
guaranteed unfinished observation window.

Every mode creates one Task through the official SDK and acknowledges an exact
`subscriptions/listen` filter for its opaque Gateway Task ID. Completion requires
a delivered Completed notification with the matching subscription identity and
agreement with current `tasks/get`, creation identity and independent expected
output. The final Task protocol carries results in `tasks/get`; it has no separate
`tasks/result` request. In `complete` mode an initial terminal notification can
satisfy delivery. The receipt records the first delivered status and whether
Working preceded Completed, distinguishing a terminal baseline from an observed
transition.

Both `cancel` and `recover` require a delivered Working notification and a current
Working read for the original acknowledged Task and creation timestamp. An early
terminal observation fails qualification. Cancellation persists its intent before
calling the official cancellation method, then requires delivered and current
Cancelled payloads with the original identity and no successful result.

Only `recover` accepts a `recovery` object. It contains `target` (`CrashTarget`)
and `replacementTimeoutSeconds` within 1–300. The target supplies `deployment`,
`pod`, `container`, `namespaceUid`, `deploymentUid`, `replicaSetUid`, `podUid`,
`containerId`, `imageId` and `restartCount`. Its Deployment must match the supplied
installation and have one Ready replica; its component and Rust server container
role must be `time-mcp`. The driver admits the selected live identities before Task
creation. The harness retains the crash watch before awaiting its initial state,
then rereads the original Task as Working before syncing its armed marker. Ops may
signal only that selected process after the persisted Task/watch handshake.
Alternatively, Ops may select the closed `recovery.runtimeSignal` profile with
`nodeName`, immutable 64-hex `nodeContainerId`, `nodeHostPid`,
`nodeHostStartTicks`, `taskPid`, `hostPid` and `hostStartTicks` (positive integers).
This profile requires Linux host process metadata and existing Docker access.
Select one signal owner. An external driver must not signal when `runtimeSignal`
owns dispatch, even after it observes the armed marker.
Ancestor proc metadata is read with fixed Docker exec arguments. It binds the Pod's node and UID, Docker node container and process start,
containerd `k8s.io` task ID/PID/Running state, and host/ancestor `NSpid` and start
ticks. The task ID comes from the selected `containerd://` container identity.
All slow preparation finishes before a fresh OAuth current read verifies the
original Task ID, creation time and Working payload. The private journal syncs
signal intent before one fixed `docker exec <node-container-id> ctr -n k8s.io
tasks kill --signal SIGKILL <task-id>` dispatch. The command shares the original
operation deadline and the native command cleanup owner. Failed or timed-out
command settlement preserves an unresolved signal intent and prohibits replay;
command success alone does not prove a crash. The final read and OS signal are
not atomic, so completion during that interval still refuses qualification.
The watch requires the old container's
exit and a Ready replacement in the same Pod. A subsequent current read must still
show the same Task Working. The harness then acknowledges a deliberately separate
replacement listener and requires later delivered Completed/current result
agreement, followed by the final replacement-target fence. A completed Task
surviving replacement cannot qualify unfinished recovery.

The create-new owner-private JSONL receipt uses
`veoveo.ai/time-schedule-task-acceptance/v2`. Each appended line is a complete
snapshot. Dispatch, cancellation and crash-watch intent are synced before their
corresponding effects. The acknowledged opaque Task ID and creation metadata are
retained before subsequent effects. The receipt preserves Working observations,
partial replacement facts, safe failure codes and digests, and unresolved outcomes
without raw errors or credentials. The shared owner lifecycle registers remote
Task cancellation/reconciliation and retains uncertain intent or unresolved
identities in its private ownership lease. Terminal settlement discharges that
remote obligation.

The operation sets its deadline once before connecting: 120 seconds for
`complete`/`cancel`, 300 seconds for `recover`. Time's worker lease lasts 120 seconds
and renews every 40 seconds; recovery must allow the old lease to expire before
another worker resumes calculation. Connection has a 75-second cap, and all nested
waits end by the original operation deadline. The selected replacement timeout
cannot extend it. Private input cannot increase these operation budgets. The
scenario allows 360 seconds of execution and one separate 30-second owner cleanup
grace.

Actual caller, original and replacement listeners, and crash-watch handles live in
retained mutex slots outside cancellable work. A cleanup action registered before
connection or Task effects closes those handles after work drops, including errors,
timeout and owner cancellation. Consuming close futures keep their original
identity and deadline across interruption. Each close has a ten-second cap within
the owner's remaining cleanup grace. Re-entry after the original cap expires fails
permanently, even if the future has since become ready. Remote Task reconciliation
and local closes share that grace. Exhaustion or failure leaves cleanup unresolved
in the ownership lease; the final receipt requires successful caller and listener
closes, including the replacement listener and watch when selected.

Neither consumer scenario activates authority releases. Authority activation,
conflict rollback and selected cross-replica routing need separate installed
qualification. Prepared cancellation and recovery modes establish acceptance only
when their required Working, replacement and terminal observations actually occur.

## Time Authority Acquisition And Activation

`cargo xtask smoke time-installed-authorities` selects the ignored authority case
in Time's existing `gateway_consumers` harness. Set `VEOVEO_TIME_AUTHORITY_INPUT`
to its private closed input. Run it only in a dedicated installation and tenant
whose source catalog and persisted active-authority pointers are empty. The
effective initial references come from that installation's packaged bootstrap.
Operations verifies the ordinary reader and administrator OAuth identities before
admission; the fixture records that attestation rather than decoding bearer claims.

| Input | Required values |
|---|---|
| `installation`, `administrator` | Selected installation, public Gateway endpoint, private reader/admin token references, admin profile and new journal path |
| `isolation` | Dedicated namespace, tenant, Work Context and verified reader/admin principal IDs; namespace and context must match the installation target |
| `initialAuthority` | Independently verified effective bootstrap pair |
| `tzdb`, `leaps` | Typed `CreateSourceRequest`, independently verified HTTPS product digest and version label, and three distinct acquisition keys per family |
| `epoch`, `epochIdempotencyKeys` | Version-one epoch bound to the bootstrap pair and two distinct publication keys |
| `relativeOffsetNanoseconds`, `expectedRelativeTaiNanoseconds`, `expectedUtc` | Independent physical arithmetic and UTC expectation, supplied before execution |

The fixture creates two sources and four acquisition jobs: one TZDB candidate and
three leap-second candidates. It checks an acknowledged acquisition's idempotent
replay and rejects changed input under the same key. Each first activation uses
the absent-pointer guard, then verifies pointer version one and release version
two. Two concurrent leap activations share one pointer guard; exactly one succeeds,
the other returns a typed version conflict, and the TZDB selection stays unchanged.
The previous leap release becomes retired while the losing candidate stays staged.
A stale activation must preserve both selections and release records.

The exact-authority subscription records its acknowledged initial snapshot
separately. Later resource invalidations accompany uncached reads of the expected
changed authority binding and acquisition provenance. Notifications carry no
mutation revision, and Time also emits broad reconciliation invalidations; this
check does not attribute a notification to one particular activation.
Epoch version one must keep its original binding and fail relative resolution
after that authority becomes inactive. An explicitly published version two keeps
the independent physical instant under the new pair. Latest/versioned reads,
resolution and conversion must agree with the supplied physical and UTC values.

One 900-second operation deadline covers connection, requests and concurrent work.
Each acquisition allows at most 66 correlated status reads spaced five seconds
apart within a 330-second observation interval and the original deadline. Admin
requests have a thirty-second cap; the whole fixture allows at most 512 requests.
The scenario provides 960 seconds and one thirty-second cleanup grace. Its private
append-only journal records dispatch intent, typed acknowledgements, safe response
status/code/digests and unresolved identities. Retained SDK close futures keep
their original caps. Operations owns retirement of sources, releases, epochs and
unresolved jobs; the fixture does not reset authority state or revive retired
releases. Acquisition interruption, restart and replica behavior require their
separately selected qualification.

## Restart And Cleanup

The readiness-only restart helper verifies the target workload and records its current Pod names.
It dispatches `kubectl rollout restart` once, waits for rollout completion and for
every old Pod to disappear, then checks the same Deployment UID, a newer generation
and unchanged positive replica count. It then reads the owner's contract through the
public Gateway connection. An explicit MCP request bypasses the SDK resource cache.
Service routing can lag Pod readiness, so transport and internal errors permit at
most forty read attempts, 250 milliseconds apart, within ten seconds. Other protocol
errors fail immediately. The read must return the selected URI's nonempty text body.
K07 checks retained state and dispatches its second mutation only after this read.
A restart has a total deadline of 75 seconds, including the
55-second rollout watch, 15-second deletion watch and public-read readiness deadline.
Cancellation kills the local
command; it does not repeat a mutation with an uncertain outcome. Command failures
report status without arbitrary process diagnostics that could expose credentials.

The optional selected-container API in
[testing support](../support/DESIGN.md#installed-process-drain) additionally admits
namespace, Deployment and Pod identities through the ReplicaSet owner chain. It
establishes the old Pod watch before a UID/resourceVersion-tested JSON Patch,
requires actual successful container termination inside configured grace, and
returns a typed drain receipt. Existing readiness-only consumers cannot claim
process exit from Pod deletion. The receipt qualifies server-process drain; GPU
throughput and visual acceptance belong to their owning scenario.

The source runner has a fifteen-minute deadline and each K07 probe has 90 seconds.
Cleanup runs after the source deadline expires; an outer test timer must not cancel
that reconciliation. SDK connection shutdown has a separate ten-second limit.
Artifact grant cleanup has
30 seconds: read the selected subject's current state, revoke it if present and
verify absence. This reconciliation also runs after a timed-out source run or
uncertain grant response. An externally killed test needs operator reconciliation
of the explicitly selected subject; no destructor claims asynchronous cleanup.
Time reconciles its two events concurrently within 30 seconds. Map allows two minutes
for cleanup, with 35 seconds for release restoration and 25 seconds for each layer.
It attempts every cleanup even if a previous operation failed.

The report uses a create-new, owner-private file and is synced before checking its
result. Each owner writes the available report before cleanup and defers any report
error until cleanup and connection shutdown have both been attempted. Failed
requirements remain inspectable when reconciliation fails. Transport failures and cleanup
failures fail the test. A source report records K checks only; full hosted-server
certification, access-policy coverage, Knowledge retrieval and GPU acceptance each
have separate owning harnesses.
