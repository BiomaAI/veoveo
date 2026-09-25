# Veoveo Code Map

This map shows who owns each part of the repository and where to find the code behind
a behavior. Ownership entries describe implemented code. Plan entries name future
components and contract changes.

For task-oriented reading paths and delivery status, start with the
[documentation guide](README.md). This map is the detailed ownership reference.

## Documentation Index

General documents define repository-wide contracts and direct readers to the owning
component:

| Document | Purpose |
|---|---|
| [`README.md`](../README.md) | installation entrypoint, development commands, and repository overview |
| [`docs/README.md`](README.md) | task-oriented entry point, which document governs what, and delivery status |
| [`AGENTS.md`](../AGENTS.md) | mandatory contribution and implementation rules |
| [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md) | normative product and architecture boundaries |
| [`CONTRACT_EVOLUTION.md`](CONTRACT_EVOLUTION.md) | accepted contract decisions (provider recovery, capacity and authority, renewable access, test tooling, evidence scope, version transitions, deployment boundaries, and transfer profiles) and the implementation work that remains |
| [`TECH_DESIGN.md`](TECH_DESIGN.md) | current implementation of those architecture decisions |
| [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md) | shared-responsibility model for containing always-on autonomous agents, and how an installation demonstrates it |
| [`WORK_CONTEXT_GOVERNANCE.md`](WORK_CONTEXT_GOVERNANCE.md) | invocation authority, output ownership, effective access, and rollout |
| [`ENTERPRISE_DEPLOYMENT.md`](ENTERPRISE_DEPLOYMENT.md) | OCI release, enterprise configuration, secrets, GitOps, fork workloads, and acceptance |
| [`FORK_DEVELOPMENT.md`](FORK_DEVELOPMENT.md) | fork layout, reviewed upstream merges, local SDK development and installation ownership |
| [`LOCAL_DEPLOYMENT_PROFILES.md`](LOCAL_DEPLOYMENT_PROFILES.md) | disposable k3d showcase profile contract |
| [`CODEMAP.md`](CODEMAP.md) | documentation index, code ownership, and change routing |
| [`RECORDINGS.md`](RECORDINGS.md) | recording datasets and layers, Artifact publication, Redap access, Arrow export, playback, disk safety, and the recording change checklist |
| [`RERUN_RECORDINGS.md`](RERUN_RECORDINGS.md) | service catalog grants, native Rerun clients, token renewal, live following, and ingress transport requirements |
| [`INSTALLATION_NEUTRALITY_FOLLOWUP.md`](INSTALLATION_NEUTRALITY_FOLLOWUP.md) | deferred register of Bioma-specific test and guide assumptions that should become installation profile inputs |
| [`RECORDING_INGEST.md`](RECORDING_INGEST.md) | external/LAN producer protocol, auth, durability, and routing |
| [`DEVELOPMENT_ITERATION.md`](DEVELOPMENT_ITERATION.md) | affected-target staging, digest-locked development rollout, focused acceptance, runtime pressure diagnostics, and iteration budgets |
| [`CONTINUOUS_INTEGRATION.md`](CONTINUOUS_INTEGRATION.md) | temporary host-local test reporting, informational GitHub presentation, and the future full GPU CI architecture |
| [`MCP_APPS_LIVE_AUDIT.md`](MCP_APPS_LIVE_AUDIT.md) | non-normative live investigation register for MCP App usefulness, Console behavior, authorization, and cluster evidence |
| [`connectors/README.md`](connectors/README.md) | third-party MCP connector catalog, recipe format, and gateway-registered upstream path |

Exploratory documents record open design work. They do not set requirements or approve
implementation:

| Document | Exploration |
|---|---|
| [`SELF_IMPROVING_HARNESS.md`](SELF_IMPROVING_HARNESS.md) | auth-aware profile strategies, MCP behavior measurements, evaluation through the scorer primitive, and possible self-improving harness designs |
| [`HARNESS_MEDIATED_MODEL_POST_TRAINING.md`](HARNESS_MEDIATED_MODEL_POST_TRAINING.md) | tool-call trajectories recorded through the deployed harness, rollout-level post-training, evaluation, and admission of candidate models |
| [`FACTORY_ISOLATION.md`](FACTORY_ISOLATION.md) | harness-neutral software-factory plan: developer workflow, author/verifier/broker architecture, OpenShell isolation, typed contracts, implementation sequence, trial acceptance, and adoption |
| [`BUILD_DEPLOY_ITERATION_AUDIT.md`](BUILD_DEPLOY_ITERATION_AUDIT.md) | measured build and rollout investigation, builder resource limits, artifact reuse, GPU image assembly, and proposed component release boundaries |
| [`REGULATED_READINESS.md`](REGULATED_READINESS.md) | shared responsibility model, control fabric, gap register, and remediation backlog for regulated work |
| [`ARTIFACT_PREVIEW_AND_APP_HANDOFF.md`](ARTIFACT_PREVIEW_AND_APP_HANDOFF.md) | artifact catalog, preview dispatch, producer and external-App registration, App handoff constraints, handler models, and open design questions |

Plans record design intent and delivery evidence. Read a plan's status and current
checkpoint before its older implementation sequence. Existing contracts apply until each
planned change lands:

| Document | Delivery and remaining work |
|---|---|
| [`FORK_DEVELOPMENT_PLAN.md`](FORK_DEVELOPMENT_PLAN.md) | deployed fork development: upstream merges, downstream migrations, and the Bioma rollout record |
| [`SPEECH_PLAN.md`](SPEECH_PLAN.md) | deployed GPU speech: private Workspace dictation, recording transcription as Tasks, timestamp playback, and JSON/WebVTT output; installed acceptance passed, with physical-microphone, scale, and C31 qualification limits recorded |
| [`AGENT_MANAGEMENT_PLAN.md`](AGENT_MANAGEMENT_PLAN.md) | delivered API and Console/Workspace authoring, revision adoption and managed lifecycle, verified with four UAV pilots; simulator flight, managed Computer templates and broader qualification are open follow-ups |
| [`WORKSPACE_PLAN.md`](WORKSPACE_PLAN.md) | delivered shared-chat client: owner-controlled membership, isolated agent context, concurrent runs, acceptance evidence, and pending installed follow-ups |
| [`REACTIVE_UX_PLAN.md`](REACTIVE_UX_PLAN.md) | delivered catalog recovery, agent feedback, personal event feed, RMCP 3.4.0 and shared subscriptions; installed acceptance and remaining cold-catalog/performance work |
| [`COMPUTERS_PLAN.md`](COMPUTERS_PLAN.md) | deployed Computers capability: Console and CLI access, named agent grants, retained storage, provider recovery and Bioma acceptance; clean/offline and broader qualification are still open |
| [`REPOSITORY_HARDENING_PLAN.md`](REPOSITORY_HARDENING_PLAN.md) | compiled repository tooling, contract enforcement, test and smoke ownership, architecture policy, supply-chain hardening, external-extension seams, and governance |
| [`RMCP_3_MIGRATION.md`](RMCP_3_MIGRATION.md) | hard cut to MCP `2026-07-28` and `rmcp` 3, official Tasks and multi-round requests, stateless transport, subscription and replica redesign, Rig migration, duplicate protocol deletion, and acceptance |
| [`PLATFORM_IMPROVEMENTS_PLAN.md`](PLATFORM_IMPROVEMENTS_PLAN.md) | multi-cycle platform-improvement plan and delivery record: completed agent, Secret, App-host, resource, provenance, and spatial work in `001`–`013`; App authority, uploads, Rerun-native recording catalog, extension release, tracing, live-view packaging, GPU memory, reasoning, and component-scoped deployment work in `014`–`023` |
| [`RECORDING_CATALOG_HARD_CUT_PLAN.md`](RECORDING_CATALOG_HARD_CUT_PLAN.md) | implementation plan for request `016`: recording datasets, immutable Artifact-backed Rerun layers, virtual catalogs, Arrow export, disk safety, activation, and acceptance |
| [`ARTIFACT_UPLOAD_PLAN.md`](ARTIFACT_UPLOAD_PLAN.md) | delivered resumable HTTP uploads, persistent browser queue, durable receipts and Python streaming; 10 GiB installed acceptance with larger-capacity measurements pending |
| [`CAPABILITY_ADOPTION_PLAN.md`](CAPABILITY_ADOPTION_PLAN.md) | unapproved proposals for weather, tabular prediction and the MCP skills extension |

Component designs live beside the code whose contract they specify:

| Document | Domain |
|---|---|
| [`apps/console/web/src/agent-management/DESIGN.md`](../apps/console/web/src/agent-management/DESIGN.md) | shared Console/Workspace agent editor, draft publication, approved connection selection and request recovery |
| [`apps/console/bff/src/agent_management/DESIGN.md`](../apps/console/bff/src/agent_management/DESIGN.md) | fixed-profile authoring HTTP and SSE routes with a separate browser session |
| [`platform/gateway/src/bin/gateway/agent_management/DESIGN.md`](../platform/gateway/src/bin/gateway/agent_management/DESIGN.md) | agent definition API, approved-model admission, policy checks and catalog events |
| [`platform/gateway/src/managed_agents/DESIGN.md`](../platform/gateway/src/managed_agents/DESIGN.md) | managed template ceilings and effective service registration; current identity and dispatch integration under implementation |
| [`mcp/contract/src/agent_management/DESIGN.md`](../mcp/contract/src/agent_management/DESIGN.md) | authoring DTOs, installation model/template validation and generated browser schema |
| [`apps/workspace/DESIGN.md`](../apps/workspace/DESIGN.md) | daily productivity client: shared-chat presentation, private Activity, live updates and client acceptance |
| [`platform/computers/DESIGN.md`](../platform/computers/DESIGN.md) | Computers domain: lifecycle, access checks, named grants, files and maintenance |
| [`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md) | the normative MCP `2026-07-28` server contract: Discover, stateless Streamable HTTP, official Tasks and multi-round input, request-scoped subscriptions, replica-safe state, schema bounds, packaging, well-known resources, and compliance |
| [`mcp/conformance/DESIGN.md`](../mcp/conformance/DESIGN.md) | typed domain-neutral hosted-server certification profiles, reports, and standalone distribution |
| [`servers/artifact-mcp/DESIGN.md`](../servers/artifact-mcp/DESIGN.md) | artifact discovery, access, publication and the Artifact App |
| [`servers/chart-mcp/DESIGN.md`](../servers/chart-mcp/DESIGN.md) | chart generation and the Chart MCP App |
| [`servers/media-mcp/DESIGN.md`](../servers/media-mcp/DESIGN.md) | provider-neutral media generation and durable webhook completion |
| [`servers/speech-mcp/DESIGN.md`](../servers/speech-mcp/DESIGN.md) | recording transcription Tasks, private dictation and the persistent CUDA worker; delivery tracked in `SPEECH_PLAN.md` |
| [`platform/gateway/src/bin/gateway/speech/DESIGN.md`](../platform/gateway/src/bin/gateway/speech/DESIGN.md) | per-chunk Speech policy checks, audit and signed internal forwarding |
| [`servers/speech-mcp/contract/DESIGN.md`](../servers/speech-mcp/contract/DESIGN.md) | lightweight public Speech types shared by service, gateway and browser edge |
| [`servers/recording-mcp/DESIGN.md`](../servers/recording-mcp/DESIGN.md) | recording catalog, queries and the Recording Explorer App |
| [`platform/policy/DESIGN.md`](../platform/policy/DESIGN.md) | policy decisions shared by the gateway and workers, indexed revisions, and current authority supplied by the caller |
| [`platform/computers/storage/DESIGN.md`](../platform/computers/storage/DESIGN.md) | retained-home journal/ext4 backend, private worker mTLS service and Docker plugin; shared-mount restart and physical writer handoff |
| [`platform/computers/host/DESIGN.md`](../platform/computers/host/DESIGN.md) | private compute-host container: daemon, provider and storage process order, retained local state and installation trust |
| [`platform/gateway/src/auth/DESIGN.md`](../platform/gateway/src/auth/DESIGN.md) | signed access-token session families, cross-replica revocation and rollout into renewable Computer grants |
| [`platform/runtimes/simulation/DESIGN.md`](../platform/runtimes/simulation/DESIGN.md) | shared hardware-GPU Isaac Sim and Isaac Lab runtime, pinned dependency profile, and conformance probes |
| [`servers/duckdb-mcp/DESIGN.md`](../servers/duckdb-mcp/DESIGN.md) | analytical SQL, Spatial, sandboxing, tasks, and data import/export |
| [`servers/frames-mcp/DESIGN.md`](../servers/frames-mcp/DESIGN.md) | local coordinate frames and transformations |
| [`mcp/apps-extension/DESIGN.md`](../mcp/apps-extension/DESIGN.md) | the MCP Apps server↔core↔UI contract for domain views and administration, including the reusable structured-resource workbench shell |
| [`MAP_APP_INTEGRATION.md`](MAP_APP_INTEGRATION.md) | consumer guide for using Map MCP resources and the reusable Map App from another MCP server |
| [`servers/map-mcp/DESIGN.md`](../servers/map-mcp/DESIGN.md) | Earth geography, map data administration, logistics routing, and immutable Optimization travel models |
| [`servers/optimization-mcp/DESIGN.md`](../servers/optimization-mcp/DESIGN.md) | NVIDIA cuOpt routing, route scenarios, convex and MILP models, independent verification, and GPU execution |
| [`servers/stream-mcp/DESIGN.md`](../servers/stream-mcp/DESIGN.md) | admitted live and replay GStreamer graphs, typed pipeline profiles, live results, and the Stream MCP App |
| [`servers/reason-mcp/DESIGN.md`](../servers/reason-mcp/DESIGN.md) | video reasoning, grounding, and audited world-model output |
| [`servers/computers-mcp/DESIGN.md`](../servers/computers-mcp/DESIGN.md) | Computers worker: retained-storage preflight, Tasks, authenticated MCP/HTTP routes and subscriptions |
| `servers/computers-mcp/tests/native_maintenance.rs` | installation-template image upgrade, recovery and reverse replacement against isolated retained storage with running product workers |
| [`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) | temporal authority, operational calendars, clock quality, and events |
| [`servers/timeseries-mcp/DESIGN.md`](../servers/timeseries-mcp/DESIGN.md) | timeseries forecasting, preview contract, and the forecast MCP App view |
| [`servers/view-mcp/DESIGN.md`](../servers/view-mcp/DESIGN.md) | static scene compositions, 3D Tiles residency, declarative overlays, and GPU frame capture |
| [`servers/uav-sim-mcp/DESIGN.md`](../servers/uav-sim-mcp/DESIGN.md) | UAV simulation sessions, principal-to-vehicle grants, Map route admission, exclusive command leases, telemetry, simulator-owned cameras, shared render products and H.264 fanout, and the UAV App |

