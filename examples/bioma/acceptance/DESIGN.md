# Bioma Installation Acceptance

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo configuration | Repository-owned typed gateway and deployment contracts, validated against the Bioma installation |
| SurrealDB 3.3.0 | Native Rust SDK, parameterized SurrealQL transactions and the platform-store schema; recovery exports use native SQL values |
| Kubernetes | Existing Deployment, Secret, PersistentVolume and PersistentVolumeClaim APIs; retained local-path storage is installation-owned |
| Native MCP clients | Official Rust SDK MCP 2026-07-28 Discover, Tasks, resource reads and request-scoped subscriptions over Streamable HTTP; bearer issuance uses the runtime-owned fixture adapter |
| Installed Stream | Owner-library run identities, completion products and analysis results; cross-replica DeepStream replay requires NVIDIA hardware |
| Candidate process ownership | Python standard-library Linux pidfd APIs; private `veoveo.ai/candidate-launch/v1` JSON with camelCase invocation, process group and start ticks; no public protocol extension |
| Installation input | Required `--installation` file using `veoveo.ai/installation-target/v1`; identities and endpoints validated against its control-plane document |
| MCP and authentication | Repository conformance CLI over public HTTPS, the hosted MCP 2026-07-28 profile, OAuth token exchange, exact Work Context and profile scopes |
| Flight scenario | Runtime-loaded `veoveo.ai/uav-sim-acceptance/v12` JSON with bounded typed mission, world, video and observation parameters |
| Recording analysis | Stream and Reason contract features, Recording catalog and checked live-part snapshot types; `veoveo.ai/uav-recording-acceptance/v1` JSON result |
| Stream live sessions | Server-owned live-session types imported through the Stream library's isolated `contract` feature |
| Stream notifications | Official workspace Rust MCP SDK; public resource subscriptions, uncached typed reads, cancellation and reconnected baselines; `veoveo.ai/stream-notification-acceptance/v1` result |
| UAV control grants | UAV-owned grant, permission and collection types through its isolated contract feature; a 60-second and 100-page traversal limit; Map owns mobility-profile references |
| Route admission | Map-owned route request, position, policy and result types through its isolated contract feature; a current aviation route Task runs before flight commands |
| World readiness | UAV-owned simulation, tile and camera state types; the scenario timeout includes reads and warmup waits; invalid bindings, failed resources and unsupported encoders fail immediately |
| Browser automation | Headed Chrome DevTools Protocol, hardware-backed WebGPU or WebGL, shared browser assertions owned by `testing/browser-smoke` |
| GPU workload evidence | Existing NVIDIA resource identity, NVENC source and concurrent workload assertions; software rendering is rejected |
| World publication | Frames-owned immutable revisions and typed frame URIs; UAV-owned validated installation binding; `veoveo.ai/uav-world-publication/v2` JSON receipt with the output file SHA-256 |
| Evidence | `veoveo.ai/uav-showcase-acceptance-evidence/v6` JSON and revision-qualified captures; `veoveo.ai/uav-showcase-phase-outcomes/v3` records domain and visual outcomes and preserves successful visual measurements even when the domain phase fails; both mark Reason `not_run` because its acceptance runs separately |
| Focused restart stages | `veoveo.ai/uav-live-view-restart-stage/v1` records each accepted container restart and its headed hardware browser observations before the next restart begins |

Browser upload, UX and resume reports use their current `/v2` tags and share one
closed report declaration in `src/reports/artifact_upload.rs`. It embeds the full
Artifact owner upload receipt. Receivers check occurrence URI equality and distinct
selected upload/occurrence identities before browser or SDK effects. Workspace
Markdown and Artifact consumer reports also use `/v2`, because their copied receipt
and observation shapes changed. The Python consumer uses an explicit camelCase
stdin peer model and the SDK's Artifact identity/address admission; its embedded
GatewayInternalIdentity keeps the frozen signed identity profile. These decoder
controls establish receiving behavior, while headed hardware acceptance and
installed SDK transfer measurements require their actual declared environments.

## Ownership

This crate validates the Bioma reference installation. Generic agent authoring and
runtime lifecycle remain in the gateway, platform store and agent manager. No Bioma
identity or migration procedure enters those components.

`knowledge_config` checks that the installation's indexer can discover its approved
sources, that every registered tool call is denied, and that its membership ends at
the Operations viewer role. User profiles expose Knowledge reads independently of
the machine's indexing profile. Collection approvals belong to this installation;
server protocol declarations are compared with the generic development catalog.

`record_restore` qualifies bound single-record insertion and transactional batch
restore in disposable database fixtures. It compares complete native record values,
rejects a conflicting final record, and checks that the failed transaction leaves
no partial writes. Its SQL lives under `tests/queries/record_restore`.

## Qualification

Run `cargo test -p veoveo-bioma-acceptance` for composition and native fixture checks.
The database harness provides isolated credentials, timeouts and owned cleanup.
No private installation exports or running pilot workloads are prerequisites for
these checks.

## Installation Assertion Delivery

