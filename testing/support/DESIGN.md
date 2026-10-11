# Testing Support

Shared testing support admits owner declarations and compiler artifacts, connects maintained protocol clients, and owns local process cleanup.

## Standards And Protocols

| Standard or format | Supported profile |
| --- | --- |
| Cargo metadata and JSON compiler messages | Actual package, native target, selected features, emitted executable and linked native library paths; no guessed target directory |
| Kubernetes core/apps v1, JSON Patch RFC 6902 | Selected namespace/Deployment/Pod identity, atomic UID and resourceVersion tests, native Pod watches and typed container termination; no deletion-only process-exit claim |
| RFC 3339 timestamps and Chrono 0.4.45 | UTC-normalized Kubernetes finishedAt/deletionTimestamp arithmetic at the API’s second precision |
| MCP | Maintained RMCP client mechanics and the repository's final Task adapter; bearer credentials supplied by the owner |
| Linux process groups and `/proc` | PID plus start-time identity, unreaped leader reservation and owned descendants |
| `veoveo.ai/smoke-scenarios/v1` | Checked tracked owner descriptor, explicit preparations, execution deadline and cleanup grace |
| `veoveo.ai/smoke-artifacts/v1` | Compiler-observed package/target graph and SHA-256 hashes for executables and required libraries |
| `veoveo.ai/framework-outcome/v1` | Exact one-case maintained libtest, pytest or node:test outcome; zero skipped cases |
| `veoveo.ai/smoke-client-failure/v1` | Private typed HTTP/MCP observations bound to endpoint, arguments, bearer and MCP message digest; camelCase fields |
| `veoveo.ai/smoke-launch/v1` | Private closed launch intent/state and optional admitted PID/startTicks group; camelCase fields |
| `veoveo.ai/smoke-stop/v1` and `veoveo.ai/smoke-teardown/v1` | Private requested cleanup bound to process identity, invocation and fresh stop ID; cleanup settlement does not imply scenario success |

## Ownership

Descriptors contain no shared list of production domains or scenario names. Tooling discovers tracked and nonignored declarations under each owner's `smoke` directory and checks their manifests against Cargo metadata. Installed production registry fixtures belong to their Gateway or Bioma composition. Reusable transport, generic schema checking and restart/client helpers stay here.

Tooling groups roots only after admitting their full normal/build and normal/test-dev effective feature sets. Artifact receipts bind compiler-observed package identities, target kinds and both feature projections. The loader requires equality with those admitted sets, hashes the emitted executable and each recorded native library with the owner SHA-256 constructor, and rejects replacement before use. Explicit path overrides must identify the same receipt. Listing and execution use the same runtime library configuration.

Each fresh admission hashes each canonical executable or library path once with a 64 KiB streaming buffer. Repeated declarations must agree on the expected digest. The map is local to that admission; later calls rehash every unique file. Binary command preparation selects the observed package/target/kind and configures its declared runtime from that same freshly admitted manifest.

Artifact receipt readers consume at most 4 MiB plus one byte and reject larger inputs before parsing. The seven-artifact Gateway schema selection occupies about 1.4 MiB with its complete per-entry compiler and feature graphs. Size admission preserves executable, native-library and graph checks.

Descriptors and artifact receipts use camelCase controlled fields and closed version tags. Cargo JSON keeps Cargo's upstream field spellings at its decoder. Ordinary enum values use snake_case. There are no old-key aliases for these initial formats.

Language preparation requires actual package, source and lock identities. Python module admission supports declared Hatch wheel package roots and checks selected uv extras and dependency groups against the owning lock entry. UV projects may use an owner-local lock or the nearest explicitly declared workspace lock. Workspace admission supports exact contained member paths and binds each selected member’s name, version and editable/virtual source to that lock. Glob members, exclusions, duplicate members, symlink layouts and shadow member locks are refused before preparation. Workspace preparation runs locked sync at the admitted root with the selected member’s `--package`; member extras, groups and module roots keep their own admission. Pytest selects an owned Python source file. Node selects an owned module or test file with package-lock version 2 or 3 whose root identity and dependencies match package.json. Optional checked executionOwner delegation binds another component's manifest and contained source; its reason supplies review context without granting authority.

## Cancellation And Results