Deployment, examples, templates, and fixtures keep their instructions beside the
material they operate:

| Document | Purpose |
|---|---|
| [`configs/stream/README.md`](../configs/stream/README.md) | operator-admitted Stream graph, profile, model, and live-ingress configuration |
| [`configs/reason/README.md`](../configs/reason/README.md) | reason catalog and runtime configuration |
| [`deploy/contract/DESIGN.md`](../deploy/contract/DESIGN.md) | typed development profile and local registry declarations shared by operational tools |
| [`deploy/runtime/DESIGN.md`](../deploy/runtime/DESIGN.md) | shared source, Helm, configuration, cluster, and GPU execution for disposable Veoveo deployment profiles |
| [`testing/deployment-smoke/DESIGN.md`](../testing/deployment-smoke/DESIGN.md) | focused Helm checks, passive or requested GitOps observation, and convergence evidence limits |
| `testing/deployment-smoke/src/flux_cancellation/` | isolated live OCI source and Helm health-check cancellation, typed observations, latency evidence, and namespace cleanup |
| [`deploy/helm/veoveo/README.md`](../deploy/helm/veoveo/README.md) | Kubernetes installation contract |
| [`deploy/helm/veoveo/DESIGN.md`](../deploy/helm/veoveo/DESIGN.md) | Workspace model admission and credential references; managed-agent namespace, admission policy and template wiring; Computers core/capacity selection, retained PVC and private host network policy |
| [`deploy/offline/README.md`](../deploy/offline/README.md) | offline bundle construction and loading |
| [`tools/image-build/DESIGN.md`](../tools/image-build/DESIGN.md) | Cargo-derived compilation inputs, artifact reuse, normalized simulator/Speech dependency publication, protected compiler-cache retention, and source identity |
| [`apps/console/web/README.md`](../apps/console/web/README.md) | local Console refresh loop, proxy routes, and authentication origin |
| [`docs/IMAGE_BUILDS.md`](IMAGE_BUILDS.md) | typed Bake planning, managed builder, cache families, and immutable image publication |
| [`docs/IMAGE_BUILD_PERFORMANCE.md`](IMAGE_BUILD_PERFORMANCE.md) | image-graph baseline, cold and warm measurements, digest equality, and incremental-build acceptance |
| [`examples/bioma/README.md`](../examples/bioma/README.md) | enterprise GitOps reference and owner-local compiled acceptance over k3d, OCI charts, Entra, and Cloudflare Tunnel |
| [`showcase/README.md`](../showcase/README.md) | reference integrations and the pattern for connecting robots and simulators |
| [`showcase/sumo/README.md`](../showcase/sumo/README.md) | SUMO/TraCI integration and operations |
| [`showcase/uav-sim/README.md`](../showcase/uav-sim/README.md) | Isaac/Cesium/Newton/Warp/PX4 UAV simulation integration and operations |
| [`showcase/uav-sim/agents/DESIGN.md`](../showcase/uav-sim/agents/DESIGN.md) | reviewed managed pilot template, seed instructions, retained identity and volume transfer |
| [`showcase/uav-sim/ACCEPTANCE.md`](../showcase/uav-sim/ACCEPTANCE.md) | deployed UAV acceptance catalog and the repeatable per-agent named-location mission E2E runbook |
| [`templates/python-mcp/README.md`](../templates/python-mcp/README.md) | Python MCP server template |
| [`timesfm-showcase/README.md`](../servers/timeseries-mcp/testdata/timesfm-showcase/README.md) | TimesFM test fixture provenance and use |

The whitepaper has one source,
[`veoveo-whitepaper.html`](veoveo-whitepaper.html). Browsers read it directly, and
the [`whitepaper PDF`](veoveo-whitepaper.pdf) is printed from it in headed Chrome
with `Page.printToPDF`, as its source header describes. The paper reflects the
catalog at its stated version. For current Workspace, Computers, Stream, Reason,
UAV live view, Recording Hub, administration and GPU behavior, read the component
designs above.

## Root

