# Testing Support

Shared testing support admits owner declarations and compiler artifacts, connects maintained protocol clients, and owns local process cleanup.

## Standards And Protocols

| Standard or format | Supported profile |
| --- | --- |
| Cargo metadata and JSON compiler messages | Actual package, native target, selected features, emitted executable and linked native library paths; no guessed target directory |
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

Framework dispatch verifies one exact selected case and reads maintained framework outcomes. Libtest admission checks the executed case name and status as well as summary counts. Extra selection-changing arguments are refused before preparation. Missing, ambiguous, zero-case, skipped and failed outcomes are errors. Failed cleanup or result admission keeps private diagnostic files and exposes only their paths. Successful exit cannot substitute for framework execution.

## Qualification

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