Preparations, exact framework listing and owner execution consume one declared deadline D. Each local command runs in an owned Linux process group. Cancellation or expiry begins one cleanup grace G. Nested local group leaders register PID and process start ticks; leaders are not reaped before their groups drain. Repeated signals cannot extend G. Exit with surviving children, changed process identity, forced termination or incomplete cleanup fails dispatch. Remote operations retain the owner's reconciliation policy.

`process::output_async` applies its caller timeout within the owner's execution
deadline. One absolute caller deadline starts before launch and covers admission
and output. Gate admission runs outside Tokio polling; an awaiting-future cancellation
flag supplements parent cancellation before payload release. The blocking handoff owns
the returned child guard, allowing checked cleanup even when its receiver is dropped.
The awaiting future owns the command. Timeout, cancellation or a failed
wait kills that command's reserved process group immediately, then allows at
most one second for group drain and leader reaping, capped by the owner's
cleanup end. Launch, awaiting cancellation and guard handoff share the captured
owner and one latched command cleanup allowance. Output waits check that original
owner's cancellation and execution deadline. A cleared or different active owner
cannot refresh either limit. Expired handoffs kill the reserved group and preserve
its registration for reconciliation. Private native fixtures pass their own lease
root and cleanup context through the same gate. Successful output has already reaped the leader. Ordinary child
guards and asynchronous children created directly by `spawn_async` keep their
configured graceful Drop policy. An unresolved forced drain preserves its
registration for owner reconciliation. Native controls use an owned shell and
descendant that ignore SIGINT and SIGTERM to qualify timeout and cancellation.

`AsyncChild::cleanup_until` observes an already admitted process group after
operation cancellation. Both launch paths carry the admission owner's Arc through
handoff. The first caller deadline is capped by that owner's cleanup end;
subsequent calls can only shorten it. The method kills the known group and reuses
WNOWAIT observation, registration removal and leader reaping after all descendants
exit. An interrupted wait can resume within the same cap. Failed, expired,
cleared-owner or replaced-owner cleanup preserves its failed state and registration.
After explicit cleanup begins, fallback Drop uses that captured cap, including when
a different owner is active. Ordinary Drop behavior applies before explicit entry.

Framework dispatch verifies one exact selected case and reads maintained framework outcomes. Libtest admission checks the executed case name and status as well as summary counts. Extra selection-changing arguments are refused before preparation. Missing, ambiguous, zero-case, skipped and failed outcomes are errors. Failed cleanup or result admission keeps private diagnostic files and exposes only their paths. Successful exit cannot substitute for framework execution.

## Qualification

### Unfinished Task Recovery

`FinalTaskSmokeClient::run_tool_delivered_recovered` creates one official MCP Task.
Its synchronous creation observer receives the admitted ID and full creation record
before another wait. An exact Task subscription must acknowledge the requested filter,
deliver Working and agree with a current `tasks/get` identity and creation time.
The owner's `WorkingTaskRecovery` checkpoint then proves a real process replacement.
Provider identity, crash signalling and independent domain assertions belong to that owner.

After the checkpoint, the helper closes the original SDK generation and reconnects
once through the same public caller configuration. The original Task must still be
Working with the same ID and creation time. A required synchronous observer records
that verified state before the replacement subscription opens. Completion requires
delivered Completed and a matching current result. No recovery path repeats
`tools/call`. A Task that completes before either Working gate refuses this profile.

The supplied total timeout starts before connection and is shortened by the active
owner deadline. Its final two seconds are reserved for SDK cleanup. Both generations
register their actual handles before subsequent waits, and consuming close futures
survive interruption. Cleanup latches its first cap, preserves errors and refuses to
poll an expired close even when that future later becomes ready. An already-ended
subscription follows the pinned SDK's normal cancellation semantics; transport and
join errors fail cleanup. The existing delivered-completion helper keeps its API.

Installed MCP clients bound HTTP connection establishment to ten seconds. They do
not impose a total response-body or read-idle timeout on subscription streams.
The caller's operation and discovery deadlines bound work, and the original owner
cleanup deadline bounds cancellation and closure. Ordinary source callers limit discovery to thirty seconds. Source conformance
owners start their fifteen-minute operation interval before connection and preflight;
the Artifact sharing owner starts its three-minute interval at the same point.
Clients and admitted mutation drivers stay outside cancellable work. One cleanup
interval reserves its final ten seconds for client closure: forty seconds for grant,
Time and sharing fixtures, and 130 seconds for Map restoration. Failed preflight
permits client closure but does not authorize fixture mutation. SDK request options
and control transport deadlines keep their existing semantics.