The nondefault `smoke` feature supplies `installation-smoke` and `installation-browser-smoke`. Their single assertion sources own the multi-domain Store, Artifact, Gateway and installation relationships they exercise. The existing acceptance target and its default dependencies keep their normal behavior. Production domain packages do not depend on this composition.

Tracked `smoke/scenarios.json` entries name actual Cargo targets and preparations. Dispatch preserves native command arguments, hardware/service prerequisites and owner cleanup. Browser assertions use the shared headed CDP and hardware admission mechanics; pure browser sampler controls establish behavioral coverage only. Interrupted remote operations keep the owning reconciliation requirement.

## Compiler Candidate Ownership

The candidate probe records the selected Pod UID and a generated private executable,
App and cache path before publishing remote effects. It keeps the remote process
leader and Linux start ticks once launch is observed. Cleanup compares the current
Pod identity and holds Linux pidfds for the leader and each admitted group member
before signalling. A pidfd must still name the inspected process on both sides of
its `/proc` admission. Python and kernel pidfd support is probed before launch.
The launching process publishes a private `veoveo.ai/candidate-launch/v1` receipt
with its invocation, group and start ticks before executing the candidate; cleanup
never adopts a process by executable name. A changed
identity refuses cleanup and keeps the dispatch intent for reconciliation.

The owning cleanup action and the normal completion path use the same removal
implementation. Local `kubectl exec` groups drain separately from remote process
settlement. Missing process identity, expired cleanup or an unconfirmed removal
cannot authorize another candidate launch. Installation GPU checks still use the
existing NVIDIA runtime and require hardware acceptance.

## Stream Cross-Replica Acceptance

The installed `stream-gpu` smoke accepts `--replica-pods WRITER OBSERVER` to pin
dispatch and observation to different ready Pods of the same ReplicaSet and image.
The operator supplies both Pods; the harness does not scale the Deployment. Each Pod
keeps the installation's required GPU resources. The probe subscribes through the
official SDK to the Task, run and result on the observer while the Task is working.
It requires a completion notification and subsequent run/result invalidations, then
compares completed Task envelopes and typed result reads across both Pods. A fresh
subscription must recover the same completed state, and both subscriptions must cancel.
Pod identities, image IDs and restart counts must stay unchanged throughout the case.

The probe writes `stream-replicas.json` in the requested work directory, including the
created run ID on failure. It dispatches once and waits at most 300 seconds. A Task
that finishes before observer admission fails qualification. Reuse of the report path
is rejected so an uncertain dispatch cannot trigger an automatic replay. Port forwards
and SDK connections belong to the harness and close when it ends. The ordinary replay
checks still require processed GPU frames, detections and Artifact outputs. Live-session
delivery belongs to its GPU owner and requires its separate installed case.

Run against two already-ready Pods with a fresh output directory:

```sh
cargo xtask smoke stream-gpu \
  --installation examples/bioma/installation-target.json \
  --pipeline-id <installed-object-detection-pipeline> \
  --producer-key-secret <installed-recording-producer-secret> \
  --replica-pods <writer-pod> <observer-pod> \
  --work-dir <new-output-directory>
```

The ordinary scenario also requires an environment file containing the installation's
internal assertion signer and Store credentials, plus an installed producer Secret.
Tokens stay out of arguments and receipts. Local port 8797 addresses the writer and
18797 the observer; both must be unused before the run. The sample generator, recording
producer and Artifact checks use the same prerequisites as ordinary `stream-gpu`.
This direct-server check does not qualify public Gateway load balancing or live-session
routing. It complements the installed public MCP and composed-flight cases.

Stream and Reason's direct GPU fixtures sign a synthetic automated service identity
with the current Work Context policy revision and the selected profile's audience.
Their assertion includes a checked `GatewayRequestContext` with fresh audit correlation,
matching actor, client, scopes and expiration. Artifact admission requires that context
before issuing read or write capabilities. These fixtures do not establish public OAuth
authentication; public acceptance uses the installation's registered client.

## Composed Flight Acceptance

The focused [Flight client](../flight/DESIGN.md) owns the installed UAV flight, live Stream, Recording replay and hardware browser acceptance obligations. It compiles the single shared browser assertion source in this component, while its package graph excludes service implementations.

## Current Copied Reports And Map Bridge

Compiler acceptance receipts use `veoveo.ai/compiler-acceptance/v4` and
retain the complete checked Stream or Reason result alongside the selected service.
Component-scope reports use version 2 around the complete current installation
receipt; their receiver compares selected locked content and unselected state.

The browser Map wrapper and capture use version 3. `browser/map_workspace.rs`
constructs resources through Map builders and admits copied query requests through
the same contract types. The existing browser harness owns HTTP and browser cleanup.
Map source-query digests come from the Map contract's cursor-independent producer.
The hosted App requires network access to the credential-free OpenFreeMap Positron
and Dark HTTPS styles at `tiles.openfreemap.org`. Its CSP admits that declared
origin. Pure request controls do not qualify headed rendering or hardware graphics.