| Path | Ownership |
|---|---|
| `Cargo.toml` | Rust workspace membership and pinned shared dependencies |
| `rust-toolchain.toml` | pinned Rust toolchain |
| `docker-bake.hcl` | local OCI image build groups |
| `.env.example` | required installation configuration and secrets |
| `configs/gateway.local.json` | generic gateway control-plane configuration |
| `configs/gateway.smoke.json` | isolated smoke control plane |
| `configs/deployments.json` | deployment contract examples |
| `configs/stream/` | admitted GStreamer graph, typed profile, TensorRT model, and live-ingress catalog example |
| `configs/reason/` | world-model checkpoint reason catalog example and deployment contract |
| `configs/view/` | server-side 3D scene-layer catalog without provider secret values |
| `deploy/contract/` | multi-source deployment v7 profiles and locks, component ownership, platform-image and managed DRA closure, rendered Secret-reference closure, split source/installation Helm values, registry transport, physical-GPU topology, publication collision preflight, schema generation, and side-effect-free validation |
| `deploy/contract/src/components/` | component ownership catalog, provenance and deployable-content digests, dependency expansion, side-effect-free mutation planning, and the receipts the component-selected installer consumes |
| `deploy/contract/src/components/bindings.rs` | full-catalog profile permissions and source/image/chart binding checks, including retained unselected inventories |
| `deploy/contract/tests/fixtures/` and `tests/support/lock.rs` | explicitly synthetic deployment locks for schema and development-image transformation checks; real installation locks are generated outputs |
| `deploy/contract/src/gateway_bundle.rs` | public gateway ConfigMap content digest shared by disposable gateway activation and GitOps acceptance |
| `deploy/contract/src/artifacts/` | validated source revisions, artifact coordinates, versions and SHA-256 digests |
| `platform/runtimes/simulation/contract/` | typed simulation build locks, GPU results and release qualification |
| `deploy/contract/src/image_release.rs` | image publication records used by component publication, with build revisions and separate runnable and attested digests |
| `deploy/contract/src/locked_images.rs` | retained qualified image versions, unique source/target repository ownership, and unambiguous build provenance |
| `deploy/contract/src/source_chart.rs` | source chart content identity shared by release publication and installation; hashes actual files independently of commit and archive metadata |
| `docs/GPU_PLACEMENT.md` | managed NVIDIA DRA artifacts, installation schema, lifecycle, conflict transition, validation, upgrade, rollback, and recovery contract |
| `deploy/local/k3d/` | GPU-capable local Kubernetes cluster and values |
| `AGENTS.md` | contract evolution, provider recovery, dependency qualification, GPU evidence, type/module, and test-harness rules |
| `docs/` | general architecture, code index, recording design, and rendered publications |
| `agents/` | agent kernel and durable agent runtime |
| `apps/` | user-facing applications and their service boundaries |
| `mcp/` | shared MCP protocol contracts, extensions, and bridges |
| `platform/` | internal platform services, persistence, and reusable runtimes |
| `platform/runtimes/computers/` | private OpenShell adapter, retained binding/storage protocol, terminal/exec streams, renewable attachment enforcement and SSH-only CLI bridge; the Computers plan tracks domain/service and installed qualification |
| `servers/computers-mcp/src/file_worker.rs` and `file_worker/` | file worker: transfer authorization, size-limited Artifact bytes, results, original-run containment and production supervision |
| `servers/computers-mcp/src/io_guard.rs` | shared command/file foreground expiry, runtime deadlines and monotonic byte-limit enforcement |
| `platform/runtimes/computers/src/file_request.rs` | private regular-file streaming adapter: process binding, size-limited binary input/output, verified byte count and hash; callers own Artifact authorization and publication |
| `platform/computers/contract/src/files.rs` | public regular-file handoff types and generated schemas: import/export intent, retained-relative paths and Artifact result metadata; the domain service owns admission |
| `platform/computers/src/secrets/files.rs` and `file_access.rs` | private authenticated file intent and Task-bound Artifact access; installation key rotation and retry comparison; stores no file bodies or gateway bearers |
| `platform/computers/src/files/` | private file authorization, encrypted request and capability preparation, one-shot dispatch, live continuation, original-run containment, result settlement, recoverable Task delivery, and metadata-only public Task access through the supervised service |
| [`platform/computers/execution/`](../platform/computers/execution/DESIGN.md) | private framed argv codec and packaged guest launcher; fixed provider command, confined launch-directory resolution and finite stdin; `files.rs`, `file_io.rs` and `file_confinement.rs` own the local regular-file helper used by the public file worker; named agent authority and Task integration live in the Computers domain and service |
| `platform/runtimes/computers/provider-patches/Dockerfile` | standalone OpenShell OCI build, verified upstream/patch trees and pinned provider toolchain; compute-host topology is separate |
| `platform/runtimes/computers/tests/native_support/guest_authority.rs` | native TLS-positive, user-authority-negative check for the guest supervisor certificate |
| `platform/store/src/gateway_control.rs` | current control-plane pointer/revision read shared by gateway and worker authority, with corrupt-pointer rejection |
| `platform/store/src/gateway_retention.rs` | batched gateway audit cleanup over the kind/time index added in migration 0072 |
| `platform/policy/` | shared policy evaluator, immutable catalog view and session-family predicate; callers own authentication, store reads and freshness |
| `platform/computers/src/authority.rs` | verified identity for operation admission and the source context that execution policy checks |
| `platform/computers/src/current_authority.rs` | fresh policy and directory checks, dispatch authorization, and recorded policy decisions |
| `platform/computers/src/operation_authority.rs` and `operation_admission.rs` | separates the owner from the principal acting on the lifecycle; named Start/Stop admission, grant-bound Task access and owner recovery through the public Computers application |
| `platform/computers/src/computer_access.rs` | merged owner/grantee discovery, Read access checks, named action scopes and batch owner lookup |
| `servers/computers-mcp/src/application/lifecycle.rs` | named Start/Stop grants over the shared operation journal, retry authorization and owner Task recovery |
| `platform/computers/src/authority_snapshot.rs` and `control_authority.rs` | policy/directory snapshot and request-scoped action/read permissions; public read paths cannot obtain a dispatch ticket |
| `platform/computers/src/control_session.rs` | signed browser session-family read and shared binding decision; logout and family expiry stop new control without cancelling accepted work |
| `platform/computers/` | Computer records, tenant/principal/Work Context ownership across clients, immutable creation and encryption bindings, capacity and fence admission, Task linking, dispatch receipts, observation budgets and settlement; `tests/{resource_ownership,cross_client_effects}.rs` cover client isolation; worker integration lives in `servers/computers-mcp` |
| `platform/computers/src/session_grants/` and `queries/*session_grant*` | browser tickets, owner inventory, installation grant limits, one-use redemption, renewal against the current session family, policy and run, and parent-bound revocation; terminal composition lives in `servers/computers-mcp`; named automation grants have a separate ledger |
| `platform/computers/src/commands/` and `queries/{queue,dispatch}_command.surql` | encrypted admission, grant/policy fences, execution slots, metadata-only Tasks, one-shot dispatch, original-run containment, interruption and result settlement, and output-capability preparation; the Computers application exposes public admission |
| `servers/computers-mcp/src/application/`, `src/protocol/automation.rs` and `src/server/automation.rs` | public command preparation, Artifact capabilities for the actual caller, grant management, results, and HTTP/MCP routes |
| `servers/computers-mcp/src/command_worker.rs` and `src/command_worker/` | command execution from the original receipt, live authorization, Artifact output publication, lease recovery, interruption containment and Task settlement; command/lifecycle worker supervision; public admission writes the same application journal |
| `servers/computers-mcp/src/maintenance_worker.rs` and `src/maintenance_worker/` | retained-Computer replacement worker: installation-admitted template profiles, provider/allocator reconciliation, policy recovery, target verification and Task status; the executable supervises its scheduling loop |
| `platform/computers/src/commands/access.rs` and `servers/computers-mcp/src/protocol/task_authority.rs` | metadata-only command Task authorization for named grantees and owners; preserves the original execution actor and limits what public responses reveal |
| `platform/computers/src/secrets/` | installation-owned key ring with separate keys for command, output-capability and maintenance-checkpoint encryption, immutable run/actor/label binding and retained-key request comparison; command bytes travel in the same guest frame |
| `platform/computers/src/maintenance/` | replacement admission, step history, resumption receipts and cancellation acknowledgement, dispatch authorization, Task leases, encrypted Capture settlement, adoption verification and atomic target adoption; provider composition lives in the Computers worker |
| `platform/computers/src/automation_grants/` and `queries/*automation_grant*` | named principal/client grants, idempotent ledger and revocation, installation limits and permission checks; `management.rs` reuses issuance's registration checks for application choices and permission hints |
| `platform/computers/src/cli_grants/` and `queries/*cli*` | stock-CLI pairing ledger, named session-bound grants, shared browser/CLI quota, profile-bound fenced connection leases and owner revocation; public stock CLI qualified in the Bioma installation |
| `platform/computers/transport/` | gateway/BFF WebSocket handshakes, terminal-v2 and private CLI relays, service-issued deadlines with clock allowance; the public CLI edge strips internal controls; no dependency on the domain store or private provider SDK; qualification is recorded in the Computers plan |
| `platform/gateway/src/bin/gateway/computers/` | fixed profile-scoped Computer control and terminal routes, action policy and audit, upstream trust and terminal relay |
| [`platform/gateway/src/bin/gateway/console/`](../platform/gateway/src/bin/gateway/console/DESIGN.md) | authenticated session bootstrap that does not depend on administrator inventory, navigation permissions, and shared branding and identity display |
| [`apps/console/bff/src/bootstrap/`](../apps/console/bff/src/bootstrap/DESIGN.md) | fixed-profile, cookie-authenticated Console session routes with typed responses and token refresh |
| [`apps/console/bff/src/workspace/`](../apps/console/bff/src/workspace/DESIGN.md) | shared browser edge for Workspace: typed chat routes, cookie credentials, CSRF, event streams and static assets |
| [`tools/xtask/src/commands/client_types/`](../tools/xtask/src/commands/client_types/DESIGN.md) | Rust schema export and pinned TypeScript conversion; `release client-types --check` detects generated-model drift |
| [`tools/xtask/src/commands/test_report/`](../tools/xtask/src/commands/test_report/DESIGN.md) | CE-06 immutable receipts, per-command source planning, publication and integrity checks, and coverage verification; installed adapters and release-lock coverage are not built yet |
| [`tools/xtask/src/commands/computers_trust/`](../tools/xtask/src/commands/computers_trust/DESIGN.md) | fresh installation-owned Computers CA/client/server/JWT and command-key enrollment with separate host, worker and operator outputs |
| `mcp/contract/src/gateway/console.rs` | shared closed Console bootstrap, branding and session DTOs |
| [`platform/store/src/workspace/runs/`](../platform/store/src/workspace/runs/DESIGN.md) | per-chat agent admission; human-turn participation and same-chat replies in `workspace/participation.rs`, with server-captured quotes tested by `tests/workspace/replies.rs`; immutable Artifact references tested by `tests/workspace/attachments.rs`; concurrent-run limits, fixed context, execution fences, cancellation and interrupted-worker recovery |
| [`platform/store/src/migrations/`](../platform/store/src/migrations/DESIGN.md) | upstream and fork migration catalogs, checksummed histories, transactional application and drift rejection; fork entries live in `platform/store/downstream/catalog.rs` |
| `platform/store/src/workspace/personal.rs` and `platform/gateway/src/bin/gateway/workspace/operations/personal.rs` | actor-private inventory, shared LIVE hints, native Task observation and SSE checked against current authorization; `apps/workspace/src/usePersonalEvents.ts` owns global attention and query invalidation |
| [`platform/store/src/workspace/operations/`](../platform/store/src/workspace/operations/DESIGN.md) | private MCP operation receipts, at-most-once dispatch claims, Task references, MRTR continuation fences and ambiguous-outcome recovery |
| [`platform/gateway/src/bin/gateway/workspace/runs/`](../platform/gateway/src/bin/gateway/workspace/runs/DESIGN.md) | `messages.rs` records human-message response intents atomically; per-chat model execution through Rig pinned to the registry, capabilities limited to what the human can currently use, streaming, private Task dispatch and independent cancellation; `feedback.rs` observes tool bodies and publishes shared execution status |
| [`platform/gateway/src/bin/gateway/workspace/operations/`](../platform/gateway/src/bin/gateway/workspace/operations/DESIGN.md) | human-scoped MCP dispatch, Task recovery, input forms, continuation, cancellation and Task subscriptions; `catalog.rs` waits for required capabilities through list-change notifications; `apps.rs` binds App calls and Task recovery to their persisted origin |
| `mcp/contract/src/workspace.rs` | typed Workspace chat, membership, invitation, message, agent, run, personal event and private MCP operation/Task HTTP types; `workspace/apps.rs` owns native App bridge envelopes |
| [`platform/gateway/src/bin/gateway/workspace/`](../platform/gateway/src/bin/gateway/workspace/DESIGN.md) | direct-human Work Context admission, chat/history/invitation routes and membership-checked event streams |
| [`apps/console/web/src/computers/`](../apps/console/web/src/computers/DESIGN.md) | Computer list, lifecycle request recovery, live invalidations, lazily loaded hardware terminal and replay/lease state machine; installed evidence is recorded in the Computers plan |
| `apps/console/web/src/generated/`, `generatedContracts.ts` | generated Computer/Console schemas and TypeScript models plus qualified pinned-Zod runtime validation |
| [`apps/workspace/`](../apps/workspace/DESIGN.md) | productivity client: shared chats, owner controls and agent response policy in `AgentParticipation.tsx`/`participation.ts`, invitation inbox, assistant-ui renderer, concurrent agents and private Tasks; `Apps.tsx`, `appBridge.ts` and `appApi.ts` host sandboxed Apps with persisted Tasks; `Attachments.tsx` owns reference selection and in-message files; `ResourceResult.tsx` renders Artifact actions; `Uploads.tsx` and `Computers.tsx` reuse shared capabilities under Workspace authorization; acceptance status is in the Workspace plan |
| `apps/console/bff/src/computers/` | Computer HTTP/WebSocket routes, Console cookie/CSRF and Origin checks, ticket endpoint and shared relay; installed evidence is recorded in the Computers plan |
| `apps/console/bff/src/mcp_client/resources.rs` | shared App/native resource subscriptions, acknowledgment, capacity limits, cancellation cleanup and source-loss retirement |
| `platform/computers/contract/` | provider-independent public Computer DTOs, collection and access inventory/revocation schemas, and terminal controls shared by the Console and MCP surfaces |
| `platform/computers/storage/` | privileged host journal/ext4 filesystem, Docker observation and recorded physical claims, handoff with loop detachment, plugin/mTLS service, and filesystem/shared-mount fault handling; installed maintenance evidence is recorded per template transition |
| `platform/computers/storage/src/filesystem/loop_devices.rs` and `recovery.rs` | private loop-node discovery and verified completion of interrupted, unclaimed filesystem allocations; retained bytes are never reformatted |
| `platform/computers/storage/src/abandonment.rs` and `journal/abandonment.rs` | physical fencing and durable retirement of never-claimed instance admissions; preserves the home, rejects late mounts and shares immutable target identity with claimed handoff |
| `platform/computers/images/` | candidate Computer user environment with pinned Ubuntu base/archive inputs and numeric process identity; separate from the privileged provider and retained allocator |
| `platform/computers/host/` | private compute-host launcher and composite OCI image; fixed configuration and trust files, ordered process startup/shutdown and isolated topology tests |
| `platform/runtimes/computers/tests/native_volume_plugin.rs` and `tests/native_support/docker_daemon.rs` | native registered-writer probe with a disposable Docker daemon, pinned image import and daemon-scoped plugin cleanup |
| `platform/runtimes/computers/src/retained_writer.rs` | matches Docker engine, provider namespace and Computer instance before admitting a retained volume; the allocator owns persisted physical handoff |
| `platform/runtimes/computers/src/retirement.rs` | one guarded provider deletion for a stopped resource/run; the deletion acknowledgement is separate from allocator proof and maintenance admission |
| `platform/runtimes/computers/src/policy_continuity/checkpoint.rs` and `protocol/maintenance.proto` | private provider-policy checkpoint encoding and source-independent recovery, bound to provider, Computer, source instance, template and run; requires authenticated journal encryption |
| `platform/runtimes/computers/src/policy_continuity.rs` and `allocation.rs::RetainedHandoff` | post-retirement additive policy restoration gated by the allocator's authenticated writer-handoff receipt; image-only declared-input transition preflight, native load confirmation and same-candidate recovery |
| `servers/computers-mcp/` | lifecycle worker, retained-storage preparation, domain policy enforcement, dispatch and Tasks, authenticated public API, validated configuration and browser grant transport; installation packaging and the Console and CLI use this service |
| `apps/console/web/src/computers/MaintenancePanel.tsx` and `maintenanceRequest.ts` | environment update selection, saved idempotent browser intent, invalidation-driven progress and visible recovery through fixed gateway/BFF routes |
| `apps/console/web/src/computers/RecoveryPanel.tsx` and `recoveryRequest.ts` | explicit recovery confirmation, saved paused/cancellation epochs, safe lost-reply retries and current-policy eligibility |
| `platform/computers/src/active_execution.rs` | one metadata query for visible command/file slots and their typed public Task identities |
| `apps/console/web/src/computers/FilesPanel.tsx`, `fileApi.ts` and `fileRequest.ts` | retained-file import/export, Artifact selection, saved retry intent, result downloads and Task cancellation |
| `servers/computers-mcp/src/application/files.rs`, `file_state.rs` and `src/server/files.rs` | public file admission, Artifact capability repair for the actual caller, Task/result authorization and metadata-only Console view |
| `platform/gateway/src/bin/gateway/computers/files.rs` | typed file request and result validation at the fixed public Computer boundary |
| `servers/computers-mcp/src/application/maintenance.rs` and `src/server/maintenance.rs` | public environment updates, stable target selection, typed progress and HTTP receipts; MCP resources and Tasks use the same domain checks |
| `servers/computers-mcp/src/application.rs` and `templates.rs` | shared lifecycle and read views, action flags, availability/quota states and original Create selection across default-template changes |
| `servers/computers-mcp/src/protocol/` and `server/` | authenticated stateless MCP, lifecycle/resource/Task surfaces, shared-outbox subscriptions and Console HTTP routes; service startup and provider readiness in `server/run.rs` and `provider.rs` |
| `servers/computers-mcp/src/server/terminal/`, `attachment_authority.rs`, `access_events.rs` and `runtime_access.rs` | Origin-checked one-use browser attachment, shared browser/CLI revocation wakes and renewal, domain-to-runtime leases, and provider resource/process verification before any bytes flow |
| `servers/computers-mcp/src/server/cli/` | narrow-credential admission, connection fencing, five-method stock CLI gRPC facade and WebSocket byte pump; public SSO/ingress evidence is recorded in the Computers plan |
| `servers/computers-mcp/src/server/pairing.rs` | Origin-checked HTTP pairing and one-use confirmation; the domain stores challenges and owns issuance |
| `platform/computers/contract/src/pairing.rs` | closed pairing DTOs, stock code and loopback-port validation, and one-use credential serialization |
| `platform/computers/transport/src/cli_headers.rs` | stock CLI credential framing at the edge and internal Bearer forwarding marked sensitive; browser cookies grant no CLI access |
| `apps/console/bff/src/computers/pairing.rs` and `cli.rs` | dedicated same-origin SSO entry/CSP and fixed-profile stock root/prefixed byte relays |
| `apps/console/web/src/computers/pairing.ts`, `CliPairingPage.tsx` and `CliConnect.tsx` | code confirmation, loopback delivery/revocation, scoped identity and uncredentialed CLI instructions |
| `servers/computers-mcp/src/config.rs` and `src/bin/computers-mcp.rs` | closed installation configuration, template/trust validation, database-scoped credentials and thin service entrypoint |
| `servers/` | independently deployed MCP servers |
| `testing/` | conformance tooling and multi-process smoke harnesses |
| `sdk/` | language SDK workspaces |
| `deploy/helm/veoveo/` | Kubernetes installation chart, chart-owned first-party service definitions, and typed component/server presets |
| `showcase/uav-sim/deploy/helm/` | GPU simulator, UAV MCP server, isolated generic pilot agents, shared H.264 stream ingress, camera-product configuration, and viewer authorization |
| `deploy/runtime/` | deployment library for component-selected installation and immutable release publication |
| `deploy/runtime/src/profile.rs` | profile validation and ordered lifecycle operations |
| `deploy/runtime/src/sources.rs` and `src/images.rs` | selected immutable source checkouts, installation input checks, qualified image inventories, and Bake selection for profile validation |
| `deploy/runtime/src/snapshot.rs` | Git blob, file inventory, and executable-mode verification for source charts, source values, and installation inputs; `snapshot/tests.rs` exercises index hints, filters, ignored files, and path confinement |
| `deploy/runtime/src/compile.rs` and `src/compile/objects.rs` | component render preparation, non-secret input closure, offline object scope, and sealed inventories for the v7 migration |
| `deploy/runtime/src/compile/inputs.rs` | publication and installation source snapshots keyed by component identity, including retained chart revisions within one repository |
| `deploy/runtime/src/compile/configuration.rs` | component-owned installation document snapshots and retained configuration files across installation commits |
| `deploy/runtime/src/compile/execution.rs` and `src/compile/tests/execution.rs` | snapshot-bound operational inputs and native Git/Helm regression for retained gateway Secret requirements and owner-specific rollout waits |
| `deploy/runtime/src/profile/execution.rs` | compiled gateway/GPU helper inputs and rejection of incompatible retained GPU configurations before installation writes |
| `deploy/runtime/src/profile/coordination.rs` | cluster-wide serialization for cooperating disposable installers, reserved Lease identity, fail-closed interruption, and conditional release |
| `deploy/runtime/src/profile/operations.rs` and `src/installed/unselected.rs` | planned-unit execution receipts and before/after unselected object and Helm observations |
| `testing/deployment-smoke/src/component_scope/` | independent native Git/OCI selected-deployment fixtures, API request metadata, Pod and Helm storage evidence, and namespace cleanup |
| `deploy/runtime/src/compile/images.rs` | release image values and rendered input closure, including retained versions of shared targets |
| `deploy/runtime/src/publication.rs` and `tools/xtask/src/commands/release/components.rs` | component chart, configuration, and image updates, OCI evidence checks, retained dependencies, and publication receipts |
| `deploy/runtime/src/compile/tests/publication.rs` | independent Git/Helm publication regressions for chart-only, configuration-only, and combined updates while unrequested repositories are unavailable |
| `deploy/runtime/src/discovery.rs` | read-only destination scope checks across all locked owners and proposed CRDs, before installation writes |
| `deploy/runtime/src/helm_bundle.rs` | complete prepared Helm renders, literal configuration preservation, and digest-bound CRD retention policy |
| `deploy/runtime/src/helm_state.rs` and `src/ownership.rs` | stored Helm manifest/hook inventories, possible recovery revisions, batched live metadata reads, and pre-write ownership checks |
| `deploy/runtime/src/installed.rs`, `src/installed/`, and `src/helm_state/snapshot.rs` | local installed provenance, stored Helm inventories, live object fingerprints, and verified reuse through the component-selected installer |
| `deploy/runtime/src/installed/normalize.rs` | Kubernetes server dry-run equivalence for normalized quantities and omitted fields when recording a successful installation |
| `deploy/runtime/src/installed/planning.rs` and `src/installed/tests/planning.rs` | installed-state planning, planned-unit execution, and live regressions for reuse, updates, missing receipts, and stale decisions |
| `deploy/runtime/src/gpu/migration.rs` and `src/gpu/migration/tests.rs` | direct device-plugin retirement/quiesce inventory before profile writes, selected workload ownership, observation checks, and isolated live API regression |
| `deploy/contract/src/components/installed.rs` | typed non-secret installed-unit receipts and provenance/inventory validation |
| `deploy/contract/src/components/history.rs` | historical Helm retirement within declared namespaces and rejection of cross-component or cross-target ownership transfer |
| `deploy/runtime/src/charts.rs` | shared chart-lock construction and validation, ordered values, rendering, and Helm release commands |
| `deploy/runtime/src/configuration.rs` and `src/cluster.rs` | pre-mutation Secret closure, gateway activation, public resources, and disposable cluster lifecycle |
| `deploy/runtime/src/gpu.rs` | managed NVIDIA DRA orchestration, ResourceSlice inventory, persistent-claim preservation, and workload placement proof |
| `deploy/runtime/src/gpu/helm.rs` | Helm v4 release metadata, allocator artifact and render verification, and atomic installation |
| `deploy/runtime/src/gpu/admission.rs` | kubelet-plugin selector, DaemonSet readiness, node taint, and pod scheduling diagnostics |
| `deploy/runtime/src/gpu/workloads.rs` | typed Deployment selector, current ReplicaSet ownership, Ready Pod/container, replica-count, and in-container GPU evidence targeting |
| `deploy/runtime/src/gpu/workloads/quiescence.rs` | child UID inventory and Pod-exit verification before retiring a device plugin |
| `testing/deployment-smoke/` | focused Helm configuration, deployment-profile, and per-revision GitOps convergence CLI; passive observation issues no reconciliation requests; `src/helm_config.rs` owns configuration assertions shared with the full suite |
| `testing/deployment-smoke/src/helm_config/gitops.rs` | immutable OCI source and generated Helm values references, checked against the Bioma reference in both component-update directions |
| `testing/deployment-smoke/src/helm_config/jobs.rs` | rendered initialization Job identity checks across Helm and chart revisions, complete spec changes, and long release names |
| `testing/browser-smoke/` | focused headed-browser acceptance over an already-running simulation, mandatory Console and standalone App host preflights, and live-view recovery checks across container restarts |
| `testing/browser-smoke/src/browser/artifact_upload.rs` | public Console upload preflight and real large-file selection, pause/reload/reselection, navigation, durable receipt, and hardware-browser evidence |
| `testing/browser-smoke/src/browser/artifact_upload/resume.rs` | continuation of an interrupted large-upload acceptance fixture with the same upload identity, independent SHA-256, and public HEAD/Range checks |
| `testing/browser-smoke/src/browser/artifact_upload/ux.rs` | installed upload keyboard/clipboard actions, receipt recovery behind filters, narrow/desktop transfer/error states, and acknowledged cancellation |
| `platform/gateway/src/bin/gateway/admin/console/artifact.rs` | direct artifact detail lookup outside the latest catalog window |
| `deploy/helm/common/` | internal labels, image pins, security and GPU helpers bundled into application charts |
| `deploy/offline/` | pinned image manifest, bundle builder/loader, offline values |
| `showcase/sumo/` | real SUMO/TraCI domain showcase |
| `showcase/uav-sim/` | Google 3D Tiles UAV simulation showcase over Isaac, Cesium, Newton, Warp, and PX4 |
| `examples/bioma/acceptance/src/pilot_consolidation.rs` | installation-only shared UAV definition cutover, with atomic drain checks and retained identity and memory acceptance |
| `examples/bioma/` | executable enterprise GitOps reference with Bioma-owned desired state |
| `examples/bioma/platform/flux/` | pinned Flux controller fixture for the local Bioma cluster; it is installed before the installation's desired state, and Veoveo's runtime does not own it |
| `examples/bioma/gitops/` | Flux Git source, OCI chart sources, platform and workload Helm releases, and installation-owned edge resources |
| `examples/bioma/gateway.json` | the reference installation's complete control plane: 16-server MCP catalog, OAuth clients, policy rules, and routes |
| [`examples/bioma/acceptance/`](../examples/bioma/acceptance/DESIGN.md) | owner-local compiled composition checks and the retained-pilot ownership migration; native record and volume recovery rehearsals |
| `sdk/python/` | Python platform package for hosted MCP servers |
| `templates/python-mcp/` | Python server template (`datasheet`) |
| `testing/fixtures/chart-library-consumer/` | anonymous cross-release Helm library acceptance fixture |
| `testing/fixtures/platform-selection/` | anonymous deployment v5 platform selection and Artifact/Frames/Map/Media/Recording/RRD image-closure acceptance |
| `testing/fixtures/fork-workload/` | in-repository Python simulation protocol fixture with typed camera and render-product declarations; it is not visual GPU evidence |
| `testing/fixtures/fork-installation/` | local fork workload selection, complete gateway configuration and protocol-only deployment closure |
| `deploy/contract/tests/fork_installation.rs` | downstream workload, reviewed upstream merge, retained image revision and separate installation configuration |
| `deploy/contract/tests/component_ownership.rs` | pure component selection, unchanged dependencies, mixed-release rejection, previous Helm inventory checks, and immutable input ownership tests |
| `deploy/contract/tests/component_reuse.rs` and `tests/support/components.rs` | independent Git-history input reuse, content-based upgrade decisions, and shared atomic component fixtures; these tests do not execute Kubernetes mutations |
| `deploy/contract/tests/source_chart_content.rs` | real Git-history and separate-checkout chart identity tests, export-attribute coverage, executable modes, and source path boundaries |
| `testing/fixtures/simulation-overlay/` | repository-neutral overlay identity and CUDA probe for shared simulation-base acceptance |
| `tools/image-build/source-freshness.rs` | content comparison and timestamp synchronization for Cargo inputs under the target-cache lock |
| `tools/image-build/` | registry-neutral managed BuildKit base configuration, shared Rust builder inputs, and the source-locked first-party Datasheet image environment |
| `tools/xtask/src/commands/image/browser_compilation_tests.rs` | real Bake and Cargo planning regression for a stable browser compiler action across standalone and platform selections |
| `tools/xtask/` | compiled repository command, enforcement, local test reporting, typed smoke prerequisite builds and dispatch, image planning, profile-registry builder configuration, and release orchestration |