Existing owner assertions and independent onboarding controls exercise declaration rejection, source containment, compiler artifact tampering, hashed/external targets, native library drift, exact framework selection and cancellation. Protocol, installation, provider and hardware prerequisites remain with those owning harnesses.

The ignored `framework_execution` integration case requires Node and the locked independent-fixture Python dev environment. Explicit selection executes actual passed, skipped, failed and missing cases through both maintained frameworks; model-only count checks cannot replace that gate. It requires no service or graphics context.

## Owner Cancellation And Fixture Settlement

The dispatcher supplies one absolute execution deadline through `VEOVEO_SMOKE_DEADLINE_UNIX_MS` and one cleanup duration through `VEOVEO_SMOKE_CLEANUP_SECONDS`. Standalone owner utilities declare a five-minute operation deadline and twenty-second cleanup duration. Owner entrypoints install SIGINT and SIGTERM observation before polling their operations. The first signal closes effect admission and fixes cleanup's end at the earlier of signal time plus G and D plus G. Repeated signals do not change that end.

Linux launch uses a parent-controlled pipe handshake. An owned helper thread consumes the configured `Command`, preserving its executable, environment, working directory and descriptors. Its audited `pre_exec` hook performs raw async-signal-safe reads, writes and closes on preallocated descriptors. The parent admits PID and `/proc` start ticks, publishes complete camelCase registration bytes atomically, and then releases execution. An abort byte also reaches a late fork. Startup consumes D; it does not create another execution interval. An unresolved launch fails with retained identity for reconciliation. The supervisor leader has a private PID/start-tick receipt separate from nested group registrations. WNOWAIT observation reserves its identity until the local group drains; forced termination alone cannot authorize reaping.

Private cleanup receipts retain dispatch intent and optional observedIdentity. Registration accepts an observed identity once and refuses rebinding or changes after settlement.

Owner cancellation actions use their existing protocol clients. Browser targets and remote operations keep their identities until their own close or settlement acknowledgement succeeds. The wrapper awaits checked cleanup during G, including on successful operations. Failed or expired cleanup leaves private unresolved receipts; forced local termination is failure and cannot settle remote outcomes.

Docker fixture creation publishes an owning intent before `docker run`. The owner's private `--cidfile` supplies the full created container ID. A name never authorizes deletion. Checked cleanup removes that admitted ID and propagates refusal or interruption. After the owning cleanup interval, Drop retains the private creation identity for reconciliation and starts no further deletion. Existing user containers are outside this ownership.

Private peer-failure reports separate supplied endpoint, argument digest and bearer digest from observed SDK HTTP status or MCP error code. Owner fixtures declare the expected response and issued claims. No receipt is produced for parser, timeout or transport failures. Negative probes disable redirect following, preventing an unrelated endpoint from satisfying the selected-route assertion. Reports exclude bearer bytes and response bodies.

Local process controls exercise launch publication, group draining, cancellation and fake-provider cleanup. Remote/browser cleanup and owning authorization controls retain their declared runtime prerequisites.

Intentional fixture teardown uses private `veoveo.ai/smoke-stop/v1` requests and `veoveo.ai/smoke-teardown/v1` receipts. Both bind PID, start ticks, owning invocation digest and stop ID. The first cancellation cause cannot be relabelled by a later stop request. A settled receipt reports cleanup only; the cancelled scenario still returns failure. The parent checks local group drain separately before accepting the fixture's teardown. Missing or mismatched receipts, unresolved remote/browser identities and ordinary exit failures refuse settlement.

### Native source and framework admission

Every selected Cargo root and native prerequisite supplies its own contained regular Cargo manifest and target source. Package identity agrees with maintained metadata before feature planning or preparation. A composition declaration cannot exempt a prerequisite from its own file admission.

Exact libtest execution uses normal capture with `--show-output` and the pretty formatter. Framework case status precedes displayed owner diagnostics; the final framework summary follows them. Named-case admission reads the status section and the final summary, preserving serial printing and stdout while refusing missing, ignored, failed or differently named execution. Extra selector arguments are refused before preparation.

## Installed Process Drain

The optional selected-container restart profile admits the namespace UID, Deployment
UID and resourceVersion, and the old Pod UID and resourceVersion before mutation.
The Pod’s controlling ReplicaSet must reference that same Deployment UID. The
selected regular container must be running and ready, with a declared termination
grace. The NVIDIA profile additionally requires positive GPU requests and limits;
ordinary server drains make no GPU claim.

