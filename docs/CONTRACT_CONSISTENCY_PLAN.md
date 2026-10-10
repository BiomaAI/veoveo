# Platform Foundations And Contract Consistency Plan

Status: The published 34-image closure and charts for `6431c30c6621` retain their
recorded qualification. GitOps is suspended during source work. Development image
extraction triggered node disk pressure; all seventeen desired Deployments and both
StatefulSets recovered by 02:42 UTC on October 10. Compiler checks and all twenty-seven
focused native recovery/consumer controls pass. Twenty-seven images at `c5a2c6c3e`,
including all seven stock-rollout images, pass staging, attested qualification and node-side registry readback.
The current source closure requires thirty Rust targets and the pending cuOpt image;
those earlier receipts do not qualify the new inputs.
The Knowledge and Time repair at `6653288ba` is published and deployed. Both Pods
pass probes and thirty seconds of stability without a restart. Normal OAuth reads
and Knowledge search reach the updated Time documents. Time completion and
cancellation pass through normal Gateway OAuth. Unfinished process recovery
remains unqualified because the attempted crash did not replace its container.
Datasheet's SDK replacement and the complete focused normal-OAuth Artifact consumer
scenario pass. Retained capability redemption and idempotent replay also pass
across one fenced Artifact service replacement. Interrupted-write recovery and
final-image qualification remain open.
Time12 passes completed-state cross-replica acceptance on the selected deployed
Time image. The same historical Task's eight independently expected occurrences,
result and authority agree through B-only routing after original A exits zero.
The watch barrier, native restoration of A and its selector, and retirement of B
all pass. SDK handles, watches and owned processes close; postflight verifies the
sole A endpoint and B Deployment, ReplicaSets and Pods absent. Unfinished recovery,
installed authority activation and final-image qualification remain open.
Gateway, Agent Manager and Knowledge run compatible qualified images against the
published full catalog. Manager is Ready with the reviewed immutable pilot template,
qualified kernel and matching admission policies. Its installed fixture implements
public OAuth authoring, prearmed SSE with current Ready checks and owned Stop/archive;
review and five native controls pass. Two installed attempts exposed fixture
configuration defects: an incomplete manifest, then insufficient profile scopes.
Archive, physical drain and restoration pass. The shared scope-admission repair
passes fourteen runtime/Gateway controls and the Manager loader control; the
corrected five-scope installed journey remains open.
Knowledge passes retained-index cold-start qualification;
generation two and restricted caller policy remain open. Reconciliation
holds stay set. Further image batches require
separate disk-space admission. Speech's previously
selected headed Workspace consumer passes with fixture audio;
Reason and the other GPU workloads stay at zero replicas. The unmodified OpenShell 0.1.2 source
package passes its focused artifact, Host and packaging checks. The owner approved
stock OIDC; package image `ea4ffc9f7` is assembled and staged with all four official
executable hashes and versions verified. Three CA roles, projected credentials,
stock mount fields and the owned image's absent retained-home target pass source
controls. The restricted Veoveo worker OAuth source passes private-key exchange,
refresh, owner authority and focused Gateway native controls; independent review
approves its contracts and Secret wiring. Optional admitted
issuer CA inputs configure issuer HTTPS trust with verification enabled. The focused
installed DuckDB Task, completion subscription and graceful-restart check passes
through normal Gateway OAuth. Focused installed DuckDB catalog/source paging,
CSV/schema, Artifact publication and usage consumers also pass; headed Workbench,
physical owner-directory/metadata, further recovery and final-image qualification
stay open. The selected DuckDB, Timeseries, Frames and Media
protocol case also passes authenticated discovery, anonymous discovery refusal,
documents, transport and Gateway health/audit checks on admitted deployed images.
Frames completed-state retention passes across one actual same-Pod process crash.
All eighteen selected stock native acceptance cases pass, including Host image
replacement, directed template upgrade/rollback and the unmodified CLI transport.
Installed identity materialization and installed qualification remain open. The fresh SSH-shell terminal source cut passes review
and owning controls. Knowledge's prearmed retained-index/current-generation cold-start
case passes installed qualification; first-ever index construction remains unqualified.
Task/subscription fixtures and complete installed A/F/H
qualification remain open. See [Current Status](#current-status).

This is the single implementation plan for the former Foundations, Contract
Consistency and Repository Hardening tracks; their required open conditions transfer
without being declared complete.

Foundations' accepted requirements still apply. The
[review decisions](#review-decisions) describe the broader architectural scope.
The [Foundations transfer register](#foundations-transfer-register) preserves every
owner row's next step, and the [hardening transfer register](#hardening-transfer-register)
assigns a disposition to that plan's remaining concerns.

The intended result is one owner for each contract, typed construction and database
admission, generated consumers, and a qualified installation. Work proceeds across
each affected dependency chain in a batch. Shared machinery does not replace domain
validation, authorization or recovery. Inventory counts below describe the review
baseline and must be refreshed before implementation; they are not counts of proven
defects or independent tasks.

## Standards And Protocols

| Standard or protocol | Role in this plan |
|---|---|
| MCP `2026-07-28` and rmcp 3.5.0 (fork revision `917e7914`) | Naming convention: camelCase keys (89 rmcp types declare `rename_all = "camelCase"`), snake_case multi-word values such as `resource_link` and `input_required` |
| `ai.veoveo/knowledge-source` | Repository-owned MCP extension through which any server publishes knowledge collections; the extension point of the kernel Knowledge module |
| JWT (RFC 7519), JWT access tokens (RFC 9068), OpenID Connect Core 1.0 | Claim names follow the JWT and OIDC snake_case convention (`client_id`, `auth_time`, `given_name`) |
| SurrealDB 3.3 and `surrealdb-types` 3.3.0 | One shared database; SCHEMAFULL tables, declared nested fields, literal types, `record<…>` links with `REFERENCE … ON DELETE`, `DEFINE FUNCTION`. Database-level system users keep authority over every table, so table permissions cannot isolate modules |
| vLLM 0.31.0, image `vllm/vllm-openai` | NVIDIA embedding profile, pooling runner and priority scheduling; the pinned FP16 profile passes local RTX 4090 qualification; installed profile selection and isolation are open |
| vllm-metal 0.30.0 (`vllm-project/vllm-metal`) | Apple silicon embedding profile on macOS 15 or later through MLX; experimental text pooling with LAST pooling and L2 normalization for Qwen3-Embedding |
| JSON Schema 2020-12 | `additionalProperties: false`, which schemars emits for `deny_unknown_fields` types |
| serde 1.0.229, schemars 1.2.2 | `rename_all`, `rename_all_fields`, `deny_unknown_fields`; schemars derives tool schemas from the same attributes |
| json-schema-to-typescript 16.0.0 | Console and Workspace types generated by `cargo xtask release client-types` |
| pydantic 2.13 (locked) | Closed wire models admit only current spellings through both Python-value and JSON decoding. Peers declare actual wire fields or qualify alias parity; the SDK floor rises from `>=2.8` to the locked minor |
| Kubernetes Jobs and Helm | Ordinary preparation and migration Jobs; database readiness and persisted prerequisites establish ordering |
| RFC 8785 JSON Canonicalization | Audit chain hashing; the audit record format is frozen |
| RFC 3986 / RFC 6570 and the pinned iri-string implementation | Concrete resource addresses and URI templates, with owner-specific typed builders |
| OCSF 1.9.0, W3C Trace Context, RFC 9162 and S3 Object Lock | Existing audit export, correlation, sealing and retention profile; qualification limits appear below |
| Cargo metadata format version 1 and OCI / BuildKit | Dependency discovery and existing source-qualified image publication; no new build engine |

Versions describe existing pins or candidates inherited from the proposal. Verify
the authoritative stable release and compatibility before adding or upgrading a
dependency. Metal is an unqualified proposed profile, not an implemented capability.

## Current Status

Thirteen Rust Task owners, including SUMO, use TaskRuntime's finite startup recovery observer.
Retained Tasks with live leases are revisited after expiry, and failed claims require
current settlement or a matching live replacement worker before recovery skips them.
Speech schedules its retained backlog behind the existing 64-slot queue while HTTP
stays available. Shared native recovery controls pass; owner shutdown controls preserve
one HTTP drain deadline and the observer's separate five-second drain. Installed
Task lifecycle, restart and final-image qualification remain open.

The full current TaskRuntime milestone suite passes 90 native tests and four
compile-fail documentation tests with two test threads. Its explicit SDK storage
exchange remains ignored in this invocation; the earlier owning exchange keeps
its recorded scope. No lease-timing diagnostic replay was needed.

Ten hosted owners use shared Resume settlement: DuckDB, Frames, Map, Optimization,
Time, Timeseries, Speech, Stream, Reason and View. The helper checks the executing
worker's live lease, preserves the first terminal outcome and reconciles one
cancellation conflict. Each owner selects whether a genuine failure takes priority
over cancellation. Initial and progress gates stop subsequent effects when the
returned Task is terminal. Native Store controls pass; Time's local-stop checks
also cover snapshot reads and publication. A transition already dispatched may
still settle. Installed cancellation, unfinished restart and replica qualification
remain open.

DuckDB keeps mutations outside the Resume helper. Its native engine control proves
that cancellation before dispatch prevents an insert, while a committed insert
survives cancellation exactly once. Map admits current parent policy before
replaying a committed feature-change receipt against an advanced layer revision.
Media settles pre-dispatch cancellation only when durable dispatch evidence is
absent. UAV preserves failure details, physical outcomes and authority pins when
cancellation changes the Task revision. These owner controls pass locally; they
do not establish provider, simulator or installed recovery.

SUMO uses shared Resume settlement for offline operations and keeps RunBatch
outside replay. Recovery checks operation-to-Task-type agreement before claiming
work, and setup errors close acquired driver and Recording resources. Its native
Store and cleanup controls pass. Actual unfinished restart, HTTP shutdown and
stepped simulator qualification remain open.

The existing owner harnesses prepare the following checks. Previously qualified
native controls pass. Time schedule completion and cancellation also pass through
normal public OAuth; recovery keeps its separate qualification gate.
Earlier installed checkpoints keep their recorded scope below.

| Owner | Prepared installed checks | Remaining qualification |
|---|---|---|
| Time | Read consumers and explicit complete/cancel/recover schedule fixtures; completion and cancellation pass; Time12 completed-state cross-replica Task/result/authority delivery and native restoration pass on the selected deployed image | Unfinished process recovery, authority activation and final images |
| Artifact | Focused normal-OAuth uploads with independent public byte/digest checks, delegated SDK reads and a separately selected retained Task-bound write capability across one service replacement; source/native controls and review pass at `dd7958d26` | Interrupted-write recovery and final images |
| Timeseries | Four-row forecast, RRD Artifact and usage; typed cancellation, connection replacement and process-crash fixtures with original Task/result agreement | Installed unfinished process recovery, selected cross-replica routing and final images |
| Speech | Raw exact-ID Task completion and unfinished cancellation with independent transcript, source and provenance checks; completion also listens to the typed transcript resource's initial current snapshot; explicit unfinished process-recovery fixture passes native controls | Installed delivery/cancellation, unfinished process recovery and final CUDA image acceptance |
| Stream | Existing GPU replay with a normal-OAuth Gateway caller, delivered completion and current resource snapshot; an opt-in unfinished recovery fixture selects a retained Recording, fences the replaced process and checks the same Task's creation identity, current Working state and recovered products. Compiler, strict lint, native controls and review pass | Installed public delivery, post-mutation updates, unfinished recovery and final images |
| Reason | Existing GPU analysis with a normal-OAuth Gateway caller, delivered completion and current resource snapshot; an opt-in unfinished recovery fixture retains acknowledged Task facts, fences the replaced process and checks the same Task's creation identity, current Working state and recovered analysis. Compiler, strict lint, native controls and review pass | Installed public delivery, unfinished recovery and final images |
| Knowledge | Full-catalog baseline, qualified retained-index/current-generation cold startup and separate restricted-caller discovery, collection/completion visibility and document denials, with retained SDK cleanup | First-ever empty-index startup, generation-two publication, installed caller-policy checks, current-generation recovery and final images |
| Optimization | Multi-page existing solve corpus, exact products, usage, public Task results and canonical Artifact bytes under allowed and denied caller contexts; separately selected coordinated control/executor replacement repeats the retained consumers | Installed GPU-produced corpus, mid-run policy changes, coordinated control/executor replacement and unfinished/cross-replica recovery |

The complete focused Artifact scenario passes public OAuth upload admission,
immutable-part retry/refusal, completion replay, cross-context denial and Datasheet
CSV/Parquet consumption. Delegated SDK checks pass metadata/URI/byte agreement,
limit-one catalog traversal, foreign-tenant isolation, byte limits and temporary-file
cleanup. Its journal records three retained publications and `consumersPassed`.
Datasheet runs the qualified C5 image with both required SDK modules verified in
the installed Pod; its replacement passes readiness, direct liveness and thirty
seconds of stability without a restart. The separately selected Artifact service
replacement now passes: the old container exits successfully, its replacement
becomes Ready, and the retained capability redeems and replays as one occurrence.
The repeated focused scenario traverses eighteen SDK pages and passes all three
consumer groups. Four occurrences are retained; the fixture's typed Task reference
does not establish an executed Task. Core services stay Ready. Interrupted-write
and unfinished-Task recovery remain unqualified. The
[progress log](PLATFORM_FOUNDATIONS_PROGRESS.md#artifact-service-replacement-and-retained-capability--october-10-2026)
binds the report, restart and retained effects.

The Time and Timeseries lifecycle fixtures pass sixteen focused native controls,
including shared crash-receipt decoding and cleanup after failed journal writes.
Process recovery requires the original Task Working before and after replacement;
early completion refuses qualification. A real million-day Time recurrence scan
produced its independent one-row result in 9.614 seconds locally. That sample does
not establish an installed unfinished window. The installed eight-window fixture
now passes completion and cancellation on the repaired Time image. Completion
agrees with all eight independently expected occurrences and delivered and current
Completed state. Cancellation observes Working before recording its intent,
receives acknowledgement and reaches current Cancelled state. Both cases close
their caller and listener. The initial recovery attempt fails before dispatch
because a second installation validation rejects the fixture's own journal.
The repair at `e07467759` retains the initially admitted typed installation and
passes strict compiler and focused native checks. A subsequent attempt reaches
delivered and current Working state and records its crash intent. Its signal
command returns zero, but the selected container never terminates or restarts;
the recovery observer fails and closes its owned transports. A subsequent normal
OAuth read through the Gateway confirms Completed with all eight expected results.
Runtime inspection identifies the unchanged container process as namespace PID one;
Operations has verified an ancestor-namespace containerd signal path for a future
attempt.
This run qualifies neither process recovery nor a product recovery failure.
Together with the three administrative API controls, the
earlier native batch has nineteen passing controls. Time recover mode selects one
300-second operation deadline; its other modes keep 120 seconds.

Time’s administrative `/active-authorities` response now exposes `pointerVersion`
with the selected release. Clients use that guard for activation;
`release.recordVersion` identifies release metadata. The response hard cut has no
compatibility alias. Three focused native controls pass, covering admitted wire
and schema, persisted pointer/release consistency and two hosted activations.
Installed authority activation remains open. Hosted Time calculations run on the
blocking pool with cooperative stop checks and the original job retained through
normal worker exit. Compiler, strict lint and ten focused native controls qualify
async responsiveness, original-job joining, Task settlement and engine behavior.
Installed cancellation passes on the repaired image. Unfinished recovery remains
unqualified until an actual process-replacement case passes.

The existing Time consumer harness now prepares a separately selected isolated
authority profile: HTTPS acquisition, first activation, concurrent pointer conflicts,
stale-guard refusal and immutable epoch rebinding. Compiler, strict lint, five focused
native controls and independent review pass. Received typed mutation identities
survive cancellation while awaiting the journal lock. The actual-HTTP regression
passes in a fresh process, and scenario discovery admits the profile. Installed
execution remains open.
The fixture observes an initial subscription baseline, later invalidations and
uncached changed authority; its notifications do not identify a causal mutation.

Knowledge uses one 256 KiB source-member limit across resource reads, source
admission, chunking and indexed-member admission. Whole-text digests, 256 chunks
per member and embedding text/batch limits stay enforced. Compiler, strict lint,
logic review and three focused native controls pass. The Knowledge and Time images
at `6653288ba` pass stage/release digest agreement, attestation and node-network
readback. Their coordinated rollout, direct probes and thirty-second stability
checks pass without a restart. Normal OAuth reads verify Time documents and the
Knowledge source/collection; scoped search reaches the indexed `time.docs` members.
All seventeen enabled Deployments and both StatefulSets are Ready; the three
reconciliation holds stay set. A broader source helper reports incomplete resource
template discovery and is recorded separately from these targeted passes. The
completed-state cross-replica fixture passes strict compiler checks, focused native
controls and independent review. Time12 passes its installed A/B execution on
`time-mcp@sha256:49b59aeff17abf7aca3250eadf41c3539ac10b118b79559188afb58a0fbbf5d4`.
The original acknowledged historical Task preserves its eight expected occurrences,
result and authority through an explicitly fenced B-only route after original A
exits zero. The native watch barrier and the full restoration sequence pass:
A becomes Ready, its original selector and sole endpoint are restored, and B's
Deployment, ReplicaSets and Pods disappear. Supervisor, native test and Operations
driver exit zero; all SDK handles and watches close. Postflight verifies five core
services Ready and removal of the private caller file. The
[progress log](PLATFORM_FOUNDATIONS_PROGRESS.md#time-completed-state-cross-replica-acceptance--october-10-2026)
binds the receipts and preserves prior failed attempts. This pass qualifies retained
completed-state delivery on the selected image; unfinished recovery, authority
activation and final-image qualification remain open.

Gateway-routed Knowledge source checks now admit each templates/tools page against
the selected typed server. Unrelated server failures are retained as limited K01
coverage; selected-server failures or malformed metadata refuse qualification.
Direct and full-profile checks still require complete discovery. Compiler, strict
lint, three focused native controls and independent review pass. Installed execution
remains open, and the earlier source-helper failure has no established backend cause.

Knowledge's closed v2 cold-start fixture now admits an explicitly selected local or
shared GPU runtime. It binds Namespace, Deployment and Service identities, checks
the ready endpoint's Pod ownership, and matches Knowledge's configured endpoint.
Compiler, strict lint, ten focused native controls and independent review pass.
A reviewed first-empty overlay may reuse the existing runtime for one trusted,
isolated Knowledge release; its Store and indexing writers stay separate. Actual
policy and key materialization, current hardware-profile qualification and the
first-ever empty-index run remain open. The owning
[design](../servers/knowledge-mcp/DESIGN.md#verification) defines the profile.

The added Speech resource and Stream/Reason public-caller cases pass compiler,
strict lint, formatting, documentation and independent source review. Their seventeen
focused native controls pass within the twenty-seven-control batch; installed execution is held.
Stream and Reason reuse the existing GPU scenarios and shared SDK cleanup owner. Their private
journals sync dispatch intent and the acknowledged Task identity before listening,
so delivery failure preserves the known identity. Resource observations establish
an initial current snapshot; subscription closure does not establish Task
cancellation. These additions do not qualify unfinished process recovery.
The [acceptance design](../examples/bioma/acceptance/DESIGN.md#public-stream-and-reason-consumers)
defines their private inputs and observation scope.

Stream and Reason now also prepare unfinished process recovery in those same GPU
scenarios. The shared SDK helper creates one Task, preserves its acknowledged ID
and creation time, closes the original connection and reconnects once after the
selected process replacement. Both delivered and current Working states must agree
before completion. Its two native controls cover seven SDK modes, including EOF,
deadline and cancellation failures. The owner batch passes all twenty selected
native controls after correcting a Reason fixture and refusing zero-test subprocess
success. Owner cleanup preserves partial Task facts and failures across interrupted
close operations. Compiler checks, strict lint and independent source review pass.
The production CLI builds and passes its command-line admission checks at
`715796541`; actual GPU recovery remains unqualified. See the
[checkpoint](PLATFORM_FOUNDATIONS_PROGRESS.md#stream-and-reason-unfinished-recovery-fixtures--october-10-2026).

The source batch at `31f2f45c3` prepares unfinished DuckDB query recovery and Speech process
recovery in their existing harnesses. Both require the original Task to remain
Working after the selected replacement before accepting delivered completion.
Timeseries and View compare stable Task identity and terminal payload while allowing
mutable transport hints. Speech and View bind replacement to their maintained Rust
server roles. Both final strict compiler checks pass, including the corrected Speech
role admission. Independent review approves the source batch. All twenty-seven distinct
focused native controls pass after correcting two synthetic resource-notification
fixtures to use the SDK's received-envelope metadata API. No product behavior changed.
One zero-selection diagnostic was corrected from the current executable's test inventory
and is not counted as a passing control. Installed execution remains unqualified.
View's existing local listener and recovered-client
cleanup are unchanged and are not qualified by its terminal-comparison update.

Optimization's Artifact GET adapter maps typed policy denials to its existing
missing-resource response. Native HTTP controls distinguish those denials from
authentication, transport, malformed-response and backend failures. The shared
restart driver prepares a coordinated container-group drain with one Pod watch
and one fenced mutation. Native controls require every selected container's
successful exit, preserve partial facts on failure, and check the replacement's
process identities and current readiness. Optimization's existing reads harness
now wires this profile to its retained consumers. One admitted installation target
supplies the restart identity even if its configuration file changes after admission.
Native input and target-replacement controls pass. Installed control/executor
replacement, unfinished work and cross-replica recovery remain open.

GitOps is suspended for the coordinated stock rollout. Both tracked Veoveo and UAV
HelmReleases suspend application reconciliation; node configuration is unchanged.
The eighteen selected stock native cases are qualified, including the aggregate Host
profile of 1 CPU, 6 GiB and 1,024 PIDs. The installed 8 CPU, 12 GiB and 4,096-PID
profile has not been activated.

Knowledge's retained-index/current-generation unattended cold-start profile passes
installed qualification on the C5 image. The observer records HTTP 503 then 200
on the same container, drains the readiness watch fence, and verifies Ready status
through ten seconds of stability.
The ordinary OAuth caller carries all thirteen required scopes. Its 89 requests
verify four sources, sixteen collections and eleven links; the verified generation
matches the active generation.
The case exits successfully after closing its observers and port-forwards, and
Knowledge stays at one Ready replica. First-ever index construction remains
unqualified. An isolated eight-lane first-empty configuration is prepared only;
its installation, empty-state proof and startup have not been admitted or executed.
The read-only empty-state inventory passes SurrealDB 3.3 syntax validation and
independent review. It checks all eleven Knowledge tables, eight static indexes
and the absence of dynamic chunk tables; actual execution still requires the
isolated database, complete read authority and stopped-writer/no-restore proof.
Generation-two publication, Host activation and the restricted
caller-policy case still require their separately selected checks.

The observer's canonical Pod decoding, readiness ordering and owned port-forward
cleanup pass compiler, strict lint, thirteen native controls and independent review.
The three installed cases stay separately selected. Typed SDK-error observations
now preserve HTTP 401/403 status and MCP error codes without private response
details. Two Knowledge acquisition controls and the canonical wrapped-error
control pass. Fifteen distinct Knowledge native controls have passing results;
two deadline-sensitive lifecycle controls require isolated serial replay after
parallel failures, whose results are retained. Knowledge HTTP and the conformance
library and test targets pass strict lint. The updated conformance tool needs a
compatible image refresh in the final closure. The cold-start installed pass does
not close the separate restricted caller-policy qualification.

The published catalog includes the isolated `knowledge-acceptance` profile and
ordinary public PKCE client for the required caller-policy check. It exposes the six approved
Time collections and Knowledge catalog resources, while refusing Knowledge's own
documents. The client's separate operations Viewer membership and audience leave
normal operator and indexing authority unchanged. Four native configuration controls
exercise canonical policy selection and pass after adding the new audience to the
existing enterprise OIDC bridge. Compiler, lint, bundle hashing and independent
review pass. The existing conformance CLI adds public-client S256 PKCE login with
private-file output; compiler, strict lint and nine local OAuth controls pass.
Typed token getters avoid whole-token serialization, and accepted callback
connections close on settlement, deadline and dropped login. The non-test CLI
at `cc1d60d0c` passes help and no-network bearer-isolation admission; a tenth parser
control admits `oauth-login` and rejects the unintended spelling.
The restricted caller-policy case still requires installed qualification. The reference
prepares seven qualified stock image pins and a generation-two module plan
generated by the matching Gateway image. The module selection, credential revision
and nineteen lanes are unchanged. Helm rendering with the actual reference values,
k3d overrides, image lock and generated plan passes. These matched inputs are
prepared for publication; they do not establish an installed generation-two result.
Host and Computers select the locked stock template image with its canonical
fingerprint and matching configuration revisions. An authoritative inventory finds
zero persisted Computer bindings, permitting this candidate hard cut without a
compatibility template. Both native reference-template guards pass. Host/Computers
activation and installed stock-template qualification remain open.
Eleven additional existing reference pins and the managed-kernel image now select
their qualified inputs. Twenty-seven of the thirty-four affected images are
qualified; these prepared pin changes do not deploy workloads or qualify the
remaining images.

The authoritative current-source image plan selects thirty Rust targets plus the
pending cuOpt image. Complete local manifests, entrypoints and root Cargo inputs
change compiler source contexts beyond the directly edited runtime packages. The
prior C5 qualification covers its recorded inputs; it does not close this source
revision. Stable compiler cache identities are preserved. Each image batch has its
own growth budget above the filesystem reserve;
the former 96-GiB admission and 84-GiB cancellation guard do not protect this node.
The remaining GPU image families require separate base-layer and disk admission.

At 02:01 UTC on October 10, disk pressure began on the shared node/image filesystem.
Kubelet first evicted Embedding and Speech; continuing pressure subsequently left
all seventeen desired Deployments and both StatefulSets without Ready replicas.
Their replacements could not schedule under the pressure taint. Stopping the build recovered approximately
145 GiB free. The installed kubelet has a 5% eviction threshold and 10% additional
minimum reclaim on a 1,967,317,549,056-byte filesystem. Its retained reclaim threshold
required approximately 295 GB available before the five-minute pressure transition
could clear. The operations owner removed 133 unused test executables and 2,055
selected old incremental directories, recovering approximately 154 GiB and reaching
320,883,544,064 available bytes. This intentionally sacrifices selected incremental
cache entries; dependency libraries, build-script outputs, Docker/BuildKit/OCI
storage, worktrees and cluster volumes are preserved. Kubelet cleared disk pressure
at 02:25 UTC without manual taint removal. By 02:42 UTC all seventeen desired
Deployments and both StatefulSets were Ready; the other eight Deployments matched
their desired zero replicas. Available space was 267,337,420,800 bytes. Both resumed
strict metadata checks pass. A subsequent targeted sacrifice of eighteen unrelated
test incremental directories recovered 9.2 GiB while preserving the pending native
batch's caches, dependency libraries, build-script outputs, images and data. The
twenty-seven-control batch then passed with 6.66 GiB peak filesystem growth against
its 16 GiB budget; the node and core services stayed healthy. Image publication and
staging still require their own admitted peak-space budget. Recovery did not change image pins
or resume application reconciliation. Knowledge needed several natural restarts;
its unattended cold-start qualification remains open.
Staging keeps `releaseEligible=false`; installed image pins and the application holds
stay unchanged. Bioma uses the documented direct publisher and Helm/GitOps image
locks. Qualify selected targets against their original authentic stage receipts with
`release images --stage-evidence`, preserve their runnable digests, and retain the resulting qualified image-release
evidence before updating consumed-image pins and validating the GitOps release
inputs. This path requires genuine publication attestations and registry readback;
stage receipts alone cannot qualify a rollout. The optional development-image-lock
command requires a publisher-generated DeploymentLock, which was not found in the
trusted retained outputs. That requirement belongs to that command and does not
block Bioma's direct publication path. The publisher now accepts a nonempty subset
of an authentic larger stage cohort while validating the complete original receipt,
rejecting duplicate targets and preserving every selected repository and runnable
digest. Strict compiler checks, five native controls and independent review pass.
Unselected rows are not claimed as qualified. All seven stock-rollout targets now
have their own accepted stage and qualified release evidence. The resource gate,
complete affected-image coverage for the final
cut, catalog-reader replacement order and installed acceptance remain open.

The first Computers/Gateway qualification rebuilt both images successfully but
failed the required staged-runtime digest comparison. Their server binaries and
103 package-version records match; the changed package-install layers contain
wall-clock text in apt and dpkg logs. The publisher accepted no release evidence,
and deployed image pins are unchanged. Thirty-one owned runtime Dockerfiles now
remove those disposable logs in the same package-install RUN. Source checks and
independent review pass. Fresh staging and qualification at `c5a2c6c3e` pass for
all seven stock targets: Computers, Gateway, Computer Host, Computer template, Agent
Manager, Knowledge and Console BFF. The first six stages execute the runtime
package-install steps without cache; qualified warm builds reproduce their runnable
digests. BFF's stage and qualification reuse cached package-install steps. Every
qualified release includes SBOM and provenance, and node-network manifest/config
readback returns 200 with matching hashes. These runs do not establish a second
independent uncached rebuild. The four-image stage grows the filesystem by 3.39 GB
at its observed peak. BFF stages in 25.98 seconds and qualifies in 5.901 seconds;
its before/after free-space snapshots do not establish peak growth. Available space
after BFF qualification is 294,392,238,080 bytes. The preceding inactive Cargo-cache
cleanup recovers 28.58 GB by filesystem measurement and exceeds the assigned 24 GiB
combined cap; the deletion manifest records that deviation. Deployed image pins
stay unchanged. The final affected closure and installed qualification remain open.

Artifact service, Artifact MCP, Media, Frames and Optimization MCP also pass staging
and qualification at `c5a2c6c3e`. Their five runnable digests agree across both native
commands, every qualified publication includes SBOM and provenance, and every
node-side manifest/config read returns 200 with matching hashes. Four runtime
package-install steps execute without cache during staging; one is cached.
Qualification reuses those steps. Stage elapsed time is 199.90 seconds, including
180.825 seconds in compilation. Free-space snapshots show 2.04 GB stage growth;
these snapshots do not establish a peak. Available space after qualification is
292,243,443,712 bytes. Deployed image pins are unchanged.

Computer Storage, the stock Computer provider, Recording's three images, DuckDB,
Timeseries, both MCP bridges, conformance and Agent Kernel also pass staging,
attested release qualification and node-network manifest/config readback at
`c5a2c6c3e`. All eleven runnable digests agree between stage and release. Their
stage takes 489.455 seconds and qualification takes 41.728 seconds. The observed
stage filesystem growth is 7.75 GB against its 12 GiB cap; the node and core
services stay healthy. Installed pins and reconciliation holds are unchanged.

Time, UAV-Sim MCP, Datasheet and Anonymous Simulation pass stage/release runtime
digest equality, attestation checks and node-network readback at `c5a2c6c3e`.
The three consumed reference pins select those runtime digests; Anonymous Simulation
is a qualified fixture/tool image without a reference workload or lock entry.
Rollout and installed acceptance remain open. The cohort's observed free-space
samples do not establish peak growth.

The refreshed image plan selects committed source `3820c0db5` from the existing clean
publication checkout and includes the qualified Manager profile-scope admission repair.
The prepared dependency graph selects thirty Rust targets and retains
the pending cuOpt executor, for thirty-one images. Planning passes; none of this
final source's image builds, stage/release checks or registry publication has run.
Earlier C5 and `6431c30c6621` receipts qualify their original inputs. Trixie and BFF
source digests change from the prepared `715796541` plan; the other three Rust
families preserve those prepared inputs. All five families still require their
final-source qualification. Stable cache identities permit reuse. The
[preparation checkpoint](PLATFORM_FOUNDATIONS_PROGRESS.md#manager-native-qualification-and-final-source-closure--october-10-2026)
records the source closure and planner outputs.

Reviewed registry retirement is complete. Native offline collection removes the
approved obsolete blobs while preserving all current pins and retained references.
All 395 retained manifests pass digest readback; the registry and core services
are healthy. Measured free space is 291,324,821,504 bytes after recovering
23.82 GiB. BuildKit caches and the current acceptance CLI are preserved. The
[retirement checkpoint](PLATFORM_FOUNDATIONS_PROGRESS.md#reviewed-registry-retirement--october-10-2026)
records the exact deletion sets and postflight. The final build still requires
resource admission; the original aggregate budget baseline is unchanged.

The unchanged Charts, simulation-runtime and UAV-runtime production inputs retain
their own accepted `6431c30c6621` publication proof. Fresh node-network manifest and
config reads return 200 with matching hashes for all three; their source and
artifact identities are preserved. The final thirty-one-image selection and the
historical thirty-four-image publication describe different sets.

| Final image family | Image count |
|---|---|
| Trixie Rust | 22 |
| Trixie browser BFF | 1 |
| Bookworm Rust: Map, Time and View | 3 |
| GPU control runtimes: Reason, Speech and Stream | 3 |
| SUMO Rust | 1 |
| NVIDIA executor | 1 |

The full catalog, including the worker section and restricted public client, is
published with runtime authentication verified: nineteen servers and nine profiles.
Publication uses current generation one; all nineteen SQL lanes and runtime
credentials are unchanged. Gateway, Agent Manager and Knowledge pass serial C5
rollouts with old Pods absent and replacement Pods Ready. Control-plane
validation and the normal operator Knowledge search/source-revision case pass.
Host and Computers stay at zero replicas, and GitOps holds remain set. Generation
two, restricted caller-policy qualification and first-ever empty-index startup
still require their separate checks.

The first Agent Manager public journey failed before its kernel became Ready because
the temporary seed omitted `agent.tenant`; its owned cleanup and restoration passed.
The complete replacement seed passes the kernel's decoder and validator. The second
installed journey loads that seed and acquires a scheduler lease, then fails MCP
authentication: its template grants one scope while the selected Gateway profile
requires five. Correlated Gateway logs report missing required scopes. This is a
qualification configuration defect and exposes a shared admission gap: installation
facts discarded each profile's required scopes. The source repair preserves those
typed requirements and rejects incomplete templates in Gateway and Manager without
adding grants. Contract-only and combined consumer compiler checks, strict lint and
independent review pass. All fourteen native authoring, catalog and Gateway loader
controls pass after correcting a pre-existing digest golden. A separate warm build
exits zero and the Manager native loader control passes, bringing this source batch
to fifteen passing controls. Installed qualification remains open.
Independent reads verify the second instance is archived at generation three, its
operation claim is cleared, no admissions, live runtime leases or episodes remain,
and its owned Kubernetes workload is gone. One runtime row and the original Bound
PVC are retained. Manager, Gateway and temporary admission policies are restored;
temporary configuration and workloads are absent, and core services are Ready.
No installed Manager lifecycle pass is established. The
[checkpoint](PLATFORM_FOUNDATIONS_PROGRESS.md#agent-manager-profile-scope-admission--october-10-2026)
records the second failure and the repair scope.

| Phase | Current state | Remaining gate |
|---|---|---|
| 0 — Shared declarations | Source qualification passes; owner admission, wire forms and schema metadata are preserved | Qualify affected installed consumers with the final cut |
| 1–4 — Modules, extension points and storage | Production composition and the independent schema fixture each declare all 19 required owners. Selected owner SQL, native records, authority and recovery pass source qualification. The repaired runner and chart pass all three isolated installation generations, managed-kernel credential replacement and retained-data checks. Kubernetes accepts all five Agent policies, and the owning admission suite passes | Qualify the final reference installation's selected owner lanes and affected consumers |
| 5 — Inbound strictness | The 447-case matrix across 112 families and sixteen Rust servers passes schemas, decoders and owning hosted checks | Preserve coverage through the naming cut and installed process qualification |
| 6 — Generated consumers | Console, Kernel and MCP Apps pass owning native and browser checks, isolation, strict lint and generation. The current 18 generated bundles agree across all 36 files; the consumer batch passes 178 browser cases and eight maintained builds | Qualify remaining affected native consumers and close the required installed F-register conditions |
| 7 — Embedding identity | Profiles, producer receipts, reclamation and search races pass source qualification. The FP16 candidate and CUDA reference pass all 1,320 vector comparisons against the 0.999 cosine gate, with a minimum of 0.999959. Ranking retains all 150 judgments across 78 cases. Capacity reaches 309.584 inputs/s against the 250 gate; interactive embedding latency is 71.439 ms against the 250 ms gate. The full production Knowledge build, concurrent search, rebuild and independent persisted readback pass. Both current client GPU controls and reference Helm/Kustomize checks pass | Qualify the actual Computer Host workload boundary, final installed profile/current generation and simultaneous GPU workload budget |
| 8 — Installation and naming cut | C33 and selected owner naming, Artifact and `resultUri` controls pass. Workspace, SUMO, Python Task admission and the coordinated template cut pass their owning checks. UAV collection pages, Chart envelope admission and stale caller/schema fixtures are repaired. Flight compiles through isolated report and browser contracts. Python offset admission matches the preserved Rust profile. The unmodified OpenShell 0.1.2 decision supersedes patched-provider companion-adoption/replay qualification. The fresh SSH shell terminal cut passes owning controls and review. Official stock artifact packaging and Host checks pass. The restricted worker profile reuses Veoveo OAuth through private-key authentication and Computers-owned role authority; its signer, transport, registration and focused Gateway native controls pass independent review. Certificate-to-user promotion stays disabled. Selected stock native Host same-image restart, stopped controller maintenance and terminal renewal/revocation pass. Running-controller crash recovery is unsupported by the accepted stock profile. All eighteen selected ignored native acceptance cases pass, including distinct-image Host replacement, directed template upgrade/rollback and stock CLI transport. Node RuntimeClass activation and installed qualification remain open; activation is gated. Remaining operator reports use current fields. Independent review accepts the materialized format and receiver closure. Native-fixture lint and producer provenance pass. Client-types checks all 18 bundles and 36 outputs | Qualify final image pins and installed format consumers; drain writers and prepare fresh reference state |
| 9 — Conformance and enforcement | The catalog checkpoint covers 22 owners and all 73 scenario declarations. Both isolated normal certification binaries and their dependency graphs exclude production owners and runtimes. The discovered dependency gate admits 19 independent hosted contract graphs and four reusable kernel graphs. SUMO's contract-only and normal server profiles compile. Discovery, compiler-artifact/native-library delivery, exact framework selection and the independent scenario's actual offline dispatch pass. The composed schema smoke passes its actual owner export and all five Gateway configuration validators. The Linux deployment-smoke batch passes 54 native cases and six GitOps command scenarios, with zero ignored cases. Owner documents agree with hosted revision 4 and catalog revision 2; generation checks all 22 owners | Qualify final generation and scenario discovery. Preserve dependency admission through the remaining cut; the current shared-host compliance fixture and transport controls pass |
| 10 — Installed acceptance | The 34-image closure and both charts for `6431c30c6621` are published; matching inputs and fresh credentials were validated, pushed and applied. All 19 owner lanes and preparation/publication Jobs completed. Reviewed PVC recovery is applied and kept Embedding/Map claim identity and checkpoint checksum retention pass. The selected four-server CPU protocol case at `1393fdc36` passes authenticated discovery, all four anonymous discovery refusals, documents, transport, Gateway health/audit and prompt isolation. Frames completed-state retention passes across one same-Pod process crash. The focused DuckDB Task, delivered completion and graceful restart pass through normal Gateway OAuth at source `10821ea750`; catalog/source paging, CSV/schema, Artifact publication and usage consumers pass at `374e25f345`. The complete focused normal-OAuth Artifact upload/SDK/Datasheet consumer scenario also passes after the qualified Datasheet SDK replacement; a separate fenced Artifact service replacement preserves capability redemption and idempotent replay as one occurrence. GitOps is suspended; Knowledge, Embedding and Speech are Ready, while Host, Computers, Reason and other GPU workloads stay off. Focused Frames and Knowledge installed cases also pass on admitted deployed images. Speech Workspace cancel-draft, explicit-send, Task-after-reload and downloads pass with fixture audio on headed RTX 4090 WebGL | Complete remaining owner fixtures, remaining owner-required recovery cases, the remaining stock activation checks, final source/image closure, first-ever empty-index Knowledge startup, generation-two publication, restricted caller policy, installed consumers, Agent Manager and hardware gates |

Qualified source batches cover Knowledge's optional-Agent authority and
module prerequisites, Media cancellation receipts and Task cleanup, Recording
completion admission, and the expanded input matrix. These checkpoints do not close
all required rows in phases 1–4 or qualify the installation cut.

C32's K09 and K10 source review covers all 22 participating owners. Map's authored
Knowledge queries now check parent classification in SQL before pagination or
decoding; the native control also verifies access revisions and invalidation after
a classification change. Complete C32 qualification still requires its runtime and
installed probes; missing or skipped probes cannot establish success. Document generation admits a real,
nonempty Standards And Protocols section for every discovered owner before writes.
The assessment, adoption and document controls pass, and generated projections
agree across all 22 owners.

The browser checkpoint passes 115 Console cases and 22 Workspace cases, with
builds and Console lint on MCP SDK 2.3.1. Six App entrypoints pass their actual
JavaScript type checks, owner-schema cases and asset builds. Their controlled
results are admitted before effects. These checks establish browser behavior and
contract agreement; they do not establish rendering or GPU execution. Kernel tool
schemas derive from their argument DTOs. The combined native build and owning suites
pass, alongside strict lint over 22 packages, scoped formatting, generated-output
verification and isolated contract dependency checks.

Phase 7 supplies immutable execution profiles, directional qualifications and
producer receipts. Its 15 embedding-client checks and 41 Store/Knowledge checks
pass, including publication rollback, receipt reclamation, all four ranking-depth
races and the hosted generation-change response. The fresh schema and independent
consumer agree on 166 owner tables plus three runner tables. Reclamation deletes
chunk rows before removing their table, then cascades generation receipts in the
same transaction. Shared runtime registries survive. The full local hardware
Knowledge workload passes; installed acceptance remains open. Runtime identity comes from an installation-supplied
qualified bundle; model and image names cannot establish measured precision or compatible
vectors.
Native and synthetic checks cannot substitute for the required NVIDIA qualification.
The current installation is untouched. The new format requires a coordinated drain
and fresh state before rollout.

The user-approved host-tracing correction records method, path and HTTP version
without query parameters. The existing TestGateway leak regression and all 14
hosting controls pass. Real Media provider generation is unqualified.
The selected OpenShell profile consumes unmodified official 0.1.2 release assets.
The source package has four executables with verified archive/executable hashes;
the official gateway contains the Docker driver. The unchanged supervisor image
is admitted by its amd64 manifest and config digests, including on installation
mirrors. Host and xtask all-target compilation, thirteen Host, eight OAuth/issuer and six
packaging controls pass. Custom provider patches and compiler/solver pipelines
are removed. The download stage pins the verified current stable Trixie image and
signed October 5 package snapshots. Ops assembled and staged package image
`ea4ffc9f7`; all four official binary hashes, versions and runtime loader checks pass.
Earlier patched-provider results do not qualify this profile.

The fresh SSH-shell terminal cut passes owning controls and independent review.
Each attachment opens a new PTY and shell; Ready reports request acceptance and
normal xterm query responses pass. Selected stock native cases pass below; deployment
and complete installed stock acceptance are open.
The selected security architecture uses installation-supplied HTTPS OIDC with a
dedicated worker OAuth credential profile, exact issuer/audience, explicit upstream
user/admin roles and short-lived credentials. Host configuration disables mTLS user
promotion and anonymous user access. Complete supervisor TLS and mandatory Sandbox
JWT stay required. Separate provider-server, worker-user and sandbox-client CA roles
and projected credentials pass owning source controls; issuer roles and JWT checks
establish authorization. Optional admitted issuer CA inputs configure HTTPS trust while
certificate verification stays enabled. Runtime private-key bearer acquisition,
centralized injection and refresh pass nine OAuth/issuer controls. The signer,
checked worker registration, current role authority and exclusive Gateway owner
controls pass. Focused Gateway native discovery, managed identity and token
authentication controls pass with isolated fixture cleanup. These checks qualify
the Veoveo edge. Remaining runtime-native and installed qualification stays open. Host `providerAuthentication` is required; the native
Host fixture supplies the profile and passes source checks. The reference
`examples/bioma/computers/host.json` supplies the matching installation resource and
roles with the admitted official supervisor image. The worker registration and
public JWKS are checked in; its private key stays outside Git. The chart mounts that
file and requires its new configuration digest. The current reference keeps
`networkPolicy.enabled=false`; OIDC TLS, JWT, audience and role checks and separate
CA roles still apply. Enabling Host NetworkPolicy requires explicit
`computers.host.issuerEgress` IPv4 CIDRs for HTTPS discovery/JWKS. Host network
isolation remains unqualified. Do not add `networkPolicy.externalEgressCidrs`
independently: it creates a policy for other workload selectors even with the global
flag disabled and requires qualification of their allowed paths. The chart supplies
no issuer credentials.
The worker private-key Secret is referenced only by its Deployment and uses numeric
mode 288 (`0440`). The checked public bundle includes its JWKS. Ops verified the
official supervisor's manifest and config in the reference registry. The node-facing
configured authority returns HTTP 200, and its pinned manifest/config match the
official identities. The earlier curl refusal came from `.localhost` handling;
no registry repair or mutation was required. The native Host case consumes the
stock supervisor; installed provider/containerd pulls and complete stock runtime
qualification remain open.
The accepted installation profile reuses Veoveo's authorization server, signing
keys, JWKS and `private_key_jwt` client authentication. Computers owns the checked
`ai.veoveo/computer-worker-authorization` catalog section, non-MCP resource audience
and closed worker role vocabulary. Its restricted client has one resource, one
scope, client-credentials grant and private-key authentication; the worker key is
separate from operator and browser credentials. No additional Entra application or
OAuth service is required. The Gateway decorates the existing registration through
an exclusive owner claim, checks current membership and rejects owner collisions
and disabled-client fallback. Shared assertion signing belongs in `platform/oauth`;
credential-file admission, token caching and provider role decisions stay with their
owners. Stock-compatible discovery exposes the JWT access-token verification
fields without claiming unsupported OpenID login or ID-token flows. The source batch
and reference guide pass independent review. Remaining runtime-native and installed
qualification stays open. OpenShell stays unmodified. Owner authorization permits
native qualification with the owning test-only issuer; stock activation requires
qualification of the accepted aggregate resource profile below. Stock driver config now emits only supported mount fields. The owned
image leaves `/sandbox/persistent` absent to skip Docker copy initialization before
registration; the allocator initializes its home outside the provider and writer
fences stay enforced. Compilation and 17 focused mount/lifecycle diagnostic controls
pass; the selected native Host case also exercises the real plugin-mounted owned
image and retained bytes. The reference catalog materializes the stock template and new fingerprint at
`1666b8f66`; activation still requires a coordinated drain.

The owner accepts stock OpenShell resource behavior. The outer Host must bound
aggregate CPU, memory and PIDs for all descendants, including supervisors; supported
workload settings retain their upstream semantics. Separate supervisor CPU, memory
and PID maxima depended on deleted private patches and are not part of this profile.
The selected native Host case passes at source `5ec40a4a5` with Host image
`6b5617f73f5a` and owned template `77eca42bc69f`, using official stock supervisor `90a6f1a7`.
It verifies authentication across replacement, allocation interruption, retained
Create/Start bytes, guest identity and supported child security/limits. Aggregate
containment checks pass under one CPU, 6 GiB and 1024 PIDs. Replacement uses the same
image and takes 2339 ms; it does not qualify image upgrade. Independent postcheck
confirms owned fixture cleanup and preserved core workloads.
At source `04dfbe966`, the selected runtime-native cases pass stopped controller
maintenance and terminal renewal/revocation. Known Stop precedes controller SIGKILL;
a fresh Stopped read then permits explicit Start with a new process, retained bytes
and a fresh shell. Renewal crosses SSH admission credential expiry without reattachment,
revocation denies I/O, and fresh authority opens a shell in the same admitted run.
The fixture targets `/sandbox/persistent`; stock `HOME=/sandbox` had denied its
marker write. Running-controller crash recovery is unsupported: the guest retains
its prior supervisor binding and rejects the replacement, causing startup failure.
Unexpected outcomes keep their run fences and cannot authorize another mutation.
Both runtime postchecks confirm owned cleanup and unchanged core workloads.
The additional stock native matrix passes twelve selected cases: runtime four, MCP
three, Execution three and Storage two. Runtime, MCP, Execution and the unchanged
Storage filesystem use the accepted `28a820619` inputs. Storage service first failed
because its synthetic writer inherited the Host entrypoint; retained daemon logs
show its exit before the first write. The reviewed fixture-only explicit sleep
entrypoint at `2f5f919a3` passes with a newly built test artifact. The final follow-on
qualifies all eighteen selected ignored native acceptance selectors, reusing the
prior fifteen proofs. The stock CLI passes in 19.430 seconds across three-second
SSH admission expiry; this does not qualify hour-long bearer expiry or public login.
Its initial failure inherited a sleep main workload, corrected by the reviewed
fixture at `cef514412`. Host replacement from `6b5617f73f5a` to `d2b5525355e0`
passes in 35.970 seconds with the same template. A pre-effect missing-local-image
attempt was settled by Ops's explicit pull, without a provider change. MCP-directed
template replacement from `77eca42bc69f` to `7d6803eedd00` and rollback pass in
77.833 seconds with a 512 MiB home and 32 MiB temporary limit. Final postchecks
confirm owned cleanup and unchanged node/core workloads. These isolated native
results do not qualify the installed 8 GiB home or installed resource profile.
The five images from `2f5f919a3` and Veoveo chart from `1e565a826` are published;
chart digest `38387a5aa7cd` and matching image/configuration/module inputs are
committed at `1e565a826` and `a508f8e6e`. The preservation update at `26c0f7ec4` keeps qualified Artifact Service and
Speech images and Speech's one replica. The existing worker OAuth key matches its
checked public JWKS; fresh trust is prepared offline without live Secrets. All 28
Computers tables are empty and no nonterminal Tasks are present, and the unbound Host PVC is expected WaitForFirstConsumer
state. Node RuntimeClass activation, live Secrets/configuration and installed
qualification remain required. The remaining stock image closure contains seven
images: Computer Host, Computer template, Computers MCP, Gateway, Console BFF,
Agent Manager and Knowledge. The prior five-image publication and native results
keep their recorded scope. Manager `b70f09038e25` and Knowledge `4a3097f3df40`
are staged from `32023cd58`; their prepared pins preserve the tracked source hold. Stage receipts and BuildKit provenance bind that source; both runtime image
configs lack an OCI revision label. Registry readback verifies the runnable digests,
not release eligibility or installed reader admission. First qualify current Manager and Knowledge images
against the unchanged live catalog, policy and enabled module composition, replace their old
readers, and restore Ready plus functional Knowledge indexing and search before
publication. Preserve Knowledge desired replicas at one while its Recreate deployment
transitions, and keep Embedding and Speech Ready. The current registry admits the old
catalog without both the worker section and its client. A reader that does not register
the new section rejects it; retained image admission is unproven. Only after compatible
reader image provenance and drain are proved may one atomic full-catalog publication
introduce the matching section and client. Chart Deployments and publication Jobs
can start concurrently, so a single ordinary Helm reconciliation cannot establish this ordering.
Stable CDN CIDRs do not block stock authentication under the current network profile.
The runtime DinD fixture uses two CPUs, 1 GiB and 512 PIDs. The installed chart
declares eight CPUs and 12 GiB. Ops generated its complete OCI base with pinned
`ctr` 2.3.4-k3s1 and verified the sole 4096-PID transformation. Node configuration
is unapplied; handler installation and finite-limit verification in a new Host Pod
must pass before activation.
Fresh volume allocation admits Engine
absence before publishing the filesystem as Ready and consumes one typed proof for
labeled creation. Existing allocations only inspect approval; missing or unapproved
metadata cannot authorize another Create. The repair passes independent review and
its complete Storage library and diagnostic controls. Earlier patched-profile native
fixture results are historical. Heap-pinned setup and assertion
phases reduce the four MCP fixtures' large async poll frames. Worker and Commands
advance beyond the original overflow. Worker exposes two fixture defects: an unsupported
stock-CLI exit deadline after revocation, then a tunnel URL that drops the server mount.
The repaired oracle checks relay closure, refusal of a fresh revoked-grant connection
and absence of post-revocation output before owned process cleanup. Its typed URL builder
preserves the mount; the affected compiler and offline controls pass. Policy continuity
admits the provider's UUIDv4 attachment epoch and configuration-instance identity
separately from immutable template settings. Replacement admits both peers' identities;
same-instance reads and watches preserve equality. All 103 runtime controls and
independent review pass. Pre-stock native Commands, Maintenance and Files cases
passed execution, containment, retained-home upgrade and rollback, file transfer
and refusal controls. They do not qualify the selected stock provider.

Worker's native Stop gate uses the production scheduler to observe the original
operation through its stored deadline and finite read budget. The fixture must
materialize that operation's Task before anchoring the maintained current-owner
listener; queueing an operation alone does not create its public Task. A real-store
control reproduces the empty subscription baseline and proves same-Task admission
without dispatch or changes to the deadline and read budget. Final acceptance still
requires Task success, original-operation settlement, unchanged resource/process
identity and the allocator-offline condition. The scheduler repair passes compiler,
strict lint, four focused controls and independent review. The Task-link correction
passes its compiler, strict lint, real-store control and independent review. The full
pre-stock native case passed original Stop settlement while the allocator stayed offline,
retained Start and proof-file readback, recovery, cancellation, denial and lost-dispatch
containment. Reviewed closed diagnostics record call stages and phase without provider
payloads.

The pre-stock Host native case passed namespace replacement, retained Docker and
Computer identity, retained bytes, a new process identity and resource limits.
The selected stock same-image restart and distinct-image replacement pass security
probes on both generations; directed template upgrade and rollback also pass. Its fixture-only mapping separates local image admission from private pull
addresses and checks manifest bytes, headers and image identity before creating
fixture state. Compiler, five controls, strict lint and independent review pass.
Every terminal fixture is cleaned up. Installed execution stays open.
Published candidates await the coordinated installed activation; retained rollback
images stay available.
The [provider package](../platform/runtimes/computers/provider/README.md#official-artifacts)
binds its copied executables to official release archive and executable hashes.

The finite continuation review covers 50 production codec variants and 16 additional
continuation families, with documentation and test examples classified separately.
The public Audit query cursor now uses `lastId`; its actual registry and Gateway
receivers refuse retired, mixed and duplicate keys while preserving partition and
ordering checks. Signed Audit formats keep their bytes. Console's saved upload rows
reject unknown and retired fields before restoration or status reads. Both repairs
pass their owning controls and independent review. The installation-pin fixture now
uses the actual seven-file Gateway bundle preimage; current pins are unchanged.
Client generation agrees across all 18 bundles and 36 outputs. The maintained Helm
configuration smoke verifies the approved Agent template's complete two-file ConfigMap,
immutable name and owner-computed digest. Installed consumer gates remain open.

The required harness, Embedding, authentication and image-copy review records 27
additional producer and receiver families with explicit diagnostic and internal
roles. The basic Embedding reference decoder now admits its complete typed v2
document before endpoint or credential access. It rejects retired, mixed and
unsupported markers and checks precision and vector relationships. Its current
fixture and CPU admission controls pass; producer bytes and hardware captures are
unchanged. The final source-use review classifies all 1,351 previously unlinked
serialization expressions and materializes their outer envelopes and receiving owners.
It identifies no additional changed-version format without a corresponding version
cut or current receiver. Nested DTOs, diagnostics and scalar encodings are classified
without counting them as independent formats. AppCatalog event decoding now uses the
generated owner schema before callbacks; its seven controls, type checks, lint and build
pass independent review. Independent review accepts this materialized source closure;
historical ledger statuses are reconciled with the accepted repair receipts. Installed
format consumers and image qualification remain open.

The full Python enforcement passes 551 SDK, 60 template and ten independent-fork cases.
Its Task-storage exchange uses the actual Cargo-selected Rust receiver against a fresh
SDK store. Typed caller fixtures and owned-container timeout cleanup replace stale
test assumptions. Conformance discovery preserves one original deadline in its typed
progress context, and transport error handling preserves the admitted I/O error.
The owning runtime and conformance libraries pass 93 and 58 controls and strict lint.
The current stock runtime/MCP all-target checks and strict lint pass. Host and xtask
all-target compilation, thirteen Host, eight OAuth/issuer and six packaging controls pass.
Historical provider image builds and native fixture results apply to the earlier
patched profile. Stock native authentication, lifecycle, retained storage and installed
acceptance remain open. Scoped Cargo cleanup retains dependency libraries, current executable links, the newest
incremental variants and frozen acceptance inputs. The two repaired Python image
contexts pass their locked package builds and development-stage publication;
these checks do not qualify the full release. Console and Workspace build-input
lists include the canonical notification fixture; their native builds and actual
frontend Docker stages pass. The 34-target release is qualified at source
`6431c30c6621`; its receipt binds each runnable image digest and publication index.
The `veoveo` and `uav-sim` charts are published at `0.1.0-6431c30c6621` with
manifest digests `ea8f452d…7e44f` and `c88d51f2…b4bc1f`. The generated module plan
binds all 19 owner lanes to the released Gateway image. Full values keep every locked
image and Deployment in the render; the HelmRelease stages Computer-dependent and
out-of-batch GPU workloads at zero replicas while the initial target selects Knowledge,
Embedding, Datasheet's Artifact consumer and their CPU source services. The UAV
HelmRelease is suspended for this first acceptance batch. Image locks and outside-Pod
runtime references use the published runnable digests. Source and render checks and
independent review accept the release inputs, committed and pushed at
`55a9c57840be02b32d13c5df76d20d6ed3a448e1`. The fresh reference k3d cluster and Flux
platform controllers are running. Its RTX 4090 uses NVIDIA driver `610.57.04` and
the device plugin advertises eight shares; this does not qualify a GPU workload.
Matching Knowledge signing and Embedding credentials were validated with the four
public configuration updates, pushed at `8055` and applied. The fresh installation
completed all 19 owner migration lanes and preparation/publication Jobs. Sixteen
initial Deployments and Agent Manager were observed Ready before Helm timed out on
eight deferred WaitForFirstConsumer claims and remediated by uninstall. The reviewed
recovery at `e4138f61f` omits those fresh unbound/no-data declarations and marks active
Embedding/Map claims for Helm keep. Bound or data-bearing deferred claims require
an owner recovery plan. Ops has resumed the corrected release and verified kept
claim UID/PV continuity and all ten pinned checkpoint file checksums after uninstall.
Knowledge and Embedding are now restored at one Ready replica each with the retained
model PVC identity unchanged. Speech also runs at one Ready replica. GitOps is
suspended; Host, Computers, Reason and other GPU workloads stay at zero replicas. Unattended cold-start qualification stays open. The old shared 197.9 GB
checkpoint/model volume is preserved because fourteen retained containers mount it.
The selected `installed-protocol` case at source `1393fdc36` passes once in
49.683753 seconds through normal `operator-initial` and administrator OAuth clients.
DuckDB, Timeseries, Frames and Media pass complete tools/resources/templates/prompts
catalogs, docs index/contract/design reads, `doc_id=de` completion to `design`,
authenticated admin documents and unknown-address rejection with -32602. Normal
FullMcp discovery excludes declared compatibility helpers; the receipt preserves all
four expected and observed tool sets before assertions.

All four mounted liveness/readiness routes return 200, missing Host returns 400 and
wrong Host returns 421. Media's direct unsigned callback returns 401 with
`invalid signature`. Administrator health returns all 19 registered server states and
check times; the operator's own profile returns 403. A filtered public Audit export
correlates an Allowed ServerHealth admission with the actual caller and request
interval. Prompt discovery returns 20 prompts while declaring seven actually
unavailable upstreams. Four anonymous official-SDK discovery attempts each return
HTTP 401. The 55-observation receipt qualifies its selected scope and records completed
client cleanup. Ops confirms Gateway and all four CPU servers Ready at their unchanged
image selections.

These observations qualify the selected A01, A02, A03 transport/resource, A04 and
unsigned-callback A07 behavior. Recording gRPC, other owner fixtures and final
source/image/configuration agreement remain open. The earlier
DuckDB export is settled as cancelled with no result URI and zero produced Artifacts;
unknown unlinked partial effects remain untouched. The focused installed-host case
qualifies one DuckDB query Task and its completion listener. Other owner Task and
subscription fixtures remain open. Artifact preparation hashes each shared file once
per admission and uses each scenario selection's effective dependency closure.

The expanded Frames A06/F-register consumer at source `a140c0571` passes once
through `frames-installed` on admitted deployed Frames image `032ce55005cb` and
Gateway image `ab2d13f78a72`. It records 207 known outcomes: 101 Created, three
Published, one expected InvalidParams conflict, one direct Converted and 101
TaskCompleted. Three worlds pages, two usage pages containing 101 fixture usage
entries, parent-scoped completion, concurrent publication/replay, current-head and
immutable metadata agreement, conversion provenance, subscriptions and five foreign
caller probes qualify the selected F31–F34 and F36 installed behaviors. Connections
close, Task subscription cleanup succeeds, and append-only fixtures are retained.
Ops confirms unchanged deployment/Pod identities, image IDs, configuration and Ready
state after that expanded run. The separate `frames-recovery` case at source
`1393fdc36` passes once in 244.127 seconds after Ops sends one SIGKILL. It observes
exit 137 with reason `Error` and an unavailable signal field, then a new container
at restart count 0→1 with the same Pod UID, ReplicaSet UID and image `032ce55005cb` Ready.
World, revision, both operation records and the full completed Task payload agree
before and after the crash; two foreign operation reads return -32602. The final
live identity fence follows all reads, and all five cleanup results pass. This
qualifies completed-state persistence and visibility across that process crash.
Ops confirms the retained workload identities and the one Ready replacement without
another signal or retry. Final source/image agreement stays open. Separate owner-required in-flight and cross-replica cases
remain unqualified.

F35 installed synthetic-reference admission and the current UAV library consumer
pass at `793bc7875`. Seven requests settle two writes and verify four readbacks;
conversion returns -32602 and the library consumer refuses dynamic ancestry. The
client closes and the append-only fixture is retained. This qualifies the synthetic
reference and current library consumer, without establishing a live producer, UAV
process, GPU execution or timestamped conversion. Frames preserves its typed producer
seam. Qualify each actual producer's route, parent IDs and timestamp semantics before
advertising it; this requirement does not mandate a new producer implementation.

The installed Timeseries case at `027e9bdee` passes one four-row NaiveTrend Task
through normal operator OAuth. Delivered completion and the full payload agree with
owner metadata, RRD Artifact metadata and bytes, recording provenance and caller-scoped
usage. A distinct administrator receives -32602 for the foreign usage read. The v2
receipt preserves both preflight catalogs and seven read observations; subscription
and both client closures pass. The Task, Artifact and usage are retained. Ops confirms
selected workload identities, images and Ready state are unchanged. Cancellation,
replica replacement and final image agreement stay open.

Gateway adds template discovery and neutral resource reads to the two existing Artifact rules;
the selected ConfigMaps, mounted configuration and Ready replacement Pod agree while
the image stays unchanged. Knowledge recovered from an indexing 503 without restart;
unattended cold-start and current-policy recovery deadlines are unqualified.

The native Media case at `027e9bdee` passes the full fake-provider lifecycle,
foreign Artifact refusals, caller-isolated catalogs and stored-generation readback
after an owned restart. Checked process stops and workspace removal pass; Ops verified
that the disposable database is gone and both native ports are free. Real paid-provider
and hardware GPU generation remain unqualified.

The focused Knowledge A08 case at source `07742e1623` passes once in 10.29 seconds
through normal Gateway OAuth on retained generation
`01a11c6f-7316-7d01-8485-291e027ea5f8`. It verifies 16 collections, four sources
(Artifact, Charts, Map and Time), 11 source-document links, catalog observations and
completions, then awaits subscription cancellation and client closure. Ops confirms
Knowledge image `7f2bd6b08476`, Embedding image `c1c9f6fd5c10`, the four source
services and Gateway are Ready, with retained model identity preserved. A separate
installed CNI check reaches the same Embedding Service IP from an admitted platform
pod with HTTP 200 using its key and 401 without it; credential-free pods bearing the
`computer-host` selector and outside the namespace receive connection-refused exit 7
and HTTP 000. Cleanup deletes all three probe Pods with UID preconditions and the
owned namespace, then stops the proxy. This qualifies the observed policy selectors
and key boundary. Actual Computer Host workload isolation, pre-indexing 503,
first-generation cold startup, final source/image agreement and simultaneous GPU
workload budget stay open.

The CPU `installed-host` selector reuses the installation
harness for one bounded query Task, delivered completion, an admitted DuckDB process
drain and completed-result retention. Compiler checks, focused behavioral
controls and independent review pass. The canonical installed scenario passes once
at source `10821ea750985a5f09922d22b8bf69a0fccf3945`: `SELECT 1 AS answer` completes,
the exact Task listener delivers the completed state, and the full typed result
agrees before and after one UID-fenced DuckDB restart. The old container exits with
code 0 inside its 30-second termination grace period, the replacement becomes Ready,
and Deployment, image, PVC and PV identities are preserved. Cleanup awaits both
listeners and the MCP client; the Task and existing database are retained. This
qualifies the focused A05/A12 case on the admitted deployed image and configuration,
without proving all-server acceptance or final source/image agreement.
The canonical `cargo xtask smoke installation-verify --scope duckdb` passes once
at source `374e25f3450a4ff0f2c34df5822bd59f15253888`, exiting 0 in 239.499 seconds
including preparation. It creates 100 fresh fixture databases and completes 102
Tasks with all 102 updates delivered. Its 114 typed reads cover three database and
three usage page observations, 101 corpus usage entries, the four-row CSV schema
and output, Artifact bytes and digest, native Task provenance and usage quantity/unit.
The final database set equals the baseline plus owned fixtures; the distinct
administrator receives owner-specific refusals. Listener and client cleanup is
awaited. Ops confirms unchanged admitted images, core Ready and suspended
controllers. These public consumers qualify the named F41/F58 checks; headed
Workbench, physical owner-directory/metadata inspection, further recovery and final
images remain open. The private scenario receipt SHA-256 is
`5ca9d5ca9094caf7e41937907f26802833ed09e66dbf29eb18ad9c400d5f021f`;
the separate Ops postcheck is
`3a4fa2f0abae7fb2a0b8bae300551232b453d6c32215cc5867b13aaee2a1d953`.
The direct unsigned Media webhook returned 401 with an invalid-signature rejection.
Installed A/F/H, unattended Knowledge startup, installed consumers and hardware
gates remain open.
The installation target checks Deployments in `veoveo`; Agent Manager in
`veoveo-agents` needs its separate bounded KubeOps rollout check and JSON receipt.

The Python SDK admits the preserved `v1:N` offset grammar and 64-bit range,
including Rust's admitted leading-plus and leading-zero aliases. It refuses
non-ASCII digits and overflow before integer conversion. Paired Rust and Python
controls and the actual registered template resource-list receiver pass.
The Computer file writer and helper select numeric header version 2 and
`maximumBytes`; codec and actual helper-process controls reject old and mixed
headers before filesystem access. Execution protocol version 1 is unchanged.
The rebuilt helper-bearing template advertises execution protocol 1 and file protocol 2.
Its pre-stock native case passed import/export of 1,000,003 bytes and SHA-256 checks,
rejection cases, uncertain short-body handling, settled Stop→Start, retained-byte
readback and stale-process refusal. Both native fixtures have completed owned
cleanup. Installing the v2 writer still requires the affected image closure,
coordinated drain and installed acceptance.

View's CUDA 13.3.1 local NVIDIA hardware smoke passes device and GPU JPEG
admission, four PNG/JPEG captures and their byte and encoder-completion checks.
The [owning smoke](../examples/bioma/acceptance/src/smoke/scenarios/view.rs)
interrupts an actual MCP-created capture under its live 180-second claim, starts
one replacement before expiry, preserves that claim until expiry and then reads
its recovered JPEG bytes. The GPU UUID stays unchanged. Caller isolation,
cancellation and graceful process drain pass, and the owned fixtures are cleaned.
The shared installed observer now requires one full terminal/deletion snapshot for
the admitted container instance, with fenced mutation and termination-grace checks.
Eight CPU controls and independent review qualify its source. View's owning installed
mode now checks local catalog bytes and the effective container entrypoint before
public effects, handles uncertain creation without replay and reconciles owned cleanup
after replacement. Compiler checks, seven CPU controls and independent review pass.
Installed consumers, cross-context Task delivery and pod termination-grace acceptance
remain open; this mode does not qualify active-Task interruption.
This local run includes no Google or billed provider work.

Linux Map qualification includes the current travel-model product cases after
the shared C02 writer correction. Optimization's nine native read cases pass.
Recording's 31 normal native
cases pass after aligning its registration and shared catalog fixtures.

The accepted Foundations installation covered sixteen Rust servers, eighteen
participating knowledge sources and the composed flight at `6d4cd2c5`. Headed
RTX 4090 WebGL supplied flight visual acceptance, with 0.198-second lag against the
one-second gate. Installed Map/UAV handoff and Computers grant consumers, and the
complete unattended cold-start gate, remain open. Ready pods alone do not close them.
Knowledge and Embedding are core services; Reason runs in its separate batch.

The current source checkpoints are:

| Concern | Implementation revision |
|---|---|
| Kernel tool schemas | `4d78a5e80` |
| Console snapshots, events and lightweight owners | `14f1f99bd` |
| MCP App contract admission and generated consumers | `808d80d29` |
| Optimization preparation gate | `b27d53adc` |
| Embedding profiles and Knowledge producer/search fences | `89cf4f9d2` |
| Map repository, product and selected-record admission | `9e0318410` |
| Recorded video, Stream, Reason and Timeseries relationships | `09540a03d` |
| Artifact wire ownership, Task bindings and access-progress admission | `6bdcbb919` |
| Time, Computers, Speech, Media, Optimization, UAV, Recording and SDK value admission | `2b32885bf` |
| Remaining current-wire consumers and complete UAV/Reason schema refresh | `e86d7a28f` |
| Isolated browser/report contracts and standalone Flight dependencies | `944e95496` |
| Python offset admission and registered template receiver | `61435785e` |
| Computer file header v2, helper admission and coordinated drain | `eb1fef5e4` |
| Recording and simulation operator reports and module identity output | `eda9987b7` |
| Safe terminal stages and bounded native failure diagnostics | `47868a1b5` |
| Stock OpenShell resources under aggregate Host CPU, memory and PID limits | `5ec40a4a5` |
| Installed Speech transcript-resource snapshot assertions | `9bc60e8cf` |
| Public Stream and Reason Task delivery, resource snapshots and retained SDK cleanup | `81802e8eb` |
| Strict saved-upload row admission before browser restoration | `28ff03897` |
| Complete Gateway bundle pin fixture | `1c645d680` |
| Public Audit cursor and generated consumer hard cut | `0b43ab6f6` |

These revisions qualify their implemented source concerns. They do not close the
wider F-register or the hardware and installed gates.

The catalog batch published in `17517f9d1` passes 101 unique Rust cases, 449 Python SDK cases
and 37 Charts cases. Template and independent-fork checks, local Charts image API
checks, generated documents and strict all-target lint over 22 packages also pass.
The development checkpoint captures the integrated source before full qualification.
Those counts cover the affected source checks, not every server's domain suite or
installed acceptance. The shared-host module suite passes with the complete current compliance
fixture. Its transport, authorization and Host assertions are preserved.

The published tree at `9ccd95fae` passes whole-workspace formatting, the shared-macro
catalog over 2,569 tracked Rust sources, and identifier enforcement over 6,426
tracked text files. The naming suite passes all 28 cases. These checks do not close
the remaining native, dependency-graph or installed gates.

The final conformance batch qualifies both actual normal certification binaries
independently of hosted development fixtures. It also passes all six conformance
integration cases, current owner-document discovery and the independent contract
scenario through the dispatcher. Its original prerequisite cache miss is resolved
by admitting the exact locked archive; preparation remains offline. Deployment
fixtures use the current policy spelling and supply Knowledge's required embedding
runtime key while preserving the owning refusal assertions. The dispatcher reports
the retained private diagnostics directory when preparation exits unsuccessfully.

The owner-document checkpoint at `f4215919c` corrects revision references and
regenerates compliance notes without changing requirement statuses or independent
formats. Four owner-document/profile integration cases, nine Python template docs
cases and two isolated packaging cases pass. The generated manuals agree with all
22 profiles. The protected shared-host documents and fixtures remain separate.

The latest source checkpoints implement Map repository/product admission and the
recorded video, Stream, Reason and Timeseries result relationships in F10, F12,
F39, F45–F49, F61, F63 and F64. Independent review accepts both source batches after
the caller and relationship repairs. The affected graph passes 358 unique native
cases, strict all-target lint over twelve packages, five isolated owner contracts
and both independent consumer profiles. The generated Apps pass 23 cases and their
type checks and builds; Reason's private protocol passes 21 Python cases. Formatting
and generation checks pass. The catalog batch is included in the published checkpoint.
Installed and hardware conditions stay open. The Artifact source pass in F25 and
the Rust portion of F27 moves pure wire values to its existing lightweight
contract and carries typed Task identities through callers and database drivers.
It adds shared access-progress admission and controls for raw UUID aliases and
retained binding corruption. Independent review accepts the completed source.
The affected graph passes 223 native cases, all-target compilation and strict lint
over 28 packages. The isolated Artifact contract and both independent consumer
profiles pass. Generated outputs are unchanged; Console and Workspace tests and
builds pass, alongside Console lint. The Artifact owner's normal dependencies
exclude MCP and runtime adapters. The current SDK batch qualifies metadata and
address admission in F27 and F68 locally; installed multi-page consumers stay open.
The remaining Computers provider suites, Speech GPU execution, external byte
stores and installed consumers remain separate unqualified gates.

The source checkpoint covers Time, Computers, Speech, Media, Optimization,
UAV, Recording and the Python SDK with their direct consumers. Independent review
accepts their source, including Recording's stage-fact admission before reconstructing
seal bytes, Media's single registry snapshot, Optimization's selected-parent checks
and pairing token/grant agreement. The affected native suites and strict all-target
lint over 29 packages pass after grouped caller and fixture repairs. Isolated
contract graphs, doctests and both independent consumer profiles pass. The latter
decode all 34 Optimization input branches before exercising their schema, default
and negative controls. Generated schemas and client types agree with their actual
producers. Console, Workspace, the UAV App and Workbench pass their affected checks;
the headed-GPU and installed gates remain open. Qualification also passes against
the main contract independently of the catalog batch: both consumer
profiles, seventeen document/setup controls, SDK and template callers, and repository
policy checks pass. The catalog's protected shared-host fixture remains separate.

Recording's public seal recovery passes over retained Writing and Staged properties,
including refusal of mismatched bytes without rewriting them or reserving another
Artifact occurrence. Sixteen independent properties encodes produce identical
complete files. Maintained readers verify their messages, CRCs, footer spans and
schema identities. Sorting the footer's schema fields corrects the producer's
unordered output and changes complete-file hashes; Phase 8 includes those newly
produced bytes in its fresh-state manifest and playback cut.

Speech's Task-to-transcript listener adopts the shared Task watch already used by
Stream and Reason. Its owning control passes actual listener delivery,
current-policy refusal and subscription-context cancellation through the notification
sink, and affected source consumers pass. Other owners also observe domain mutations,
grants, usage or live runtime state; those sources keep their current delivery semantics.
F62's raw installed subscription updates, cancel-recovery and coordinated replacement
remain open; the Workspace case below qualifies its named consumer actions.

At source `f4bff966e`, Speech's maintained native GPU report-v2 and hosted-contract-v4
controls each pass once on the RTX 4090 with checkpoint
`73175eb7aeb0d82f1e2a6b53b3aabc10a90bcd0b`; the existing interpreter,
package and model pins in the [Speech design](../servers/speech-mcp/DESIGN.md#packaging)
are unchanged. They cover file and live transcription,
private dictation, Task observation and cancellation, recovery and output-ID reuse.
Hosted conformance passes 34 checks with two optional absent-surface skips.
With Knowledge and Embedding retained, GPU telemetry shows at least 9,927 MiB
free. Ops confirms both native fixtures and the monitor cleaned up,
with core Ready at the postcheck. These selected cases do not close the full
simultaneous peak-load budget.
The browser's visibility, software-renderer warning and hardware-loss guards pass
compiler, strict lint, three pure hardware controls and independent review.
The maintained `speech-workspace-verify` passes once at source `8184ec52d6`, covering
cancel-preserved draft, explicit send, Task observation after reload and verified
downloads. Initial and final headed graphics probes identify RTX 4090 WebGL;
software WebGPU is excluded. Audio comes from the fixture MediaStream through the
real AudioWorklet and CUDA, which does not qualify a physical microphone. The
owner closes its target, and Ops confirms process cleanup, Ready workloads and
suspended controllers. The private browser receipt SHA-256 is
`81c786061eefc8e7d2e5ca4b5fb4ca9df339f08ec59521ee3f163b9b66a25217`;
the separate Ops postcheck is
`d2413f48fc77070ba966b4e98bb171660fecb490eb097fc508b41182e18ef9b1`.
The committed Speech overlay
at `e38c1b114` stages in 7.07 seconds with the correct Veoveo source/revision labels.
All eleven filesystem layers match the `d85a05cf5` image, including its eight
dependency layers. That image passes offline CUDA worker startup as UID 10001
with networking disabled and a read-only root filesystem; readiness identifies
the pinned checkpoint and RTX 4090. Owned-container cleanup completes with Docker's
forced-stop fallback. This direct-worker check does not qualify production host
shutdown. Installed Speech image `7f7e27c242bd` is Ready and its mounted
`healthz` and `readyz` return 200. The strict single-Deployment rollout gate fails:
Helm restored Gateway's checksum annotation from its saved manifest and restarted
that Pod. Gateway recovered Ready with the same admitted active policy revision
and digest; Knowledge and Embedding are Ready. The parent Kustomization and platform
HelmRelease are suspended again.
The ordinary human Workspace upload-policy read now returns 200 with
`allowed=true` after Artifact service image `e26082a19437`. Its decoder accepts the
Gateway's camelCase profile wire shape; the serializer regression and typed upload
fixtures pass four existing native controls, compiler checks, strict lint and review.
This installed upload and Speech browser pass does not qualify raw subscription
updates, cancel-recovery, all F62 consumers, coordinated replacement or separate owner-required in-flight recovery. The selected
production Rust host now passes idle SIGTERM: its old container exits successfully
before deletion and inside the 90-second grace period, and its replacement is Ready
on the same image. The postcheck confirms core workloads and controllers were
preserved.

The [progress log](PLATFORM_FOUNDATIONS_PROGRESS.md) preserves checkpoint details and
historical failures. Reuse accepted checks while their source, dependencies and
execution environment match; rerun checks affected by shared changes. Historical
status statements do not override this plan or its required transfer registers.

The conformance transfer and foundational naming declarations are integrated.
The naming declarations pass focused all-target compiler and behavior checks.
The core frontend, Bioma/Gateway and five direct smoke consumers compile
with their actual features. The standalone conformance graph excludes domain
implementations, and an independent contract-only server passes authenticated
onboarding. Focused discovery, compiler-artifact selection and local lifecycle tests
pass. The actual smoke command admits the Python SDK workspace lock and lists all
73 scenarios. Locked preparation and execution bind the SDK workspace member and
the independent fixture to their admitted source paths and prepared interpreters;
their project and lock files remain unchanged. These checks cover the selected
profiles, without qualifying arbitrary extras or dependency groups.
Independent review accepts Candidate's admitted launch receipt and retained process
handles, and preservation of caller timeouts under the owner deadline. Flight's
production MCP calls preserve acknowledged Task identity, reverse cleanup order and
one landing dispatch across ordinary errors and interruption. Focused local process
controls and five maintained-peer Flight cases pass. The maintained Node/pytest
selection case passes positive execution and refusal of skipped, failed and missing
cases through both frameworks, with zero ignored cases in the owning native result.
Final build-boundary checks and installed process cleanup remain unqualified.
C33's paged input/output schema inspection, owner-observed body evidence
and generated declarations pass their affected source controls. Repairs for the six
review classes pass the naming matrix and hosted-peer degradation cases. Independent
review accepts dictionary/reference contexts, dependency-member accounting and the
supported literal-association profile. The full conformance library passes 57 cases,
including 29 naming controls. Actual SDK and Chart declaration consumers pass their
affected checks; remaining owner discovery needs qualification against the final cut.
Artifact startup passes real runtime preparation, history, identity, rotated-credential
and checksum controls without storage effects on refusal. Its write-marker/readback
controls and 35 contract cases pass; the SDK's 39 affected cases pass. Bioma's typed
control-plane digest passes normal comparison for the captured Frames prompt cut.
Generated consumers and final composed build qualification remain required.
Independent review accepts Map's native active-release adapter, bootstrap closure and
nested raster/quality-report admission. Nineteen selected native cases pass, including
actual SQL row decoding and producer schema captures. All 23 helper cases and the pinned
GDAL GeoPackage inspect/decode/encode roundtrip pass after the command-field repair. DuckDB and
Timeseries pass their composed compiler checkpoint and production source review.
DuckDB's catalog and Task-result identity controls, Timeseries' source and RRD
admission controls, and Frames' transactional authority controls pass on Linux.
The repaired schema fixtures agree with complete outputs from their current
producers, including Timeseries' consumption of the DuckDB source schema. Their batch includes the
shared Workbench reader and Media's page contracts. Media's complete owner and direct
smoke-consumer cut passes default, contract-only and runtime-only compiler checks and
seven selected library controls. Eleven current native and receiver cases pass,
including billing-kind/metadata agreement, current and mixed-format admission and
the actual generated schema comparison. Wider generated consumers and installed
delivery remain unqualified. Time's repaired public clock,
prompt and HTTP path adapters pass source
review, and Time's authenticated hosted control passes. Speech's current-format owner,
retained reader, hosted admission, worker protocol and schema comparison pass their
Rust checks. Its ten Python protocol controls pass both decoder paths and actual
worker refusal before effects. Independent source review accepts the finite attribution,
Python admission and final schema annotations. Generated consumers and fresh NVIDIA
qualification remain open. Frames' complete owner and
direct-consumer cut passes compiler and
contract/schema isolation checks. Its native library, server and contract suites
pass all 43 cases after the Artifact metadata fixture repair in `c68ab4850`.
The shared wildcard-removal admission and real startup guard pass their affected
controls; installed hosted checks remain open. Time's batch in `b3a2006d7` passes
114 distinct native cases, with one installed gateway-source check still unrun.
Optimization/cuOpt and View complete their
source passes and pass grouped compiler checks. Their fifty selected library cases
pass; sixteen Optimization Python protocol cases pass without executing a solver.
The current Optimization native read suite passes all nine cases through the
shared C02 writer and reader. Its authorization and retained-result assertions run
with the canonical `resultUri` product field. The two owning
[cuOpt GPU tests](../servers/optimization-mcp/tests/cuopt_gpu.rs) pass in 1.42 seconds
on the NVIDIA RTX 4090 with driver 610.57.04 and the pinned cuOpt 26.8.0/CUDA 13.3
executor. They qualify routing, LP, QP, QCQP, SOCP, MILP and empty-matrix model
execution; the isolated executor and its socket are removed after the run. Installed
MCP hosting, durable Tasks, route scenarios and `agent-pilot` are unqualified.
View's actual App producer comparison, maintained
schema generation, three consumer cases and App build pass. View's GPU JPEG source
passes independent review, compiler checks and isolated contract consumers. Its
exportable Vulkan buffer supplies RGB bytes to CUDA and the explicit nvJPEG GPU
encoder without full-frame CPU readback. Native process and queue controls qualify
fatal exit before storage destruction, orderly teardown after completion, queued
cancellation and server-owned shutdown despite retained Task handles. They do not
establish hardware execution. View selects CUDA 13.3.1, CUDART 13.3.29 and
nvJPEG 13.2.1.68 with verified image, signed package and complete header inputs.
The loaded NVIDIA 610.57.04 driver advertises CUDA 13.3 and satisfies the image's
vendor admission constraint. Matching cudarc driver bindings, regenerated nvJPEG
bindings and contract-only compilation pass. Each service keeps its own supported
CUDA user-space bundle; the shared host driver must satisfy each selected image.
View owns the exception to the newer upstream CUDA 13.4 profile. Review it by
2026-11-06; upgrading requires a compatible host driver and complete image and GPU
qualification. The selected 13.3 profile requires no desktop interruption.
Local hardware and lifecycle qualification passes as described in
[Current Status](#current-status). Installed consumers, cross-context Task delivery
and pod termination-grace acceptance remain open.
Most module, lookup and consumer mechanisms are implemented. The remaining phase
gates require final source coverage and composition checks followed by installation
qualification; an open owner row does not by itself establish missing code.

View's startup lease observer preserves live leases, current cancellation and
provider observation. Its trusted claim-handoff read distinguishes deletion from
server reassignment before payload decoding. TaskRuntime passes all 78 ordinary
native cases and four compile-fail doctests. Its maintained Rust/Python storage
exchange passes with current Cargo-selected runtime and Gateway binaries over a
fresh owned database. Nine fixture controls qualify container-ID cleanup and
retained setup and cleanup diagnostics; independent review accepts the batch.
The maintained async command helper includes launch
admission in the caller deadline, prevents late payload dispatch after cancellation
and bounds killed-group reaping to one second. Support passes 32 library controls,
three dependency-boundary controls and its explicitly selected Node/Python framework
case. Those boundary checks admit Modules' declaration profile and reject its
database runner. Private process fixtures carry their explicit registration root and
cleanup budget through launch and Drop.
Two View recovery controls and both smoke helper controls pass; stalled observer
shutdown reuses its unchanged-source control. Docker warnings stay separate from
container IDs and JSON. Review accepts the recovery read, Docker handling and
dependency boundary. The command context carries its original owner and first
cleanup end through launch, output waiting and a late handoff. Native controls
cover cleared or replaced owners, dropped receivers, expired caller allowance and
a thirty-millisecond owner grace during delayed admission. They also cover fixture
failure under an inherited dispatcher environment. The caller-first correction
latches the first effective minimum while the original owner is still running;
the added late-handoff control, six process controls and seven gate controls pass.
Independent review accepts that correction. View's local GPU restart and recovery
pass; its installed, cross-context and pod termination-grace gates are recorded in
[Current Status](#current-status). UAV's complete owner and
direct-consumer source pass covers fifty-four paths and passes the grouped compiler
and isolated contract checks. The three missing nested camelCase declarations are
repaired. The final Rust-produced schema capture and normal comparison pass, and
all twenty-five Python protocol cases pass against that capture. They exercise
current, retired and mixed fields through actual HTTP and decoding paths before
dispatch. Wider native execution, generated consumers and hardware gates remain open.
Stream's complete source pass includes the canonical recorded-video selection
and snapshot types and their Reason consumers. Replay resource reads now check the
actual Artifact body against the selected Task, source snapshot and terminal product;
the recorded-run usage producer constructs a closed owner metadata value. Video's
seven contract cases and two current native adapter cases pass against its actual
source snapshot and digest. Stream's fourteen selected native controls pass,
including current registration, SQL completion admission and comparison with its
complete producer schema. Both actual SDK parser controls pass against the
maintained runner built with the pinned DeepStream SDK. They establish current
request admission and refusal before socket or graph effects; GPU processing and
installed live/replay consumers remain unqualified.
The current Linux batch passes Frames' selected reads and catalog controls, and
DuckDB, Timeseries and Frames' hosted argument refusal. Map resolves its six owner
failures and passes all nine affected controls, including actual producer capture,
SQL catalog recovery and invalidation, closed summary admission and Chrono schema
agreement. Its route and matrix summaries now belong to the lightweight contract
with checked typed ID/address relationships; the contract-only build and normal
dependency graph pass. Independent review accepts that batch and its prompt repair.
Map's current travel-model selection passes all ten cases, including the five
product paths that use the shared C02 helper.
Document checks prove the authorized Knowledge read and source-byte limit; Kernel
model-read accounting needs separate qualification. Stream passes all three repaired
native controls. Reason's schema correction installs with preserved migration history
and prerequisites; its sampled settlement and all 22 malformed-write refusal cases
pass. All six affected Reason controls pass, including lookup admission and the
findings query's access checks before decoding and pagination. The combined Map,
Stream and Reason batch passes eighteen affected controls. Physical SQL columns keep
their existing names; nested checked documents follow their JSON profile. A freshly
rebuilt Knowledge harness emits configuration byte-for-byte identical to the captured
inputs: nineteen collections, 153 members and 78 judged cases, with the same query task,
chunking and measured runtime profile. The actual FP16 candidate uses a distinct
vector space. Reference loading checks its dtype, Rust report
admission requires the same declared precision, and Helm supplies the selected
dtype explicitly. Twelve pure collector controls, four report controls and three
affected Helm controls pass; independent review accepts the combined source.
The actual FP16 profile and fresh capture preserve the checkpoint, nineteen
collections, 153 members, query task, chunking and 78 judged cases. CUDA reference
comparison passes for all 1,242 document vectors and 78 query vectors; candidate and
reference ranking retain all 150 judgments. Capacity and priority scheduling pass
their actual hardware controls, and typed admission binds all four matching reports
into the candidate runtime bundle. Knowledge's six-policy controls pass the corrected
generation DDL and appended member-field migration; migration-zero bytes stay intact.
The same recorded-corpus initial-build control passes with concurrent lease renewal.
The full production workload builds and rebuilds 1,242 chunks across all nineteen
collections, serves nineteen searches during rebuilding and retrieves every one of
the 150 judgments across 78 cases. A second Store connection verifies the persisted
evaluation. Full SQL search timing is separate from the embedding latency gate.
The current four-vector fixture contains complete output from the CUDA producer;
served-vector comparison and the shared client's priority case pass against it.
Reference configuration selects the original admitted FP16 runtime bundle with the
measured four-CPU/12 GiB serving-process limits. Six owning Helm cases pass. The
Bioma render verifies the profile and full public bundle after its OCI predicate
admits digest-only images. The ordinary smoke binary compiles with its declared
optional persistence dependency. The full normal Helm command passes reference
Helm/Kustomize agreement, the packaged pilot template, module ordering and credential
confinement, object-store checks and both SUMO and fork profile validation.
SUMO's current Recording configuration preserves its values, scopes and quotas.
The deployment receiver resolves module binding keys through its typed Helm
selector adapter instead of decoding them as enum JSON values. Both owning
controls pass with the original generated plans, including active-lane requirements,
disabled consumers, conjunctions and unsupported-key refusal. Independent review
accepts this deployment source batch. These checks qualify source admission and
rendered configuration; installed lifecycle and GPU allocation remain open.
Both gateway configurations declare revision 4 for all eighteen hosted domain
servers; the external Rerun bridge keeps its explicit adapter profile. Stream's
registration control rejects the retired revision member and verifies every
hosted declaration. The reference gateway and UAV rollout checksums cover their
current delivered bytes.
The cluster and embedding process stay stopped between these local batches.
Knowledge's owner/configuration cut
and Reason's full owner/private-runner cut pass their current grouped compiler check;
the independent contract consumer also compiles. Reason's typed request producer,
captured fixture and current results receiver pass after rebuilding the embedded
consumers. Independent review accepts the repair, including the separate request and
Artifact rejection controls. Knowledge's wider native pipeline and remaining producer
captures stay open. Recording's manifest-publication intent reached a
passing API compiler and schema-only isolation checkpoint. Its current recovery
batch adds transactional source-selection fencing and persisted properties
preparation. The complete Recording and gateway/deployment compiler checkpoint
passes. Native retained-seal recovery passes on the normal test stack, including both
source-race commit orders, restart and policy controls, and the permitted post-seal
Derived lifecycle. The current general-query control and all five pure metadata
controls pass. Its manual places the compliance declaration directly under the
required heading, and the actual startup parser control passes without Store, cache
or Redap setup. Recording's remaining 21 native controls pass, including complete
producer captures and ordinary comparisons with capture disabled. Its ID schema
comparison uses the shared naming-profile builders and preserves full equality.
Properties selection is fenced before file effects and rechecked
before publication; manifest staging syncs a temporary file before promotion.
Independent review accepts the native Helm identity adapter and single-document
offline admission after their owning chart, Helm and rejection controls pass.
Recording's manifest fixture is generated by its maintained producer and passes the
ordinary complete-body and publication-descriptor comparison. Twenty-six current
native controls cover RRD source preservation, Reader cache authority and retained
live parts, Video snapshot admission, Hub ingest and diagnostics, and Redap's URI
and manifest profile. The H.264 restart control compares extracted source units and
both normal and discontinuous MP4 payloads with the fixed fixture. Hub diagnostics
declare v2 for their camelCase JSON while the ingest protobuf and route keep v1.
Reader snapshots are native values; the checked Video contract owns serialization.
Independent review accepts these repairs. Recording's Store-backed framed-RRD
controls pass with actual SDK bytes, bootstrap, current-head reconnect, durable
updates and layer rollover. SQL visibility refusal precedes decoding at admission
and layer transitions. The maintained Redap archive/catalog wire check passes through
the production scoped gRPC service with actual SDK bytes, selected catalog grants,
excluded keys and expired-token refusal. All three playback controls and four
upstream read-profile controls pass. Installed ingest and grants, mandatory GPU
decode and headed playback stay open.
Computers' public vocabulary, typed terminal profile and decoder source batch compiles
with its BFF callers. Its isolated contract all-target check passes after making the
Artifact service's runtime dependency explicit. Twenty-six owning contract cases,
twelve compile-fail doctests, five Console saved-file cases and nine native secret
cases pass. The hosted file-transfer control proves refusal before domain effects;
provider, generated-consumer and installed qualification stays open. Agents/Workspace's
thirty-nine-path source batch compiles with its gateway, BFF and direct Bioma browser
callers. It covers operator-control and manifest producers, generated browser
admission and selected Task identity in Workspace projections. Nine selected native
controls and five Console admission cases pass. Maintained generation supplies six
current Agent and Workspace outputs. The actual Manager/UAV Helm wiring fixture
passes its typed digest capture and normal comparison while preserving the native
AgentContent hash profile. It qualifies chart emission and owner hash behavior;
production pins, wider owner checks and installed consumers remain open. The remaining
gateway, deployment and offline producer/receiver pass compiles. The compiler receipt,
Map browser resources and query digests, component receipts, Helm evidence admission
and independent simulator fixture use their current owner types. The simulator's ten
receiver cases pass, alongside the native Candidate receipt and Map producer/HTTP
receiver controls. Component receipts pass after their actual synthetic-lock capture
and normal comparison. Helm's typed archive-digest correction passes its enrichment
and ten preflight controls. Time, UAV and Knowledge clear their current
receiver failures. Recording's 27-case Rust timestamp capture agrees with its generated
schema and Console receiver, including omission and null. The SDK's checked timestamp
preserves all twelve actual Artifact producer cases through four decoder paths;
104 affected SDK/template controls pass. Artifact's full public timestamp schema pass
passes its current compiler and actual producer checks. Console's schema validator
admits all 24 captured receipt and metadata roots, including Chrono's extreme years,
and the sharing input controls preserve omitted and null timestamps. These schemas
qualify lexical structure; Rust and the SDK separately admit calendars and UTC
ranges. The Console upload reader's calendar and UTC-range repair passes all thirteen
queue controls. Malformed open-session timestamps refuse before file hashing, part
sends or completion while preserving saved progress; malformed receipts refuse
callback and cache admission. Its shared browser scalar preserves Recording's
27-case chronology controls. Independent review accepts this focused repair.
Stream and Reason
responses adopt the shared lightweight Task status vocabulary without changing its
valid wire values. The embedding collector's current input model checks known fields
and selected member/vector relationships before model loading, with a 512 MiB byte
limit shared by its Rust writer. Both actual Rust-produced capture fixtures pass
normal comparison. Six Python receiver cases pass, including all thirty-four
foundation URI cases through four decoding paths, original-byte retention and refusal
before model or CUDA effects. Those receiver checks establish admission; the
measured NVIDIA profile is qualified separately above. The maintained client
generator and comparison pass for the captured owner inputs. Console and Workspace
type checks and their affected consumer tests pass after updating the direct
Artifact callers; both application builds pass. The shared Workbench's maintained
builder produces the current reader, and all six page controls pass. The final
consumer comparison covers all 18 generated bundles and 36 files. The maintained
builders refresh five packaged HTML assets; eight builds, six App type checks and
Console lint pass. Optimization's maintained capture agrees with its current producer
in all six contract-schema controls. View's actual producer capture and three App
controls pass after its owner digest repair. Remaining owner-native and installed
qualification stays open for these server cuts.

View hashes composition content through an ordered borrowed struct of owner types.
Exhaustive wire destructuring requires every new field to enter the preimage or be
explicitly excluded; only creation time and the digest itself are excluded. The
former JSON-map intermediate changed ordering with `serde_json/preserve_order` and
promoted typed floating-point values. Both feature profiles now pass the same 42
permanent controls without ignored cases. Request and authority digests and the
composition ID are unchanged. The current v2 content digest and captured compositions
belong to the coordinated fresh-state cut; old rows and bytes stay protected until
replacement qualification. This repair does not establish GPU execution.

Computers' normal Linux selection passes 144 cases across execution, storage, host,
runtime and MCP packages. That selection ignores seventeen container, provider and
privileged cases. One of them now passes against the staged template from
`9ccd95fae`: the native maintenance fixture performs upgrade, initial-create
recovery and rollback while retaining its 8 GiB home. It admits candidate image
`sha256:0a17f61a5713fe05a006517b598d6df53c4c1b4a0c7b69f20409b1501f08d105`
in a private two-template configuration and preserves the source image. Owned
containers and the fixture home are removed after the run; baseline stores and
retained homes are unchanged. The other sixteen native cases and installed grant,
pairing and recovery checks remain open. Production configuration still selects
one template and has no maintenance transition.
The conformance repairs cover direct-hosted callers, observed authorization failures,
effective Cargo features, package ownership, exact framework selections and cleanup
under cancellation. Naming repairs cover dictionary shape and key agreement, annotated
local references, unsupported dynamic references and preservation of the generator's
single transform pass. Run focused incremental `cargo check` during implementation
and repair compiler errors immediately. Compiler checks precede independent review,
which assesses logic and contracts. Run broader suites at completed batch checkpoints
and deployment checks when source qualification permits rollout. Final owner discovery,
format materialization and installed acceptance
remain required.

| Former Foundations phase | Disposition in this plan |
|---|---|
| 0 — Plan retirement | Completed retirements stay retired; this consolidation transfers the two remaining overlapping plans without claiming their work complete |
| 1–2 — Identifier cut and installation targets | Accepted baseline; preserve current identifiers and use installation-owned inputs in phase 10 |
| 3 — Contract corrections and types | All 68 owner rows transfer below; phases 0 and 3–6 implement them and phase 10 qualifies their consumers |
| 4 — Unified audit | Accepted implementation; preserve the owning audit contract and changed-consumer acceptance, including ordinary export and the stated Object Lock limit |
| 5 — Store simplification | Accepted outbox removal, changefeeds/LIVE recovery, reference cleanup and payload separation; preserve measurements and qualify changed readers, restart and transaction behavior |
| 6–8 — Knowledge extension, adoption and service | Accepted selective-source baseline; phase 7 owns proposed identity changes, phase 10 closes cold startup and affected source/consumer acceptance |
| 9 — Reason findings | Accepted publication/grant checkpoints; F61 preserves broader result typing and installed recovery, with Reason qualified separately |

### Identifier Hard Cut

The `veoveo.ai` identifier cut and installation-target inputs are implemented.
Preserve their checks and current consumers. New naming changes belong to phase 8;
they do not reopen the completed identifier migration.

### Unified Audit Log

Preserve one record per logical action, authorization before reads, committed audit
writes, partition visibility, sealing, export and current-format restart behavior.
The owning [audit design](AUDIT.md) and component designs define the contract.
The bundled RustFS store does not qualify compliance-mode Object Lock. Ordinary
export is required; retain the accepted limitation in
[regulated readiness](REGULATED_READINESS.md), gap G9. No new historical-data reader
or conversion is required by this plan.

### Knowledge Service

Preserve approved-source indexing, policy-filtered catalog and search, subscriptions,
generation restart and GPU embedding. Knowledge-source adoption remains selective;
this plan does not require every server to publish collections. The NVIDIA runtime
and 0.6B model selection have qualified checkpoints. The existing accepted priority
limitation uses the client's bulk-concurrency bound; preserve the owning runtime's
declared behavior rather than claiming unqualified scheduler priority.

### Modular Types And Server Contracts

[CE-13](CONTRACT_EVOLUTION.md#ce-13-modular-types-and-server-owned-contracts)
continues to govern domain ownership. `platform/types` owns protocol-independent
mechanics and extension traits. Each server library owns its scope vocabulary, IDs,
resource variants and DTOs through an isolated `contract` feature. MCP traits consume
those types without enumerating domains. Contract-only consumers exclude MCP,
database, provider, GPU and asynchronous-runtime dependencies. A separate contract
crate needs a concrete dependency or release reason.

Builders retain domain IDs and validate parent, provenance, authority and field
relationships before exposing a usable value. Authorization is checked at use time.
SQL applies tenant, owner, Work Context, labels and parent selection before decoding,
ranking and limits. Domain runtime code owns those queries and driver conversion.
The shared macro and module work must preserve these requirements.

## Remaining Work And Completion Boundary

1. Complete the required rows in the Foundations transfer register through phases
   0, 3, 4, 5 and 6; phase 10 qualifies their remaining installed consumers.
2. Close remaining implementation and consumer gaps under the accepted phases
   0–9. Preserve qualified native results within their recorded scope; final-image,
   installed recovery and hardware requirements still need their own results.
3. Run phase 10 against the final selected images, preserving the accepted audit,
   Knowledge, recovery and hardware requirements.
4. Audit each required phase, F-row and required H-row. Record the implementation
   revision, affected checks and any unresolved failure in the current status.
   A native pass alone never closes an installed or hardware requirement.

Deferred proposals in the follow-up register do not block this completion boundary.
They stay visible and require a scope decision before implementation. Required work
cannot be reclassified as deferred merely to declare completion. Real Media
generation remains explicitly unqualified under the existing user restriction.

## Review Decisions

| ID | Recommendation | Reason |
|---|---|---|
| R1 | One plan, with required transfer rows and explicitly deferred proposals | Avoid duplicate implementation passes and preserve unfinished obligations |
| R2 | Finish typed IDs, owned Task payloads and fail-closed admission in the main phases | These are existing Foundations requirements, not post-phase-8 extras; classify genuinely opaque payloads separately |
| R3 | Fresh reference state for storage cuts; no historical backfills | The installation has no data to preserve; current-format recovery and transaction rollback remain required |
| R4 | Put generic cursor mechanics below MCP in `platform/types` | Domain contract features must remain independently consumable |
| R5 | Phase 2 proves its ports; phase 3 closes the full dependency graph | Store's Computers dependency is removed in phase 3, so it cannot be a phase 2 exit gate |
| R6 | Ship C33 enforcement with the phase 8 naming change | A new contract rule needs its conformance check in the same cut |
| R7 | Keep Apple embedding qualification as a separate proposed profile | NVIDIA Foundations completion must not depend on unavailable Mac hardware; no CPU fallback is permitted |
| R8 | Keep security-policy, offline-tooling and governance follow-ups visible without making them current-goal blockers | They were not part of Foundations; future CI gates still require the decision described in CONTINUOUS_INTEGRATION.md |

Review covers the breadth of mandatory derives (D14), kernel/module classification
and migration-job cost (D8–D10), the naming scope (D1–D6), and embedding space reuse
(D13). Each phase records its accepted scope and outstanding qualification. D13's
source architecture review and local NVIDIA profile qualification are recorded in
Phase 7; installed profile and simultaneous-workload qualification remain open.
Existing owning designs and AGENTS.md govern behavior
until their qualified replacements land.

## Working Rules

- Implement one concern across its owners, consumers, fixtures and documents before
  the validation batch. Keep one Cargo build graph, aggregate independent failures,
  and reuse unaffected checks. A helper or individual server is not automatically a
  new build, publication or deployment checkpoint.
- Inspect branch and worktree ownership before editing. Use coherent commits after
  affected native checks pass. During this refactor, prefer few active worktrees and
  integrate completed batches into `main` promptly. Use another worktree when its
  isolation or coordination benefit warrants the cost; this is a working preference.
  Preserve shared build caches and avoid duplicate Cargo targets. Coordinate the
  naming cut with other writers rather than assuming another agent is idle.
- Run native framework commands. `cargo xtask smoke` dispatches existing harnesses;
  it does not reimplement domain assertions. No CI or committed test-report system
  is introduced by this plan. Documentation changes run `cargo xtask enforce docs`.
- Stop the reference cluster during editing, compilation and image builds. Start
  only the workloads needed for installed acceptance, with Knowledge and Embedding
  enabled; qualify Reason separately. GPU workloads request their devices and fail
  closed. Visual acceptance requires headed hardware WebGPU or WebGL.
- Check disk before large builds. Clean owned experiments and superseded outputs;
  preserve useful Cargo/BuildKit caches and all active, rollback and dependency OCI
  references. This resource discipline applies to this plan, not a new repository rule.
- The reference installation is disposable. Drain writers and reset incompatible
  state together; add no aliases, historical conversions or dual readers. Current
  transaction rollback, restart recovery and unresolved-operation fencing remain
  required. Prepare selected images before application reconciliation after a reset.
- Publish composed qualified batches through the
  [reference runbook](../examples/bioma/README.md) and
  [development iteration guide](DEVELOPMENT_ITERATION.md). Keep Docker fixtures
  separate from image assembly and scanning. Match installed checks to actual digests.
- Improve inexpressive contracts within accepted scope, explain material tradeoffs,
  and update their designs and CE register. Preserve authorization and GPU guarantees.
  Changes to shared host implementation require the existing user-directed review.
- Record a required deferral at its exact code path and in Deferred Work; continue
  independent work. A deferral is not a passing gate. Do not run a real Media
  generation without a change to the existing user restriction.

## Phases

| Phase | Change | Depends on | Deployment |
|---|---|---|---|
| 0 | [Core macros and shared building blocks](#phase-0-core-macros-and-shared-building-blocks) | — | Ordinary upgrade; no wire change |
| 1 | [Module contract and lanes](#phase-1-module-contract-and-lanes) | 0 | Native bootstrap qualification; module lanes start empty |
| 2 | [Kernel extension points](#phase-2-kernel-extension-points) | 1 | Ports qualify natively; catalog ownership activates with the fresh installation in phase 8 |
| 3 | [Module ownership of persistence and queries](#phase-3-module-ownership-of-persistence-and-queries) | 1, 2 | Fresh fixtures; storage cut deployed with phase 8 |
| 4 | [Database field types](#phase-4-database-field-types) | 3 | Fresh fixtures; no historical-data upgrade |
| 5 | [Inbound strictness](#phase-5-inbound-strictness) | 0 | Qualify all affected senders and receivers together |
| 6 | [Generated cross-language types](#phase-6-generated-cross-language-types) | 0 | Generated clients accompany their contract changes |
| 7 | [Embedding profiles and identity](#phase-7-embedding-profiles-and-identity) | Reviewed D13 | NVIDIA identity qualification; Metal deferred |
| 8 | [Installation cut](#phase-8-installation-cut): lane baseline and JSON naming | 0–7, with Metal excluded | One coordinated fresh installation; C33 ships here |
| 9 | [Conformance and repository enforcement](#phase-9-conformance-and-repository-enforcement) | 0; complete before phase 10 | Required hardening transfers; catalog groundwork may precede phase 8 |
| 10 | [Installed acceptance and closeout](#phase-10-installed-acceptance-and-closeout) | 8, 9 | Required owner and composed acceptance |
| — | [Follow-ups](#follow-ups) | Per item | Explicitly deferred proposals |

Phase 0 establishes mechanics before further owner adoption. Phases 5 and 6 can run
alongside module work once those interfaces settle. Land coherent source changes
after their native gates; do not deploy incompatible intermediate storage or wire
formats. Phases 3–8 qualify against fresh current-format fixtures and compose into
the installation cut. Deploy an earlier compatible batch only when its installed
check resolves a concrete risk. The naming cut updates both sides together.

## Decisions

These are implementation recommendations for review. They do not amend the accepted
CE register or owning designs by appearing in this plan.

| ID | Decision | Recommendation and consequence |
|---|---|---|
| D1 | JWT claim sets and the identity family they carry: `Principal`, `InvocationAuthority`, `WorkContextOutputPolicy`, `WorkContextGrant`, `InvocationProvenance`, `GatewayRequestContext`, `AccessTokenSubject`, `GatewayInternalIdentity` and the internal claim structs | Keep snake_case under the JWT convention. These types are the claim sets and also the snapshots stored in task owners, computers grants and refresh families, so authorization SQL and the Python SDK claim models stay unchanged |
| D1a | Wire documents that embed identity types | Gateway work-context configuration gets a camelCase `OutputPolicyConfig` that converts into `WorkContextOutputPolicy`. `ArtifactProvenanceWire` replaces its embedded `InvocationProvenance` with its own camelCase fields and tag `invocationMode` |
| D2 | Declared identifiers | Tool names, prompt names, prompt argument names, URI template variables and completion argument names stay snake_case. All 188 multi-word template variables and every tool name already comply |
| D2a | Naming roles | CamelCase applies to DTO field names; dictionary keys follow their admitted key vocabulary or identity profile. Scope and Task names, resource/extension identities and versioned format tags keep their owner or protocol grammar. Ordinary controlled enum values use snake_case. Source-emitted schema classification must distinguish these roles without a server registry or a subtree-wide exemption |
| D3 | Storage and phase 8 transition | Drain writers and bootstrap empty SurrealDB, object storage and Map DuckDB. Qualify phases 3–8 with fresh current-format fixtures and deploy them together. Reject old formats with an actionable diagnostic; add no backfills, dual readers or historical-data qualification |
| D4 | Audit | Freeze the audit record, block, checkpoint and export formats. The audit contract owns copies of the knowledge enums it embeds. Archives exported before the cut verify with the same format |
| D5 | Versioned formats | Every versioned format whose keys or values change bumps its tag, and legacy tags move to the `veoveo.ai/<name>/v<N>` form as they bump. The `ai.veoveo/knowledge-source` identifier stays, because the extension is unreleased and the cut is coordinated |
| D6 | Upstream-shaped formats | Exempt: Flint chart tool arguments (`chart_spec`, `theme_spec`), the Rerun map-provider spelling at the Rerun boundary, WaveSpeed payloads, cuOpt API names, Valhalla, ntpd-rs and OGC 3D Tiles. Veoveo configuration values that select a Rerun provider become snake_case (`open_street_map`) and convert at the boundary |
| D7 | Branch and commits | Follow Working Rules: one composed implementation and validation batch per concern, coherent commits, and explicit coordination with other writers before the naming cut. A new worktree is not required for each module |
| D8 | Kernel and modules | The kernel is a set of required modules with a dependency order: store base, identity, policy and gateway, artifacts, tasks, audit and Knowledge. Computers, Agents, Workspace and Recordings are platform modules. Map, Time, UAV, Frames and Media are server modules. See [Kernel And Modules](#kernel-and-modules) |
| D9 | Extension pattern | Ports and adapters with injection at the composition root, for capabilities the kernel hosts but does not own, such as module HTTP surfaces in the gateway. The kernel library defines each extension point as a trait or a builder registration, plus a versioned SQL function when a check runs in the database. A module crate provides the adapter. Each binary's builder binds adapters for the modules the installation enables. An unbound port fails closed. No dependency-injection framework |
| D10 | Migration execution | Every module holds its own migration lane and declares an execution image plus command. Composition images may execute several owners' distinct lanes. Database readiness precedes ordered lane completion and control-plane publication on fresh install and upgrade; migration credentials mount only into Jobs. The runner enforces logical table ownership before applying a lane |
| D11 | Knowledge | Knowledge is a kernel module. Its extension point is the `ai.veoveo/knowledge-source` protocol: a server becomes a knowledge source by declaring collections, with no Rust adapter, and Knowledge consumes sources through the gateway as an MCP client. Collection approvals are kernel policy. Future source capabilities extend the protocol contract |
| D12 | Embedding device profiles | Keep the qualified NVIDIA cluster profile required. Preserve Metal as deferred proposal X1 for separate Apple hardware qualification. No CPU profile or automatic fallback. Do not advertise Metal support or change GPU rules before it is qualified |
| D13 | Embedding space identity | Model, checkpoint revision, dimension, pooling, normalization, numeric precision and maximum input tokens. The runtime image, vLLM version and device leave the identity. Keep runtime image, version and device in execution provenance. Space reuse requires reference-vector and retrieval qualification on the declared model/configuration; a failed qualification needs an explicitly distinct space or rejection, not a fabricated setting change |
| D14 | Macros | Veoveo defines five core macros: the `resource_address` and `id` attributes, the `Vocabulary` derive and `embedded_document!` in `platform/macros`, and `server_docs!` in `mcp/contract`. Their generated code calls ordinary traits in `platform/types`. Checked models and opaque cursors are generic types, and error types use `thiserror`. Any other macro is a declared exception. No new third-party crate: strum, nutype, parse-display and serde_with were evaluated in [phase 0](#evaluated-crates) |
| D15 | View GPU JPEG ownership | Keep Vulkan/CUDA interop and generated nvJPEG bindings with View's serial renderer. Match device UUIDs, preserve stored sRGB bytes, and warm the explicit GPU encoder before readiness. Runtime-only dependencies stay outside the contract feature. Share a completion deadline across submitted stages; release memory only after completion. A native completion failure terminates the existing View process without destructors or a core dump, preserving interrupted Task recovery. Qualify the selected runtime on its required host driver before claiming support |

## Kernel And Modules

### Classification

The composed owner declarations contain 166 static tables: 63 in seven kernel
modules and 103 in twelve optional modules. The runner adds three bookkeeping
tables. Generation-specific Knowledge chunk tables are created at runtime and are
outside these static counts.

| Layer | Module | Tables |
|---|---|---|
| Kernel | Store base | `changefeed_checkpoint` |
| Kernel | Identity | `tenant`, `enterprise`, `principal`, `principal_group`, `work_context`, `oauth_client` |
| Kernel | Policy and gateway | `policy_revision`, `profile`, `profile_server`, `gateway_*` (11), `mcp_server` |
| Kernel | Artifacts | `artifact_*` (12), `share_link` |
| Kernel | Tasks | `task`, `task_input`, `task_idempotency`, `task_produced_artifact`, `task_used_artifact`, `provider_job`, `provider_event`, `domain_usage` |
| Kernel | Audit | `audit_*` (12) |
| Kernel | Knowledge | `knowledge_*` (8) |
| Platform module | Computers | `computer` and `computer_*` (28) |
| Platform module | Agents | `agent` and `agent_*` (8), `managed_agent*` (3), `wake` |
| Platform module | Workspace | `workspace_*` (9) |
| Platform module | Recordings | `recording_*` (9) |
| Server module | Map | `map_*` (23) |
| Server module | Time | `time_*` (8) |
| Server module | UAV | `uav_*` (5) |
| Server module | Frames | `frame_world`, `frame_world_revision`, and `coordinate_operation` |
| Server module | Media | `media_task`, `media_task_context`, `media_usage` |
| Server module | Optimization | `optimization_task` |
| Server module | Reason | `reason_analysis` |
| Server module | Stream | `stream_run` |

Phase 1 qualifies execution dependencies. Tasks and Artifacts refer to each other
through `task.result_artifact` and `artifact_occurrence.task`; Tasks and Gateway have
similar typed links. A native SurrealDB 3.3 probe confirms that a field can declare a
record type before its target table exists. Phase 3 preserves these typed links and
distinguishes their schema graph from the migration execution graph.

The runner may admit forward `record<>` types in owned field definitions between
selected kernel owners, including nested unions and containers. This permission
applies only to plain field types without `REFERENCE`. Casts, typed variables,
function signatures, record literals and field expressions keep their existing
admission rules. Optional owners still require declared dependencies. Reads, writes,
analyzers and function calls do not gain permission from a field's type. Qualify
reciprocal kernel fields and rejection of executable or optional-owner access before
accepting the change. The migration execution graph must remain acyclic; the table's
top-to-bottom execution order stays subject to full-lane qualification.
Audit's `target_ref` includes Computers; Phase 3 must preserve audit semantics while
resolving this kernel reference to an optional module.
Optional modules depend only on kernel modules and on modules they declare.

The embedding runtime is required because Knowledge search needs it. NVIDIA is the
qualified profile; D12 records the separate Apple proposal.

### Ownership Status

| Concern | Current state |
|---|---|
| Persistence ownership | Workspace, Agents, Map and Recording repositories and queries live in their owning modules. Their native suites and fresh owner-lane composition have qualified source checkpoints. Store supplies shared connections and kernel services |
| Optional-module dependencies | The reusable gateway excludes optional owner runtimes from its normal/build graph. Store and Audit use the registered Audit target codec; Computers supplies its implementation. Audit's Computers dependency is test-only. Knowledge runtime consumes Policy's resolver port; server composition selects the separately gated Agent adapter from admitted plan lanes. Native checks and isolated dependency profiles pass |
| Schema ownership | Owners declare separate current-schema lanes through `ModuleSetup`; Store owns kernel lanes. Optimization and UAV use owned lookup tables. The installation must activate the composed lanes in phase 8 |
| Runtime kernel access | Qualified owner APIs cover Computers, UAV, Reason, Stream, Frames, Workspace, Agents, Artifact and Map projection recovery. Task, Media and Knowledge authority adapters also pass native qualification. Source review reconciles 495 optional-owner query assets, variable targets and record links. The user-approved host tracing correction passes its leak regression and hosting controls |

Knowledge's dependencies on the gateway, store and `mcp/contract` are kernel-to-kernel
under D11.
Its runtime dependency on Agent persistence violates that separation. The HTTP
fixtures which exercise managed clients must install the Agents lane, but that
prerequisite does not qualify static clients on a kernel-only installation.

### Module Contract

A module declares itself through a checked `ModuleSetup`, the counterpart of
`McpServerSetup`. The declaration types live in a new dependency-free crate,
`platform/modules` (`veoveo-modules`), so a module's `schema` feature exposes its
catalog without pulling in a runtime.

| Field | Meaning |
|---|---|
| `name` | Module identity, for example `map` |
| `layer` | `kernel` (required, ordered) or `optional` |
| `ownership` | Typed table, function and exact analyzer claims: prefix families plus explicit names such as Frames `coordinate_operation` and Agents `wake`. The registry rejects overlapping claims. Kernel API functions have an explicit owning lane under `fn::kernel::*` |
| `lane` | Ordered, checksummed migrations included with `include_str!` from the module's `migrations/` folder |
| `execution` | Declared image and command for the lane. An existing composition image may execute several owners' distinct lanes; separate images or processes require an operational reason |
| `requires` | Kernel modules and versions it requires; for an optional module, also the optional modules it declares |
| `extensions` | Extension points the module binds |

Each module keeps `migrations/` and `queries/` side by side. Kernel modules may share
the `platform/store` crate, each in its own folder with its own lane, and split into
crates when a concrete need appears. The shared runner moves from
`platform/store/src/migrations` into `veoveo-modules` behind a `runner` feature,
generalizing the existing downstream lane into named lanes.

### Rules For Modules

1. A module defines, alters and removes only its own tables, functions and analyzers. A kernel
   module references only earlier kernel modules. No kernel module references an
   optional module. An optional module references another optional module only
   through a declared dependency.
2. An optional module reads and writes kernel data through kernel Rust APIs: task
   runtime, artifact client, audit writer and identity. When its query needs a kernel
   check inside the database, it calls a versioned `fn::kernel::*` function owned by
   a kernel lane.
3. A module links to kernel rows with `record<…>` fields and `REFERENCE … ON DELETE`,
   so the database owns cleanup.
4. Kernel extension points replace module columns on kernel tables. A server that
   looks up tasks by its own keys writes rows in its own table, linked to the task and
   written in the task's transaction.
5. All modules share one database, because kernel and module rows commit in one
   transaction and link by record. Ownership is logical: declared object claims plus lane; it is not tenant isolation.

The runner enforces rule 1. Before applying a lane it parses every statement and
rejects any `DEFINE`, `ALTER` or `REMOVE` outside the module's declared table and
function/analyzer claims, including nested statements. Unsupported statement classes fail
before execution. Database EDITOR and system-user privileges do not provide per-module
table isolation. The SDK does not expose the required AST. The private runner adapter pins upstream
`surrealdb-syn` and `surrealdb-sql` together at 3.3.0 and applies a fail-closed supported
profile. The [module design](../platform/modules/DESIGN.md#sql-admission) records bounds,
unsupported constructs and upstream internal-API replacement conditions. All selected
bodies pass admission before runner bookkeeping or any body executes.

The module foundation adds exact analyzer ownership because production SQL defines
`platform_search` for shared kernel search and `knowledge_text` for Knowledge. Store
claims the former and Knowledge the latter. Analyzer callbacks remain unsupported until
their effects are qualified. Store also owns the reserved lane/header history tables;
owner SQL cannot access them.

Migration versions begin at zero, and initialized empty lanes use a header rather than
a sentinel version. Fixed per-migration minima and completion receipts preserve prior
history when dependency lanes grow. `Satisfied` requires the dependency's compiled lane
to be fully current before new dependent work. The complete catalog distinguishes known
disabled optional histories from unknown lanes; selection includes every kernel and
optional prerequisites. Publication status checks selected completion.

### Extension Pattern

Two extension mechanisms cover the platform.

| Mechanism | Used for | Example |
|---|---|---|
| Protocol | Capabilities other servers contribute through MCP | Knowledge sources declare `ai.veoveo/knowledge-source` collections; Knowledge discovers and reads them through the gateway |
| Port and adapter | Capabilities the kernel hosts in its own process but an optional module owns | The gateway mounts Computers, Speech, Recordings, Agents and Workspace HTTP surfaces through a route registration API on its builder, in the style of the hosting builder |

The composition root may depend on optional modules; reusable kernel libraries may
not. Cargo metadata gates operate on packages, so put composition dependencies in a
separate package or an independently qualified feature graph. Do not claim that
`cargo tree` distinguishes library and binary dependencies inside one package.

A port is a trait or a builder registration in the kernel library, plus a versioned
`fn::kernel::*` SQL function when a check runs in a query. The module's crate provides
the adapter, and the binary's builder binds it when the installation enables the
module. An unbound port refuses the capability, and the gateway reports bound and
unbound ports at startup and in server health.

### Module Migrations

| Step | Behaviour |
|---|---|
| Build | Each owner declares the image and command executing its lane through the shared runner. An existing composition image may execute several distinct lanes |
| Install | Qualify database readiness, ordered kernel and enabled optional lane completion, then control-plane publication on fresh install and upgrade. Hook weights alone do not establish readiness |
| Credentials | A migration credential is mounted only into migration Jobs. Runtime pods keep database-scoped data credentials |
| Checks | Each Job verifies `requires` and ownership, then applies pending migrations with the existing per-migration transaction and checksum history |
| Lifecycle | A disabled optional module's lane never runs. Enabling it later applies the lane from version zero. Retiring a module is a final migration in its own lane |

## AGENTS.md Changes

`AGENTS.md` stays unchanged while the plan is iterated. Each change below lands in
the commit of the phase that makes it true.

| Phase | Section | Change |
|---|---|---|
| 0 | Macros (new section) and Strong Types | Core macro catalog, the exception rule, and the shared building blocks |
| 1 | Module Boundaries | Kernel and module structure, `ModuleSetup`, ownership rules |
| 1 | Repository Map | `platform/modules` and the module design document; [CODEMAP.md](CODEMAP.md) rows for each moved component |
| 3 | Database First | SurrealQL location and parameter rules |
| 4 | Database First | Declared fields, opaque payloads, literal types, record binding |
| 5 | Strong Types | Inbound strictness |
| 6 | Strong Types | Generated cross-language types |
| 7 | GPU Execution Is Mandatory | Qualified embedding identity; Metal rules only if X1 is separately accepted and qualified |
| 8 | Naming | A JSON Naming section |
| 8 | MCP Server Contract | C33 and C02 `resultUri` through [mcp/contract/DESIGN.md](../mcp/contract/DESIGN.md) |

The implementing phase updates only rules it has made true. Keep protocol-independent
mechanics below MCP, retain owner-defined validation, and name any exception with its
reason. Strict inputs apply to controlled shapes; opaque provider maps and negotiated
extension metadata retain their declared extensibility. External formats keep their
own spelling. No proposed rule is installed merely by consolidating the plans.

Phase 8 adds C33 with its conformance check, updates C02 to `resultUri`, and advances
the hosted MCP contract from revision 3 to 4. The separate requirement catalog advances
from revision 1 to 2. The controlled declaration models and their generated projections
admit camelCase fields in the same C33 source batch. Registration checks, SDK declarations
and server compliance projections change together under the coordinated drain;
revision 3 inputs receive an upgrade diagnostic. The MCP protocol remains `2026-07-28`. Phase 9
provides the typed catalog and generated projections; its catalog work may land
before phase 8. Record reviewed architecture changes in
[CONTRACT_EVOLUTION.md](CONTRACT_EVOLUTION.md) when their implementation is qualified.

## Phase 0: Core Macros And Shared Building Blocks

The baseline inventory identified 103 `macro_rules!` macros: 85 in production code across 27 crates and
18 in tests. Most re-implement a handful of shapes, each with its own parsing,
formatting, serialization and schema code. Phase 0 replaces them with five core
macros, a few generic types, and plain functions. A macro is an exception that needs a reason, and every crate checks the core
catalog before writing repetitive code. The current tree has four procedural
definitions and two approved declarative definitions, with no other local definitions.

### Inventory

| Shape | Baseline local macros | Count | Replacement |
|---|---|---|---|
| Typed resource addresses | `address` in 7 crates, `address_traits` in 6, `string_schema` in 4, `address_wire` in 3, `wire_traits`, `wire_address`, `single_address`, `result_uri` | 24 | `#[veoveo_types::resource_address(...)]` |
| Identifiers, names, keys and digests | `coordinate_id` in 3 crates; `catalog_id`, `controlled_id`, `domain_id`, `id_type`, `identity` and `name` in 2 each; `typed_id`, `secret_typed_id`, `identifier`, `id`, `deployment_id`, `artifact_uuid_id`, `uuid_id`, `hex_id`, `map_id`, `output_id`, `public_id`, `recording_identity`, `rrd_id`, `text_identity`, `travel_key`, `typed_string` | 31 | `#[veoveo_types::id(...)]` |
| Validated text and bounded numbers | `request_text`, `checked_string`, `counter`, `finite_number`, `non_negative_quantity` | 5 | Hand-written newtypes with `TryFrom`, or `Checked<T>` |
| Closed string vocabularies | `scope_enum`, `vocabulary`, `declare_task_types`, `string_enum` in Store and Time | 5 | `#[derive(Vocabulary)]` |
| Embedded documents | `server_docs`, over the `embedded_document!` proc macro in `mcp/knowledge-extension/macros` | 1 | Kept as core macros |
| Checked models | `checked`, `checked_model` | 2 | `Checked<T>`, a generic type |
| Opaque cursors | `cursor`, `cursor_type` | 2 | `OpaqueCursor<T>`, a generic type |
| Error and early-return helpers | `fail` in 5 servers, `invalid`, `invalid_value` | 7 | Functions and `thiserror` |
| Other repetition | `typed_record_id`, `add_schema`, Console BFF `handler`, `mutation` and `read_root`, legacy bridge `catalog_result`, and View URI-parser dispatch `admit` | 7 | Trait default methods and generic functions |
| Third-party trait delegation | `impl_scoped_redap_service`, which applies one authorize-or-deny policy across the generated Rerun gRPC service trait | 1 | Declared exception |
| Test helpers | `check` in 9 suites, `capture` in 2, `corrupt`, `declared`, `empty`, `make`, `qualify`, `rejects`, `single` | 18 | Generic test functions |

The baseline also recorded 94 `*Wire` mirror structs and 74
`try_from = "…Wire"` types repeating a field list for validation, 52 cursor types
with owner encoding and an estimated 46 handwritten unit errors. The final same-file
and split-definition refresh found 41 handwritten unit errors, all with static
messages and no source. Phase 0 adopts shared mechanics where the admitted and wire
shapes agree; the concrete model and cursor exceptions appear below.

### Core Macros

The four procedural macros live in `platform/macros` (`veoveo-macros`),
re-exported by `veoveo-types`. The catalog also admits `server_docs!`, which must
expand in the calling crate to select documents, and the owner-declared third-party
Redap trait delegation. Generated code calls ordinary traits in
`platform/types`, so each macro stays thin and its behaviour is readable Rust.

| Macro | Kind | Generates |
|---|---|---|
| `resource_address` | Attribute | Owner-declared struct/enum routes delegate parsing and typed construction to ordinary `ResourceRoute` descriptors. Cached, checked, component and enum forms generate standard derives, constructors, accessors and formatting. Public field/tail codecs and owner profiles preserve admission, wire, schema metadata and error mapping. Encoding-aware schema fragments are qualified against typed builders and admitted aliases; codecs may omit fragments for broader structural schemas |
| `id` | Attribute | Text, hex, prefixed and UUID forms generate standard derives, checked conversions and shared `Identity` capabilities. Ordinary owner profiles select admission, generation, wire/schema policy and errors. `parse` admits input; `new` generates a fresh ID. Namespaces, deliberate secret exposure and redaction belong to each owner. Required unrestricted UUID const/storage construction is explicit; database delegation adds no SDK dependency to the foundation. Unusual storage uses the custom form |
| `Vocabulary` | Derive | For an enum of unit variants: one spelling per variant (snake_case by default), `ALL`, `as_str`, `Display`, `FromStr`, serde and the JSON Schema enum from the same spelling, compile-time checks for empty or duplicate spellings, and opt-in hooks `scope` (OAuth token syntax and `ScopeDefinition`), `task_type` (`TaskTypeDefinition`) and `surreal` |
| `embedded_document!` | Function-like | Embeds a document at compile time with its SHA-256; implemented in `platform/macros` |
| `server_docs!` | `macro_rules!` in `mcp/contract` | Embeds a server's `AGENTS.md` and `DESIGN.md` through `embedded_document!` |

### Generic Building Blocks

| Building block | Home | Replaces |
|---|---|---|
| `Checked<T: Check>` | `platform/types` | Repeated checked-model plumbing: deserialize once, run the owner’s relationship checks, then expose immutable access. Serialization and schema describe the admitted shape. No unchecked constructor or mutable dereference bypasses validation |
| `OpaqueCursor<C: CursorCodec>` | `platform/types` | Typed positions and immutable owner codec instances preserve existing envelope, version, length, alias and context profiles. Parse retains admitted wire; stateless owner adapters supply String Serde explicitly |
| Error types | Each crate, with `thiserror` | Unit error structs with hand-written `Display`; helper macros become functions |

### Owner Qualification Constraints

UUID generation and admission are separate profiles. Computers RequestId admits
v4/v7 and its provider admits v4/v7/v8. TaskId accepts UUID aliases and versions while
generating v7. Artifact accepts parser aliases and requires RFC v7. Map owns v5
namespace and stable-key generation. Id adoption preserves each profile and current
schema, including Recording's unconstrained string and Computers' UUID pattern.
OAuthRefreshToken preserves explicit exposure and String serialization, redacts
Debug/Display, and does not zeroize. This concern deliberately corrects rejected-token
errors to redact the secret; constructor, FromStr and Serde failures share that rule.

Address schema patterns require encoding-aware qualification against typed builders;
mechanical adoption preserves snapshots. Checked models preserve owner validation,
immutable admitted values and intentional Wire/domain separation. The generic does
not remove every wire mirror merely because its fields look similar.

Cursor adoption preserves each owner's codec and envelope. Time's hex JSON and
Artifact's prefix-plus-ID forms cannot become base64url during Phase 0. A new cursor
encoding requires an explicitly coordinated Phase 8 cut with affected consumers.

### Evaluated Crates

| Crate | Finding |
|---|---|
| strum 0.27 (already a transitive dependency) | Derives `Display`, `FromStr`, `AsRefStr` and variant lists, but its casing is declared separately from serde's, and it has no compile-time spelling checks or Veoveo trait hooks; `Vocabulary` keeps one spelling for every consumer |
| nutype 0.8.0 | Validated newtypes with serde, but schema support targets schemars 0.8, not the pinned 1.2.2 |
| parse-display | Formats and parses URIs by interpolation and regular expressions, outside the shared URI layer |
| serde_with | `SerializeDisplay` and `DeserializeFromStr` replace two short impls; the derives already cover them |

### Exceptions

A new macro is an exception. It needs a concrete repetition that functions, generics,
traits and builders cannot express, it lands in `platform/macros` with tests and an
entry in that crate's design document, and the commit that adds it names the reason.
A macro outside `platform/macros` is allowed only when it must expand in the calling
crate, like `server_docs!`, or when it delegates a third-party trait, like
`impl_scoped_redap_service`; the owning design names it. Third-party derives and
attributes such as serde, schemars, thiserror, rmcp, `SurrealValue`, clap, tokio and
tracing are dependencies, not Veoveo macros.

### ResourceAddress Concern Qualification

The route derive and complete owner macro migration preserve constructor signatures,
wire acceptance, Serde forms and owner schema profiles. Descriptor selection precedes
field admission. Optimization wrong-root plus malformed-ID inputs therefore report
`InvalidUri` instead of the legacy identifier error; Stream and Reason wrong-sibling
plus malformed-ID inputs report `InvalidResource` instead of `InvalidId`. Matching-route
identifier errors and RPC rejection are unchanged. Focused owner tests qualify this
intentional diagnostic ordering change. Shared route, complete-owner native and
isolated-contract gates pass. Independent codec fixtures qualify canonical and
admitted encoded patterns, nonempty query and tail constraints, reserved characters
and absolute-end rejection with a JSON Schema validator. Compiler expansion review
qualifies the Map and Time pilots. The helper/error concern and complete macro gate are qualified below; installed
acceptance requirements remain open.

### Checked And Cursor Concern Qualification

Owner field-preserving models use `Check` and nominal `Checked` storage. Builders
and decoding share value relationship checks; schema identity, inline policy, defaults
and field profiles stay with the owner. Explicit transformations continue to own
redundant-ID and fixed-limit projections, catalog-grant sorting and retrieval dataset
or evaluation normalization. Scene composition builders normalize declared inputs
before admission; decoding still rejects noncanonical retained records.

Observation keeps its direct Wire adapter because collection agreement requires an
explicit collection. Dictation snapshots retain public mutable progress fields and
share only their private ID/address Check across construction and decoding. Time
release references, Frames and Artifact metadata, domain views and frozen Audit
records keep their intentional Wire/domain separation.

Opaque cursor owners preserve codec-specific envelopes, limits, canonicality and
context. Artifact's Copy prefix representation and Map Knowledge and Finding's
on-demand encodings use ordinary codecs without a new String cache. Session-bound
UAV codecs retain caller context; structured Audit, Task and Store positions do not
become opaque String tokens. The owner pass covers 31 nominal opaque cursors and four
anonymous text boundaries without imposing a common encoding. Shared and complete-owner
native, schema/admission, contextual misuse, immutable-access, contract-only isolation
and filtered runtime-boundary checks pass. Strict scoped lint and document gates qualify
this concern. The helper/error concern and full macro gate are qualified below.

### Helper And Error Concern

Scalar owners use ordinary nominal types and shared local validators. Audit counters
keep canonical decimal String Serde in human and binary formats and their i64 bounds.
DuckDB request text keeps verbatim Unicode, nonblank/NUL admission and its existing
schema descriptions. Map and Optimization keep copied const getters, finite-number
rules and negative-zero profiles. Static unit errors use thiserror without changing
messages, implemented traits or source behavior; contextual errors keep owner logic.

Task error helpers await existing settlement, then each caller explicitly returns.
A tool error settles a Succeeded Task with MCP `isError=true`; only the existing
result-serialization failure uses Failed. Hub record admission, BFF handlers, legacy
catalog metadata, conformance schema ordering and View parser ordering keep their
owner semantics. Generic test helpers replace local declarations. The macro policy
parses the complete tracked source tree against the exact six-entry catalog.
Combined native checks qualify frozen Audit formats, owner contracts and schema
profiles, task error settlement and the affected runtime entrypoints. Independent
contract graphs exclude hosted MCP, database, GPU and server-runtime dependencies.
Scoped strict lint, parser misuse tests and the full tracked-source catalog pass.
The Phase 0 behavioral audit covers typed address and identity admission, independent
extensions, wire stability, discovery agreement and every declared macro. Explicit
model and cursor exclusions are the owner representations described above. These
checks establish behavior. Authoring acceptance also requires compact owner
declarations, reusable profiles and helpers that make correct declarations easier
for developers and coding agents to write. The shared implementation may grow when
that cost buys clearer authoring and stronger types.

### Declaration Repetition

The applied repair provides declarative UUID, prefixed, text and hex forms and shared
resource constructors and accessors. Owner-selected profiles preserve generation,
namespaces, accepted spellings, error mapping, binary representation, schema identity
and secret redaction. Custom admission hooks cover representations outside the ordinary
forms. Map IDs now use a two-line declaration with a shared owner profile; Computers
IDs select their UUID admission profile and error context. Typed resource builders
reject IDs from another domain at compile time.

Evaluate declaration ergonomics and type safety before implementation size. Ordinary
owners declare their form and domain policy once; shared mechanics supply standard
derives, serialization and convenience methods. New owners must compose through
their own profiles and types without adding domain vocabulary to core. Compile-time
misuse checks and consumer tests establish that these conveniences preserve admission,
wire formats and schemas. Additional shared code is acceptable when it makes these
contracts easier to author and maintain.

Measure complete owner families and the shared implementation together, including
validators, schema helpers and every descendant of a moved file. Compare both the
current tree and the tree before the Id migration; report subsequently added
capabilities separately. Use those measurements to expose maintenance cost; neither
a smaller macro catalog nor a negative line count establishes authoring quality.
Existing value, wire, schema and independent-consumer tests continue to protect
behavior during the correction.

The applied repair removes repeated ordinary-ID derives and conversions. The measured
set covers 183 Rust and manifest paths, including the original declaration owners,
moved Gateway ID implementations, shared URI and checked/cursor helpers, and every
owner in the cursor serialization batch. It compares `ba20a34d^` with the qualified
address-generation batch. Complete owner, facade and manifest files remain counted. Qualification
includes standalone tests, colocated test modules and Rustdoc fixtures.

| Source category | Before migration | Qualified repair | Change |
|---|---:|---:|---:|
| Implementation/API | 22,615 | 25,329 | +2,714 |
| Qualification | 5,517 | 9,538 | +4,021 |
| Total | 28,132 | 34,867 | +6,735 |

Within that measured set, the complete `platform/macros`, `platform/types` and
macro-policy files add 3,654 implementation/API lines. The remaining owner files
remove 940. Smaller owner declarations have not offset the shared implementation
cost, which stays visible in the assessment. These are whole-file
partitions; neither isolates the cost of a particular capability.

This scope includes checked-value, cursor, numeric and optional-catalog changes as well
as later strict input admission. Their contributions need paired accounting with code
removed from original owners; subtracting new modules alone would hide migration costs.
The measured growth does not establish the declaration machinery's final cost or a
historical reduction. All shared mechanics stay charged to their replacement.

UUID profiles share focused const constructors. The macro frontends share derive checks,
and address emission shares field-shape and component construction. The cursor batch
removes 32 String conversion implementations across 16 owner types. Its complete
17-file family, including the shared helper, removes 138 implementation/API lines and
adds 93 qualification lines. Explicit stateless admission preserves contextual parsing,
retained wire aliases and owner schema identity. Shared and owner native suites,
the independent contract consumer, workspace lint and repository checks pass.
Time's five collection-bound cursors now share one codec with distinct typed collection
profiles. Their complete family removes another 42 implementation/API lines and adds
78 qualification lines, preserving envelope bytes, retained aliases and schema identity.
The reviewed Reason and Stream subscription wrappers keep their owner implementations:
the proposed shared form increased their size or changed malformed-address error precedence.
Redap wire adapters and Rust convenience API variants keep their owner implementations
unless a shared form improves their authoring and preserves their contracts. Their
line count does not block later phases.

The correction uses attribute front ends for ID and resource declarations so they
can generate standard derives as well as implementations. Ordinary public traits
continue to own behavior. Declaration forms select storage and generation
capabilities; ordinary owner profile types supply admission, error mapping,
serialization and schema policy. Prefixes and typed route fields stay in each
concrete declaration. Profiles need no runtime registry, and custom hooks remain
available for contracts outside the standard forms. Replace the old public derives
in the same owner adoption pass.

Normalize internal convenience APIs and update all workspace callers in that pass.
Use `parse` for admission, `new` for fresh generation and one copied UUID accessor
convention. Preserve required const construction without adding compatibility forms
for old method names. UUID admission and generation are separate policies: adding a
v7 generator does not restrict an owner's previously admitted UUID versions. Text
profiles call their existing validators directly; they do not repeat complete
validation through a second grammar description. Parse each declaration once and
pass a typed configuration to its emitter. Schema identity overrides reuse generated
metadata instead of copying descriptions into owner callbacks.

The observation batch and declaration repair are qualified. The address generator
admits each field into a typed cache or component plan, with explicit scalar/query/tail,
argument and accessor choices. One emitter consumes that plan without mutating frontend
options to suppress duplicate getters. Native checks preserve optional scalar codecs,
cache spellings and route-before-convenience diagnostics. Owner contracts, independent
consumers and workspace compilation pass. Against `64a3be86a`, this batch adds 51
implementation/API lines and 108 qualification lines across the same measured family.
That cost is accepted for the checked field model and shared emission. Persistence
extraction can proceed; historical line growth does not block that work.

### Rollout And Gates

Phase 0 preserves serialized forms. Establish and qualify the shared mechanics, then
migrate each concern across all affected owners and consumers in one pass, deleting
its local duplication. Commit coherent concerns after the aggregate checks; do not
run a separate full validation cycle for each crate. An intentional schema tightening is a distinct reviewed change with affected
consumer checks; mechanical migrations preserve existing schema snapshots. The macro-definition gate in `cargo xtask enforce rust` checks the six declared
definitions; `--macros-only` runs that same tracked-source policy independently.
The current Rust wrapper runs all workspace features and suites; individual concerns
use scoped native checks and independently resolved contract consumers.

| Gate | Pass condition |
|---|---|
| Declaration authoring | Ordinary IDs and addresses declare their form and owner policy without repeating standard derives, serialization glue or convenience implementations; typed profiles, builders and misuse checks make correct declarations easier to write; complete-owner and shared-code comparisons report cost without requiring net line reduction |
| Address tests | Round trips, reserved characters, duplicate or unsupported query parameters, malformed identifiers and wrong parents, as the Strong Types rule requires |
| Id and vocabulary tests | Each form rejects invalid input on every constructor and deserializer; duplicate or empty spellings fail to compile |
| Misuse tests | `compile_fail` cases for a template that names an unknown field, a field without a typed identifier, and an invalid scope spelling |
| Wire stability | Mechanical conversions preserve wire fixtures and schemas; separately declared tightenings have explicit changed fixtures and consumer qualification |
| Discovery agreement | Each server's published resource templates equal the templates its address types declare |
| Macro check | `cargo xtask enforce rust --macros-only` reports exactly the core catalog and declared exceptions, including nested declarations; the default Rust gate runs the same policy first |

## Phase 1: Module Contract And Lanes

Phase 1 creates the structure later phases place code into. Its initial native batch
may leave production schema, migration histories and bootstrap unchanged until execution
hosts are qualified. Phase 1 completion still requires the declared commands and Jobs;
this staging does not waive those gates.

The native foundation is qualified. Production composition and the independent
schema fixture declare all 19 required owners. Their catalog declares 166 owner
tables, 39 functions and two analyzers. Store also
owns the lane history, migration history and installation preparation tables. These
claims declare target ownership without admitting the existing mixed production SQL.
The runner qualifies nonempty lanes, fixed prerequisite identity, known disabled
histories, independent lane execution, transaction rollback/concurrency and
timeout/drop cancellation.

The current integration batch adds the gateway's composition-generated plan and
separates mixed-schema preparation, selected lane execution and runtime-authenticated
control-plane publication. A checked installation generation fences old Jobs;
credential revision changes restart database clients, including managed agents through
their existing drain and recovery path. The deployment compiler runs the selected
gateway image to generate the plan before rendering. Source-validation fixtures do
not prove that image's commands work. Native process checks qualify concurrent fresh
preparation, dependent lanes and publication without process restarts, plus stale
generation rejection inside lane and revision transactions. Rendered checks qualify
the generated lane set, credential confinement and stable Job identities. Integration
builds and strict lint pass. The staged gateway image executes the plan generator
without network access and passes the isolated `module-installation-verify` scenario.
Three installed generations qualify fresh preparation and publication, runtime account
rotation, unchanged Job identity, disabled-lane history, stale preparation/lane/publication
rejection and later module enablement. The fixture applies selected chart resources
directly; it does not qualify full Helm rollback, hosted workload startup or reference
activation. The recovery extension starts a real gateway, manager and idle kernel
and checks workload and lease replacement, retained signing identity and PVC content,
unchanged-installation replay, and zero episodes through teardown. Its native
configuration, signing-key, ordered-watch and observer-failure checks pass. A native
database check runs production control-plane publication through fixture definition
creation, publication and managed provisioning without modifying the published Work
Context. The kernel's native admission check also proves episode persistence precedes
dispatch.
The runner distinguishes a top-level typed native commit conflict from an uncertain
commit. Its [lane retry policy](../platform/modules/DESIGN.md#history-and-execution)
permits 16 attempts within 60 seconds and rechecks preparation, history and
prerequisites before replay. Transport failures, timeouts and nested causes cannot
admit replay. Native RocksDB concurrency, rollback, cancellation and classification
controls pass. Diagnostics admit selected resource identities and error categories
while keeping manifests and raw stderr private.

The chart at `0180518d9` uses owner wire fields, binds each model key to its selected
Secret and rejects duplicate environment names. All five Agent policies pass
Kubernetes 1.37 server validation. The owning Manager admission suite admits normal
Pods and Deployments and rejects duplicate key, URL, model-ID and logging entries on
both surfaces through the selected policy.

The complete isolated lifecycle passes with the staged gateway, manager and kernel
images ending in `ff19ca`, `29b734` and `2ebbe5`. All three generations complete their
preparation, migration, publication and runtime authentication checks. Credential
rotation rejects the old account after proving database readiness; stale commands
are refused. Managed replacement preserves signing identity and PVC content, uses
a higher fence, renews twice and survives replay. Ordered observations establish
Pod retirement and old-lease drain. The kernel creates no episodes through shutdown.
The run reports no execution or cleanup failure; owned namespaces, volumes, cluster,
network and private kubeconfig are removed. This qualification covers the selected
CPU-only fixture. Hosted domain startup and final reference activation remain open.
Production SQL redistribution belongs to Phase 3.

| Work | Detail |
|---|---|
| Module contract | `platform/modules` crate with `ModuleSetup`, `ModuleName`, layer, typed ownership claims, `Migration` and lane types; a module design document beside it, the counterpart of `mcp/contract/DESIGN.md` |
| Runner | Native `veoveo-modules` runner with prepared SQL admission, named append-only histories and prerequisites. Explicit mixed-schema preparation preserves existing Store migrations until Phase 3 moves their ownership |
| Ownership validator | Statement-level check of every lane migration against the module's declared table/function/analyzer claims, including explicit non-prefix names |
| Module declarations | One `ModuleSetup` per module, initially with an empty lane; declare owner paths and lane execution image plus command. Composition images may host multiple distinct lanes |
| Kernel order | Qualify the declared target DAG and inventory current-schema cycles, including Agents participant import mutating Workspace. The target Workspace → Agents order moves that participant import into Workspace in Phase 3. Phase 3 removes incompatible associations before final closure |
| Migrate commands | Qualify the declared lane commands and execution images; composition images may execute several owner lanes |
| Helm | Database readiness, preparation proof, kernel and enabled optional lane Jobs, then control-plane publication on fresh install and upgrade. Migration credentials belong to preparation/lane Jobs; publication and serving use the database runtime account |

| Gate | Pass condition |
|---|---|
| Runner tests | Lane ordering, `requires`, dependency order, concurrent Jobs and transaction rollback, using the existing isolated SurrealDB fixtures |
| Ownership tests | Reject foreign claims, nested foreign mutations and unsupported statement classes before executing any statement |
| Helm tests | `cargo test -p veoveo-deployment-smoke` and `cargo xtask smoke helm-config`, plus fresh-install and upgrade qualification of DB readiness → lane completion → publication on fresh install and upgrade, including rendered commands and credential mounts. Pre-install hooks cannot wait for ordinary DB resources not yet created; hook weights alone do not establish readiness |

## Phase 2: Kernel Extension Points

Phase 2 introduces and qualifies ports for optional-module capabilities. Remove the
dependencies owned by those ports here. The Store persistence edge closes in phase 3;
the full kernel graph gate belongs there.

| Kernel dependency | Extension point |
|---|---|
| Gateway binary hosts Computers, Speech, Recordings, Agents and Workspace HTTP surfaces | Route registration on the gateway builder; each module crate supplies its routes and the gateway library depends on none of them |
| Server indexes on `task` (migrations `0042`, `0093`) | Task-runtime transaction hooks: registered adapters supply typed rows for their declared tables at creation and settlement. The runtime owns transaction statements, Task links and settlement guards. Links use `record<task>` with `REFERENCE ON DELETE CASCADE` |
| Module SQL reading kernel tables | Versioned `fn::kernel::*` functions in kernel lanes, starting with task visibility for a caller and artifact readability. Visibility resolves the persisted record inside its owner function and filters module queries before limits or grouping |
| `platform/store` depends on `veoveo-computers-contract` and hosts optional module persistence | Resolved in phase 3, when that persistence moves into its modules |

| Gate | Pass condition |
|---|---|
| Dependency check | Isolated library metadata shows the port-owned optional dependencies removed. Record the Store→Computers persistence edge for phase 3; qualify the composition root separately |
| Port tests | Each port has a test with the module adapter bound and one with it unbound, which refuses the capability |
| Suites | Gateway, task-runtime and affected module suites |

The first adoption batch uses Optimization's existing Task catalog consumers. Required
adapters must be bound before creation or recovery. Creation includes the module row
and idempotency claim in one transaction; terminal transitions and recovery failures
settle that row under the Task's current-state guard. Adapters cannot supply transaction
SQL. Reusable ownership declarations exclude composition image and command settings.
Optimization reads qualify catalog-to-Task relationships after caller filtering, and
selected corrupt rows remain errors. Catalog selection and Task decoding share one
native read transaction, including completion queries, so concurrent settlement or
deletion cannot mix committed views. Validation rejects inconsistent selected rows;
it does not remove them from a page after SQL selection. Shared Task owner, context and
usage predicates use the same object and label-shape guards as the kernel function.
Native fixtures install the actual owner lanes
after mixed-schema preparation. Existing Task indexes stay until the coordinated
storage cut; this additive batch does not activate the reference installation.

The gateway executable lives in `platform/gateway/composition`
(`veoveo-gateway-composition`) and keeps its executable name and image. The reusable
library delegates Computers transport pooling, Recording catalog admission and
managed-agent authorization to their owners. Native checks qualify required bindings,
catalog reload identity, TLS cache retirement, current managed authority, the real
installation commands and the affected consumers. Image source selection uses each
compiler family's actual Cargo feature graph and keeps nested packages separate;
the optional adapters cannot pull gateway code into Console's build inputs.

The managed-claim ownership cut passes its native Rust and Python checks. Agents
exposes `ManagedAgentToken` through its contract feature; the OAuth port carries
typed admitted contributions. Independent review accepts the registration profile,
immutable storage, current owner authority and receiver format checks. Recording
configuration and optional policy actions have moved to their owners and pass native
admission, policy and schema checks. The real conformance export validates the five
installation catalogs. Affected consumers and seven independent contract/runtime
builds pass; the existing Store/Audit Computers contract edges belong to Phase 3.
Installed catalog startup admission remains open. Agents' authoring and operator-control
contracts and dependent Workspace models have moved out of MCP. Native tests and
affected consumer checks pass. Independent builds qualify both pure contracts, the
Workspace App SDK profile and the Agent catalog adapter. Generated browser schemas
are unchanged. Kubernetes admission and the native-binary blob-projection check were
not executed in this batch; they require their declared environments.
The route registration builder supplies a typed shared context and validates required
bindings before starting factories. Each domain supplies its own handlers. Independent
route fixtures qualify a nested namespace without a core prefix declaration, bearer
admission and audits, unknown profiles and rejection of encoded aliases. Module scopes
track request handlers, streaming bodies, upgrades and background work. Closing
admission and draining use one 30-second deadline; cancelled cleanup observers do not
reset it. Startup errors, occupied listeners and route conflicts await cleanup before
returning. Streaming cancellation reports interruption and ordinary streams preserve
trailers. Composition drains module work before closing audit delivery.

Agents supplies a typed capability-reader port and a native MCP adapter independent
of Workspace operation state. Both owners reuse gateway-native transport and discovery
mechanics; callers supply client capabilities and progress observers. Workspace may
depend on Agents, while Agents works with Workspace disabled. Owner suites qualify
current policy, caller credentials, durable admission and cancellation. Stopped module
admission cannot create a new durable operation, and transport cancellation does not
settle an unresolved provider outcome.

Owner-wire extraction follows the route move. The managed token binding belongs in
Agents' contract feature and producer configuration belongs in Recording's contract.
A composition-supplied registry declares reserved access-token
claims and catalog sections separately from their bound owner codecs. A configured section or signed
reserved claim requires its owner adapter. Disabling an adapter cannot turn a managed
identity into an ordinary static client. Public OAuth verification enforces this
rule; unrelated external JWT claims keep their current treatment.
Core stores immutable admitted extension payloads at the serialization boundary,
while owner handlers retrieve concrete types through registry-bound typed keys.
Reject duplicate keys, core-name collisions and replacement of verified claims.
Recording contributes generic protected-resource descriptors for OAuth cross-reference
validation. Publication and reload use the same admissions. Preserve the current JWT
and catalog wire spellings until the composed schema, authority and isolated-contract
checks qualify the ownership cut.

Recording's cut includes its producer, dataset, application and stream IDs, all
Recording policy actions and targets, catalog indexes and ingest evaluator. Its
contract owns the vocabulary and configuration; its gateway adapter owns current
admission and policy evaluation. Reusable authentication identifiers and upstream
connection configuration belong below MCP. Keep connection security separate from
MCP transport selection when moving these shared DTOs. Recording's plain HTTP ingest
endpoint currently declares `transport: streamable_http`; remove that misleading
field from its owner configuration and update fixtures, installation inputs and
schema consumers together. This intentional configuration correction joins the
coordinated installation cut; do not retain a compatibility field or alias.

Policy configuration admits action names through the registered kernel and owner
vocabularies. Rust callers supply the owning enums through typed constructors. Each
action family validates applicable rule selectors; unknown actions and selectors
that do not apply must fail admission. Preserve action spellings, target schemas,
deny precedence, producer and tenant checks, scope and label checks and missing-condition
diagnostics. Initial decoding, publication, persisted reload, installation validation
and composed schema generation use the same catalog section registry. Transfer the
policy matrix and native ingest, blueprint and layer-publication tests with their
owner. Deleting core match arms without moving their validation is incomplete.

The action cut covers all 21 optional-module actions together: fourteen Agents
actions, `ComputerAttach` and six Recording actions. Kernel actions keep a closed
enum. Each owner declares its closed vocabulary, supported targets and rule-selector
constraints; composition registers it once. External action names resolve through
that registry, and runtime callers use private handles obtained from typed owner
keys. Reject duplicate, unknown, unbound and cross-registry actions and targets before
policy evaluation. Generated schemas advertise only bound actions and target kinds;
qualify schema and runtime agreement with an independently declared owner vocabulary.
Preserve Agents'
selector restrictions and audit-operation mapping, Computers' resource-capability
checks and simultaneous attachment/read authorization, and Recording's distinct
ingest and layer rules. Agents and Computers keep their existing shared target
shapes; only Recording contributes new owner target types. Shared deny precedence
and principal predicates stay in the policy engine, and owner adapters cannot
override a denial. This is one action-registry concern across all affected consumers,
not a Recording-only port followed by separate mechanical passes.

Knowledge authorization and indexing and Computers authority snapshots also read
the full catalog. The pure [catalog composition](../platform/gateway/catalog/DESIGN.md)
supplies the same owner codecs and policy declarations to these readers, gateway publication and schema
generation. Runtime libraries receive the registry explicitly; generic policy and
gateway libraries must not import that recipe or the optional owner implementations.
Readers cannot ignore configured sections merely because they do not use those
domains. Qualify a real full catalog and the same unknown/unbound failures through
both gateway and standalone readers. Check package cycles before placing the recipe;
feature flags do not remove Cargo package cycles.

Authoring DTOs, managed-instance operations, operator-control and conversation models,
template validation, model connections and configuration digests belong to Agents'
contract. Move them and their browser/schema consumers before the
Phase 3 ownership audit. Their separate installation inputs do not require additional
gateway catalog sections. A dependency graph without owner crates does not prove
modularity while core still defines an optional owner's domain model.
The pure contract admits models and templates against typed facts from one catalog
revision: context-to-tenant relationships, installed profiles and approved secret
purposes. Callers project authenticated tenant and scope facts for visibility checks.
Agents' separate `catalog` adapter validates the complete control-plane revision
against the supplied registry before projecting installation facts. Manager and the
gateway use this adapter; Manager does not acquire a gateway runtime dependency for
configuration validation. Facts describe the admitted snapshot and do not establish
current authorization or installation readiness.
Move the projected `GatewayToolName` below MCP with its parser and schema identity.
Qualify the existing model, template and ConfigMap digest profiles with fixed expected
values, preserve public-field redaction and regenerate the browser contract. An
external contract-only consumer must exclude MCP, Store and runtime policy dependencies.
Workspace's revision view and preview embed these authoring models. Move the complete
Workspace DTO module and its schema producer into the existing Workspace library in
the same cut; leaving them in MCP would create a reverse dependency on Agents.
Store uses independent persistence IDs and introduces no package cycle here, so
this extraction needs no new contract crate. Workspace's `contract` feature enables
only Agents' contract. Its separate `app-contract` feature owns the native MCP App
envelopes and complete browser schema bundle. This profile includes the pinned SDK's
default features and async dependencies; it is separate from the pure contract.
Preserve those typed SDK results. Explicit client and HTTP transport features belong
in Workspace's `gateway` feature. Qualify
both lightweight contracts independently and preserve the Workspace-to-Agents
dependency direction, existing browser filenames and wire schemas.

Internal assertions need the checked execution attribution, rather than the public
owner claim. Agents' admitted authority supplies the existing audit contract's
`AuditManagedExecution`, with its nominal instance ID and positive generation and
dispatch epoch. Gateway signs that projection in the internal request context.
The verifier checks its relationship to the automated service identity, client,
tenant, Work Context and absence of a session family. Audit construction consumes
the same projection and preserves the frozen audit format. Generic hosts do not
need an Agents codec or a second extension registry.

This projection is gateway-attested execution evidence, not an authorization grant.
Knowledge's `src/authority.rs` uses the current binding for live registration checks;
it must retain every instance, generation, epoch, registration, scope, role and tool
comparison when consuming the projection. Gateway discovery fingerprints must also
include it. Qualify missing or unexpected attribution, malformed counters, source
collisions, current revocation and stale discovery. Public token shapes and stored
audit records stay unchanged. The internal assertion format requires a declared,
coordinated gateway/server drain and explicit rejection of unsupported formats;
absence must never downgrade a managed identity to a static client. The implemented
cut requires `veoveo.ai/gateway-internal-assertion/v2` and the closed
`veoveo.ai/gateway-request-context/v2` marker in both Rust and Python. It preserves
the host builder and verifier call interfaces. If further implementation
requires changes in shared hosting, present them for user-directed review first.
Phase 8 upgrades every internal-assertion producer and receiver under that drain.
Phase 10 must qualify managed and static service calls, browser session attribution,
and rejection of unsupported formats through the installed Rust and Python receivers.
Native qualification of the ownership cut does not close that installed gate.

## Phase 3: Module Ownership Of Persistence And Queries

Phase 3 moves module code to its owner and puts every SurrealQL statement in a file.

| Work | Detail |
|---|---|
| Store split | Map, Workspace, Agent management and Recordings persistence moves out of `platform/store/src` into the crates their `ModuleSetup` names, with their query files. Knowledge persistence moves into its own kernel module folder and lane inside `platform/store`. `platform/store` keeps the connection and the kernel modules |
| Query files | 374 inline `.query("…")` sites and about 115 query files mixed into `src/` move into owners' `queries/` folders. The 110 `format!`-built statements become bound parameters or `type::table($name)`; any remaining dynamic statement is listed in the owner's design with its reason |
| Kernel access | Optional module SQL on kernel tables moves to kernel APIs or `fn::kernel::*` functions: Computers 27 statements, Reason 5, UAV 4, Map 3, Optimization 3, Frames, Stream and Artifact |
| Schema ownership | Move current definitions into owner lanes and bootstrap fresh fixtures. Do not deploy these storage changes over the old installation; phase 8 publishes the composed cut |

| Gate | Pass condition |
|---|---|
| Layout audit | No SurrealQL in Rust strings; every `.surql` file sits in a `queries/` or `migrations/` folder of its owner |
| Ownership audit | Module query files reference only their own tables, declared dependencies, record links and `fn::kernel::*` |
| Dependency closure | Isolated Cargo graphs show no optional-module dependency in reusable kernel libraries, including Store; composition packages may bind declared modules |
| SQL admission | Tenant, owner, context, labels, parent and operation selection remain in SQL before decoding, ranking and limits; kernel functions preserve the same policy |
| Suites | Store, task-runtime, gateway, computers and every moved module's suite against real SurrealDB |

The layout gate applies to complete statements. Owners may compose fixed statements
from their query files through the SDK into one transactional request, including
optional writes selected by typed inputs. Preserve bindings, result positions,
rollback and policy checks. A transaction does not need one duplicated file for
every statement combination. Predicate, identifier and token interpolation still
requires replacement or an explicitly qualified grammar exception. Parser tests may
deliberately transform file-owned fixtures to exercise rejection; that test behavior
grants no runtime SQL construction API.

Runtime ownership includes foreign-record dereferences as well as explicit table
queries. The current source batch moves Frames operation writes and reads to the
existing Task selection export, preserving their owner-only access profile.
Workspace people, invitation admission and run context consume four Identity-owned
metadata and search exports. Workspace keeps chat policy and selected-member limits,
and invitation admission stays inside its transaction. Source review and SQL syntax
checks pass, as do the Frames, Workspace and direct Identity export native suites.
The affected consumer compile and strict lint checks pass. These owner APIs and the
composed Task storage cut qualify together as a source checkpoint.

Computers reads Gateway control revisions and refresh families, and Identity
tenant and principal records, through owner exports inside admission and dispatch
transactions. The implemented exports preserve revision/digest agreement, family
revocation and expiry, enabled state, and identity relationships. Grant creation
needs the admitted family's expiry to cap its own lifetime. Identity reads need
the tenant slug and principal kind, issuer and subject for exact comparisons.
Keep these comparisons in the existing transaction and before collection limits.
The export calls do not replace Computers' policy decisions or provider fences.
Its operation, execution, transfer and maintenance journals carry a declared
Task tenant record derived from the same owner used for Task admission. This lets
cross-tenant worker queues use Task lifecycle selection without copying Identity's
record-ID construction into SQL. Worker recovery checks retained tenant identity
without requiring the tenant or enterprise to stay enabled. Disabling authority
must still allow workers to contain dispatched effects, settle results and release
acknowledged retention pins. New admission requires enabled authority. Qualify
missing, disabled, foreign and changed records along with grant expiry and
current-result acknowledgment.

Media's kernel access includes provider-job and event writes, Task
waiting and settlement, prediction and generation reads, and billing selection.
Typed Tasks-owned APIs implement the shared journal transactions. Media owns the
provider payload interpretation and the declared prediction/result lookup fields;
its SQL must not inspect `provider_payload` or the Task result envelope. Preserve
webhook deduplication, event/job/Task correlation, cancellation semantics and atomic
settlement with the existing fake-provider tests. Media uses `WebhookWait`; the
`ProviderWait` observation-lease API cannot represent its authenticated callbacks.
The new journal transactions check Task, tenant and provider associations and update
Media's lookup when the provider assigns an external identity. Terminal settlement
also runs the owner's Task contribution. Billing selects provider completion even
when the local Task was cancelled. Preserve the first authenticated terminal
provider observation independently of Task settlement: a local Artifact publication
failure cannot rewrite a successful provider outcome. Qualify concurrent receipt,
submission binding and cancellation with transaction-level correlation checks;
checks performed before the transaction do not protect a later journal write.
The creation contribution's identity stays immutable. A later provider identity
enters through a checked owner projection in the journal transaction, with the
same rollback and replay guarantees as the Task contribution.
Tasks exposes provider observation facts through an owner API that checks the
job, Task, server, tenant and provider relationship. Media queries use those facts
and its own lookup fields; provider payloads pass whole to Media for decoding.
Caller reads still apply Task selection before page limits. Billing requires a
recorded provider terminal event, independently of the local Task outcome.

Media must authenticate callback-to-dispatch correlation before an external job ID
has been bound. WaveSpeed's [signature profile](https://wavespeed.ai/docs/verify-webhooks)
covers the event ID, timestamp and body; the callback URL's Task ID is outside that
signature. Persist a private Media-owned dispatch binding before submission and
require it alongside the provider signature. Keep callback secrets out of request
errors and logs. Qualify replay against two unbound Tasks, restart, binding races and
late terminal observations after local cancellation. A submission timeout preserves
an unresolved outcome and cannot authorize resubmission or a failed Task settlement.
The host currently includes query parameters in request spans through
`DefaultMakeSpan`. Path-only tracing and a captured-log regression check are required
before deploying callback binding credentials. This protected host edit requires
the user's approval under the installed-acceptance handover.
The installation cut must drain the previous callback profile; missing binding
credentials fail rather than selecting an older handler.

Knowledge's reusable runtime receives Policy's typed internal-client resolver and
has no normal Agent persistence dependency. Agents owns the separately gated managed
adapter. The static resolver rejects managed attribution, and its observation tables
exclude Agents. Kernel-only HTTP and subscription fixtures assert that Agent tables
are absent. Current-authority checks run before delivery.
Server composition selects the resolver from the admitted `ModulePlanDocument`
through `VEOVEO_MODULE_PLAN`. The `managed-clients` feature makes the adapter available;
selecting the Agents lane binds it. Modules supplies read-only prerequisite admission
over committed installation and migration identities. Missing selected adapters or
drifting preparation fail configuration before workers start. The existing full-registry
runner status still rejects a partial registry.
Preserve collision, instance generation, dispatch epoch, enabled-state, tenant,
Work Context, scope, role and tool checks, plus managed registration-change observation.
These source checkpoints do not qualify installed HTTP/subscription authority or
current-generation recovery. Phase 10 must exercise both kernel-only and managed
selection without inferring readiness from table existence.

Artifact's subscription deadline query runs through its owning Store API, which
applies Artifact admission to retention and grant-expiry selection before returning
the next deadline.

Agents' managed-instance provisioning creates its service principal through the
Identity export. Definition publication, ownership transfer, executable reads,
registration and reconciliation also use Identity APIs. These calls preserve each
enclosing transaction, identity collisions, capacity rollback and current dispatch
checks. Source review reconciles 495 production query assets through their Rust
include users, owner declarations and binders. Private variable-target helpers
receive owner IDs or fixed owner table names. Stored dereferences use declared
record links; UAV's Agents reads and Workspace's Agent functions have explicit
module dependencies. Bound command paths and whole opaque values do not establish
foreign-record access. This source check adds no new native or installed evidence.

UAV and Reason need distinct caller and maintenance profiles. UAV preserves its
indexed mission-plan and execution lookup, while Tasks owns terminal settlement
and retention selection. Reason findings follow Artifact read access across Task
owners; an owner-scoped Task reader would change that policy. Complete these reads
with the planned owner lookup tables from Phase 4 where they require the same
writers and queries. Kernel exports own Task and Artifact admission; Reason keeps
result interpretation and provenance checks. Apply the combined policy before page
limits, and preserve observation digests, expiry deadlines and rollback. These
owner chains qualify together before closing either phase's affected gates.

The UAV, Reason and Stream batch writes typed lookup rows through Task creation
and settlement contributions. Caller reads use Task selection; maintenance and
Artifact-shared findings use a separate Task lifecycle export that returns no
request or result payload. Its optional whole-result comparison lets Reason reject
a changed retained result after admitting the reader. Reason stores that validated
MCP envelope as an integrity snapshot and queries only its declared finding and
provenance fields. Artifact owns current read admission and observation facts.
Reason compares the complete admitted Artifact metadata with its typed publication
receipt before pagination, preserving provenance checks without querying inside
Artifact's opaque metadata. Time-dependent kernel leaves accept a native database
timestamp supplied once by the enclosing query. Native qualification passes for
cross-owner grants, revocation, selected corruption, expiry, atomic contributions
and UAV's indexed execution and unresolved-retention fences. Mission paging and
completion reject crossed execution/plan associations even when both missions
belong to the same caller. Read transaction cleanup passes for timeouts and dropped
request awaiters.
Task completion and domain product success have distinct meanings. Reason and
Stream can complete a Task with an MCP tool-error envelope. Their typed lookup
settlement must represent that outcome without product links, while preserving
the completed Task and its error envelope. Positive fixtures must use the real
transition; deliberate corruption fixtures use an explicit separate write.
Reason and Stream add optional schema lanes to the composition. The independent
schema consumer composes nineteen owners; all four plan fixtures come from the
current Gateway producer. Fresh lane execution, disabled-owner absence, replay and
the installation command lifecycle pass. The final installation cut must update
the reference selection and generate its plan from the published image. Keep the
installed image's existing plan intact until that coordinated cut.

Computers owns its changefeed decoder in `platform/computers`. Store consumes checked
owner observation declarations and enumerates kernel tables only. Native checks cover
checkpoint replay, malformed-row rejection, actual schema retention and listener
cleanup, including cancellation before a registration response arrives. Independent
schema consumers exclude database and service runtimes. The
[observation design](../platform/store/src/changefeed/DESIGN.md) records delivery and
the pinned SDK's cleanup limitations.

Domain repositories still need to wrap the shared
connection and kernel services; their records, errors and queries move together.
Agents persistence belongs in `agents/runtime`, Workspace in `platform/workspace`
and Map in the server's persistence feature below its spatial runtime. Agent chat
imports mutate Workspace participants, so Workspace owns that transaction and consumes
typed Agent revision admission. Agents must not depend on Workspace.

Kernel SQL exports keep their owned read profiles. A mutating export declares the
owned table and fields it may update; read-only exports keep their existing limits.
Agent result settlement reads its Task route through a Gateway export, then calls
the Task owner's retention-release export with typed Task and server references.
Both calls run inside the Agent transaction, preserving the route/server check and
rollback of retention release when Agent settlement fails. This composition does
not require foreign reads inside an export or calls between kernel exports.

Owner-local SQL functions may compose after admission inspects their complete bodies
and transitive effects. Calls between optional owners require a declared dependency,
a concrete migration minimum and a read-only callee. Kernel exports stay explicit
versioned leaves. Admission rejects unresolved calls, recursion and argument-side
mutation. A later function overwrite must not grant writes to a surviving permission,
field expression or other read-only caller.

Recording persistence needs a lower `platform/recordings/store` library. Recording MCP
already depends on Hub and Reader, while those libraries and Video also consume its
catalog and ingest persistence. Putting that persistence in the server creates a
package cycle. The existing Recording contract stays limited to public models; Hub,
Reader and RRD own different runtime concerns. The new library has independent schema
and persistence features and adds no process. The server reexports its schema declaration
under the existing module identity. Move each owner's native tests with its repository,
and qualify the complete dependency chain against fresh owner lanes in one batch.

Owner repositories wrap the shared Store connection; they cannot retain inherent
methods on the foreign `PlatformStore` type. Establish the shared observation and
transaction-error APIs before parallel owner edits. Fresh qualification fixtures
must bootstrap their selected owner lanes directly. The current fixture's mixed
`migrate_on_connect(true)` bootstrap cannot prove owner independence or disabled-owner
absence. Qualify those cases and prerequisites before accepting the moved suites.

Fresh lanes omit `task_used_frame`: no production reader or writer uses it, and its
`frame` endpoint names a table removed by the world-revision model. Remove its claim
and fixture expectation without assigning it new semantics. Optimization's existing
catalog replaces its three old indexes on kernel Task payloads. UAV's mission lookup
can use its indexed mission plans and execution links to select Task record IDs
directly. Preserve current authority and parent checks before ordering or limits,
and qualify the native query plan without a Task table scan before retiring
`task_uav_plan`. This prerequisite does not complete Phase 4's payload-column work.

Computers' composed schema declares `computer.owner_context.authority` as a required
closed object, including its nested authority, output-policy and provenance fields.
The controlled-fields migration replaces the initial FLEXIBLE declaration before
index installation. Keep real Computer writes, indexed ownership selection and
rejection of absent or record-valued authority in the native and installed gates;
the nested declarations are implemented rather than pending Phase 4 work.

Audit's Computer target also crosses this dependency cut. Computers must own its
typed target codec and lookup-reference projection. Composition binds that codec to
Audit's extension slot; persisted and query decoding reject unbound targets. Preserve
the existing target JSON without an extra wrapper, canonical record bytes and the
same-transaction append path. Audit's owner lane stores the admitted lookup as an
opaque native record reference without enumerating optional tables. Its field type
grants neither `REFERENCE` cleanup nor executable dereference rights. The registered
owner codec checks the lookup projection, and readers verify it against the target.
Preserve the target index and existing exact
draft-target list matching. Qualify independent registration, malformed targets,
canonical hashes, commit/rollback and Audit-only bootstrap before removing the
Computers contract dependency.

Establish contextual Audit admission before that dependency cut. Audit owns an
immutable target registry whose typed registrations bind an owner decoder, closed
schema and lookup-reference projection together. Computers supplies its target type
and discriminator. An admitted extension serializes directly as the existing target
object; registry identity and lookup metadata never enter canonical record bytes.
Unknown, unbound and duplicate discriminators, collisions with core variants, and
contributions from another registry fail admission. The generic claim-extension
registry's drop-unknown behavior does not apply to Audit.

Target-bearing drafts, records, queries and reader models decode with the registry.
Private wire DTOs may carry unadmitted input, but domain APIs accept checked values.
Driver rows extract the stored value before contextual admission; live reads, page
reads, sealing, export and indexing use the same admission path. Query targets are
checked before SQL execution, and SQL keeps its complete target equality predicate.
Preserve duplicate-field rejection at external JSON boundaries.

The composed reader schema uses those same registrations to form a closed union,
preserving owner definitions and rejecting schema-name collisions. Audit-only
composition has no Computer target or table. The installation keeps the read codec
for a supported domain when its workload is disabled; codec registration does not
activate that workload or its persistence lane. An installation missing a required
codec rejects the record with a configuration diagnostic rather than skipping it.
Qualify HTTP/CLI inputs and generated browser readers with the persistence consumers.

## Phase 4: Database Field Types

The inventory found 88 `TYPE object FLEXIBLE` fields inside SCHEMAFULL tables.
Their nested shape is not closed by the table declaration. It also flagged about
50 Computers predicates for missing-value review, including
`(…output_policy.data_labels ?? []) ALLINSIDE $labels` and
`…classification = NONE OR …`. Distinguish missing required authority from a
legitimately optional field before changing policy. Qualify missing indexed paths
and uniqueness alongside the declarations.
The repository already uses the strong form for `task.authority` (14 declared nested
fields) and for artifact upload descriptors and manifests.

Phase 3 assigns each table to its owner lane. Phase 4 updates those current-format
definitions and bootstraps empty fixtures. No migration copies historical payloads;
the reference installation receives the composed fresh-state cut in phase 8.

### Declared Authority And Identity Fields

These 28 fields become non-FLEXIBLE objects with every nested field declared from
the Rust type that writes them. `option<…>` appears only where the Rust field is an
`Option`, and arrays declare their element fields with `field.*`.

| Area | Fields |
|---|---|
| Computers ownership and authority | `computer.owner_context`, `computer_operation.{owner_context, actor_context, execution_authority, dispatch_authority}`, `computer_maintenance.{actor_context, execution_authority}` |
| Computers executions and transfers | `computer_execution.{authority, binding, dispatch_authority, effective_limits, output_access}`, `computer_file_transfer.{authority, binding, dispatch_authority, effective_limits, artifact_access}` |
| Computers grants and maintenance policy | `computer_{automation,cli,session}_grant.authority`, `computer_automation_grant.execution_limits`, `computer_maintenance_resume.{authority, decision}`, `computer_maintenance_policy.envelope` |
| Gateway sessions | `gateway_refresh_family.principal` |
| Managed agents | `managed_agent.{identity, resources, public_key}`; the UNIQUE indexes on `identity.client_id`, `resources.workload`, `resources.credential_secret` and `resources.volume_claim` then always apply |

All 28 root declarations are present and non-FLEXIBLE in the composed current schema.
Computers applies its controlled-field declarations after the base file and before
the indexes; inspecting only its base file gives an obsolete field profile. The
qualified controlled-storage batch covers these owners. The wider path, bound-value
and relationship audits below still govern phase 4 completion.

The normalized `Principal` and its group memberships have a controlled shape.
Close their Rust and Python decoders together; Python's extra-field preservation
and Rust's silent dropping must not produce different stored authority snapshots.
Close the outer `PolicyDecision` while preserving registered owner targets. Raw
external JWT claims keep their existing handling before normalization. Qualify
both normalized-field rejection and external-claim acceptance with the affected
generated consumers.

The `TaskOwner` snapshot moves out of the FLEXIBLE `task.request` into a declared
`task.owner_context` object. Keep `task.owner` as the existing `record<principal>`
link and preserve its indexes. Update every `request.owner.*` query to use
`owner_context.*`; the Python SDK task runtime writes the same shape.

Store owns the driver record for this snapshot, and Task Runtime converts its public
`TaskOwner` into that record. Preserve the snapshot's invocation authority and check
its agreement with `task.authority` and the indexed identity fields before admitting
a task. Reconstructing the snapshot from those columns would erase that consistency
check. Declare the request envelope's `input`, `status_message`, `ttl_ms` and
`poll_interval_ms` fields; only `input` holds an opaque domain or provider value.
Transactions that compare the expected request must also compare the expected
`owner_context`, preserving rejection when ownership or authority has changed.
Rust and Python snapshot decoding must enforce the same identity, Work Context and
invocation-authority consistency checks.

Task timestamps preserve nanoseconds across Rust, Python and native changefeed
replay. The schema derives private `created_at_exact` and `updated_at_exact` strings
from the native datetime columns because the pinned Python driver decodes datetimes
at microsecond precision. Checked timestamp types retain those strings for keyset
cursors and compare-and-set bindings through the driver's native datetime encoder.
Public Task fields keep their existing RFC 3339 shape. Qualify cross-writer paging
and stale-snapshot rejection when two timestamps differ within one microsecond;
neither comparison may reduce the stored clock precision.

The Task storage cut implements nominal driver
IDs, closed authority decoding and all known Rust and Python consumers. Python unit,
query syntax and current SDK/template wheel checks pass. Native Task storage checks pass for
schema rejection and rollback, current-format reconnect, precise timing metadata and
rejection of native database values inside JSON input. Grant array objects have
explicit key assertions because nested unknown-field writes bypass the pinned
database's ordinary object closure. Updated fixtures assert rejected writes while
preserving SQL-before-decode checks with malformed values the schema admits.
The SDK/database/template batch passes 217 tests, including real Rust/Python claims,
exact timestamp replay, nanosecond stale-snapshot rejection and cross-writer keyset
pagination. The affected Rust policy and result consumers pass 28 focused checks;
the current 18-package compile/lint graph and isolated Store runtime build pass.
Real installation commands, independent schema consumption and generated clients
also pass. The other Phase 4 field families and installed acceptance remain open.

### Opaque Payloads And Declared Lookups

Keep genuinely opaque payloads FLEXIBLE. A stored JSON object is not necessarily an
opaque payload: the bind audit also identifies controlled envelopes that need typed
driver adapters. Every value a query reads from inside a FLEXIBLE field becomes a
declared column. When the payload's table and the reading code share an owner, the
column goes on that table. When an optional module reads a kernel payload, the module
writes its own lookup table through the phase 2 task hooks.

| Query today | Declared replacement | Owner |
|---|---|---|
| `result.payload.structuredContent.result_uri` (indexes in `0042` and `0098`) | `task.result_uri`, indexed; C02 makes the product URI a kernel concept | Kernel tasks |
| `request.input.common.problem_id`, `run_id` (indexes in `0042`) | `optimization_task` with `task`, `problem_id`, `run_id` | Optimization |
| `request.input.plan_id` (index in `0093`) | `uav_task` with `task`, `plan_id` | UAV |
| `result.payload.structuredContent.travel_model_id`, `created_by` | `map_travel_model_task` with `task`, `travel_model_id`, `created_by` | Map |
| `result.payload.structuredContent.results_artifact.artifact_id` and `metadata.provenance.analysis_id` (reason `knowledge/*.surql`, reason and stream `index.rs`) | `reason_analysis` and `stream_run` lookup tables with `task` and `record<artifact_occurrence>` links | Reason, Stream |
| `definition.frames[WHERE frame_id = …]` | `frame_world_revision.frame_ids` | Frames |
| `payload.consumed_at` | `gateway_authorization_code.consumed_at` | Kernel policy and gateway |
| `content.execution.template_revision` | `agent_definition_revision.template_revision` | Agents |
| `source.kind` (`maintenance_commit.surql`) | `computer_maintenance.source_kind`, a literal type | Computers |
| `encoding::json::decode(canonical_json).provenance.base_release_ids`, `restriction_ids`, `facility_ids` (Map store `owned.rs`) | `map_route.base_release_ids`, `restriction_ids`, `facility_ids`, declared string arrays written in the same transaction as `canonical_json` | Map |

Task completion supplies an optional typed `ResourceUri` alongside its opaque result.
Persist that address in `task.result_uri` in the settlement transaction and preserve
it through Rust and Python snapshots. The MCP adapter validates the C02 product
address against the completion payload; the Task storage implementation does not
interpret MCP keys or server-owned result schemas. Qualify product completions,
completions without a product, tool errors, malformed or mismatched addresses and
replayed settlement. Update every writer in the same cut, including Media's webhook
settlement and owner contributions.

The MCP error flag alone does not determine whether a result has a product. A failed
command may retain an addressable output; validate its URI and link by the same rule.
Input references and updates to existing control state do not establish a separate
result resource. Owners declare that distinction in their typed output contracts.
Map reachability returns inline polygons and a calculation ID; remove
`reachable_area_uri`, which has no retained resource or read route. Map's persisted
route, matrix and other published products use their existing address builders.
View capture returns its Frame address. Frames and DuckDB declare an Artifact
address only when that result materializes an Artifact.

The 28 JSON-in-string columns stay as text. `canonical_json` serves byte-exact
idempotency comparisons in Map, Time and UAV, `geometry_json` carries GeoJSON, and
`schema_json` and `style_json` carry user documents. After phase 4 no query decodes
any of them.

The lookup inventory is not exhaustive. Agent mutation queries also compare
execution kind, template and complete parameters. Give queried controlled values
declared fields and derive them from the validated content in the same write.
Parameters stay opaque where their schema belongs to the selected template; compare
the whole value to preserve the managed-update constraint. Preserve the draft's
execution-kind constraint as well.
The Agent instance HTTP projection hydrates the full retained revision through the
repository's checked decoder and reads its declared `execution.template`. Native
checks cover definition and tenant agreement, corrupt revisions and private reads.
The owner function `fn::agent_chat_revision` selects the declared revision execution
field at `e878b4d3a`. It requires the chat kind alongside tenant, audience, publication
and revision-digest checks, without inspecting the FLEXIBLE content.

Audit view admission and list filtering use declared profile, whole-target and
whole-detail lookup fields derived from the admitted draft in the same write.
Native checks cover lookup agreement, filtered paging, view admission and rollback.
Registered target admission and frozen draft bytes and hashes are preserved.
Audit export delivery compares the declared block head hash and complete expected
block. The writer derives the lookup in the same write, and the reader checks its
agreement with the admitted block. The delivery transaction does not inspect a
path inside the FLEXIBLE block.

Optimization's catalog queries select the declared operation, terminal status and
product address after kernel caller admission and before limits. The reader hydrates
selected Tasks in the same transaction and checks catalog/request/result agreement.
Authorized corrupt rows produce integrity errors; SQL excludes denied rows. The
catalog does not duplicate complete requests, solver models or capability secrets.
Current native reads pass through the shared product-result helper with canonical
`resultUri` admission. Installed GPU readiness and resource reads remain required.

Both Knowledge catalog writers derive declared approval, scope, change-signal and
entity-kind lookups from validated `CollectionRegistration` values. Catalog,
completion, observation, statistics and search-depth queries consume those fields.
Whole approval equality, current source leases and scope selection precede ranking
and limits. Hydration admits complete registrations without querying paths inside
the FLEXIBLE document. Members and generation-specific chunk tables declare the
controlled observation envelope. SQL returns scalar statistics after caller and
source-policy selection. Generation collection requirements declare their record
links and revisions through the owner driver record. These source changes preserve
observation wire forms and revalidation; installed qualification remains open.

Gateway writes the profile's policy version as declared metadata alongside the
admitted profile. Artifact's SQL guard selects the policy through that field and
hashes the complete profile and policy documents. The service checks document and
lookup agreement before initial admission and retained access. Native checks cover
missing and inconsistent metadata, revision scoping and rollback; owner-defined
catalog kinds keep their registered vocabulary.

Computers maintenance settlement and resume use declared source resource/process
lookups and compare the complete typed source and progress. Native maintenance
checks preserve variant admission, Computer identity, replacement fences and
current-format recovery receipts.

### Literal Types And Bound Values

The owner review narrows the original ten-field literal inventory to seven closed
vocabularies: `coordinate_operation.kind`, `map_acquisition.phase`,
`map_restriction.kind`, `map_restriction.effect_kind`, `map_source.adapter_kind`,
`map_source.authority_class` and `time_acquisition.phase`. The database already
uses 150 literal types.
The current schema declares all seven as literal unions. Time's acquisition phase
uses its owner vocabulary through the contract, service and driver record; native
tests qualify rejection and rollback.

Remove the unused `agent_owner` and `membership` relations with their
`AgentOwnerEdge` and `MembershipEdge` records in the fresh schema cut. Neither has a
producer or consumer that defines a role vocabulary. Configured group roles and
Work Context membership use their existing contracts.
Remove the schema-only `mcp_interaction` table with its ownership claim and schema
inventory entries. Its input-request and input-response fields have no current
writer or consumer; the implemented Task and Agent input flows own their storage.
`gateway_control_object.object_kind` accepts owner-registered catalog extensions.
Keep that field extensible, validate the shared `ExtensionName` syntax, and qualify
an independent owner's kind. A kernel enum would require core edits for new owners.

Code that binds a wire type into SurrealQL binds a store-owned record type instead.
The inventory includes the artifact upload descriptor, layout and manifest compared
in the artifact upload queries, workspace operation commands
(`$command.run_fence`, `app_uri`), time activation expectations and agent mutation
plans. Each owner lists every `.bind` and classifies the bound type.

Artifact upload descriptors, layouts and manifests already use Store-owned driver
records. Workspace operation commands and Time activation snapshots also use their
owners' driver records. Computers maintenance binds typed source, progress and
resume values through its strict storage codec, preserving whole-value comparisons
and native encoding. These qualified cuts do not close the complete bind audit.

The following controlled adapters are implemented and pass their owning native
checks. They belong to the bind audit even where SQL compares only a whole object.
They use owner codecs and declared fields; a generic JSON wrapper does not establish
the known shape. The table records each cut's required behavior.

| Adapter | Required cut and qualification |
|---|---|
| Gateway `state/auth_state.rs` and `state/subscriptions.rs` | Replace generic serialization of authorization requests/codes, JWT revocations and resource subscriptions with typed driver records. Subscription and revocation payloads duplicate their declared columns; use one representation. Preserve OAuth scope and PKCE data, checked Principal snapshots, expiry, atomic one-time consumption and record identity agreement. Store must not depend on MCP to obtain these types. |
| Task Runtime `types.rs::failure_to_open_object` | Give the controlled failure envelope a driver record shared by transition, recovery and webhook settlement writers and checked Rust/Python readers. Keep domain error codes extensible and failure details opaque. Preserve absent versus explicit-null details where the public contract distinguishes them. Qualify Rust/Python writes and reads in both directions. |
| Task Runtime input requests | Replace `TaskInputRecord.request`'s `OpenObject` with a typed driver record for `method` and `params`. Declare the outer database fields and apply method admission on retained reads as well as writes. Parameters and response objects keep their protocol-owned open shape. |
| Frames `state/worlds.rs` and `state/operations.rs` | Bind typed world definitions and operation provenance rather than `OpenObject`. Preserve tree validation, immutable replay, digest and frame-index agreement, and SQL admission before decoding denied rows. |
| Computers command output and file adapters | Replace generic object conversions for sealed output/file access, effective file limits and completed file results with owner adapters. Ciphertext remains opaque inside its typed envelope. Preserve full-value compare-and-set checks, nonce/key fields, capability deadlines and Task acknowledgement identity. |
| Agent runtime and kernel wake producers | Model the five known `WakeKind` payload envelopes in the owning runtime and bind their driver records. Replace controlled string-key extraction in conversation and priority handling with checked variants. Preserve task-result and input-response payloads whose contents belong to a tool or model, wake deduplication, lineage, coalescing and recovery. |
| Agent deferred tools | Type the known `DeferredToolDescriptor` storage envelope and the first-party `agent_task.result` success/error delivery variants. Keep backend-specific descriptor contents and tool outputs open. Qualify incomplete-descriptor recovery, result delivery and retained reads. |
| Agent readiness, episode bindings and definition receipts | Declare the controlled `agent.managed_ready` and `agent_episode.managed` fields. The instance mutation query reads `managed.instance`; preserve that stop predicate through declared fields. Close the known definition receipt envelope while retaining its checked replay decoder and template-owned parameters. |
| Workspace Agent revision receipts | Close the retained `WorkspaceAgent` result envelope and check its identity and relationships on add/adopt replay. Reuse the owning record rather than introducing another result model. |
| Media `state.rs::prediction_payload` | Preserve the known prediction envelope through a typed owner driver adapter. Provider input and timing payloads remain open, and unrecognized provider statuses must remain nonterminal. Qualify stored prediction round trips without inventing a schema for model-specific data. |

The current batch supplies five private Task contribution driver envelopes: Map's
retained travel-model input and successful result, Media's retained request and
successful result, and Reason's retained successful result. Preserve whole-value
comparisons, including absent/null distinctions. Map keeps the original admitted
request; Media keeps its existing normalized `RunArgs` representation across
creation, dispatch, provider association and reads. Successful-result snapshots
preserve the complete admitted MCP envelope. The shared Task contribution remains
an extension boundary; core must not enumerate these owner types.
Use owner-local nominal records for these snapshots. Map's input and each retained
result keep an immutable original JSON value alongside the typed interpretation;
a fallible constructor admits both together, and accessors expose the typed value.
Strict native decoding uses that same admission. Encoding preserves the original
value, including admitted omission and null differences; reserializing a parsed
request can insert defaults and change equality. Media's request adapter instead
preserves its existing normalization. Share native JSON conversion mechanics and
keep contextual Task, product and provenance checks in their current owners.

Knowledge generation requirements and chunk rows, and Audit append/indexing rows,
now use owner driver records. Their native record links and transactional checks
keep the existing storage profiles. Canonical JSON text and explicitly open metadata
remain separate from these controlled envelopes.
Checkpoint `d75a5acea` qualifies Audit's sealing records, export values and lookup
adapters together with Gateway's control-plane snapshot codec. Eleven owning native
checks, strict all-target lint and independent review pass. Audit preserves frozen
block/checkpoint values, hash spellings, native identities, timestamps and seal
fencing. Gateway keeps registered extension documents open and preserves snapshot
admission and revision hashes without adding a Store dependency on MCP or Gateway.

Checkpoint `e2211a9bf` qualifies Media's cancellation receipt adapter, including its
JSON timestamp/outcome profile and late authenticated callbacks after cancellation.
It keeps the shared Task journal domain-neutral and preserves
provider correlation. Its `media_task` receipt follows Task deletion; capability
contexts and usage rows keep independent creation and expiry rules.

The final source pass reconciles 1,799 production database-binding call sites and
helper calls. Typed scalars and collections, accepted nominal records and genuinely
opaque extension/provider values retain their classifications. This is a call-site
inventory, not constructor-level proof for every value supplied through a helper.
It identified the following final nominal-adapter families:

| Owner | Final nominal adapter |
|---|---|
| Audit | Export block, payload and checkpoint query values use existing frozen-document codecs; view and page target/detail filters use their existing owner codecs. Closed rejection/class/outcome scalars do not need object wrappers. |
| Gateway | A Gateway-owned control-plane snapshot codec replaces the generic write object through a generic Store payload or local write record. Registered extension documents remain open; current read admission and revision hashes stay unchanged. |
| Python Tasks | Owner-native constructors replace repeated content, idempotency, input, request and authority dictionaries. Preserve native RecordID/datetime values, unsigned values, null/absence distinctions and sorted authority sets. |

These changes enforce adapter consistency; the review established no authorization
or data-corruption defect in the prior adapters. They require no new database schema.
Store must not acquire MCP
or Gateway runtime dependencies to type an owner snapshot.

Checkpoint `42d98f00b` qualifies the Python repair. Independent review, 111 focused
codec cases and 70 live Task storage/runtime cases pass, including Rust/Python
interoperability, native SDK encoding and rejection of native values inside JSON
payloads. Native record references, timestamps and full-value comparison predicates
preserve their existing profiles.

Gateway's storage cut moves the required OAuth and PKCE scalar declarations and
their admission rules into `platform/gateway/contract`. Store imports those types
without MCP runtime dependencies. Reuse the existing checked Principal storage
model across authorization codes and refresh families, giving shared records names
that describe both uses. Update the adapters together and preserve their published
spellings. The MCP policy and runtime DTOs do not need to move to qualify these
driver records.

### Migration And Gates

Fresh bootstrap tests write through current producers and read every affected table
through current consumers. Define `task.owner_context`, the closed request envelope
and the lookup columns in owner lanes, then update every Rust and Python writer,
query, index and typed driver record.
Current-format restart and transactional rollback tests remain required.

Review the Computers missing-value predicates with their policy owner in this phase.
An absent required authority field denies; a legitimately optional label has its
explicit policy semantics. Schema strictness alone does not prove admission safety.
Exercise malformed and foreign rows, revocation, limits and competing transactions.

Move remaining relationship candidates to `record<table>` with `REFERENCE … ON DELETE`
where the database can enforce the intended behavior. Test each adopted cleanup rule;
record why a rejected candidate needs owner-managed lifecycle instead.

Source review counts 356 record-bearing field declarations, including 21 explicit
reference cleanup/rejection clauses; generated Knowledge chunk links are additional.
Existing cleanup, independent products and owner-managed retention keep their prior
qualification. The final six families have the following source-reviewed lifetimes,
documented beside their owners:

| Relationship | Lifetime and parent-delete behavior |
|---|---|
| `gateway_task_route.source_task` | Retain the route as a retry fence after Task pruning or route expiry. Access still checks expiry and current authority. |
| `coordinate_operation.task` | Retain immutable operations. Missing Task parents deny reads and writes; direct operations have independent authority. |
| `uav_mission_execution.{task,plan,lease}` | Retain execution identity for reconciliation and physical-work fencing. Missing plans do not release Task pins. |
| Artifact upload parts and publication links | Retain admission/publication receipts. Recovery removes orphaned bytes under leases and generation fences; published products have independent lifetimes. |
| `recording_projection_receipt.grant` | Receipt expiry cannot exceed grant expiry. One cleanup transaction removes expired receipts before grants; a missing grant denies access. |
| Knowledge generation/member/sync collection links | Removed registration denies access without deleting generations or unrelated collections. Coordinator-fenced reclamation owns indexed-data cleanup. |

This review requires no additional cascade or deletion rejection. Existing owner
tests cover parent admission, retention fences and cleanup transactions. Source
classification does not supply a new native or installed test result.

| Gate | Pass condition |
|---|---|
| Suites | Store, task-runtime, computers, gateway and module suites against real SurrealDB |
| Negative write tests | Direct database writes reject undeclared keys and missing required keys in each closed object; valid typed producers and opaque provider inputs still work |
| UNIQUE tests | Managed-agent duplicates fail |
| Path audit | No SurrealQL path reads inside a FLEXIBLE field or decodes JSON text |
| Bind audit | Every controlled `.bind` value uses an owner driver record or typed scalar/collection; opaque provider payloads remain explicitly classified |
| Admission and cleanup | Missing required authority denies; filtering precedes limits; current-format recovery, rollback and each adopted reference cleanup rule pass |

## Phase 5: Inbound Strictness

Of 1,553 schema-bearing `Deserialize` types, 834 lack `deny_unknown_fields`. This
inventory requires classification: it includes external and open-ended
formats as well as controlled inputs. Phase 5 closes undeclared keys on controlled
inbound shapes and checks schema/decoder agreement.

The MCP source batch covers the sixteen Rust server contracts and Rust template.
The source inventory found 121 distinct input roots across those owners and SUMO:
70 needed closure, while 51 already used strict decoders. The independent consumer
checks the 114 production root schemas. Existing owner tests pair rejection with
valid inputs and preserve provider payloads, authored properties and typed dictionaries.
Strict wire adapters preserve Artifact's flat sharing request and public tagged unit
variants. Map's recursive CQL2 schema now describes its typed expressions directly.

Shared conformance checks reachable controlled objects through local references and
composition, with explicit traversal limits. The Rust template proves that an unknown
argument produces a completed `isError` response through the hosted gateway.
All sixteen production Rust servers now qualify that response for representative tools
through authenticated native HTTP fixtures. Valid DTO controls and owner state checks
separate malformed arguments from missing required values and side effects. Unavailable
provider and GPU handles isolate admission; these fixtures do not qualify execution.
The expanded variant matrix below passes local qualification at `91357aeca`.
The coordinated installation cut carries the stricter decoders and matching callers together.

The source matrix contains 447 cases across 112 owner-local families. Its explicit
[consumer inventory](../testing/fixtures/server-contract-consumer/tests/controlled_inputs/owners.rs)
registers every fixture and its decoded owner type:

| Owner | Cases | Owner | Cases |
|---|---:|---|---:|
| Artifact | 8 | Reason | 5 |
| Computers | 7 | Recording | 5 |
| DuckDB | 26 | Speech | 2 |
| Frames | 21 | Stream | 3 |
| Knowledge | 2 | Time | 30 |
| Map | 240 | Timeseries | 24 |
| Media | 1 | UAV | 23 |
| Optimization | 34 | View | 16 |

The original 52 cases remain qualified and unchanged. Independent source review
accepts the expanded matrix. All sixteen owning hosted tests and both independent consumer
profiles pass. Recording's binary target is built with its required `redap` feature.
The independent consumer
checks schemas and serialized-byte decoding in both feature profiles. Existing
hosted tests reuse the cases to reject unknown fields, tags and missing required
values with completed tool errors and unchanged domain state. Untagged inputs
compare decoded values, with explicit CQL and Timeseries alternative assertions.
Fixtures preserve omitted defaults, admitted open feature/provider payloads and
typed dictionaries. Scalar URI routes, outputs and administrative-only types retain
their separate owner checks. These cases establish argument admission; they do not
qualify domain execution or installed behavior.

| Inbound surface | Change |
|---|---|
| MCP tool inputs (`Parameters<T>` types in every server) | `deny_unknown_fields`; schemas state `additionalProperties: false`; an unknown argument returns RMCP's completed `isError: true` result |
| HTTP request bodies: gateway admin, artifact service, Console BFF | `deny_unknown_fields`; unknown keys return 400 with the field name |
| Protocols between Veoveo processes: Map helper, cuOpt executor, reason and speech runners, stream `gst-runner`, UAV runtime adapter | Strict on both sides; Python peers use pydantic `extra="forbid"` |
| Configuration files and environment JSON | `deny_unknown_fields` on every loader type |

The qualified native batch covers HTTP bodies, runtime configuration and the private
Map, cuOpt, Reason, Speech, Stream and UAV process interfaces. HTTP consumers share
`platform/http::RequestJson`; their contract-only features exclude it. Configuration
admission keeps gateway module sections registry-driven and closes each registered
owner's controlled values. Python peer models use the locked Pydantic 2.13.5 profile.
Affected Rust suites, Python peer and SDK tests, browser tests and builds, generated
types and workspace lint pass. The image recipes include the matching locked Python
dependencies. Final image and installed process qualification remain open; the
simulation Python-layer update needs composed hardware acceptance.

Recording Hub's `sensor-sim --stack` loader admits each flat sensor variant through a
closed typed decoder. Nested wave, track and coordinate values reject extra fields;
sensor IDs run constructor validation during decoding. Native tests cover every variant,
required fields, optional duration defaults and file-loader diagnostics.
The UAV overlay validates typed state, acknowledgements, completion results and lifecycle
events before HTTP or NDJSON serialization. Shared fixtures cover Python emission and
Rust decoding, including all seven camera rigs. Output errors expose validation kinds
without payload values; unresolved Recording keys stay separate from physical completion.
The [Phase 6 schema comparisons](#phase-6-generated-cross-language-types) qualify five
complete private JSON protocol graphs. Stream's C++
decoder passes its native build; installed runner checks remain open.
`AccessSubject` now reports undeclared map keys through the shared HTTP extractor while
preserving its adjacent-tag sequence profile. Native checks cover both subject kinds,
binary tags and value redaction. Malformed tag and identity values keep a generic 400.

Types that read upstream provider responses accept additions: WaveSpeed, OAuth and
OIDC discovery, Kubernetes, Valhalla, ntpd-rs, cargo metadata and buildx. serde
cannot combine `deny_unknown_fields` with `#[serde(flatten)]`, so each flattened
inbound type moves to explicit fields or a validated `try_from` wire struct.

| Gate | Pass condition |
|---|---|
| Server suites | Each server has a test that an unknown tool argument returns `isError` |
| Conformance | Controlled tool-input objects state `additionalProperties: false`; explicitly opaque maps preserve their declared schema and are tested separately |
| Consumer suites | Console, Workspace, Python and Node suites pass, proving in-repo clients send exact shapes |

Known domain IDs stay typed through internal APIs, query inputs and consumers. Finish
the F-row DTO relationships alongside strict decoding rather than adding wrappers
without caller adoption. TaskRuntime exposes typed request/result parameters for
owner-known payloads, while shared transport may retain honestly heterogeneous data.
Domain adapters decode those payloads before use. Classify each raw JSON occurrence;
a count of `Value` fields is not proof that every one can share a closed schema.

## Phase 6: Generated Cross-Language Types

Browser generation covers agent management, audit, computers, Console, speech,
Workspace, agent control, Artifact transfer, Recording playback, App catalog and
cluster inventory. Each bundle comes from its Rust owner. The BFF exposes its
installation DTOs through a contract-only library feature. Gateway App import and
discovery DTOs belong to `platform/gateway/contract`, allowing browser contracts to
exclude MCP runtime dependencies. Both browser clients consume these owner types;
browser presentation models and selected-file requirements stay local.

The BFF contract excludes MCP, HTTP, async runtime and database dependencies.
Installation snapshots and upload notifications use generated owner schemas;
their browser consumers pass. Snapshot and row-event contracts include every
Task recovery class, including `provider_wait`, and tie each entity to its row
schema. Browser upload phases and file state remain presentation models.
The wider F-register still governs owner DTO relationships and installed consumers.

The BFF's existing contract feature owns the nineteen installation snapshot and
summary structs. Gateway composition keeps projection,
authorization and replay behavior and imports those declarations. Bootstrap and
server-health declarations belong in the lightweight Gateway contract. That crate
cannot aggregate Agent summaries because the Agent contract already depends on it.
Agent lifecycle lives below persistence in the existing Agent contract feature;
Artifact, Recording and access vocabularies come from their existing owners.

Task status and recovery use a small contract crate below Store and Task Runtime.
Importing a Task Runtime feature from Store would create a package cycle. The new
contract's default graph excludes database and service dependencies; Store explicitly
selects its optional native vocabulary adapter. The existing shared derive supplies
that adapter. Qualification must preserve literal database kinds, enum ordinals,
wire spellings and native optional values while removing the duplicated recovery
declaration. This ownership change does not alter Task lifecycle or stored values.

One typed event model ties each existing SSE entity name to its row schema. The
browser validates rows before changing its cache, including nested summaries and
operation tags. Upload notifications join their Artifact transfer schema bundle and
keep their current snake_case fields. Preserve required nullable fields separately
from omitted optionals, timestamp serialization and all six emitted upload states.
Browser queue phases and selected-file state stay local.

UAV's private snapshot derives eight endpoint roots from the actual Rust adapter types:
world, command and operation requests, their responses, state and NDJSON events.
Recursive comparison includes private Recording state, unresolved completion keys,
seven command variants and three operation variants. Shared fixtures preserve omitted
fields, explicit nulls, all camera rigs and physical-completion admission. Python's
finite battery range and native Rust decoding qualify the bounded conversion to
`f32`; schema annotations alone do not establish runtime range validation.

Map normalization, cuOpt execution, Reason inference, Speech worker messages and UAV
have complete private Rust schema snapshots. The shared Python checker follows their
reachable graphs in each wire direction: Rust serialization to Python validation, and
Python serialization to Rust deserialization. The checker
qualifies its supported structural subset, including finite enum-keyed maps and bounded
numeric conversion with exactly representable inclusive endpoints, and rejects
unsupported assertions or compositions. It does not establish general schema equivalence.
Owner types supply actual numeric admission: cuOpt's nonnegative scalar exposes its
existing lower bound, and Reason's private limits use fixed-width integers. Contextual
filesystem, task, source-range and grounding checks remain with their owners. Installed
process and hardware acceptance remain open.

The SDK's normalized Principal uses `PrincipalAssurance` with Rust's closed
vocabulary. Checkpoint `68c9f430c` passes 101 focused Python tests and five Rust
signed-context tests. The shared fixture qualifies nonempty actor and source
assurances alongside empty defaults; unknown normalized values reject. External
JWT claim parsing keeps its separate normalization step.

The controlled-input batch has completed source review and native qualification.
The current source integration is grouped into three batches. Browser generation covers
installation, Task, Artifact/grant, Agent and policy projections and upload SSE.
Agent built-in tools derive their schemas from their implemented owner types.
The separate memory plan owns the behavioral replacement described below.
MCP App consumers use owner contracts for results,
continuations and errors. The already-qualified private protocol graphs require
their image and hardware checks in Phase 10, rather than another source migration.

| Consumer | Change |
|---|---|
| Console and Workspace | New schema bundles in `tools/xtask/src/commands/client_types/mod.rs` for every hand-mirrored contract; hand-written interfaces become imports from `src/generated`; `npm run build` then type-checks every field access |
| Python peers of the Map helper, cuOpt executor, reason and speech runners and the UAV runtime | pydantic models with `extra="forbid"`; one test per protocol compares the model's JSON Schema with the Rust schema snapshot |
| MCP App assets (View, Stream, Timeseries, UAV, Map and shared workbench) | Use owner-generated contracts and runtime admission for controlled results, continuations and errors. Keep shared workbench domain payloads open. Qualify Charts against its pinned upstream packaged contract rather than inventing a Rust owner; phase 8’s literal scanner supplements runtime behavior checks |
| Agent kernel built-in tools | Derive controlled schemas from owner types and qualify the invoking clients for the implemented `memory_query`, `memory_write`, `timeline_query` and `resource_read` tools. The separate [agent memory plan](AGENT_MEMORY_PLAN.md) owns their later memory-interface replacement |
| SDKs, templates and cross-server clients | Complete the wider owner-local types and builders in the F-register, including catalog continuation and current-format result consumers |

| Gate | Pass condition |
|---|---|
| Client types | `cargo xtask release client-types --check` |
| Console and Workspace | Both clients run `npm test`, `npm run build` and `npm run lint` |
| Python | Protocol schema comparison tests and each package's suite |

Kernel schema generation preserves the current tool behavior and removes the
handwritten copies of their argument schemas. It qualifies defaults, tagged write
variants, unknown-field rejection and explicitly open SQL rows. The agent memory
plan separately delivers `memory_sql`, agent-owned tables, memory outlines and context
views. Its replacement tool must derive its schema through the same owner mechanism;
this plan does not declare those behavioral changes delivered by typing today's tools.

## Phase 7: Embedding Profiles And Identity

The implemented `EmbeddingSpace` uses
model, checkpoint revision, dimension, pooling, normalization, numeric precision and
maximum input tokens as its vector-space identity. Query instruction and chunker version stay in
the Knowledge generation specification. Runtime image, vLLM version, GPU and driver
remain recorded as execution provenance.

The source architecture review accepts that separation subject to the following
admission and provenance requirements. The embedding contract owns the space and
execution-profile types. A qualified profile binds one space to the effective serving
configuration, immutable image and checkpoint inputs, and the qualified GPU and driver
environment. Installation configuration selects that profile and its endpoint.
Client construction, indexing and search require an admitted profile; equal space
values alone cannot authorize runtime reuse. Missing or mismatched profiles fail with
a configuration diagnostic before vectors enter an index or a search.

The installation's pinned deployment and checkpoint verification establish those
claims. `/v1/models` establishes only the advertised model name. It does not attest
the checkpoint, image, pooling, precision or hardware. Keep that trust limitation
explicit and qualify the deployed configuration through the existing GPU harnesses;
this change adds no attestation service or serving sidecar.

Initial qualification cannot require the bundle it is meant to produce. The existing
verification harnesses collect candidate measurements from an explicit measured
execution profile through a verification-only transport. It shares the production
protocol validators, deadlines, request budgets and secret handling, but cannot act
as the production embedding provider. Candidate measurements never activate a
production generation or manufacture passing qualification receipts.

Use the existing owner-typed corpus, query task and chunker for GPU reference and
vector-retrieval comparison. Record that report as runtime compatibility measurement;
it does not qualify Knowledge's SQL or hybrid search. Passing reference, retrieval,
scheduling and capacity reports can establish the first checked runtime bundle.
Then run the existing full GPU Knowledge retrieval, rebuild and concurrent-search
workload through the production client with that bundle before the installation
selects it. Preserve corpus fingerprints and all consumer acceptance gates across
both steps. Source and synthetic checks cannot replace either hardware step.

The pooling runner's successful embedding response does not establish CUDA graph
execution. Both vLLM 0.30 and 0.31 pooling output constructors drop graph statistics
before the logger, even with the reporting option enabled. Qualification observes a
real request through vLLM's maintained Proton graph-attribution profiler in an
isolated local diagnostic run. The production chart omits profiling because the
provider's API-key middleware does not protect its profiling controls. Capacity,
vector-quality and production Knowledge measurements use profiling-disabled serving
processes. Diagnostic and measurement runs must agree on their image, checkpoint and
serving settings, with each process's facts recorded. Local hardware qualification
passes for the selected pinned 0.31 FP16 profile. Preserve previous reports;
they cannot establish another image or execution profile.

For the reference installation's selected 0.6B profile, declare these acceptance
limits before the new hardware run. Candidate vector ranking requires mean recall
at ten of at least 0.95 and no judged recall loss against the CUDA reference.
The full production workload uses the same report-bound 0.95 minimum and must
additionally retain every judged relevant member in the first ten results for all
78 cases, giving mean recall 1.0. Inspect its per-case results; candidate ranking
does not establish hybrid retrieval quality. The existing six-batch scheduling
fixture requires at least 250 inputs per second, an interactive embedding response
within 250 milliseconds and completion ahead of at least four bulk batches.
These are qualification limits for that fixture, not an installed search latency
objective. Preserve failed reports and investigate failures rather than lowering
the limits or changing judgments after measurement.

Knowledge records which qualified execution profile produced each indexed batch and
retains its immutable provenance alongside the data. Publish those associations in
the existing owner transaction, so committed vectors cannot lack their producer
record. A profile ID must identify immutable contents; conflicting contents reject.
A runtime change cannot relabel stored vectors. Execution provenance stays
outside the generation fingerprint, whose query instruction, chunker and collection
requirements still apply. Reuse requires qualification of the selected query runtime
against the runtimes that produced the retained vectors, not just separate successful
self-tests of each runtime. Admission covers every retained producer profile and
preserves that condition across concurrent batch publication and generation changes.
The first matrix admits only the qualified NVIDIA profile;
later image, configuration or hardware combinations need their own qualification.

Qualify this change on the existing NVIDIA profile. Reference vectors must meet the
existing 0.999 cosine threshold, and the existing retrieval harness must demonstrate
the declared retrieval behavior. A small reference set alone does not prove that
arbitrary runtime upgrades preserve a space. Define the supported runtime/configuration
matrix and negative cases in the owning design. A failed qualification rejects reuse
or creates an explicitly distinct space; do not disguise drift by inventing a changed
setting. Quantized and unquantized checkpoints remain different spaces.

| Work | Gate |
|---|---|
| Contract and consumers | Update embedding, Knowledge, generated schemas and Helm; all consumers agree on the new identity and retained execution provenance |
| Space reuse | Qualify supported query/index runtime combinations with reference vectors and retrieval checks; unchanged qualified spaces preserve the active generation, incompatible or unknown combinations cannot reuse it |
| Provenance and refusal | Producer associations survive restart and same-space runtime changes; missing, altered or unqualified runtime/configuration entries reject before indexing or search. Native fixtures prove these paths without claiming GPU qualification |
| Current generation | Fresh-state cut activates one generation under the final identity; avoid an unnecessary intermediate production reindex before phase 8 |
| GPU and capacity | Required NVIDIA resource, CUDA refusal, readiness, declared priority/bulk bound, throughput and search latency pass on hardware |
| Network and Helm | Installed CNI selectors and Embedding key admission pass the focused same-Service-IP controls; actual Computer Host workload isolation stays open. `veoveo-deployment-smoke` Knowledge/embedding Helm suites pass. The explicit shared-runtime cold-start profile passes native admission; its narrow cross-namespace policy and key access still need installed qualification |

The Metal proposal is retained in X1. It requires separate review and Apple hardware;
it does not block this phase or the current NVIDIA acceptance.

## Phase 8: Installation Cut

Phase 8 composes the current owner lanes and naming change into a fresh installation
(D3). All incompatible producer and consumer images deploy together, without rolling
overlap. Native qualification precedes publication; phase 10 qualifies the installation.

### Lane Re-Baseline

Phase 3 moved definitions into owner lanes; phase 4 qualified their declared fields.
Freeze the current composition of append-only owner lanes for the installation
cut. Preserve accepted migration identities and known disabled-owner histories.
Compare two independently fresh bootstraps of the intended current schema;
no old installation is converted or maintained.

| Gate | Pass condition |
|---|---|
| Schema equivalence | Two independent fresh kernel and optional-lane compositions agree on complete table definitions, fields, indexes, references, events, functions and analyzers; sensitivity controls detect changed child definitions |
| Ownership | Every table, function and analyzer has its declared owner; ownership changes fail comparison |

The current nineteen-owner composition passes both native schema controls and all
six independent-consumer controls. Its fresh capture contains 173 tables with
owner assignments, 3,745 fields, 342 indexes, one event, 39 functions and two
analyzers. Native field and index mutations fail equality; separate controls cover
event, function, analyzer and ownership changes. These checks qualify fresh source
composition, while installed lifecycle acceptance stays open.

### JSON Naming Cut

The inventory found 1,241 serde types with multi-word snake_case keys, kebab-case
values or conflicting attributes: 703 wire, 118 external, 206 database, 44 internal
and 170 mixed. D1 moves most mixed types into the exempt identity family.

Naming applies to serialized roles, rather than every string or object key. Typed
dictionary keys represent vocabulary values or identities; they are not DTO field
names. Scope and Task vocabularies can contain colon, dot and hyphen spellings under
their existing admission profiles. Resource identities, extension keys and format
tags follow their declared grammar and D5 versioning. D2 keeps the declared protocol
names separate from tool argument DTO field names.

Current schemas do not distinguish these roles reliably. Scope/Task vocabulary
derives emit unclassified string enums, and finite enum-keyed maps can resemble
closed structs. The cut must emit narrow scalar and dictionary classifications from
their owners and shared declaration mechanics. Generic C33 validation defaults
unclassified controlled enums to snake_case, validates classification structure and
supported grammar, and continues checking nested controlled shapes. External, JWT
and frozen-format exceptions identify their precise profile and subtree; an
identity annotation cannot exempt an enclosing DTO. Source qualification establishes
the truth of an exception that remote schema inspection cannot prove. Adversarial
controls reject unknown profiles, invalid Scope/Task spellings, incorrect key roles
and annotations that hide unrelated fields. No core domain registry is introduced.

C33 inspects complete remote discovery catalogs, advertised tool input and output
schemas, and adopted extension metadata. Owners supply typed generated-schema evidence
and safe observations for payloads outside that discovery surface. Reports distinguish
remote schemas, owner-generated schemas with observed values, and source-only checks.
Resource and prompt descriptors need no new public body-schema protocol for this cut.
Each owner's producer, decoder, consumer and installed checks still qualify its required
payload families. C33 uses mixed verification: remote checks validate structure and
built-in grammar, while source qualification establishes the truth of owner, JWT,
frozen and external declarations. Missing required evidence cannot pass as a skipped
check or an empty traversal.

C33 admits DTO field names with `[a-z][A-Za-z0-9]*` and ordinary controlled string
values and D2 identifiers with `[a-z][a-z0-9]*(?:_[a-z0-9]+)*`, matching the whole
string. Protocol fields such as `_meta` belong to their declared envelope positions.
Gateway tool inspection resolves the typed server/local projection before checking
the local name; prompt discovery preserves its existing unqualified names. Human
titles, descriptions and completion values do not become controlled identifiers.
References carry their schema location and naming context through traversal, including
the generator's declared definitions container. Schema and dictionary-name containers
must stay distinct when a definition happens to be named `id` or `$id`.

| Class | Casing after the cut | Members |
|---|---|---|
| Wire | camelCase keys, snake_case values | Tool inputs and outputs of every Veoveo server, resource bodies, HTTP APIs, SSE events, runner and helper protocols, cross-component artifacts, configuration files, `CallToolResult` structured content |
| Stored copy of a wire document | Wire shape | Task request tool arguments, task results, artifact metadata, control-plane and knowledge documents, Map, Time and UAV `canonical_json`, Frames definitions, agent manifests |
| Internal record | snake_case, unchanged | `SurrealValue` records, phase 4 declared fields, columns and lookup tables, Store `TaskRequestRecord` and `TaskOwnerRecord`, runtime `TaskOwner`, server durable task envelopes, computers bindings and sealed envelopes, BFF cookies, the recording forwarder queue |
| JWT claim format | snake_case, unchanged (D1) | The identity family and the gateway token claim structs |
| Frozen | unchanged (D4) | `AuditRecord`, `AuditDraftWire`, `AuditBlock`, `AuditBlockHead`, `AuditCheckpoint`, audit export configuration and payloads |
| External | upstream spelling | 118 types: OAuth and OIDC bodies, OCSF, Kubernetes, S3 XML, GeoJSON and CQL2, WaveSpeed, Valhalla, ntpd-rs, Docker, k3d, Helm, cargo metadata, buildx, Rerun, OpenShell |

A type stored through `#[derive(SurrealValue)]` can change its serde casing for wire
use without changing its record shape. Phase 4 moved every queried value into declared
columns and lookup tables. Existing columns and SurrealQL paths keep their names;
the writers read typed Rust fields, which the compiler checks. Artifact's changed
write-request preimage additionally requires an owner-defined request-format field
in reservation and redemption state. New writes bind the typed current format, and
every receiver admits it before rebind, redemption or effects. Missing or unsupported
markers refuse without filling them in; valid current-format content changes keep
their existing pending-reservation rebind behavior. The owner schema and current
recovery controls qualify this addition alongside the fresh-state drain. Artifact's
accepted migration 0 stays unchanged; migration 1 adds the required marker without
a default or backfill. The `read_v1` API keeps its migration-0 prerequisite. Artifact
startup must admit the installation identity, completed preparation and compiled
prerequisite lanes, including both Artifact migrations, before object-store effects,
recovery or HTTP binding. Reuse the runner's read-only prerequisite API; the Artifact
process does not prepare or migrate the installation.

The sealed-properties JSON inside Recording's deterministic RRD layer keeps its
native field names and hash preimage. Its schema-only `Frozen` naming declaration
identifies that specific RRD 0.38.1 representation; it does not exempt public
Recording views or expand the audit formats frozen by D4. Qualification must prove
the declaration's scope and unchanged properties bytes through the owning producer
and decoder. Projection unit maps declare dictionary keys using the selected Rerun
component schema from the same generator as their value schema.

Recording must persist its closed manifest publication intent in its existing owner
lane before calling Artifact. That intent binds the original source revisions, seal
time, reserved occurrence and complete publication bytes. Recovery validates the
selected Artifact's actual body and metadata against the intent before staging or
completing the seal. A changed dataset revision cannot replace the original hash
input. Reservation must check the complete selected layer set and Blueprint in one
transaction. Selected-record `FOR UPDATE` reads detect concurrent changes at commit;
new source selections must advance the existing Recording revision transactionally
because predicate scans do not lock future members. Source mutations also check the
deterministic intent identity with `FOR UPDATE`, including when it is absent. Native
controls must cover both commit orders before claiming a frozen publication snapshot.
The persisted selection freezes its member layers. Later Derived layers that the
sealed lifecycle permits remain outside that selection and can appear in catalog
and playback reads. Retained manifest and Properties admission must match every
selected member and reject unselected non-Derived layers. SQL selects typed member
IDs before pagination and checks disallowed extras across the parent; a general
catalog page cannot establish complete source admission. Later Derived work cannot
change the manifest bytes, source digest or original seal time. Qualify its complete
layer lifecycle and retained reads alongside selected-member refusal.
Local staging files derive from that intent and cannot establish remote
failure or authorize another occurrence. The appended owner migration must preserve
accepted migration identities and refuse absent or unsupported intent on recovery.
The native retained-seal control qualifies these refusals and both source-race commit
orders. Final generated and installed receivers still need qualification.
Properties preparation must likewise persist its complete checked preimage in the
existing properties-layer row before file effects. Writing and Staged retries use
that snapshot's original source epoch and seal time. Advancing a source revision
cannot regenerate their bytes from the Recording's mutable fields. Missing or
contradictory preparation refuses; no historical reconstruction or timestamp-field
overload supplies the missing fact. The owner migration and native recovery controls
must qualify this addition alongside manifest publication.

Console's Recording timestamp adapter preserves the original wire string and orders
whole UTC seconds with Chrono's nanosecond and leap-second extension. Date supplies
Gregorian calendar validation without deciding chronology. The owner schema declares
Chrono's extended-year and offset profile rather than the narrower generic date-time
validator. The actual 27-case Rust producer capture, generated schema and browser
receiver qualify ordering within one millisecond, leap seconds, range limits and
omitted/null end times. The shared schema carrier also serves Artifact's public
timestamps; that owner requires its own final schema and decoder qualification.

| Format | Handling |
|---|---|
| Audit chain (RFC 8785 and Ed25519), audit export destination IDs and seals, audit checkpoint files | Frozen (D4) |
| Public structured Audit query cursor | Unversioned keyset uses `lastId`; retire `last_id` across the owner and actual Gateway/Console readers under the coordinated drain. Signed formats stay frozen |
| Computers AEAD associated data, HMAC inputs and sealed plaintexts; BFF cookies; forwarder queue `stream.json` | Internal, unchanged |
| Internal gateway JWT and gateway OAuth access token claims | JWT format, unchanged (D1) |
| Gateway control-plane SHA-256, Frames `spec_digest`, knowledge registration and member revisions, `runtime_template_revision`, View composition digests, Map request digests, Recording source-snapshot SHA-256 | Recomputed after the cut |
| Artifact write-request hash domain `veoveo.artifact-write-request.v1` | `veoveo.ai/artifact-write-request/v2`; the serialized owned request grammar changes. Preserve NUL framing and bare blob digest, and recompute reservation, rebind, redemption and retained receipt bindings |
| Hosted MCP contract revision 3, `veoveo.ai/hosted-mcp/v3`; requirement catalog revision 1 | Revision 4 / `veoveo.ai/hosted-mcp/v4`; catalog revision 2 with C33; registrations and all declarations change together |
| Hosted-server conformance profile v1 | v2; its admitted hosted-revision grammar advances from v3 to v4 |
| Development image lock v1 | v2; tagged origin payload fields change alongside the already camelCase root |
| Independent fork fixture state v2 | v2; emitted keys and values already use the current profile. Public decoding rejects snake_case aliases and closes markers; bind the changed schema and package identities |
| Recording manifest v9 | v10 |
| Playback manifest v10 | v11, coordinated with the Console BFF |
| Reason and stream results, speech transcript, travel-model artifact, optimization problem and solution documents | Next version with a `veoveo.ai/<name>/v<N>` tag |
| Media generation result `veoveo.ai/media-generation/v1` | v2; change the actual publisher, retained Task snapshot and public read/Task consumers together, preserving provider-native registry and response payloads |
| cuOpt executor `veoveo.ai/cuopt-executor/v1`, reason runner request v3, speech worker v1, Map helper `schema_version: 1` | Next version, both sides in the same commit |
| Computer regular-file header `veoveo.ai/computer-files/v1` | v2 with numeric header version 2 and export `maximumBytes`; drain file operations and select a rebuilt matched helper template. Execution protocol version 1 is unchanged |
| UAV acceptance scenario v12, deployment v8, deployment lock v8, component mutation plan v2 | v13, v9, v9 and v3; `deploy/contract/src/decoding.rs` rejects the old versions with an upgrade diagnostic |
| UAV private control HTTP `/v1/state`, `/v1/world`, `/v1/commands`, `/v1/operations`; `veoveo.ai/uav-world-publication/v1` | Control routes advance to `/v2` on both peers; world publication receipt advances to v2 with the changed binding body |
| Owner-defined opaque continuations | Advance affected envelope versions; qualify every codec and receiving validator |
| Map source-feature query domain `veoveo.ai/map/source-feature-query/v2` | v3 with renamed cursor fields and recomputed admitted-request digest |
| Console `localStorage` key `veoveo.uploads.v1` | `veoveo.uploads.v2` |
| Offline bundle and `images.lock.json` | `schema_version` 2 |
| Map analytics DuckDB | `SCHEMA_VERSION` 12, rebuilt |
| Installation pins in `examples/bioma` | Regenerated: `controlPlaneRevision`, `knowledge.configurationRevision`, `computers.configurationRevision`, `host.configurationRevision`, the agent template `config_map` and `config_digest`, the UAV world `contentSha256` |

The table is a starting inventory. D5 also requires a version bump for every other
changed versioned format, including Recording projection handles and catalog grants.
Refresh that inventory against the final producer and consumer graph before the cut.
Recording video selections and source snapshots change in their owning public contract.
Stream and Reason reuse those models. The snapshot's SHA-256 covers its ordered wire
JSON, so the renamed fields change the digest input. Producers and receiving validators
must agree on the recomputed digest; native reader records and source blob bytes keep
their existing profiles.
The current source census locates 35 `CursorCodec` implementations, including three
shared test fixtures; generic codecs can serve several concrete collection profiles.
Other continuation implementations and consumers require their own review. Inspect
each codec's actual bytes. The unversioned authoring continuation carries only admitted
feature-ID bytes and needs no casing change. Preserve each encoding, parent and query
context when advancing a changed format.

The materialized format review records producers, receiving validators and digest
preimages, including the smoke and process formats, Bioma's Candidate receipts and
Phase 7's Embedding contract and consumers. Independent review accepts this source
closure and its repair receipts; the current scope is recorded under Current Status.
Private diagnostics do not establish a passing test result. Unchanged camelCase
formats need no artificial version advance, and schema or package identity changes
are classified separately from instance-byte changes.

Final image and installed qualification remain open. Verify the simulation-overlay
identity files through their Docker COPY routes and receiving probes. Recompute
reports bound to changed inputs through their owning tools and qualify the installed
receivers. Historical hardware reports cannot qualify new image bytes.

Preserve the implemented `veoveo.ai/gateway-internal-assertion/v2` and
`veoveo.ai/gateway-request-context/v2` formats. Include every Rust and Python producer
and receiver in the coordinated image and drain closure. The JWT casing exemption
does not allow an old installed peer to survive the cut.

The naming cut runs in four waves. Partitions identify ownership and may proceed
independently when their dependencies permit; they do not require multiple agents. Sizes count types, literal sites and files from the
inventory.

Wave 1 covers shared contracts. The serial core runs in dependency order.

| Crate | Work | Size |
|---|---|---|
| `platform/types` | Identity family keeps snake_case with explicit `rename_all` and a JWT-format doc line; other wire types camelCase | S |
| `mcp/knowledge-extension` | Kebab values `work-context`, `selected-work-context`, `selected-work-context-members`, `subjects-in-context` become snake_case | S |
| `platform/artifacts/contract` | `ArtifactMetadataWire` and the other wire types camelCase; provenance per D1a | S, wide reach |
| `mcp/contract` | Control plane and every configuration type camelCase, including `OutputPolicyConfig` and the approval types; `bootstrap.rs`, `deployment.rs`, `usage.rs`, `docs.rs` `contract_revision`, agent templates and models; JWT claim structs unchanged; `product_result` reads `resultUri` | L |
| `platform/computers/contract` | Remove the nine `result_uri` field renames | S |

These have independent ownership after their shared interfaces settle.

| Crate | Work | Size |
|---|---|---|
| `platform/audit/contract` | Freeze with explicit casing; own copies of embedded knowledge enums | S |
| `platform/knowledge/contract` | `catalog-only` becomes `catalog_only` | S |
| `deploy/contract` | 15 kebab enums become snake_case; version bumps from the format table | S–M |
| `platform/frames/contract` | Wire types camelCase | M |
| `platform/recordings/contract` | Manifest v10, playback manifest v11 | M |
| Map contract module | Routing, authoring and catalog contract types camelCase; 11 enums gain `rename_all_fields`; GeoJSON and CQL2 types stay external | XL; lands early because Frames, View, UAV, Optimization and Knowledge embed it |

Wave 2 covers kernel services and platform modules.

| Partition | Work | Size |
|---|---|---|
| Kernel store, task runtime and Knowledge | Column and lookup writers follow the renamed fields; Knowledge catalog wire types | S–M |
| Gateway and policy | Admin and SSE wire types camelCase; control-plane store and hash follow; console projection literals | S–M |
| Artifact service and audit | Upload API wire types camelCase; audit frozen | S |
| Recordings | Hub, RRD and video wire types | M |
| Agents and Workspace | Manifest and configuration camelCase; hand-written tool schemas such as `max_rows`; `ModelConnection` environment JSON | M |
| Console BFF | Wire DTOs camelCase; OAuth types external; cookies internal | S |

Wave 3 covers server modules. Each partition owns its Rust crate, schema snapshots and
fixtures, prompts, tool descriptions and served documents, MCP App assets, and
non-Rust peer.

| Size | Server partitions |
|---|---|
| XL | `map-mcp` in three partitions: routing side, authoring side, and helper IPC with `servers/map-mcp/data` plus the Map Explorer app (rebuild `assets/workspace-app.html`) |
| L | `optimization-mcp` with the cuOpt executor and the travel-model artifact version shared with Map; `time-mcp` |
| M | `uav-sim-mcp` with `showcase/uav-sim/runtime` and scenario v13; `stream-mcp` with `gst-runner` and its catalog; `view-mcp` with `preview-app.template.html`; `reason-mcp` with its runner and catalog; `media-mcp` |
| S–M | `duckdb-mcp`; `timeseries-mcp` with the forecast app |
| S | `artifact-mcp`, `speech-mcp` with its runner, `frames-mcp`, `knowledge-mcp`, `recording-mcp`, `computers-mcp`, `showcase/sumo/sumo-mcp`, `mcp/apps-extension` |

The generated Workbench configuration and its consuming stream hook change together
to `toolName`. Phase 6 admits the current owner spelling before stream effects;
phase 8 changes that spelling across the producer, generated schema and consumer.

Wave 4 covers consumers and installation.

| Partition | Work | Size |
|---|---|---|
| Console and Workspace | Regenerate client types; phase 6 makes stale accesses compile errors; remove the `structuredContent \|\| structured_content` style fallbacks; `localStorage` v2 | M |
| Python | Shared `WireModel` base in the SDK replacing the two existing camel helpers; `populate_by_name` removed; SDK artifact, usage and docs models; `templates/python-mcp`; `testing/fixtures/fork-workload`; `testing/recording-catalog-sdk` | M |
| Node | `servers/chart-mcp` `contract_revision`; `tools/screenshots/capture.mjs` | S |
| Configuration | Seven gateway control-plane files (about 1,850 occurrences); `configs/deployments.json`; Stream and Reason catalogs; `configs/view/layers.json`; agent manifests; `examples/bioma/knowledge/indexing.json`; `examples/bioma/uav-sim-world.json`; the `values.yaml` and `k3d-values.yaml` pass-through sections | L |
| Helm | Agent policy CEL references, `agent-admission.yaml`, `agent-manager.yaml`, `server-bootstrap.yaml`, stream and reason templates, `values.schema.json` (37 entries), map-provider values | M |
| Pins and offline bundle | Every pin in the format table; `deploy/offline` scripts and lock | S |
| Harnesses | Owner smoke sources in Bioma acceptance, Flight, Gateway composition and the other owning components; shared clients in `testing/browser-smoke` and `testing/support`; `mcp/conformance` fixtures and literals; `testing/fixtures/gateway-request-context.json` | M |
| Documents | Served server documents, C33 in every generated owner compliance section, prompts, tool descriptions, `docs/*.md` examples and prose, `showcase/uav-sim/agents/instructions.md` | M |

One-time audit scripts run from the scratchpad and are not committed. They must
report zero unexplained items before the naming commit.

| Audit | Pass condition |
|---|---|
| Serde scanner | Every remaining multi-word snake_case key or kebab value belongs to an exempt class |
| Literal scanner | No snake_case key for a changed format remains in Rust, TypeScript, JavaScript, HTML, Python, C++, JSON, YAML or Markdown outside exempt formats |
| Hidden fallbacks | No `\|\|` snake fallback in App HTML; no `populate_by_name` or `alias` on wire models |

| Scope | Command |
|---|---|
| Rust | `cargo fmt --all -- --check`; strict Clippy and affected tests with the qualified feature graph. Run computer execution, storage and host suites on Linux; cross-platform runs do not substitute |
| Client types | `cargo xtask release client-types --check` |
| Console and Workspace | `npm --prefix apps/console/web test`, `run build`, `run lint`, `run test:browser`; the same for `apps/workspace` |
| Map app | `cd servers/map-mcp/app && npm test && npm run build` |
| Python | `cargo xtask enforce python`; the reason runner, cuOpt executor, Map data and UAV runtime suites |
| Node | `node --test servers/chart-mcp/*.test.mjs`; `mcp/apps-extension` pagination tests |
| Helm and deployment | `cargo test -p veoveo-deployment-smoke`; `cargo xtask smoke helm-config`; `helm lint deploy/helm/veoveo -f examples/bioma/values.yaml` |
| Repository | `cargo xtask enforce docs`; `cargo xtask enforce identifiers` |
| C33 | Shared conformance checks discovery, schemas and extension metadata against the naming classes, including exemptions; Rust/Python declarations agree |

## Phase 9: Conformance And Repository Enforcement

This phase takes the required architecture and verification work from Hardening.
It supplies reusable checks for phases 0–8; it does not wait until the naming cut to
start the typed catalog. Keep domain assertions with their owner and use maintained
Rust, browser and SDK frameworks. Source scanners supplement structural and runtime
checks, rather than becoming the sole proof of compliance.

| Work | Owner and gate |
|---|---|
| Typed requirement catalog and compliance profiles | `mcp/contract` owns stable IDs, statuses and justified overrides. Adding a requirement forces profile coverage; served declarations and document projections derive from the same catalog. Extend a reviewed phase 0 mechanism or declare a justified macro exception |
| Generic conformance | `mcp/conformance` checks an arbitrary registered server without production dependencies on concrete domains. Move domain schemas and provider fakes to their owners. Independent fixture and current server suites pass |
| Component discovery and dependency direction | Cargo metadata and path conventions discover components, documents, package roles and feature graphs. Enforce the reviewed kernel/module and contract-only boundaries without a hand-maintained server registry |
| Owner-local smoke and dispatch | Discover component scenarios and their requirements. Move domain cases to their component, compositions to their example/showcase, and deployment checks to deployment owners. Adding a server requires no central smoke enum or conformance registry edit |
| Shared harness mechanics | Reuse `hosting::testing` for applicable HTTP/signing fixtures. A shared smoke support library may own process cleanup, bounded readiness, redaction and declared GPU/browser prerequisites; it imports no domain implementation and does not replace maintained framework behavior |
| Typed deployment relationships | Preserve route, mount, scheme, policy, source, image, artifact, recording and GPU validation across the selected installation. Support the qualified replica semantics; do not restore the retired blanket singleton assumption |
| Onboarding and module responsibility | Required owner documents, standards sections, package naming and isolated contracts are checked. Split mixed responsibilities when needed; neither file length nor deletion of the top-level `testing/` directory is a completion criterion |

The reviewed catalog design puts a closed requirement vocabulary and exhaustive
metadata in `mcp/contract`. Each owner supplies one complete checked compliance
profile. Construction and decoding reject missing, duplicate or unknown requirements
and empty explanations. Raw JSON decoding rejects duplicate fields, including escaped
spellings of the same key. Generated language catalogs carry the Rust owner's
whitespace rule for note admission; language-specific trimming cannot change it.
A new catalog requirement forces every owner to declare its
status; profiles cannot silently inherit `met`. `not_applicable` is admitted only
for a catalog-defined condition, and conformance compares that condition with
discovery. A server declaring Knowledge cannot use it to bypass C32.

Each owner authors `contract-compliance.json` beside its documents. Rust, Python and
Node decode that file into checked models; they do not maintain independent status
lists. The catalog has revision 2, separate from hosted contract revision 4.
Profiles and served declarations bind both revisions, and consumers reject
unsupported revisions.

The contract resource and the marked compliance section in each owner's manual
derive from that profile. Generate the checked-in manual before embedding it, and
check for stale output. The embedded bytes and their content digest stay identical
to the served document; runtime rendering does not replace `embedded_document!`.
Rust setup and Python/Node loading compare the marked manual section with the
profile's deterministic rendering. A matching document digest cannot admit a stale
profile. Python and Node package the generated catalog and owner profile for offline
loading. Python's isolated Hatch hook and runtime share one stdlib-only admission
module, without importing the runtime dependency graph into the build hook.
Unsupported revisions reject. The permissive Markdown parser stops being the
source of declaration authority.

Checklist IDs and runtime check IDs identify different things. Preserve the current
`VV-MCP-*` check names and map them to typed checklist requirements; Knowledge owns
its K-series checks. A declared `met` status and a report containing skipped checks
do not establish runtime qualification. Acceptance covers complete-profile rejection,
C32 applicability, language parity, generated-section and embedded-digest agreement,
and onboarding the independent server without a new core registry entry. C32
applicability is checked against Discover in both directions. Checked Rust setup
already adopts docs Knowledge, so its template keeps C32 applicable with a pending
qualification reason. Complete the independent fixture's partial declaration with
explicit pending entries rather than an implicit `met` baseline. SUMO needs its
missing adjacent documents and authenticated well-known surface; profile generation
does not establish C18–C21. C33 joins the catalog with the naming cut in phase 8.

The reviewed dispatch design discovers owner `smoke/scenarios.json` files from
tracked and nonignored source inventory. Cargo metadata supplies package and target
identity; Python and Node use their package manifests. Listing scenarios requires
neither owner execution nor production builds. Typed native targets and prerequisites
replace the central scenario enum. Descriptors declare hardware, network, identity,
billed operations, execution deadlines and cleanup grace, and contain no credentials
or shell commands. Unknown targets, duplicate identities, cycles, unsafe paths and
unsupported revisions fail before effects.

Cargo compiler-artifact output supplies executable paths for the selected prerequisite
closure. A private artifact manifest carries those paths to owning harnesses, including
configured external target directories, hashed test executables and owner-staged native
libraries. The DuckDB-consuming harnesses keep their Cargo build-script staging and
receive the resulting library paths. Qualification launches a relocated DuckDB consumer,
a hashed test target and a target built under an external directory.

Build grouping compares effective transitive feature selections. Separate an isolated
contract or certification selection when another root would enable runtime dependencies.
Conflicting executable profiles reject. Prove the actual standalone certification
binary's normal/build graph independently of hosted fixture and development graphs.
Preserve the existing native Cargo product layout when maintained graph projections and
compiler receipts can identify the admitted feature sets. Identical host and target
sets may be reported as shared. An unresolved difference fails before preparation with
an actionable execution-profile diagnostic. A distinct target layout requires a
demonstrated qualification need and an explicit profile; discovery alone cannot move
every scenario into another build cache.
Exact integration-test selection must prove that the requested case executed. Missing,
ignored, skipped or zero-executed selections cannot pass through a successful exit code.

Each harness owns assertions and cleanup. At execution deadline D, or earlier
interruption, the dispatcher requests cancellation. Cleanup ends G after the first
cancellation, no later than D plus G; repeated signals cannot extend it. Preserve registrations for separately owned
local process groups and remote fixtures. Before forcing local groups to stop, observe
the direct child without reaping it, then kill the groups and reap the child to avoid
signalling a reused process identity. Verify local group drain separately from complete
fixture cleanup. A forced termination fails and reports unresolved fixture identities.
An observation timeout cannot settle an external mutation or authorize a retry.

Generic certification and shared harness support must exclude production domain
contracts and implementations. Move every domain utility and schema assertion to its
owner or composition, update its callers, and preserve maintained protocol fixtures.
Freeze the complete command, scenario, source-import and executable-caller transfer map
before editing. It includes composed schema exports, token utilities, provider fakes,
Media completion and subscription assertions, and native runtime-library delivery.
Keep the shared issuer and verifier with their runtime owner. Generic certification uses
its bearer input; owner tooling supplies tokens for direct hosted checks through the
same signing implementation. Update the issuing callers and private credential handling
in the same cut. A new signer or silent removal of direct hosted acceptance is invalid.
The MCP contract gates runtime dependencies and modules with a positive runtime feature.
Its declaration-only selection preserves one checked catalog implementation and the
default selection preserves the runtime API.
Shared support cannot import `hosting::testing` through its normal dependency graph;
owning hosted fixtures continue to use it through their development graph.
Relocated targets reuse existing owner packages and gate support binaries and their
dependencies behind a non-default smoke feature. Compact Rust harness packages for
non-Rust owners need the concrete assertion and dependency-isolation reason; they
create no service boundary. Installation acceptance uses the existing reference
composition. A harness that needs the Gateway installation registry belongs to
Gateway composition or the Bioma reference installation. The registry composes
Map and Media schema contracts; a domain package cannot depend back on that
composition without creating a package cycle. Domain-only assertions stay with
their owner. The transfer map distinguishes assertion ownership from executable
delivery and reuses each assertion once. Metadata qualification checks those
cross-component targets and prerequisites. Generic schema export keeps the
profile/report pair, while Gateway composition keeps every full-export filename
and its offline bundle consumers.
The existing focused flight client moves to `examples/bioma/flight` with its package
and binary identity intact. Its client-only dependency selection remains separate
from the full installation acceptance feature. Pure shared libraries and explicit
composition-owned source modules preserve one helper implementation without importing
the Gateway executable's runtime graph. Qualify the actual flight binary's normal/build
graph and unchanged native arguments alongside the moved assertion and document paths.
The final dependency proof covers the actual certification binary and the eventual
discovered-scenario graph. Completing the catalog does not establish generic C33,
H03 or H05 acceptance against the current production-dependent registry. Reuse existing
scenarios while moving their ownership; relocation does not require another harness
or an installed replay of unchanged behavior. C33 lands in phase 8 with the naming
rule. Future repository/security tooling follows X2–X5.

## Phase 10: Installed Acceptance And Closeout

Run the affected native suites before publication. Linux qualification includes
Computers `--tests` and its execution/storage/host crates, Map `--lib`, Recording,
TaskRuntime and deployment-smoke with Helm and GNU timeout. GPU suites use hardware.
`recovery_classes_and_leases_are_enforced` now uses a two-second lease fixture and
passes in the full current TaskRuntime suite. Report a future parallel failure and
any justified diagnostic replay separately.

Computers selects unmodified
[`OpenShell 0.1.2`](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.2).
Official artifact identity and source packaging checks pass. Package image
`ea4ffc9f7` is assembled and staged; all four official binary hashes and versions
are verified. The owner approved supported OIDC with certificate-to-user promotion
disabled, upstream roles, complete supervisor TLS and mandatory Sandbox JWT. Three
CA roles, projected worker inputs and optional admitted issuer CA pass owning source
controls. Driver config
uses stock mount fields; the owned image leaves the retained-home target absent to
preserve registered-writer fencing. The reference catalog now contains its stock
image and fingerprint; deployment requires the coordinated drain. The existing Veoveo issuer,
restricted worker registration, public JWKS and reference Host/worker/chart inputs
are implemented with `private_key_jwt`. Installation must materialize the worker
registration, current Host/worker configuration, trust Secrets and worker-only
projected private-key Secret, then activate matching admitted images and
configuration through the coordinated drain. The selected stock native results are
recorded in [Current Status](#current-status): all eighteen selected native cases
pass, including stock CLI transport, distinct-image Host replacement and directed
template upgrade/rollback. Node RuntimeClass activation, new-Pod 4096-PID verification,
live identity/configuration materialization and installed provider/containerd pulls
remain qualification gates.

Earlier patched-provider containment, companion adoption and terminal replay results
apply to the pre-stock profile. Each current attachment opens a fresh SSH shell with
acceptance Ready and renewable lease controls. Qualify this profile before closing
F23 or A09.

The affected installed image closure is `computer-host`, `computer-template`,
`computers-mcp`, `mcp-gateway`, `console-bff`, `agent-manager` and `knowledge-mcp`;
BFF packages both browser clients. Provider and storage images are Host build inputs.
Preserve the official supervisor mirror's manifest and config digests. The first five
image pins, published Veoveo chart and matching configuration/module revisions keep
their recorded artifact qualification. Manager and Knowledge prepared pins add
source-bound compatible readers whose installed admission still needs proof. Roll
those readers against the actual old catalog and enabled module composition, drain
old readers, and prove Manager Ready plus Knowledge Ready and functional indexing
and search before atomically publishing the matching worker section and client.
Preserve Knowledge desired replicas at one through Recreate and keep Embedding and
Speech Ready. Ordinary concurrent Helm reconciliation cannot enforce this sequence;
the tracked Veoveo and UAV holds stay set. Only the later admitted activation applies
matched images/configuration, three-role trust, the projected worker private key and
optional issuer CA through the coordinated drain.
The current reference keeps NetworkPolicy disabled while preserving all TLS and
token checks. Enabling Host isolation requires actual issuer/JWKS CIDRs and separate
network qualification; worker egress policy changes must qualify every selected
workload path together.

The committed reference inputs materialize the admitted template catalog and new
fingerprints in Host, worker, default and execution/file lists. Preserve retained catalog identities; a changed
default applies to new requests. Existing bindings need a qualified maintenance
transition, and the reference's empty transition list grants no automatic adoption.
Drain previous readers and workers after settling pending operations. Keep unresolved
resource fences, retained homes, journals and matched rollback artifacts. Preserve
the qualified native Host and selected native consumer profiles while qualifying
installed public OAuth, terminal, CLI and file journeys. Headed
terminal acceptance still requires hardware graphics.

Publish the complete affected image closure for the final wire/storage cut: servers,
gateway, Console/Workspace, agents, SDK servers, runners, simulation/UAV consumers and
migration images. Use the image dependency graph to determine which images changed;
reuse compatible qualified bases. Drain writers, stop incompatible workloads and
bootstrap empty database, object storage and Map DuckDB through the reference runbook.
Prepare images before reconciliation, apply generated locks and configuration, run
owner migration Jobs, re-provision managed agents and activate the Knowledge
generation. The audit chain begins at a new genesis. No old/new wire overlap is allowed.

Use existing `cargo xtask smoke` scenarios and owning native/SDK/browser harnesses.
All public MCP checks use the gateway as a normal OAuth client. Direct pod checks
are limited to the transport/probe/webhook cases that require them. Select workloads
in batches; keep Knowledge and Embedding enabled and run Reason separately.

| ID | Required check and result |
|---|---|
| A01 | Every selected pod becomes Ready. Direct mounted `healthz` and `readyz` return 200 when healthy. Do not add anonymous per-server health passthroughs to the gateway |
| A02 | Through the gateway, tools/resources/templates/prompts list successfully; unauthenticated discovery fails. Read `{scheme}://docs`, contract and an owner document, complete `doc_id=de` to `design`, and read authenticated admin docs |
| A03 | Bad or unparseable resource addresses return -32602. Through a port-forward, missing Host is 400 and a disallowed Host is 421, including probes and Recording gRPC |
| A04 | Admin server health returns every registered state and `checkedAt`, denies non-admin callers and records ServerHealth. One failed upstream does not break federated prompt discovery. DuckDB and Timeseries completion work |
| A05 | Exercise each owner's domain read/tool and declared durable Task lifecycle, including completion, cancellation and delivered subscriptions. DuckDB task updates and current Task admission survive the applicable replica/restart cases |
| A06 | Frames `frames://worlds` listener receives the create-world invalidation; immutable revision subscription returns -32602. Complete the F-register paging, mutation and operation consumers |
| A07 | Direct Media unsigned webhook POST returns 401 with `invalid signature`. Existing fake-provider lifecycle passes. Real generation stays unqualified under the user restriction |
| A08 | Knowledge remains unready until indexing is ready; docs and lists obey current caller policy. Qualify unattended cold startup, approved-source search, namespace/key isolation, actual Computer Host isolation, subscriptions and current-generation recovery |
| A09 | Computers CLI uses its own grants without a gateway token; oversized MCP input returns 413. Admin terminal, pairing, maintenance, grant consumers and current-format interruption/recovery pass |
| A10 | Speech hosted conformance runs on the GPU host. Recording gRPC works through the playback host and headed hardware playback meets its freshness gate. View's required rendering and GPU JPEG path pass |
| A11 | Map runs without `--admin-scope`; its consumers and Map/UAV handoff pass. UAV streams survive shutdown, drain cleanly and preserve unresolved outcomes for recovery/operator reconciliation |
| A12 | Restart an affected hosted server; its old pod exits inside the termination grace period. Exercise owner-required cross-replica and current-format recovery rather than inferring them from Ready |
| A13 | Composed flight and Artifact isolation preserve their accepted mission, live Stream, replay/reconnect, return and landing gates. Headed hardware visual evidence is mandatory; software rendering cannot substitute |
| A14 | Final C33 discovery/schema checks and real consumers agree on `resultUri`, camelCase wire keys and declared exemptions. The F-register and required H-register entries have no unresolved required implementation or acceptance |

Report server × check with pass, fail or explicitly unqualified, source/image identity,
request, status and response excerpt. Each failure includes pod logs and whether it
predates the change or is a regression; use unknown when not established. Keep dated
runs in the progress log and current blockers here. Reuse accepted checks only when
their actual inputs and environment are unchanged.

A host-contract defect is reported for the existing user-directed review; do not
silently change `mcp/contract/src/hosting` or `platform/task-runtime/src/hosting.rs`.
A domain defect belongs to its server. Close required rows only after their conditions
pass; record an explicit accepted limitation where acceptance cannot be claimed.

## Risks

| Risk | Mitigation |
|---|---|
| A derive hides behaviour reviewers need to see | Generated code only calls ordinary traits in `platform/types`; `cargo expand` or the qualified native compiler expansion diagnostic is reviewed for the first migrated crate; normal toolchain checks establish acceptance |
| A macro grows into a general code generator | Each core macro has one shape; a new need becomes a declared exception or a new reviewed macro, never a new option on an unrelated macro |
| The ownership validator misreads a statement | Allow-listed grammar when no SurrealDB parser is available; tests with each statement kind |
| The kernel module order has a cycle | Phase 1 qualifies the target DAG and inventories current cycles; Phase 3 resolves embedded associations and link ownership before final closure |
| A port's unbound refusal hides a configuration mistake | The gateway reports bound and unbound ports at startup and in server health |
| The store split breaks a cross-module query | Phase 3 ownership audit; every moved module's suite against real SurrealDB |
| A declared field misses a key the Rust type writes | Negative writes and current producer/consumer tests reject the mismatch |
| A phase 4 writer omits required fields | Fresh bootstrap, negative writes and current producer/consumer tests cover every changed table; restart and rollback use the current format |
| Strict inputs reject a client the suites do not cover | Phase 5 qualifies the affected consumer closure; phase 10 covers installed clients on the final images |
| The proposed Metal pooling path fails qualification | X1 stays deferred and unadvertised; NVIDIA remains the required profile |
| Removing the image from the space identity admits drifted vectors | Every supported runtime/configuration passes the reference and retrieval matrix before reusing a space |
| The re-baseline drops or changes a definition | Schema equivalence gate on a fresh bootstrap |
| A hand-written consumer drifts during the naming cut | Phase 6 generated types, the literal scanner, rebuilt App bundles and every consumer suite |
| Model-facing text teaches old names | Documents partition and the prompt and description audit; strict tool inputs fail loudly |
| A Helm template references a renamed values key and renders empty | `helm-config` smoke, the CEL admission test, and a template reference audit |
| `main` moves during phase 8 | Coordinate the cut window with other writers and repeat affected qualification if the source changes |

## Hardening Transfer Register

This accounts for the remaining hardening scope without inheriting its stale command
menu or GitHub workflow assumptions. Completed mechanisms stay in their owning designs.
A required row is part of this plan's completion boundary. Deferred rows are tracked
below and are not silently approved by this consolidation.

| ID | Former concern | Disposition and completion owner |
|---|---|---|
| H01 | P0 toolchain, language/configuration checks, unique binaries, xtask, Justfile retirement | Delivered baseline; preserve native qualification and fix current failures in the affected phase. Do not replay the 2026-07 audit as new work |
| H02 | Typed MCP checklist, compliance profiles and generated projections | Required, phase 9 catalog and phase 8 C33; no copied per-server requirement list |
| H03 | Dependency direction, component discovery and onboarding | Required, phases 1–3 and 9; isolated contract consumers and discovered Cargo graphs |
| H04 | Contract types, generated configuration and controlled schemas | Required, phases 0, 4–6 and 8; include package/deployment identity at its owner |
| H05 | Generic conformance, component smoke, discovered dispatch and shared support | Required, phase 9; preserve installed coverage through moves and remove obsolete dispatch paths |
| H06 | Deployment/control-plane validation and module responsibilities | Required, phases 1–3, 8 and 9; preserve implemented typed validation and qualified replica behavior |
| H07 | Image publication, builder graph, cache identity, SBOM/provenance and simulation base | Delivered mechanisms in image/deploy/runtime designs; preserve their invariants and affected qualification, do not introduce another build engine |
| H08 | Rust policy, local fast hooks, dependency/license/vulnerability checks | Deferred X2; no new hook or broad policy gate before baselining and review |
| H09 | Container, Kubernetes, secret, shell, TOML and documentation scanner policy | Deferred X3; existing native Helm, docs and release checks remain required |
| H10 | Rust ownership of offline bundle build/load/verification | Deferred X4; phase 8 still updates the existing bundle format and consumers when its wire schema changes |
| H11 | Vulnerability reporting, contribution routing, CODEOWNERS, license/notices | Deferred X5; selection of legal/license policy requires an explicit owner decision |
| H12 | GitHub CI, protected main, required checks, scheduled scanning | Superseded as a current gate by CONTINUOUS_INTEGRATION.md; future worker/delivery decisions remain X5 |
| H13 | Published facade dependency closure, external release products and Helm library | Fork/deploy designs govern implemented surfaces; independent publication qualification remains X6, without assuming a public registry or service |
| H14 | Blanket deletion of `testing/` and relocation by directory name alone | Superseded by owner/responsibility checks in phase 9; remove only an emptied, obsolete owner |
| H15 | Nextest, broad property tests, fuzzing, mutation tests, Miri, Loom, coverage and sanitizers | Preserve the original explicit deferral as X7; defect-specific testing may still be required |

## Foundations Transfer Register

These 68 rows preserve the former Phase 3 inventory's next owning change, with stable
IDs for review. The old checkpoint column remains in Git history and the
[progress log](PLATFORM_FOUNDATIONS_PROGRESS.md). Shared mechanics land once; rows
with the same owner are implemented and tested together. They are not 68 sequential
build or deployment batches.

All rows are required unless their condition is already covered by reusable accepted
qualification. Type/URI/DTO work belongs to phases 0 and 5; persistence and SQL work to
3–4; cross-language adoption to 6; installed, recovery and hardware conditions to 10.
Before implementing a batch, enumerate its concrete files and affected consumers.
Record an explicitly opaque field or rejected reference candidate with its owner and
reason rather than growing an unbounded generic typing task.

| ID | Surface | Remaining condition |
|---|---|---|
| F01 | Foundational primitives | Qualify the finite owner conditions in F13, F15, F23, F25, F27, F29, F44 and F59; accepted owner builders and relationship admission need no additional generic type pass |
| F02 | Independent extension traits | Preserve domain-owned authorization through the installed owner conditions in F13, F23, F27, F29, F44 and F59; any further source repair must identify a concrete owner contract or consumer defect |
| F03 | Scope declarations | Qualify installed producer admission with the next composed publication; wider field relationships remain in their owner rows |
| F04 | Resolved invocation authority | Preserve complete authority when extracting domain contracts; qualify installed policy and composition consumers |
| F05 | Concrete URI components | Adopt through domain constructors with specific ID types; qualify each family's spelling and parameters |
| F06 | HTTPS network addresses | Adopt this profile where other domain contracts require HTTPS; qualify installed source consumers |
| F07 | Resource templates | Qualify installed current resource-template declarations and their owner builders. Any source repair must name the affected owner, advertised template and concrete builder mismatch; all-template source qualification is not claimed |
| F08 | Gateway completion and audit targets | Qualify installed completion authorization, owner-registry binding and current-format audit reads; preserve distinct admitted concrete-resource and URI-template profiles |
| F09 | Platform identity and attribution | Preserve these contracts during domain extraction; qualify installed identity and policy behavior with the affected services |
| F10 | Map | Qualify remaining installed product-specific consumers and selected body/index agreement; preserve the source-qualified typed repository APIs and immutable DTO admission |
| F11 | Coordinate vocabulary | Qualify installed consumers with the current absolute frame-ID profile |
| F12 | Map identity admission | Qualify current-format installed consumption of the specific owner IDs and preserved admitted aliases |
| F13 | Time | Qualify installed expression, projection, calendar, clock and HTTPS-source consumers using the source-qualified shared construction/decoding admission; preserve distinct zone profiles and the advertised reserved-expansion template |
| F14 | Time identity admission | Qualify installed admission; use the distinct current public and stored ID profiles when strengthening metadata construction |
| F15 | Time scalar admission | Qualify installed numeric boundaries, recurrence and acquisition lifecycle consumers with the source-qualified scalar and status/phase/staged-release admission; preserve the current engine and producer rules |
| F16 | Time intervals | Qualify current-format installed schedule Tasks and restart recovery |
| F17 | Time active-pointer admission | Qualify installed parent admission and transactional conflict rollback |
| F18 | Time authority contexts | Qualify installed restart/replica behavior and the declared coordinated upgrade |
| F19 | Time authority metadata | Qualify current binding validation and installed startup |
| F20 | Time resolution metadata | Qualify installed resolve/convert decoding and epoch behavior after authority activation; computed representations remain the engine's responsibility |
| F21 | Time activation preflight | Qualify installed activation and concurrent conflict rollback |
| F22 | Digest wire profiles | Preserve each owner's admitted bare-hex spelling and hash preimage through typed Speech, UAV, Optimization and Recording/publication APIs; qualify installed Time and changed consumers |
| F23 | Computers | Qualify final image pins and installed pairing, grant/Execute-limit and collection consumers; preserve the qualified matched provider image, retained maintenance/recovery native matrix, shared admission and live permission checks |
| F24 | Speech | Owning CUDA/dictation/recovery/cancellation controls, offline image assembly and no-network worker packaging pass; image `7f7e27c242bd` is Ready with health/readiness 200. Installed Workspace cancel-draft, explicit-send, Task-after-reload and downloads pass with fixture AudioWorklet input and headed RTX 4090 WebGL. Selected production Rust host idle SIGTERM passes with successful old-container exit inside grace and a same-image Ready replacement. Raw subscription/cancel-recovery and coordinated replacement remain open; the strict Speech-only rollout gate failed on Gateway checksum drift. |
| F25 | Artifact plane model | Qualify installed access/service consumers and external byte stores with the source-qualified lightweight Artifact contract; keep verified caller, policy and transport in their adapters and reuse unaffected byte-plane/SQL controls |
| F26 | Artifact identity and URI admission | Qualify installed consumption of the current identity and URI contract |
| F27 | Remaining Artifact references | Qualify installed issuance/redemption, current-format capability recovery and multi-page SDK consumers with the source-qualified Task/digest, access-decision and metadata/address admission; preserve external transfer locations and the declared coordinated profile cut |
| F28 | Artifact attribution construction | Qualify installed metadata consumption; native construction, schema/decoder and independent-consumer checks already have a passing checkpoint. |
| F29 | Media public contract and hosted setup | Qualify installed registration, catalog continuation and Artifact presentation consumers with the source-qualified registry snapshot and admission; qualify real provider recovery budgets when provider execution is permitted. The approved host tracing correction passes its owning controls |
| F30 | Frames hosted setup | Qualify current resource admission and subscriptions through installed clients |
| F31 | Frames world reads | Installed paging and parent-scoped completion pass at `a140c0571`; qualify the final selected images with current catalog consumers |
| F32 | Frames mutation inputs | Installed publication, concurrent replay and conflict refusal pass at `a140c0571` under the selected writer policy; repeat qualification on the final selected images |
| F33 | Frames world metadata construction | Installed current-head, revision and frame metadata agreement pass at `a140c0571`; qualify current world metadata consumers on the final selected images |
| F34 | Frames operation references | Installed direct and Task conversion provenance reads pass at `a140c0571`; qualify consumption and provenance checks on the final selected images |
| F35 | Frames stream references | Installed synthetic-reference admission and the current UAV library consumer pass at `793bc7875`; qualify the final selected images and each actual producer's route and parent IDs before advertising it |
| F36 | Frames usage visibility and pages | Installed pages, owned reads, foreign-caller refusal and subscriptions pass at `a140c0571`; qualify current catalog consumers on the final selected images |
| F37 | Frames operation visibility | Installed completed-state direct/Task retention and foreign operation refusal across one same-Pod process crash pass at `1393fdc36`; qualify the final selected images with current-format consumers |
| F38 | Timeseries resource admission | Installed Artifact metadata, bytes and RRD provenance consumption pass at `027e9bdee`; qualify current consumers on the final selected images |
| F39 | Timeseries forecast admission | Installed four-row NaiveTrend output and Artifact handoff pass at `027e9bdee`; qualify final selected images and preserve summary/preview relationships, finite-point checks before publication and the 501-point bound |
| F40 | Timeseries usage | Installed caller-scoped usage and foreign-administrator refusal pass at `027e9bdee`; qualify coordinated replica replacement and current consumers on the final selected images |
| F41 | DuckDB usage and discovery | Focused installed database/usage page consumers, exact fixture membership, usage quantity/unit and owner refusal pass at `374e25f345`. Headed hardware Workbench and final images remain open |
| F42 | Optimization usage and contract | Qualify the coordinated control/executor replacement and installed reads |
| F43 | Optimization catalogs | Qualify current catalog permissions and installed consumers |
| F44 | Optimization resource admission | Qualify installed mandatory GPU readiness and problem/run/solution/output resource reads with the source-qualified admission, verification findings and digest preimages; preserve contextual feasibility |
| F45 | Map travel-model reads and shared references | Qualify installed page traversal with current consumers and source-qualified identity/manifest admission |
| F46 | Map restriction reads | Qualify installed summary pages and the declared effect/limit policy with source-qualified geometry, family, record, validity and finite ordered vertical-band admission |
| F47 | Map product addresses | Qualify installed lineage and derivation consumers with source-qualified parent, revision and Artifact checks; external country/state codes keep their distinct types |
| F48 | Map route handoffs | Qualify the published Map/UAV handoff and current installed mission admission with source-qualified product DTO relationships |
| F49 | Map routing authority | Qualify installed routing, activation conflict rollback and prepare/commit/projection order with source-qualified exact/set SQL selections and lifecycle policies |
| F50 | Map source catalog | Qualify current source-ID and document admission and installed page traversal |
| F51 | Map mobility catalogs | Qualify installed traversal with current consumers; UAV grants/handoff and the flight harness consume Map-owned addresses |
| F52 | UAV Map admission | Qualify current adapter/Task restart recovery and installed grant/mission behavior |
| F53 | UAV execution exclusion | Retain complete observations across process loss; qualify current-format installed recovery and operator reconciliation |
| F54 | Media usage and prediction reads | Qualify current catalog consumers and installed subscription recovery |
| F55 | Media generation result | Qualify current-format installed delivery, caller isolation and restart behavior |
| F56 | Native Task identity | Qualify installed consumption; native APIs, changed admission adapters and isolated-contract consumers already have a passing checkpoint. |
| F57 | Shared public Task reads and notifications | Qualify current-format installed delivery and audit domain-specific adapters for additional policy; preserve transactional owner/context/operation rechecks and the qualified rollback/race cases. |
| F58 | DuckDB source contract | Focused installed CSV/schema/source consumption, catalog pages and Artifact publication/readback with bytes/digest and native Task provenance pass at `374e25f345`. Physical owner-directory/metadata inspection, further recovery and final images remain open |
| F59 | UAV contract | Qualify installed GPU consumers and recovery with the source-qualified adapter reply parents and accepted/rejected semantics, shared world digest/frame and Rust/Python portable state/child admission |
| F60 | UAV scopes | Qualify current scope enforcement on every installed UAV replica |
| F61 | Reason | Qualify installed delivery, restart recovery and GPU behavior with source-qualified full results, visible summary/provenance relationships and owner-derived summaries |
| F62 | Task-backed resource notifications | Speech uses the Stream/Reason shared Task watch; owning delivery, current-policy refusal, context cancellation and affected source consumers pass. Its installed Workspace Task-after-reload consumer passes. Raw installed subscription updates, cancel-recovery, other required consumers and coordinated replacement remain open; preserve independent domain-change sources. |
| F63 | Stream | Qualify installed replay/live results and browser consumers; preserve source-qualified replay/detection, visible terminal provenance and browser admission before state effects |
| F64 | Shared recorded video | Qualify installed snapshot digests and Stream/Reason consumers; preserve signed timeline indices and source-qualified shared construction/decoder admission |
| F65 | View | Local NVIDIA captures, retained-lease restart and recovery, cancellation and process drain pass. Qualify installed consumers, cross-context Task delivery and pod termination-grace acceptance with the selected Vulkan/CUDA profile. See Current Status. |
| F66 | Recording | Typed IDs, sealed-property recovery and immutable publication pass locally. Native controls qualify the manifest body and descriptor, RRD nonrewrite, retained Reader authority, Video snapshots, Hub diagnostics, cross-segment encoded payloads and Redap's URI/manifest profile. Store-backed framed-RRD delivery qualifies actual bytes, bootstrap, reconnect, updates, rollover and SQL refusal at admission and layer transitions. The scoped Redap archive/catalog wire check passes. Qualify final installed ingest and grants, mandatory GPU decode and headed Rerun playback |
| F67 | Shared consumers | Keep imports direct and preserve authorization, identity serialization, and schemas |
| F68 | SDKs, clients, templates, and showcase servers | SDK Artifact metadata/address/compliance and usage parent/finite/derived-total admission pass affected SDK and template callers locally. Preserve explicit external/native Task profiles and qualify installed multi-page consumers |

Map's related and evidence references adopt the existing generic `ResourceUri`
profile with their eight allowed schemes, 1024-byte item limit and 64-item list
limit. The coordinated admission cut requires lowercase schemes and absolute,
hierarchical escaped references. It preserves generic query, fragment and port
syntax without imposing the stricter concrete address profile or normalizing
stored text. The owning authoring design declares the rejected URL-only spellings;
builder/decoder checks qualify the change before the installation cut.

Artifact write-capability Task references adopt `ArtifactTaskId` admission and
canonical lowercase hyphenated UUID emission. Issuance and redemption compare
the admitted Task identity rather than the submitted alias spelling. This changes
the existing string-bound writer profile as well as RFC-variant admission. The
coordinated drain and fresh-state cut recreates retained write rows and worker
capability snapshots inside durable Task requests. Issuance, redemption, repository
bindings and recovery readers use the same admitted identity. No historical
normalization or fallback is added. Alias round trips and current-format recovery
qualify the replacement before publication.

## Deferred Work

Required implementation deferrals remain blockers. Each new row names the F/H/phase
requirement, owner, precise cause, next check and corresponding code TODO. Preserve
existing `TODO(foundations)` entries until resolved; use this register rather than
creating a second active plan. The open F-register and phase gates still prevent
completion.

The C02 writer and Rust/Python completion readers now admit only canonical
`resultUri`, reject obsolete and mixed spellings, and validate the product's typed
address against its resource link. The focused TestGateway product matrix passes.
SUMO and Datasheet own the same spelling in their producer schemas. Native Task
APIs, snapshots and database columns keep their declared `result_uri` profile.
Map's travel-model and Optimization's native read selections pass after the shared
writer correction. The hosting fixture supplies its complete current compliance
profile and embedded manual; the original transport, authorization and Host
controls pass. These source results do not close final installed acceptance.

The user approved the host-tracing correction, and independent review accepted it.
The existing TestGateway control reproduced synthetic token and signature values
in the previous span and passes with the replacement. Request spans record method,
path and HTTP version; handlers receive the original query. Compiler checks and
all 14 current hosting controls pass. Affected images still require publication
and installed acceptance.
The shared contract design's opening still declares revision 3 and a retired root
export, while the implemented hosted profile and all domain registrations declare
revision 4. Its prepared documentation correction names the current numeric export
and protocol tag. Those user-owned documentation changes remain separate from
the applied tracing correction; no registration or runtime change is required
for this declaration repair.

Optimization's catalog-startup implementation deferral is resolved. Its
[`RuntimeInstallation`](../servers/optimization-mcp/src/composition.rs) gate verifies
preparation, installation identity and both Tasks and Optimization histories before
recovery or HTTP startup. The native startup matrix and nine read cases passed for
their qualified pre-cut source checkpoint. The current naming-cut native read suite passes. Phase 8
and F44 still require the prepared installation to become ready with its mandatory
GPU executor. This native result does not close their installed checks.

View's F65/A12 restart implementation now uses shared
[`startup recovery observation`](../platform/task-runtime/src/recovery/observation.rs).
The finite startup set wakes on native Task changes or its earliest lease deadline;
SQL selects current server, nonterminal status and expired leases before decoding.
Both recovery APIs preserve the existing class, contribution and claim guards.
The shared [`Task recovery observer`](../platform/task-runtime/src/recovery/observer.rs)
owns cancellation and drain separately from retained Task handles. Real-Store controls prove replacement
before expiry, one winning claim afterward, renewal, current cancellation,
provider observation and excluded malformed payloads. They preserve the lease fence
and require no second restart. Shared controls cover observer lifetime, while
native View controls cover snapshot admission. The shared claim-handoff check reads
current durable state before dismissing a conflict. A missing Task settles the handoff.
Present Tasks must match the admitted server, operation, recovery profile, owner and
input before terminal settlement or another worker's live lease can settle it.
Unresolved unowned work ends serving. Owning controls inject claim conflicts over
real Store state; that injection does not establish a naturally occurring transaction
race. The maintained async process owner enforces command termination and one-second
reaping under timeout or cancellation, retaining unresolved cleanup receipts.
Its real-process controls and View's stalled observer drain pass. Typed peer policy
refusal replaces the smoke's broad error assertions. The trusted recovery read
rejects a different server before decoding its payload and returns absence only
for a missing record. Public reads keep their existing visibility. The Docker
helper preserves stdout for container IDs and JSON; only explicit log collection
combines its streams. One command deadline constrains launch admission and output
collection. Cancellation prevents late payload dispatch and preserves unresolved
launch receipts. TaskRuntime and support pass their owning native selections,
including explicit Node/Python framework execution. The SDK-owned storage exchange
also passes against current Cargo-selected artifacts and a fresh database. Its
fixture admits the invocation's container ID before cleanup and retains private,
redacted receipts for uncertain outcomes. Setup diagnostics survive successful
cleanup and remain alongside cleanup diagnostics on dual failure. The real exchange
and nine fixture controls pass, and independent review accepts the final source and
artifact bindings. Private nested fixtures use the maintained launch
gate. Review accepts the recovery read, output handling and dependency boundary,
and the repaired context carries each fixture's explicit lease through launch and
Drop. Async admission, output waiting and late child handoff share the original
owner and latched command cleanup interval. Replacing or clearing the active owner
cannot rebind that context or start another interval. Six process controls and seven
gate controls pass against the final source; explicit Node/Python framework execution
also passes. These controls include failed fixture cleanup and a thirty-millisecond
original owner grace with delayed admission. They model a pre-exec delay, not a
kernel fork stall. Ordinary production launch and complete-fixture settlement
semantics stay intact. The corrected command context latches the initial minimum
when the caller cancels first, including an original owner with a grace below one
second. Its added owner-still-running control passes, and independent review accepts
the narrow correction. The earlier full Support selection is reused for unchanged
branches; current process and gate controls qualify the affected paths.
The existing owning GPU smoke passes the local F65 hardware and lifecycle
requirements recorded in [Current Status](#current-status). Installed consumers,
cross-context Task delivery and pod termination-grace acceptance remain required.
The local run preserves NVIDIA admission and includes no Google provider execution.


The real Media generation check is explicitly unqualified under the user's restriction.
It must appear as such in the final acceptance report; the fake-provider pass cannot
be reported as a real provider pass. The accepted Object Lock and embedding-priority
limits are recorded in Current Status and their owning designs.

## Follow-Ups

These proposals are outside the required finish criteria. They need their own scope
and acceptance decision before implementation; none is silently removed by retirement
of the old plans. Promote an item into a required phase explicitly when that decision
is made.

| ID | Proposal | Owner and qualification needed |
|---|---|---|
| X1 | Apple silicon embedding profile | Embedding runtime: first verify the candidate pinned vllm-metal unquantized Qwen3-Embedding-0.6B endpoint, LAST pooling/L2, priority behavior, reference vectors and retrieval on Apple hardware. Only then add a uv-managed launcher, external endpoint Helm profile and provenance. No CPU/profile fallback; unqualified support is not advertised |
| X2 | Rust/local feedback and dependency policy | Repository tooling: baseline workspace lints, unsafe exceptions, metadata/audience/publishing rules, reproducible lock use and a thin optional fast hook. Qualify pinned license/source/duplicate/advisory/unused-dependency tools; preserve published-facade closure checks where a release actually promises publication |
| X3 | Repository and supply-chain scanning | Tooling/deploy owners: qualify discovered-input secret, container, Kubernetes, Dockerfile, shell, TOML and prose checks with exact tool pins and useful diagnostics. Scan workflow languages only when such workflows exist. Keep one dependency-update authority per ecosystem; do not duplicate existing SBOM/provenance machinery |
| X4 | Offline tooling hard cut | `deploy/offline`: typed manifest/layout, builder, loader and integrity checks replacing shell orchestration in one change, with owned cleanup and private/offline artifact support |
| X5 | Repository governance and future CI | Repository/operator owners: security reporting, ownership, contribution and license/notice decisions. Follow CONTINUOUS_INTEGRATION.md for GPU worker design; stable results and an explicit decision precede protected branches, merge gates or scheduled delivery. No GitHub runner dependency is implied |
| X6 | Independently published SDK/conformance/deployment products | Fork/deploy owners: review actual distribution needs, supported versions and private-registry/offline use. Verify external dependency closures if publishing; do not revive the retired external-extension delivery program by copying its old checklist |
| X7 | Advanced correctness program | Owning crates: nextest, broad property testing, fuzzing, mutation testing, Miri, Loom, coverage or sanitizer matrices only with a concrete selected scope |
| X8 | Independently loaded runtime modules | Module owner: evaluate loading without rebuilding images only after static module composition and lanes are qualified |
| X9 | Future typed SurrealQL construction | Store/module owners: watch stable upstream SDK support; adopt only when it preserves typed binds, SQL admission and query-plan behavior better than the qualified file-based queries |

## Reviewer Checklist

- Confirm the required finish criteria and X-register deferrals; no open Foundations
  condition should disappear or become optional through consolidation.
- Review R1–R8 and D1–D14, especially module ownership, composition dependency checks,
  macro scope, JSON exceptions and embedding space qualification.
- Check that phase dependencies are executable and fresh-state cuts need no historical
  conversion. Verify producer and consumer changes land together.
- Require concrete file/consumer selections per implementation batch and acceptance
  reports tied to deployed images. Preserve unaffected passes without claiming coverage
  for new shared contracts.
- Confirm that the H-register covers every hardening concern, including deferred policy
  and governance, and that no old CI or directory-removal mandate is reintroduced.