## Placement Rules

The top-level directories express ownership rather than implementation language. A Rust
crate belongs beside the system it implements; Rust is not an architectural boundary.

| Root | Put code here when it owns |
|---|---|
| `servers/` | a hosted MCP server with its own protocol surface, deployment image, and domain behavior |
| `mcp/` | protocol contracts, transport extensions, or bridges shared by more than one server |
| `platform/` | internal control/data-plane services, durable stores, and reusable execution runtimes |
| `agents/` | autonomous agent behavior or durable agent scheduling |
| `apps/` | a user-facing application and its application-specific backend |
| `testing/` | cross-component conformance, smoke, and deployment verification |
| `sdk/` | a language-native client or server-development package |
| `showcase/` | an end-to-end domain integration that is not part of the core installation |

MCP servers do not live under a generic `tools/` root. They expose resources, prompts,
tasks, subscriptions, notifications, and structured content in addition to tools, so
`servers/` names the deployable boundary without narrowing the protocol.

## Shared Contracts

### `mcp/apps-extension`

MCP Apps (SEP-1865 / ext-apps "2026-01-26") support: pinned protocol
constants (`io.modelcontextprotocol/ui`, `text/html;profile=mcp-app`), typed
`_meta.ui` shapes, server helpers (capability declaration, `ui://` app
resources, tool links), and host helpers (capability declaration, app
detection, and visibility checks). `admission.rs` resolves the linked and imported
tools a caller may invoke from Console and Workspace Apps.
`asset.rs` loads immutable startup snapshots of packaged App HTML. Map and
Stream declare their image asset inputs separately from Rust compiler sources.
The Console owns the generic reactive-resource
adapter that carries final-profile `subscriptions/listen` wakes across the pinned Apps
bridge without exposing domain payloads. `mcp/apps-extension/DESIGN.md` defines the
server↔core↔UI contract: domain reads are resources, mutations are tools, and
views are `ui://` apps rather than bespoke admin REST or hardcoded console pages.

### `mcp/bridges`

| Component | Responsibility |
|---|---|
| `stdio` | Same-version MCP `2026-07-28` bridge for one explicitly owned local child; the HTTP endpoint is stateless and discovers the child through the final lifecycle |
| `legacy` | Optional, isolated MCP `2025-11-25` external connector for one configured local stdio child or remote HTTP endpoint; it exposes only MCP `2026-07-28` tools, resources, prompts, and completions toward Veoveo and never fabricates Tasks, MRTR, subscriptions, or deprecated capabilities |


### `mcp/contract`

This crate owns types shared across services. A domain tool schema stays in its server,
even when that server is first-party.

| File | Responsibility |
|---|---|
| `access.rs` | artifact access levels, user/group subjects, grant composition |
| `agents.rs` | authenticated operator-message, durable input-request decision, wake-receipt, and pending-input view contracts |
| `artifact_service.rs` | artifact-plane requests, capabilities, share links, native async port |
| `artifact_service/upload.rs` and `artifact_service/upload/policy.rs` | resumable HTTP upload identities, descriptors, receipts, errors, explicit quota policy, and checked multipart layout/manifest validation |
| `internal_auth/upload.rs` | dedicated signed upload assertions bound to the checked control-plane and Work Context |
| `duckdb.rs` | shared DuckDB source types and safe read-function SQL fragments |
| `coordinates.rs` | shared coordinate spaces, world/revision/frame identities, complete frame-tree vocabulary, WGS84 positions, and operation provenance |
| `docs.rs` | build-embedded server documents, once-built revision/compliance declarations, compliance parsing, and llms.txt rendering; observed capabilities come from Discover and list methods |
| `uri.rs` | hosted-server resource URI construction and shared one-segment document URI parsing |
| `storage.rs` | artifact metadata, release state, compliance labels |
| `gateway.rs` | gateway control-plane aggregate and public re-exports |
| `gateway/ids.rs` | validated identity and configuration newtypes, including principal display metadata, which authorization never reads |
| `gateway/auth_config.rs` | IdP, authorization server, OAuth client surfaces |
| `gateway/server_config.rs` | hosted server and profile exposure contracts, including cross-server App resource dependencies |
| `gateway/policy.rs` | actions, targets, rules, effects, audit reason model |
| `gateway/runtime_state.rs` | durable auth/runtime record contracts, including display metadata continuity across authorization-code and refresh grants |
| `internal_auth/request.rs` | signed request-context consistency, original JWT principal and source-token metadata for consumers that recheck current authority; shared Rust/Python fixtures in `testing/fixtures/gateway-request-context.json` |
| `gateway/validation.rs` | fail-closed cross-reference and invariant validation |
| `internal_auth.rs` | Ed25519 signing keys, JWKS trust, internal issuer/verifier |
| `deployment.rs` | Connected/offline Kubernetes topology contract |
| `bootstrap.rs` | generic installation-time server bootstrap envelope, constants, and semantics |
| `tasks.rs` | platform task ownership and durable routing vocabulary; official MCP Task wire types come from `rmcp` |
| `provider.rs` | provider job/event contracts; no status polling API |
| `subscriptions.rs` | request-scoped resource and list-change event hub for final `subscriptions/listen` streams |
| `protocol.rs` | sole final MCP revision, shared cache lifetimes, and W3C trace metadata validation |
| `transport.rs` | stateless Streamable HTTP configuration, no-session adapter, and the 8 MiB whole-response JSON limit |
| `telemetry.rs` | tracing/log initialization and guards; explicit blocking OTLP/HTTP clients for OS-thread batch export |

### `platform/recordings/rrd`

Owns cross-domain Rerun/RRD spacetime types, adapters, encoded-video boundary
inspection, encoded-video extraction and MP4 remux, ingest-part discovery, segment
verification, recording-layer Store ID normalization, deterministic properties layers,
and Arrow IPC export.
[`DESIGN.md`](../platform/recordings/rrd/DESIGN.md) specifies these shared file operations. Domain results that do not overlap Rerun
concepts stay local to their MCP crate.

### `platform/recordings/reader`

[`DESIGN.md`](../platform/recordings/reader/DESIGN.md) specifies the shared analysis reader.
`read.rs` owns authorized plans and snapshots; `access.rs` owns shared visibility and
confined paths; `cache.rs` owns verified bounded Artifact materialization and leases.
It depends on neither Hub nor Recording MCP. Server-only Blueprint validation stays
in Recording MCP. Stream and Reason consume this library through the video materializer.

### `platform/recordings/video`

Owns video selection and task-start materialization for Stream replay and Reason. It
takes Recording reader plans, combines immutable Artifact-backed layers with acknowledged
live ingest parts, and remuxes the selected H.264 range without re-encoding.

### `platform/recordings/protocol`

Owns the versioned protobuf contract for authenticated recording streams, batches,
checkpoints, discovery, and errors. Gateway, Recording Hub, the forwarder, and smoke tests
share this crate.

## SurrealDB Platform Store

### `platform/store`

The only durable platform persistence layer.