A native resourceVersion-zero Pod watch supplies the initial selected object before
one JSON Patch tests the Deployment UID and resourceVersion and changes its restart
annotation. Existing annotations pass through private stdin. A failed response does
not permit another mutation. The observer requires a successful selected-container
termination with finishedAt inside both deletion grace and the caller’s shorter
admission window. Kubernetes timestamps use RFC 3339 second precision. The admitted containerID and restartCount bind the process instance. One full
same-instance Pod snapshot must contain both successful termination and deletion
metadata. Regressing process state, disappearing deletion metadata and repeated
opaque resource versions fail; the observer never orders resource versions.
Deletion or Ready alone cannot qualify exit. Watch errors, gaps, premature closure and missing terminal state fail.
Owned process groups are cancelled on every exit path. Replacement readiness and
public routing are checked independently after drain.


The coordinated selected-Pod profile accepts two to eight distinct regular-container
profiles. Every member shares the admitted namespace, Deployment, ReplicaSet and
Pod identities. Individual members select ordinary-server or NVIDIA resource
admission. One watch supplies the full initial Pod snapshot before one fenced
patch. Each member must report exit code zero for its admitted containerID and
restartCount inside its own deadline and Pod deletion grace. Partial successful
exit facts survive a later member failure; group success requires every member.
A selected group permits one attempt, including after an ambiguous patch response.

Replacement admission requires one ready Pod with new Pod and ReplicaSet UIDs,
new selected containerIDs, unchanged admitted imageIDs and the same resource
profiles. The controlling ReplicaSet must belong to the same Deployment. The
Deployment must advance by exactly one generation; an intervening rollout fails.
Public routing and the final Deployment guard complete the restart receipt.
The receipt contains instance/image digests and typed per-member termination facts.
Owners call `verify_drain_group_replacement` after retained-state reads. It admits
the current route, then rechecks the receipt’s Pod/ReplicaSet UIDs, selected process
identities, readiness, resource profiles and the same Deployment generation. Resource
versions may advance without ordering; a changed process identity fails the fence.

Before watch launch, the group registers its actual retained handle with the shared
owner. Local timeout, watch gaps and global operation cancellation await the same
consuming cleanup future within the original cleanup deadline. Sticky cleanup
failure and partial progress prevent an empty slot from proving successful closure.
CPU controls exercise profile admission, partial and failed exits, wrong instances,
stale watches, same-image replacement and real subprocess cleanup. Installed owners
qualify Kubernetes rollout and GPU workload behavior with their own fixtures.


## Installed Process Crash Observation

The read-only crash profile reuses selected-container admission and the native Pod
watch. Its owner supplies a declared Deployment, namespace/ReplicaSet/Pod UIDs,
container name, current containerID, imageID and restartCount. Live namespace and
Deployment readiness, the controlling ReplicaSet relationship and a Ready running
container must agree in the read-only admission before domain effects. Watch arming
repeats that admission after the owner settles its mutations. The initial watch object
then checks the same identities before the observer arms. The caller
stores the watch before waiting for that object. A cancelled admission therefore
keeps a pending handle for awaited cleanup outside the owner's operation timer.

The observer issues no signal, patch or restart. An operations owner separately
admits and signals the selected container after the domain owner has persisted its
settled-state snapshot and readiness marker. Recovery requires the same Pod UID,
ReplicaSet and image, one restart-count increment, a different containerID and Ready.
The old instance must report exit 137 with its correlated termination timestamp.
A reported positive signal must be 9. Missing or zero signal is recorded as unavailable;
it does not establish a signal observation. Explicit OOMKilled or another reported
termination reason fails the crash profile. The operations dispatch receipt is required
alongside these observations to qualify a deliberate kill.

Watch errors, closure, deletion, repeated resource versions and identity/count/image
drift fail qualification. The observer preserves partial termination facts in its
caller-owned state. It rechecks the unchanged Deployment generation and namespace UID
after readiness, then reuses the existing ten-second public contract-read admission
before returning. After retained-state reads, a final live admission requires that
same observed replacement container, restart count and template generation and records
its completion timestamp. Every watch has a deadline of at most 300 seconds; closing its owned
native process group is awaited for up to five seconds outside the operation timer.
This profile qualifies persistence of previously settled operations. It does not
establish recovery of a mutation interrupted in flight or cross-replica consistency.