| File | Responsibility |
|---|---|
| `config.rs` | root/database auth configuration and validation |
| `migrations.rs` | ordered SurrealDB 3.2 schema migrations |
| `migration_preparation.rs` | online index preparation before migration 0072; checks the physical definition and readiness without changing the published checksum |
| `models.rs` | persisted Rust record and enum definitions |
| `ids.rs`, `table.rs` | domain-specific record IDs and table identities |
| [`workspace/`](../platform/store/src/workspace/DESIGN.md) | shared-chat persistence: transactional membership and invitations, immutable messages, committed event order, and replay; clients and agent execution both read and write through it |
| `recording_catalog.rs` | recording datasets and layers, durable read grants, projection receipts, expiry, and cleanup |
| `administration.rs` | bootstrap, runtime user, migration administration |
| `identity.rs`, `identity/ensure.surql` | tenant/principal/group resolution; transactional identity creation and presentation-only principal updates that preserve current disablement and security fields |
| `gateway_runtime.rs` | control revisions, auth state, refresh/JWT runtime records |
| `artifacts.rs` | blob, occurrence, grant, share, capability transactions |
| `artifacts/publication.rs` and `artifacts/register.surql` | shared typed publication content and transactional occurrence, grants, and outbox registration with immutable tenant/digest blob reuse |
| `artifact_uploads.rs` and `artifact_uploads/` | typed upload ledger, policy-bound idempotent admission, and atomic tenant reservations |
| `artifact_uploads/parts.rs` and its SurrealQL statements | immutable part descriptors, generation-fenced receipts, shared transfer budgets, and unknown-length reservation windows |
| `artifact_uploads/lifecycle.rs` and `artifact_uploads/publication.rs` | fenced initialization/finalization, manifest freeze, atomic occurrence and receipt publication, cancellation, and retained cleanup accounting |
| `migrations/0050_artifact_uploads.surql` | durable upload/part state, storage accounting, and repository-owned current-authority digest functions |
| `artifact_reads.rs`, `artifact_reads/` | task-bound read delegation, current policy identity, and atomic distinct-occurrence quotas; specified in the Artifact service design |
| `coordinates.rs`, `frame_worlds.rs` | coordinate-operation persistence plus authored frame worlds and immutable tree revisions |
| `map.rs` | source, release, active-pointer, mobility, restriction, snapshot, route, matrix, and acquisition persistence |
| `map_authoring.rs` | Work Context-scoped feature layers, immutable schema/style/feature revisions, atomic changesets, heads, publications, and authoring outbox events |
| `map_projection.rs` | indexed Map changeset replay up to the committed Map head |
| `map_presentations.rs` | immutable publication products plus publication-pinned map compositions and revisions |
| `time.rs` | authority sources and releases, active pointers, acquisitions, calendars, epochs, clock policy, and events |
| `recordings.rs` | recording lifecycle and visibility |
| `recording_ingest.rs`, `recording_blueprints.rs` | producer streams, idempotent batch checkpoints, immutable producer Blueprint revisions, and journal state |
| `usage.rs` | shared domain/media usage records |
| `resource_changes.rs` | shared domain LIVE invalidations, coalescing and reconnect recovery; composed into Time and Recording resource hubs |
| `outbox.rs`, `changefeed.rs` | transactional events, checkpoints, LIVE acceleration and database-wide changefeed pages that complete transaction tails before cursor advancement |
| `live_views.rs` | append-only audit records for simulator live-view products and ephemeral viewer authorizations |
| `migrations/0040_uav_vehicle_authority.surql` | UAV-owned principal-to-vehicle grants, admitted single-vehicle mission plans, and exclusive command leases scoped by tenant and Work Context |
| `store.rs` | connection and transaction helpers over domain records |

Migrations `0001` through the current version live under `migrations/`. Runtime services
never apply them; installation bootstrap does.

Shared real-store integration setup lives in [`testing/fixtures/DESIGN.md`](../testing/fixtures/DESIGN.md).
Computers admission and Task recovery reuse its isolated pinned database lifecycle.

## Durable Tasks

### `platform/task-runtime`

`runtime/subscriptions.rs` owns per-Task baselines, filtered outbox replay and shared
wake-source lifetime. Workspace `operations/progress.rs` ties measured progress to its
request and dispatch fence.

`src/provider_resume.rs` joins an authorized domain recovery request to the shared
observation lease and cancellation epoch in one transaction.

| File | Responsibility |
|---|---|
| `types.rs` | runtime configuration, recovery classes, pins, claims, outcomes |
| [`DESIGN.md`](../platform/task-runtime/DESIGN.md) | durable Task and recovery-class contract, provider observation, migration and rollback |
| `runtime.rs` | create/idempotency, update, cancel, finish, subscriptions and prune |
| `leases.rs` | distinct execution/observation claims and lease renewal |
| `provider_transaction.rs` | fences domain journal writes with the current Task observation lease in one transaction; cancellation prevents new dispatch |
| `recovery.rs` | restart profiles; uncertain provider outcomes and cancellations stay pending |
| `mcp.rs` | conversion from stored state into official RMCP Task and DetailedTask types |
| `service.rs` | protocol-neutral service that RMCP handlers call |
| `lib.rs` | focused public API |

Task state lives in this runtime. RMCP defines the Tasks wire types.

## Gateway

### Library surface: `platform/gateway/src`

| Path | Responsibility |
|---|---|
| `catalog.rs` | validated active catalog and profile/server lookup |
| `control_store.rs` | immutable SurrealDB control revisions and activation |
| `auth/` | access tokens, OIDC, ID-JAG, client assertions, immutable principals, and independently typed OIDC display labels |
| `policy.rs` | gateway catalog adapter for the shared evaluator in `platform/policy` |
| `mcp_support.rs` | MCP URI rewriting, including declared cross-server resource identities |
| `mcp/authorization.rs` | per-method/profile/server target authorization |
| `mcp/discovery.rs` | discovery cache keyed by caller authority, concurrency limits, per-server failure isolation, and list-change invalidation |
| `mcp/tools.rs` | aggregated tool list with opt-in compatibility helpers; isolates a failing server by default and fails the whole list for `fail_closed` discovery profiles |
| `mcp/resources.rs` | failure-isolated resource lists plus fail-closed read/subscribe routing |
| `mcp/prompts.rs`, `completion.rs` | prompt and completion aggregation |
| `mcp/tasks.rs` | upstream Task client and the opt-in Task tools for clients with weak Task support |
| `mcp/health.rs` | `health_url` GET probes; only a success status counts as healthy |
| `mcp/upstream*.rs` | authenticated Streamable HTTP, session-local protocol state, and catalog-revision-scoped sharing of transport-equivalent HTTP/TLS clients |
| `state/audit.rs` | policy decision and audit records |
| `state/auth_state.rs` | durable OAuth authorization and replay state |
| `state/refresh_tokens.rs` | refresh family issue/rotate/replay/revoke/GC plus signed-in display-label continuity |
| `state/session.rs` | access-token session-family binding and revocation checks |
| `state/subscriptions.rs` | durable subscription ownership and forwarding |
| `state/task_routes.rs`, `state/task_routes/ownership.rs` and [`state/task_routes/DESIGN.md`](../platform/gateway/src/state/task_routes/DESIGN.md) | opaque upstream Task mapping, idempotent concurrent registration, actor/provenance ownership, permission checks and route expiry; a version-0 reader handles rows from before migration 0081 added ownership metadata |
| `secrets.rs` | secret-source models and environment/file/Vault resolution |

### Binary surface: `platform/gateway/src/bin/gateway`

| Path | Responsibility |
|---|---|
| `server.rs` | router assembly only |
| `runtime.rs` | shared application state and HTTP clients |
| `oauth/`, `oauth_grants/` | authorize/callback/token and grant handlers |
| `admin/control_plane.rs` | control revision read/update |
| `admin/tasks.rs` | policy-checked cancellation through the owning server's official Tasks endpoint |
| `admin/artifacts.rs` | release/grant/link mutations through artifact service |
| `admin/console/mod.rs` | console snapshot handler, trusted display names for every principal (authenticated identity wins), branding, stream cursor bootstrap |
| `admin/console/projection.rs` | tenant projection load and per-entity summary builders |
| [`admin/console/DESIGN.md`](../platform/gateway/src/bin/gateway/admin/console/DESIGN.md), `stream.rs` | live Console SSE: LIVE wake hub, one database replay cursor, tenant filtering, limits and current agent leases |
| `admin/console/health.rs` | background MCP server health prober and cache |
| `admin/server_proxy.rs` | generic policy-checked proxy to a hosted server's contract-defined admin API |
| `artifact_download.rs` | authorized/audited large download proxy |
| `artifact_upload.rs` | profile-authorized public resumable upload routes, signed current-policy binding, and streaming proxy |
| `recording_playback.rs` | authorized/audited playback manifest and framed live-stream pass-through |
| `audit.rs` | common admin authorization and operation audit helpers |

`gateway.rs` is the thin CLI/serve entrypoint.

## Artifact Plane

### `platform/artifacts/service`

[`DESIGN.md`](../platform/artifacts/service/DESIGN.md) specifies the internal Artifact
plane and task-read delegation.

| File | Responsibility |
|---|---|
| `service.rs` | policy enforcement, grants, release, shares, quotas, retention |
| `ledger.rs` | repository contract and in-memory test implementation |
| `ledger/surreal.rs` | SurrealDB repository adapter |
| `service/read_capability.rs`, `ledger/read_capability.rs`, `ledger/surreal/read_capability.rs` | delegated read policy, focused repository contract, and durable adapter |
| `service/write_capability.rs` | task-bound output capability issuance and redemption, inherited sensitivity labels and occurrence publication |
| `http/read_capability.rs` | current task scope, Artifact read routes and gateway-authenticated issuance/revocation |
| `store.rs` | memory/S3 blob storage and signed download behavior |
| `store/multipart.rs` and `uploads/` | restartable multipart storage adapter, transfer orchestration, session views, and finalization/cleanup recovery |
| `store/s3_multipart.rs` and `tests/s3_upload.rs` | S3 enumeration and native installed-storage acceptance for uncertain acknowledgements, restored handles, and physical cleanup |
| `auth.rs` | internal assertion verification and plane caller |
| `http.rs` | internal artifact API plus `/s/{token}` redemption |
| `http/disposition.rs` | occurrence-owned UTF-8 download filenames for full, range and HEAD responses |
| `http/uploads.rs` | upload assertion verification, size-limited control bodies, and streamed part transport |
| `config.rs` | fail-closed store/database/audience configuration |

### `platform/artifacts/client`

HTTP implementation of the `ArtifactPlane` interface used by domain servers and the
gateway. It forwards the caller's existing signed identity; it never signs one.
`read_capability.rs` also consumes explicitly issued task-read authority through
the dedicated internal read routes.

### `servers/artifact-mcp`

The MCP server for the artifact plane. `handler.rs` owns tools/resources,
`prompts.rs` owns reusable workflows, and `subscriptions.rs` owns update notification
plumbing.

## Domain Servers

Rust MCP servers share this module layout:

| Local module | Responsibility |
|---|---|
| `contract.rs`, `contract/`, or `domain/` | tool, resource, problem, and result types owned by the domain |
| `engine.rs`, `forecast.rs`, `compiler/`, or another focused engine module | pure domain computation or deterministic provider compilation |
| `verification/` | provider-independent checks when the domain publishes separately verified results |
| `executor/` | private typed provider protocol and client; not a public API |
| `state.rs` | server-local provider and domain models, not task persistence |
| `uris.rs` | server resource URIs |
| `artifacts.rs` | task-bound capability preparation/redemption |
| `administration.rs` | transport-neutral domain administration behind `map:…`-style admin-scoped MCP tools |
| `assets/` | self-contained `ui://` MCP App views served through `read_resource` |
| `bin/server/config.rs` | validated CLI/environment configuration |
| `bin/server/internal_auth.rs` | required gateway assertion middleware |
| `bin/server/ownership.rs` | principal/tenant/label task ownership |
| `bin/server/task_extension.rs` | final extension adapter over TaskRuntime |
| `bin/server/app_state.rs` | dependency composition and recovery |
| `bin/server/outputs.rs` | result models, resource links, and usage reporting |

Current MCP crates under `servers/` are indexed here:

| Path | Primary ownership |
|---|---|
| `servers/artifact-mcp` | MCP resources, tools, prompts, and subscriptions over the artifact plane |
| `servers/duckdb-mcp` | arbitrary analytical SQL, ingest/export, and DuckDB Spatial |
| `servers/frames-mcp` | complete rooted frame worlds, immutable revisions, coordinate conversion, and operation provenance |
| `servers/map-mcp` | Earth geography, feature authoring and products, source and raster releases, reusable spatial derivation, mobility validation, logistics routing, and immutable cuOpt travel models |
| `servers/media-mcp` | webhook-completed provider media work and artifact outputs |
| `servers/optimization-mcp` | typed cuOpt routing and route-scenario problems, convex and MILP models, GPU execution, independent verification, and immutable problem/run/solution records |
| `servers/stream-mcp` | admitted live and replay GStreamer execution, typed pipeline profiles and results, encoded preview, and the Stream MCP App |
| `servers/reason-mcp` | local recorded-video reasoning, grounding, and Rerun annotations |
| `servers/recording-mcp` | recording catalog, queries, subscriptions, and sealing |
| `servers/timeseries-mcp` | time-series analysis, forecasting, evaluation, and artifact output |
| `servers/timeseries-mcp/src/bin/server/usage_index.rs` | authorization-filtered usage discovery with stable task ordering and opaque cursors |
| `servers/time-mcp` | temporal authority, clock assessment, operational calendars, mission timelines, and events |
| `servers/view-mcp` | immutable scene compositions, owner and Work Context scoped geospatial views, shared 3D Tiles streaming, GPU overlays, and captured frames |
| `servers/uav-sim-mcp` | provider-neutral UAV simulation sessions, principal-to-vehicle grants, Map route admission, exclusive command leases, missions, telemetry, tasks, recording references, simulator-owned logical cameras, one shared tiled GPU product, authenticated H.264 fanout, and the UAV App |

The packaged Node chart server keeps its Veoveo boundary beside the image:

| Path | Responsibility |
|---|---|
| `servers/chart-mcp/server.mjs` | mounted Streamable HTTP lifecycle, hosted identity, well-known resources, and derived declaration over the pinned upstream chart server |
| `servers/chart-mcp/internal-auth.mjs` | fail-closed Ed25519 gateway-token verification shared by the chart MCP and admin docs routes |

### UAV Simulation Integration

| Path | Responsibility |
|---|---|
| `servers/uav-sim-mcp/src/server/live_view.rs` | actor/browser stream authorization, shared camera-product selection, token renewal, closure, expiry, and connection state |
| `servers/uav-sim-mcp/src/server/live_stream.rs` | authenticated WebSocket admission and byte-transparent H.264 forwarding from simulator-owned camera products |
| `servers/uav-sim-mcp/src/server/live_view_audit.rs` | audit records for camera, product, authorization, denial, expiry, and revocation events |
| `servers/uav-sim-mcp/src/server/runtime_events.rs` | strict authenticated adapter-ready and final-ready HTTP stream ingestion, immutable binding reapplication trigger, and subscribed live-camera notification |
| `servers/uav-sim-mcp/assets/live-app.html` | self-contained camera selection and multi-view live App |
| [`showcase/uav-sim/agents/`](../showcase/uav-sim/agents/DESIGN.md) | managed pilot seed instructions and domain memory ownership; geographic work data remains outside agent memory |
| [`showcase/uav-sim/deploy/helm/`](../showcase/uav-sim/deploy/helm/DESIGN.md) | simulator packaging and managed pilot manifest/memory schema under `files/agent-template`; no per-pilot workloads or credentials |
| `servers/uav-sim-mcp/src/server/agent_targets.rs` | App message-target discovery from managed identity and active vehicle grants, with shared database catalog invalidation |
| `showcase/uav-sim/map/` | Map-owned named-place and operational air-network source fixture for the showcase |
| `showcase/uav-sim/runtime/` | thin domain overlay on the shared Isaac runtime with Cesium, a repository-owned batched Warp plant, Newton Experimental rigid views, PX4 HIL lifecycle, RTX domain sensors, logical cameras, shared RTX/NVENC camera products, direct Stream publication, and Rerun publication |
| `showcase/uav-sim/runtime/veoveo_uav_sim/fleet_runtime.py` | 30 Hz CUDA fleet simulation, direct Newton Experimental tensor-state writes, and ordered 60 Hz PX4 HIL publication without MuJoCo-Warp stepping |
| `showcase/uav-sim/runtime/veoveo_uav_sim/plant_warp.py` | one fused CUDA kernel for batched motors, force, torque, native Newton body integration, launch-surface contact, and HIL sensor sampling |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera.py` | operator-camera orchestration over focused rig, smoothing, product, and health modules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_rigs.py` | target sampling and desired poses for every supported camera rig |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_smoothing.py` | frame-rate-independent position and shortest-arc orientation filters with typed reset rules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_products.py` | one continuous tiled RTX/NVENC product and keyframe-aware H.264 access-unit ring shared by every streamable logical camera |
| `showcase/uav-sim/runtime/veoveo_uav_sim/render_pose.py` | diagnostics comparing simulator camera poses with rendered Hydra frames |
| `showcase/uav-sim/runtime/veoveo_uav_sim/physical_camera.py` | body-and-mount USD sensor camera, separate from smoothed operator views |
| `showcase/uav-sim/runtime/veoveo_uav_sim/hydra_camera.py` | physical-camera Hydra product, CUDA AOV-to-native-RTSP configuration, encoded-frame pairing, and nonblocking sensor health |
| `showcase/uav-sim/runtime/veoveo_uav_sim/rtsp_h264.py` | pod-local RTSP client, interleaved RTP parser, RFC 6184 depacketizer, and native encoded access-unit delivery |
| `showcase/uav-sim/runtime/veoveo_uav_sim/h264.py` | strict native Annex B GOP parsing and decoder-reentrant SPS/PPS/IDR qualification |
| `showcase/uav-sim/runtime/veoveo_uav_sim/runtime_events.py` | retained nonblocking adapter-ready and final-ready lifecycle edges over the authenticated private HTTP stream |
| `showcase/uav-sim/runtime/veoveo_uav_sim/tile_lifecycle.py` | generation-safe reduction of redacted native Cesium load events, cache-preserving provider-session replacement proved by prepared geometry, prepared materials, and rendered coverage, and current coverage state without polling |
| `showcase/uav-sim/runtime/patches/cesium-0.29.0-external-viewports.patch` | pinned headless viewport-authority switch that prevents interactive window discovery from clearing simulator-managed Cesium views |
| `showcase/uav-sim/runtime/patches/cesium-0.29.0-lifecycle-events.patch` | pinned Omniverse extension events, load generations, and query-secret log redaction |
| `showcase/uav-sim/runtime/patches/cesium-native-ca0311f-tile-load-events.patch` | pinned Cesium Native child-content failure delivery through the existing tileset callback |
| `servers/uav-sim-mcp/src/server/world_bootstrap.rs` | strict startup application and reactive same-binding reapplication of an installation-owned immutable world binding |
| `showcase/uav-sim/deploy/` | commit-addressed OCI publication, MCP-configured GPU simulator workload, four identity- and storage-isolated generic pilot workloads, shared H.264 ingress, continuous camera products, versioned persistent cache, typed sensor configuration, and network policy |
| `showcase/uav-sim/scenarios/` | reusable world trees plus strongly typed live mission and acceptance parameters outside the Isaac image context |
| `examples/bioma/uav-sim-values.yaml` | reference camera, product, public gateway origin, and recording tenant binding |
| `testing/flight-smoke/src/domain.rs` | runtime world publication plus credentialed Google tiles, PX4, independent live Stream processing, Recording Hub replay, Reason, and concurrent GPU acceptance |
| `testing/flight-smoke/src/domain/showcase.rs` | showcase UAV cameras and products, authenticated Console checkpoints, Rerun playback, and evidence tied to a revision |
| `testing/browser-smoke/src/browser.rs` | shared headed Chrome attachment, hardware WebGPU-or-WebGL enforcement, opaque-origin App hosting, Map workspace viewport acceptance, dedicated simultaneous-viewer windows, Console live-view interaction, and screenshots |
| `testing/flight-smoke/src/cli.rs`, `src/domain/` | focused flight CLI, scenario validation, authenticated MCP client, world admission, live Stream assertions, and artifact checks; contains no service implementations |
| `testing/browser-smoke/src/main.rs` | focused headed-browser commands and versioned evidence manifests for the Map workspace and UAV visual workflows |
| `testing/browser-smoke/src/restart.rs` | focused same-document native live-view recovery across independent MCP-pod and simulator-container restarts, including proof that MCP replacement leaves the GPU pod unchanged |
| `testing/browser-smoke/src/browser/recording_acceptance.rs` | scoped Redap network evidence, live-source continuity, archive-request rejection, and nonblank Rerun viewport measurement |

### Geospatial Domains

Geospatial work is split across three servers:

| Path | Responsibility |
|---|---|
| `servers/map-mcp` | Earth geography, complete immutable source features, COG rasters and terrain derivations, reusable spatial geometry and mobility validation, authored GeoJSON/JSON-FG layers, OGC GeoPackage vector transfer, source acquisition, release activation, DuckDB Spatial analytics, CRS and geodesic work, geofences, restrictions, Valhalla land routing, network routing, matrices, Optimization travel models, and reachable areas |
| `servers/frames-mcp` | ECEF-rooted world trees, geodetic/static/dynamic transforms, immutable revisions, coordinate conversion, batch tasks, operation provenance, artifacts, and usage |
| `servers/view-mcp` | static scene compositions, configured 3D scene layers, camera rigs, pinned Map/Frames/Artifact inputs, overlays, NVIDIA-accelerated rendering, and frame resources |

The crate-local design documents own their protocol, administration, persistence, and
deployment details.

View composition work starts in `src/contract/composition.rs`, which owns
typed identities, inputs, Frames bindings, overlay geometry, styles,
validity, and bounds. `src/composition.rs` resolves artifact bytes and
converts validated overlays into GPU render products. `src/state.rs` owns
principal and Work Context scoped composition, view, capture snapshot, and
frame state. `src/mcp.rs` publishes the tools and resources, while
`src/server/tasks.rs` persists recoverable capture snapshots.

Map authoring is split by responsibility. `src/contract/features.rs` owns feature wire
types and bounds, while `src/contract/compositions.rs` owns publication products and
composition contracts. `src/contract/transfers.rs` owns durable import, export, and
vector-product task contracts. `src/authoring/service.rs` applies Work Context policy
and optimistic concurrency. `src/authoring/projection.rs` replays the
SurrealDB Map changeset log through a fixed committed Map head;
`src/authoring/projection/recovery_tests.rs` verifies indexed paging, persisted
checkpoint recovery, unrelated traffic, and incomplete-revision rejection,
while `src/authoring/query.rs` owns the parameterized DuckDB Spatial and CQL2
queries. `src/authoring/query/performance.rs` owns the 10k, 100k, and
million-feature R-tree plan, correctness, maintenance, latency, throughput, and
storage gates. `src/authoring/presentations.rs` owns immutable products and
composition revisions. `src/authoring/transfers.rs` owns GeoJSON
and RFC 8142 transfer plus GeoParquet 1.0 and MVT 2.1 products.
`data/src/map_data/feature_package.py` and `src/feature_packages.rs` own the
pinned-GDAL OGC GeoPackage inspection and conversion boundary.
`src/mcp/authoring.rs` publishes the write and query tools. `src/server/tasks.rs` owns
durable execution and task-local staging, while
`src/server/tasks/feature_transfers.rs` owns GeoPackage-aware transfer execution.
`app/` owns the MapLibre bundle pipeline and the permission-aware source.
`app/bridge.js` owns parent-only MCP messages, request deadlines and subscription
acknowledgments; `app/resources.js` owns permission-filtered, concurrent snapshot
reads and targeted refreshes. `app/workspace.js` composes them with the map UI,
while `assets/workspace-app.html` is the generated self-contained Map MCP App
for composition viewing, feature authoring, and administration. The SurrealDB schemas are
`platform/store/migrations/0025_map_authoring.surql`
and `platform/store/migrations/0026_map_authoring_products.surql`.
`platform/store/migrations/0047_map_projection_sequence.surql` adds the recovery
index and transactional Map commit head.
`platform/store/migrations/0048_map_projection_head_backfill.surql` initializes
populated installations from existing rows after the index migration commits;
`platform/store/tests/surreal_integration/map_projection.rs` verifies that upgrade.

Immutable acquisition products use a separate analytical path.
`src/contract/source_products.rs` owns complete source-feature, raster-product,
query, and derivation contracts. `src/release_products.rs` projects activated
GeoJSON, GeoJSON Sequence, and raster metadata into DuckDB Spatial.
`data/src/map_data/adapters/` owns source normalization, while
`data/src/map_data/raster_ops.py` performs the controlled GDAL derivations.
`src/raster.rs` supervises that helper and `src/server/tasks.rs` owns its
durable artifact publication. `src/geography.rs` owns direct
position, location, and corridor inspection plus restriction publication,
surfaced through tools such as `inspect_position`, while `src/analytics.rs`
owns the sandboxed DuckDB Spatial engine behind those reads.
`src/analytics/performance.rs` owns the active-source 10k, 100k, and
million-feature R-tree plan, correctness, latency, throughput, and storage
gates.

Reusable spatial planning is split from routing. `src/contract/spatial.rs`
owns the operation and persisted-result schemas. `src/spatial/derive.rs`
implements pure geometry, `src/spatial/projection.rs` owns the local
map-projection profile, and `src/spatial/validation.rs` resolves mobility envelopes
and active restrictions. `src/spatial/mod.rs` binds catalog authority,
provenance, and DuckDB persistence.

### Optimization And Travel Models

| Path | Responsibility |
|---|---|
| `servers/map-mcp/src/contract/travel_models.rs` | `veoveo.io/travel-model-artifact/v1` cross-server wire profile, controlled location and vehicle-type IDs, bounds, provenance, and Map record |
| `servers/map-mcp/src/routes/service.rs` | route and Valhalla matrix construction, immutable mobility-profile versions, persisted operational snapshots, unavailable arcs, and the validated `veoveo.io/map-route-handoff/v1` cross-server handoff |
| `servers/map-mcp/src/server/tasks.rs` | travel-model publication task, owner visibility, neutral artifact manifest identity, and resource notifications |
| `servers/optimization-mcp/src/domain/` | public routing, route-scenario, convex, MILP, solution, verification, and solver-profile contracts |
| `servers/optimization-mcp/src/compiler/` | deterministic conversion into cuOpt routing arrays and sparse mathematical structures |
| `servers/optimization-mcp/src/verification/` | cuOpt-independent routing feasibility, mathematical feasibility, integrality, and objective checks |
| `servers/optimization-mcp/src/executor/` | private Unix-socket protocol and Rust client |
| `servers/optimization-mcp/executor/` | pinned Python cuOpt 26.08 GPU adapter and hardware health check |
| `servers/optimization-mcp/src/bin/server/` | MCP tasks, GPU queue, problem/run/solution resources, artifact publication, prompts, and identity |
| `servers/optimization-mcp/src/bin/server/index.rs` | authorization-scoped lookup, stable pagination with opaque cursors, completion search, and usage discovery |
| `deploy/contract/src/lib.rs` | portable Optimization capability, Optimization image closure, and mandatory `cuopt-executor` GPU scheduling declaration |
| `deploy/helm/veoveo/definitions/domain-services.yaml` | single Optimization Pod, CPU control container, one-GPU cuOpt sidecar, shared socket, memory-backed shared memory, and persistent workspace |
| `examples/bioma/images/` | independent platform and UAV image locks, each matching its release’s rendered image closure |
| `testing/smoke/src/bin/smoke/scenarios/agent_kernel.rs` | full Pilot mission flow through gateway task dispatch, cuOpt MILP execution, independent verification, wake delivery, and durable memory |

### Temporal Domain

| Path | Responsibility |
|---|---|
| `servers/time-mcp` | authority-bound time resolution and conversion, calendar expansion, timeline validation, interval algebra, clock assessment, mission epochs, and temporal events |
| `servers/time-mcp/src/acquisition/` | bounded IANA TZDB and leap-second acquisition, validation, compilation, and staging |
| `platform/store/src/time.rs` | tenant temporal catalog, optimistic release activation, acquisition-to-release provenance lookup, owner events, and clock policy |

[`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) covers the
protocol, authority, administration, deployment, and synchronization-observation
contract.

Media-specific ownership:

| Path | Responsibility |
|---|---|
| `servers/media-mcp/src/provider.rs` | provider-neutral registry/submission adapter |
| `servers/media-mcp/src/webhook.rs` | signature parsing and constant-time verification |
| `servers/media-mcp/src/bin/server/generation_task.rs` | durable submission/WebhookWait/terminal flow |
| `servers/media-mcp/src/bin/server/artifact_tools.rs` | explicit small-content compatibility helper |
| `servers/media-mcp/src/bin/server/retention.rs` | platform-owned retention reconciliation |

DuckDB-specific ownership:

| Path | Responsibility |
|---|---|
| `servers/duckdb-mcp/DESIGN.md` | public contract, runtime boundary, tasks, persistence, deployment, and limits |
| `platform/runtimes/duckdb/` | engine runtime with resource limits, closed Spatial axis policy, effective-setting verification, and sandbox primitives |
| `mcp/contract/src/duckdb.rs` | cross-server source types |
| `mcp/contract/src/digest.rs` | typed SHA-256 provenance digest shared by server contracts |
| `servers/duckdb-mcp/src/contract.rs` | server-local tool request and result types |
| `servers/duckdb-mcp/src/engine.rs` | adapter from server results to the shared runtime |
| `servers/duckdb-mcp/src/bin/server/ownership.rs` | derived owner workspaces and database resolution |
| `servers/duckdb-mcp/src/bin/server/sql_ops.rs` | direct and task SQL operation contracts and interruption behavior |

Simulation runtime ownership:

| Path | Responsibility |
|---|---|
| `platform/runtimes/simulation/Dockerfile` | shared Isaac Sim, Isaac Lab, Warp, Newton, MuJoCo, RTX streaming, and non-root runtime image |
| `platform/runtimes/simulation/simulation-runtime.lock.json` | typed compatibility identity, source revisions, immutable components, GPU boundary, and driver floor |
| `platform/runtimes/simulation/requirements.lock` | hash-locked Python dependency closure for the selected Isaac Lab profile |
| `platform/runtimes/simulation/probes/` | import-identity and hardware-GPU conformance evidence |
| `tools/xtask/src/commands/release_cache.rs` | scoped Cargo executable and incremental-cache retention under the build-directory lock |
| `tools/image-build/control/` | shared pinned Buildx, BuildKit registry configuration, declared resource limits and CPU telemetry, and cross-worktree builder lease used by image release and certification |
| `tools/xtask/src/commands/image/benchmark.rs` | controlled compiler source-edit comparisons, temporary input variants, CPU quota experiments under the shared lease, and compiler-only evidence |
| `tools/xtask/src/commands/image/cache_benchmark.rs` | fresh Cargo target comparisons against a pinned compiler cache, ordinary artifact identity, and typed cache hit/miss evidence |
| `tools/xtask/src/commands/image/worker_benchmark.rs` and `tools/image-build/control/src/experiment.rs` | exported compiler-result reuse, same-host worker isolation, source-edit comparisons, and disposable worker cleanup |
| `testing/smoke/src/bin/smoke/scenarios/simulation.rs` | deployment-lock registry authorization, published environment invariants, local image materialization, GPU certification, and retained transcripts |

Simulation live-view ownership:

| Path | Responsibility |
|---|---|
| `mcp/contract/src/live_view.rs` | shared provider-neutral logical-camera, camera-product, viewer-authorization, GPU-capacity, health, and WebSocket H.264 contract |
| `servers/uav-sim-mcp/src/contract.rs` | UAV session, control-grant, Map handoff consumer, mission-plan, and live-view schemas built from the shared contract |
| `servers/uav-sim-mcp/src/server/state.rs` | composed simulator, control-authority, task, logical-camera, and product services |
| `servers/uav-sim-mcp/src/server/control_authority.rs` | Work Context-scoped principal-to-vehicle grants, strict Map handoff admission, mission plans, and exclusive command-lease lifecycle |
| `servers/uav-sim-mcp/src/server/live_view.rs` | actor/browser stream authorizations, stable shared-product selection, expiry, closure, and connection telemetry |
| `servers/uav-sim-mcp/src/server/live_stream.rs` | authenticated browser WebSocket sessions that forward camera-owned Annex B H.264 products |
| `servers/uav-sim-mcp/src/server/live_view_audit.rs` | append-only live-view audit writes without runtime coupling |
| `servers/uav-sim-mcp/src/server/runtime_events.rs` | strict authenticated adapter-ready and final-ready stream receiver, immutable binding reapplication trigger, and MCP subscription projection |
| `servers/uav-sim-mcp/src/server/service.rs` | MCP tools, resources, subscriptions, well-known surface, and live-view orchestration |
| `servers/uav-sim-mcp/assets/live-app.html` | self-contained all-camera WebCodecs MCP App with shared H.264 delivery |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera.py` | simulator-tick camera orchestration, frame transforms, and shared camera/target time |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_rigs.py` | desired-pose computation for follow, chase, orbit, look-at, stabilized-mounted, formation, and fixed rigs |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_camera_smoothing.py` | half-life translation/quaternion filtering and reset rules |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_products.py` | one continuous tiled RTX/NVENC product, RTSP receiver, and viewer-independent H.264 ring for the complete logical-camera set |
| `showcase/uav-sim/runtime/veoveo_uav_sim/operator_health.py` | CUDA, RTX, NVENC, camera-product, frame, and latency evidence |
| `showcase/uav-sim/runtime/veoveo_uav_sim/runtime_events.py` | retained nonblocking adapter-ready edge before world admission and final-ready edge after visual admission |
| `showcase/uav-sim/runtime/veoveo_uav_sim/tile_lifecycle.py` | reactive, deduplicated provider generation state derived from native Cesium lifecycle events and render coverage observations, including expired provider-session reset |
| `showcase/uav-sim/runtime/veoveo_uav_sim/server.py` | simulator-local control boundary for camera and product realization |
| `platform/store/src/live_views.rs` | durable audit persistence for camera, product, authorization, denial, expiry, and revocation facts |
| `platform/store/migrations/0036_remove_simulation_view_mirror_state.surql` | forward-only removal of obsolete mirrored desired/runtime state |

## Recordings

### `platform/recordings/protocol`

`proto/veoveo/recording/ingest/v1/ingest.proto` is the public wire schema.
`src/lib.rs` owns media types, route constants, limits, digest validation, and generated
types.

### `platform/recordings/hub`

[`DESIGN.md`](../platform/recordings/hub/DESIGN.md) covers Hub's terminal journal
quarantine and restart recovery.

| File | Responsibility |
|---|---|
| `ingest_http.rs` | cluster-internal authenticated protobuf routes and typed error responses |
| `ingest.rs` | authenticated batch journal and materializer: producer authorization, atomic no-clobber journal and Blueprint publication, quota-bound append, ordered live parts, static-context snapshots, decoder-reentrant capture-layer rollover, publication recovery, and restart reconciliation |
| `ingest/recovery.rs` | preserves unaccepted terminal-stream journal bytes and their immutable recovery receipt without changing stream acceptance |
| `diagnostics.rs` | counters for authenticated-ingest acceptance, duplication, publication backlog, spool reservations, free-space headroom, and last success |
| `blueprint.rs` | complete Blueprint-store validation, application association, and confined immutable paths |
| `spool.rs` | segment writer for `LogMsg` streams: capture-layer encode, flush, fsync, size/age freeze, idle completion, decoder-reentrant rollover, and crash recovery into a fresh `.rN` file |
| `catalog.rs` | dataset and recording identity, capture timestamps, layer verification, and catalog transitions |
| `layer_files.rs` | confined discovery of writing and committed capture-layer files |
| `publication.rs` | scoped Gateway streaming publication, occurrence verification, and local recovery cleanup |
| `config.rs` | validated raw gRPC spool and capture-layer limits |
| `archive.rs` | one-time object-store compaction, GoP rebatching, footer encoding, and atomic archive publication |
| `bin/spooler.rs` | thin composition of authenticated ingest, loopback Rerun receiver, catalog, and shutdown |
| `bin/hub_smoke.rs` | Rust crash/restart/rollover/catalog smoke scenarios |

### `platform/recordings/forwarder`

| File | Responsibility |
|---|---|
| `src/batch.rs` | per-recording accumulation, per-video-sample batches, decoder-reentrant boundary detection, complete RRD encoding, and splitting by byte size |
| `src/queue.rs` | bounded-memory fsynced producer queue, stream identity, checkpoint acknowledgement, and disk backpressure |
| `src/oauth.rs` | RFC 8414 discovery and `private_key_jwt` client-credentials tokens |
| `src/client.rs` | typed protobuf discovery, open, append, Blueprint publication, and finish operations |
| `src/runner.rs`, `src/blueprint.rs` | canonical-host transport routing, reactive loopback Rerun burst draining, Blueprint demux, failure backoff, restart resume, and graceful drain |
| `src/config.rs` | validated identity origin, installation transport, key, queue, batching, and shutdown configuration |
| `Dockerfile` | production sidecar image with the forwarder and loopback readiness utility |

### `platform/recordings/video`

`src/lib.rs` owns the `RecordingVideoSelection`/`IndexRange`/`VideoTimelineKind`
selection contract, `VideoSourceLimits`, clip materialization authorized by a read plan
with MP4 remux and no transcoding, and recording-URI validation.

### `servers/recording-mcp`

`src/bin/server.rs` advertises stable discovery roots and templates. Recording content
changes notify accepted resource readers without invalidating App discovery.

`contract.rs` owns recording, layer, seal, playback-manifest v9, Blueprint, and live
descriptor types. `service.rs` resolves authorized MCP and playback plans and publishes
properties layers. `service/projection.rs` owns projection receipts, concurrency,
scratch, and Arrow downloads. `platform/recordings/reader/src/cache.rs` owns verified
Artifact-to-PVC materialization and eviction. `blueprint_cache.rs` supplies
the server-owned Blueprint identity validator. `playback.rs` owns playback grants, dataset-scoped virtual
Rerun catalogs, finite Blueprint sources, and the scoped read-only Redap service.
`live_playback.rs` keeps recording-scoped static context across ingest generations, limits
temporal history, and rewrites messages to the stable playback identity.
`live_stream.rs` frames complete RRD batches for the authorized WebViewer `LogChannel` and
distinguishes an empty-channel bootstrap from a current-head transport resume.
`uris.rs` owns recording identities. `bin/server.rs` composes the authenticated manifest,
framed live route, Redap, projections, MCP transports, storage readiness and diagnostics,
and Artifact publication.

### `servers/stream-mcp`

| Path | Responsibility |
|---|---|
| `src/contract.rs`, `src/contract/live.rs` | replay, video, result, sampling, detection, timeline, and output types; the pure live-session module is also compiled by the focused flight client |
| `src/catalog.rs` | validated admitted GStreamer graphs, typed profiles, live ingress, and immutable model catalog |
| `src/executor.rs` | native replay-runner protocol and response validation |
| `src/annotation.rs` | derived Rerun bounding-box annotation layers |
| `src/artifacts.rs` | shared artifact-plane adapter |
| `src/uris.rs` | `stream://` URIs |
| `src/bin/server/live.rs` | owner-scoped live runner lifecycle plus result and encoded-preview ring buffers |
| `src/bin/server/recording_output.rs` | optional non-blocking fan-out of existing H.264 units to the pod-local Recording forwarder |
| `src/bin/server/app.rs`, `assets/live.html` | self-contained Stream MCP App resource for actual encoded video, typed overlays, and decode-path reporting |
| `src/bin/server/` | auth, replay tasks, live sessions, prompts, resources, notifications, and composition |
| `gst-runner/` | native operator-admitted GStreamer graph execution with NVIDIA decode/inference and typed event output |
| `Dockerfile` | DeepStream 9 development/runtime multi-stage image |

`platform/recordings/reader` owns the Artifact-backed read plan, and
`platform/recordings/video` owns selection and materialization over it. A Stream replay
task stores an Artifact read capability and revalidates it through the shared reader
after a restart. Live Stream sessions read their admitted ingress directly and do not
depend on Recording Hub.

### `servers/reason-mcp`

| Path | Responsibility |
|---|---|
| `src/contract.rs` | reasoning tasks, decode policy, grounding, results, and output types |
| `src/catalog.rs` | validated world-model checkpoint and reasoning pipeline catalog |
| `src/executor.rs` | world-model runner protocol and response validation |
| `src/grounding.rs` | typed Stream-results grounding subset extraction |
| `src/annotation.rs` | derived Rerun provenance and event annotation layers |
| `src/artifacts.rs` | shared artifact-plane adapter |
| `src/uris.rs` | `reason://` URIs |
| `src/bin/server/` | auth, tasks, prompts, resources, notifications, and composition |
| `runner/` | Python world-model runner: typed protocol, GPU frame sampling, vLLM inference, and locked image assets outside Rust compilation |
| `runner/src/reason_runner/video.py` | exact packet timestamps, NVDEC device surfaces, and owned CUDA observation tensors |
| `runner/src/reason_runner/gpu_model.py` | single-process Qwen3-VL embedding adapter, decoder memory reservation, and CUDA handoff to vLLM |
| `Dockerfile` | vLLM runtime image with the server binary and installed runner |

Reason copies a size-limited grounding subset into the stored request at submission. It
materializes recording video with a persisted task-read capability and a verified recording
cache, so it never keeps the submitter's gateway bearer. The runner binary ships in the
image and the engine is compiled per site at deployment, so the server reports not-ready
until both are present.

## Python Servers

### `sdk/python`

The shared platform package for hosted Python MCP servers. It is the Python
counterpart of the workspace crates a Rust server composes. Rust defines every wire
shape and schema, and this package follows it.

| Module | Responsibility |
|---|---|
| `contract/` | identity, artifact-plane, and usage wire models |
| `internal_auth.py` | gateway Ed25519 assertion verification and ASGI middleware |
| `host.py` | host-authority validation and 421 rejection |
| `deployment.py`, `pagination.py` | mount identities and cursor pagination |
| `schema.py` | self-contained JSON Schema 2020-12 generation for MCP tool inputs |
| `task_extension/` | typed official Tasks SDK-hook adapter, models, and projection |
| `tasks/` | durable SurrealDB task runtime port: leases, CAS transitions, outbox, recovery, prune |
| `artifacts.py` | artifact-plane HTTP client, capability redemption, size-capped in-memory reads, and streamed URI/file consumption with cancellation cleanup |

### `templates/python-mcp`

The template for new Python servers, shipped as the working
`datasheet` dataset-profiling server. `contract.py` and `engine.py` own the
domain; `server/` mirrors the Rust per-server module split (config, ownership,
official Tasks adapter, durable task, MCP surface, composition).

## Agents

### `agents/runtime`

SurrealDB-backed agent, episode, task watcher, wake, lease, and scheduling persistence.

| File | Responsibility |
|---|---|
| `control.rs` | database-authenticated operator messages and input-request decisions scoped to their tenant and Work Context, with UUIDv7 idempotency, wakes, actor attribution, and a domain-neutral conversation view over wakes and episodes |
| `runtime.rs` | lease-fenced agent mutations, inactive-manifest reconciliation, race-safe input-request terminal waits, and atomic terminal-delivery consumption with first-party Task retention release |

The [managed instance store](../platform/store/src/agent_management/instances/DESIGN.md)
owns admitted provisioning intent, retained capacity, service registration
and controller generation fences.
[`instances/`](../platform/gateway/src/bin/gateway/agent_management/instances/) in
the management gateway owns template-derived admission, lifecycle operations,
public views, and image-only template adoption checked against the previous
template digest. The lifecycle controller drains retained writers before activation.

The [agent catalog store](../platform/store/src/agent_management/DESIGN.md) owns
definition authoring, immutable executable revisions, mutation replay and publication
audience fencing. It is separate from episode scheduling. The
[management gateway](../platform/gateway/src/bin/gateway/agent_management/DESIGN.md)
owns authoring policy, approved model connections, publication validation and
catalog invalidation. `import.rs` owns the explicit offline installation seed and retained
chat-binding conversion; startup never overwrites authored definitions. Its [HTTP contract](../mcp/contract/src/agent_management/DESIGN.md)
generates shared browser types. The agent-management plan tracks client/runtime
wiring and the managed-instance controller.

The [durable runtime design](../agents/runtime/DESIGN.md) defines scheduler leases,
atomic managed episode admission, terminal stop semantics and Task result retention.

### `agents/manager`

The [manager design](../agents/manager/DESIGN.md) owns namespace-scoped managed
kernel provisioning. `tests/admission.rs` qualifies rendered CEL policies against
the actual resource composer and a temporary Kubernetes namespace. `reconcile.rs` advances durable claims, `credentials.rs`
correlates retained signing keys, `resources.rs` composes fixed workloads, and
`kubernetes.rs` owns HTTPS requests and native watch recovery. Installation
admission policy constrains the controller's Kubernetes authority.

### `agents/kernel`

The [kernel design](../agents/kernel/DESIGN.md) owns managed publication overlays,
dispatch preflights and budgeted execution.

| File | Responsibility |
|---|---|
| `manifest.rs` | agent, model, profile, tool, budget, and MCP resource-subscription configuration models |
| `episode.rs` | reasoning episode lifecycle |
| `background_tasks.rs` | immediate model-visible handoff from accepted task-backed tool calls to credential-rotating kernel watchers |
| `tools.rs` | MCP tool dispatch and durable task descriptor capture |
| `tasks.rs` | detached watcher lease/resume/result-to-wake flow |
| `wake.rs` | outbox/changefeed wake delivery, including heartbeat-only batches acknowledged without an episode |
| `memory.rs` | persistent memory API over analytical stores |
| `timeline.rs` | snapshot dataframe read-back over the agent's RRD segments capped to the most recent rows because rows become model input |
| `context.rs` | per-episode context assembly as a view over the memory planes |
| `llm.rs` | episode LLM construction from the manifest's provider-neutral model config |
| `input.rs` | durable application input for protocol-neutral deferred tools |
| `replay.rs`, `summary.rs` | domain-truth rebuild from the decision log and deterministic episode summaries |
| `rrd.rs`, `recorder.rs` | episode/world Rerun recording |
| `budget.rs` | enforced episode/tool/cost budgets |
| `connection.rs` | final-profile gateway client epoch, serialized request-boundary credential freshness, acknowledged request-scoped listener restoration, and deferred-task resolver |
| `resource.rs` | resource reads through the current profile, episode-local accounting, text validation, and fixed-field correction diagnostics |

### `platform/gateway/src/bin/gateway/admin`

| File | Responsibility |
|---|---|
| `agents.rs` | policy and audit boundary for user or service agent messages, actor-attributed conversation reads, pending input-request reads, and decisions; resolves the caller's tenant and Work Context before using the runtime control plane |

## Console

### `apps/console/bff`

| File | Responsibility |
|---|---|
| [`DESIGN.md`](../apps/console/bff/DESIGN.md), `browser.rs`, `oauth.rs` | shared PKCE login, exchange and refresh with separate Console and Workspace OAuth clients, cookie encryption domains and return-path authority |
| `session.rs` | XChaCha20-Poly1305 cookies, CSRF material, and bounded same-origin `BrowserReturnPath` authority |
| `app_host.rs` | typed `/apps/{server}/{page...}` route authority, public no-store entry document, and caller-authorized App bootstrap |
| `api.rs` | snapshot, SSE, mutation, artifact preview/download, and same-origin CSRF-protected agent-message/input-request BFF routes; browser credentials and database authority never enter an MCP App |
| `apps/catalog_events.rs` | App catalog SSE lifecycle, source-loss recovery, discovery reconciliation and unchanged-snapshot suppression |
| `artifact_upload.rs` | shared Console/Workspace cookie and CSRF-protected upload proxy; the selected browser application fixes the authenticated profile |
| `recording_playback.rs` | authenticated playback-manifest and framed live-stream pass-through; no archive bytes or BFF session store |
| `apps.rs`, `mcp_client.rs`, `workspace/apps.rs` | MCP Apps host backend: per-auth client pool, public gateway Host preservation, failure-isolated App catalog, standalone descriptors, sandboxed frames, declared agent-message targets, allowlisted tool calls, resource reads, listener/subscription admission, cancellation on token replacement, and one multiplexed resource-wake stream per App |
| `config.rs`, `viewer_config.rs` | validated public/gateway/OAuth-resource/MCP-transport and embedded-map configuration, profile binding, redacted provider credentials, and the authenticated no-store Rerun map configuration |
| `outbound_http.rs` | additive installation CA trust shared by Console HTTP, streaming, live, MCP, and Kubernetes clients |

### `apps/console/web/src`

| File | Responsibility |
|---|---|
| `App.tsx`, `bootstrap.ts` | authenticated application shell, identity-scoped query clients, Computers navigation, permission-gated inventory and catalog-driven MCP App entries |
| `uploads/` | persistent, identity-scoped artifact upload queue, worker hashing, progress transport, and accessible selection/recovery panel; specified in its local `DESIGN.md` |
| `csrf.ts` | ephemeral CSRF state shared by JSON and raw-body upload transports |
| `browserApp.ts`, `browserHttp.ts`, `artifactUrls.ts` | Console/Workspace entrypoint selection, shared cookie/CSRF transport and fixed file URLs; shared capability components do not choose their own profile |
| `appHost.tsx`, `StandaloneAppHost.tsx`, `standaloneBootstrap.ts` | minimal standalone App entry, authorized same-path bootstrap, shared OAuth/CSRF settlement, authorized title, and Console return link |
| `views/Recordings.tsx` | searchable lifecycle browser and lazy Rerun playback workspace |
| `components/GovernedRerunViewer.tsx`, `rerunSources.ts`, `rerunLiveChannel.ts`, `recordingRrdFetch.ts`, `rerunMap.ts` | recording-scoped Redap archive or recent-history live playback, persistent WebViewer lifecycle, producer Blueprint-first opening, one native incremental-RRD or lazy-archive receiver, same-origin RRD authorization, duplicate-free current-head reconnect, event-driven rollover without cursor forcing, archive-only credential renewal, and installation-owned browser map-provider activation |
| `views/Agents.tsx`, `agentControl.ts` | reactive agent state, actor-attributed conversation, idempotent message submission, pending input-request decisions, and client-owned UUIDv7 retry identity |
| `views/` | platform-plane views (overview, work, artifacts, MCP, apps, access, audit, cluster); Computers has a native view in `computers/`, and other domain views ship as MCP Apps |
| `drawers/ArtifactDrawer.tsx` | artifact preview, recording provenance, download, release, grant, and share-link workflows |
| `drawers/` | remaining detail drawers with mutation workflows |
| `components/ArtifactPreview.tsx` | size-limited text and inline image/audio/video/PDF previews with explicit access-denied states |
| `identity.ts`, `components/IdentityText.tsx` | trusted display-name resolution for arbitrary principal ids with UUID-compacting fallback, rendered across access, agents, and artifact views |
| `components/` | reusable primitives, tables, toolbar, and the promise-based confirm dialog |
| `queries.ts`, `queryClient.ts` | TanStack Query keys, snapshot/apps/cluster queries, mutation hooks with targeted cache patches |
| `live.ts` | EventSource console stream feeding row upserts into the snapshot cache |
| `theme.ts`, `ThemeProvider.tsx` | persisted Console theme registry, semantic palette selection, and MCP App light/dark host context |
| `apps/` | MCP Apps host: one exported opaque-origin sandbox policy, shared iframe component, stable postMessage bridge, closed internal navigation, declared agent messages, resource-read adapter, and fetch-backed multiplexed SSE wake decoder; `resourceTransport.ts` shares fixed-host resource subscriptions with Workspace |
| `auth.ts` | one-way authentication transition shared by every 401 handler |
| `api.ts` | ordered same-origin BFF calls and CSRF rotation |
| `types.ts` | TypeScript snapshot and mutation response shapes |
| `styles.css` | responsive work-focused visual system with accessible type-scale tokens |

## Testing And Conformance

| Path | Responsibility |
|---|---|
| `mcp/conformance` | reusable domain-neutral MCP certification library, thin CLI, schemas, profiles, authenticated same-origin well-known-surface checks, live declaration binding, and standalone image |
| `testing/flight-smoke/` | [focused composed-flight harness](../testing/flight-smoke/DESIGN.md), with server-owned wire types and client-only dependency closure |
| `testing/smoke/src/bin/smoke.rs` | smoke command dispatcher and digest-addressed simulation certification entrypoint |
| `testing/smoke/src/bin/smoke/scenarios/` | Rust process/deployment scenarios |
| `testing/smoke/src/bin/smoke/scenarios/artifact_consumers.rs`, `artifact_consumers/python.rs` | installed public known/unknown-length uploads, CSV/Parquet MCP interoperability, and full-size Python SDK streaming observations asserted by Rust; direct-plane fixture identities stay separate from public OAuth evidence |
| `testing/smoke/src/bin/smoke/scenarios/recording_catalog_sdk.rs`, `testing/recording-catalog-sdk/` | installed service-token grant and native Rerun Python Catalog SDK query against k3d, including fresh-grant reconnect |
| `testing/smoke/src/bin/smoke/scenarios/candidate.rs` | Stream and Reason compiler candidates in their installed NVIDIA runtimes, executable and payload identities, private listeners, and verified process cleanup |
| `testing/smoke/src/bin/smoke/scenarios/recording_fixture.rs` | authenticated completion of explicitly selected video-test recording fixtures; production recordings are rejected |
| `testing/smoke/src/bin/smoke/support/` | process, HTTP, auth, fixture, usage helpers |
| `testing/smoke/tests/` | static deployment/offline contract tests |
| component-local `tests/` | focused live SurrealDB and service integration tests |
| `testing/local-test-report.json` and `testing/test-receipts/` | committed v3 per-check index and immutable attempt history, with materialized input manifests and observed toolchains |
| `testing/evidence-checks/` and `testing/coverage/` | owner-reviewed source commands, Cargo dependency selection, external test inputs and explicit required coverage profiles |
| `.github/workflows/local-test-report.yml` | lightweight presentation of the committed local test report; it performs no substantive build, deployment, GPU, or browser acceptance |

Smoke lifecycle, retry, assertion, and cleanup logic belongs in the Rust harness, not in shell recipes.

## Change Routing

- Change shared identity/policy/artifact semantics in `mcp/contract`, then update the
  platform store and every affected boundary.
- Change persistence shape in `platform/store` with an ordered migration and matching Rust API.
- Change durable task lifecycle and its official MCP Task surface in
  `platform/task-runtime`; hosted handlers use `rmcp` Task types directly.
- Change a domain tool schema in its owning `servers/*-mcp` server, not the gateway.
- Change MCP Apps protocol constants or helpers in `mcp/apps-extension`; app views live beside
  their server (`servers/{server}-mcp/assets/`), and the console host surface in
  `apps/console/bff` (`apps.rs`) plus `apps/console/web/src/apps/`.
- Change a domain administrative surface in its owning server as scope-gated MCP tools and
  resources, and represent it in the browser through the server's MCP App view — never a
  bespoke admin REST router, BFF proxy route, or hardcoded console page.
  Computers is the accepted exception: its native Console view uses domain-owned
  commands and generated contracts, indexed above.
- Change browser behavior through `apps/console/bff` plus `apps/console/web`; do not expose gateway
  tokens to JavaScript.
- Change Recording Explorer bulk projection delivery through
  `apps/console/web/src/apps/recordingProjectionStream.ts`. Only the Recording
  Explorer may receive its transferable stream.
- Change public routes in Helm ingress, then extend the Rust deployment smoke.
- Change installation image/config content in Helm, the offline lock/builder, and
  deployment contract together.
- Deliver install-time domain configuration through the generic `serverBootstrap` values only
  when the owning server defines an installation-time contract. Frames worlds are runtime MCP
  state and must never be placed in Helm bootstrap.
