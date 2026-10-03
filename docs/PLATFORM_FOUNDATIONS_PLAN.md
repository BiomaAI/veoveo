# Platform Foundations Plan

Status: In progress. Phase 3 still has implementation work, and several installed,
recovery and visual acceptance gates remain. The plan is not ready for retirement.

## Current Status

The Task input-response fix is source-qualified at `af1101d4`. It joins the scope,
analytical-request and native Task-ID batches awaiting publication. The reference
installation's last selected flight batch is `bf0fe31d`; the cluster and BuildKit are
stopped during development. Reason runs only for its separate acceptance.

| Phase | Qualified checkpoint | Remaining work |
|---|---|---|
| 0 — Retire finished plans | Accepted | Retire this plan only after full acceptance |
| 1 — Identifier hard cut | Implemented and installed | Final composed flight, Rerun playback/timing and landing visual checks |
| 2 — Installation targets | Implemented and used by installed harnesses | Run the outstanding installed scenarios with the selected service targets |
| 3 — Contract corrections and types | Foundational types, server libraries, scope declarations and SQL admission have qualified batches | Complete the [migration inventory](#migration-inventory-and-status), then publish and qualify its remaining consumers |
| 4 — Unified audit | Native and installed audit checks pass | Preserve those guarantees through the pending composed release and final audit |
| 5 — Store simplification | Outbox removal, native feeds, reference cleanup and payload separation are implemented; measurements and installed certification pass | Remaining domain/current-format recovery and composed acceptance |
| 6–7 — Knowledge extension and adoption | All eighteen participating sources pass their declared installed checks; domain K09/K10 review is recorded | Preserve qualification for changes made by the remaining contract work |
| 8 — Knowledge service | Nineteen-collection catalog, search, CUDA embedding, isolation and restart checks pass; controlled model comparison retains 0.6B | Remaining shared-contract consumers and final composed acceptance |
| 9 — Reason findings | Installed publication, result grants, revocation and separate restart checks pass | Broader result/reference types and their installed qualification |

The [progress log](PLATFORM_FOUNDATIONS_PROGRESS.md) records revisions, commands,
measurements and failed attempts. Its older entries describe their own checkpoints
and do not override this status or the requirements below.

## Remaining Work

1. Finish the implementation gaps in the [Phase 3 inventory](#migration-inventory-and-status).
   Its owner rows include IDs, DTO relationships, URI builders, SQL query inputs,
   resource notifications and remaining SDK/client adoption. Completed extraction
   and passing contract-only builds do not establish complete domain adoption.
2. Publish the accumulated qualified changes as a composed batch. Deploy through the
   reference runbook, then qualify current resource consumers, admission, paging,
   grants, subscriptions and recovery for the affected services. Run Reason separately.
3. Close the inventory's outstanding cross-replica, provider and current-format
   restart requirements, including UAV outcome recovery and the Computers installed
   batch. Native checks do not substitute for those installed cases.
4. Move every Rust server onto the shared host, as
   [Hosted Server Adoption](#hosted-server-adoption) describes. DuckDB, Timeseries,
   Frames and Media are migrated, and their native suites pass. Installed acceptance of
   the four migrated servers is outstanding.
5. Resolve [Deferred Work](#deferred-work): Rerun timeline/playback/timing and landing
   visual acceptance. Keep the declared hardware, freshness and flight-health gates.
6. Audit every phase's numbered requirements and acceptance conditions against the
   final source and deployed revisions. Update owning designs and standards registers.
   Delete this plan and its CODEMAP row only when all required work is accepted.

## Standards And Protocols

| Standard or protocol | Role in this plan |
|---|---|
| MCP `2026-07-28` extensions and `_meta` key rules | Identifier forms and the `ai.veoveo/knowledge-source` extension |
| RFC 3986 / RFC 6570, iri-string 0.7.14; Python rfc3986 2.0.0 and uri-template 1.3.0 | Concrete URI validation, ASCII resource templates, typed MCP descriptors and expansion qualified against domain builders |
| RFC 9110, RFC 9111, RFC 8246 | Revision, freshness, and immutability semantics for knowledge reads |
| W3C DCAT 3 | Catalog model in `knowledge-mcp` |
| SurrealDB 3.3 | Audit records, change feeds, LIVE queries, table views, record references, catalog, `FULLTEXT` BM25, and `HNSW` indexes |
| vLLM 0.30.0 pooling runner, OpenAI Embeddings API, `Qwen/Qwen3-Embedding-0.6B` | Embedding runtime on a hardware GPU |
| `veoveo.ai/installation-target/v1` | Installation input for installed smoke scenarios |
| OCSF 1.9.0, W3C Trace Context, RFC 9162, RFC 8785, S3 Object Lock | Audit record export, correlation, sealing, and write-once retention |


## Working Rules

- Work autonomously. Do not stop to ask the user questions. When a choice is open,
  take the option closest to the designs this plan links, or the best-supported guess,
  and record the choice in the commit message.
- Implement each concern across all affected components in one pass, including its
  consumers, fixtures and documentation. Do not complete a separate implementation
  and validation cycle for each MCP server. Finish the whole pass before compiling,
  collect independent failures together, and fix them in a second pass.
  An individual helper, query or ID wrapper is not a validation checkpoint. Use reads,
  formatting and query parsing during implementation; run an early test only to resolve
  a concrete uncertainty that would change the implementation.
- Select the affected packages, targets and features once per batch. Inspect development
  dependencies and preserve the qualified Cargo feature graph, build directory and
  environment. A stable compilation graph does not require executing every test in it.
  Keep one Cargo pipeline. For broad type migrations, use `cargo build --keep-going`
  with the selected test targets to collect independent compiler errors, fix them
  together, then proceed to tests using the same graph. Avoid a separate `cargo check`
  pass when the required test build provides the same compiler feedback.
- Run the selected behavioral and consumer checks together after the implementation
  pass, then workspace/deployment acceptance at an integration milestone. Select cases
  by the changed behavior and consolidate failures before editing again. Use
  `--no-fail-fast` for independent test targets; it does not aggregate compiler errors.
  Passing focused cases count toward the batch's qualification; exclude them from the
  remaining run while their source, dependency inputs and environment are unchanged.
  A failure or follow-up edit repeats only checks whose inputs or behavior changed.
  Reuse current native executables for fixture-only retries. Rebuild after source or
  dependency changes. Test-name filters narrow execution but still compile selected
  targets; choose targets first and avoid blanket `--workspace --lib` feedback runs.
- Include direct consumers when a shared contract changes. Broaden qualification when
  the affected dependency closure requires it, rather than after every local edit.
  Deployment configuration uses the normal harness binary and `helm-config`; gateway
  integration tests run when their integration changes. Publish and perform installed
  acceptance once the composed batch passes local qualification. A documentation-only
  edit runs the documentation check and does not repeat native behavioral suites.
- Evaluate contract adequacy during each change. Improve outdated or inexpressive
  contracts when the accepted architecture requires it, without waiting for the user
  to identify the problem. Explain material tradeoffs, update the owning design and
  this plan, and record ownership or repository-wide decisions in
  `docs/CONTRACT_EVOLUTION.md`. Qualify the replacement before advertising it.
- Apply the [modular type architecture](#modular-types-and-server-contracts) in every
  phase. Keep domain vocabulary with its owner, preserve specific types through
  internal APIs, and provide focused builders where construction needs validation.
  Phase 3 establishes the shared interfaces; later work must use and extend them
  without adding server-specific dependencies to core.
- Do not let a blocker stop progress. When one step cannot finish, work around it,
  leave a `TODO(foundations): <what remains and why>` comment at the exact code path,
  add a row to [Deferred Work](#deferred-work), and continue with the next step. Defer
  only what later work does not depend on.
- A deferral never weakens a requirement. Do not replace a GPU path with a CPU path,
  skip an audit record that must commit, relax authorization, or mark a failing check as
  passing. Leave that work deferred and visible instead.
- The reference installation at veoveo.bioma.ai holds no data to preserve. Stop,
  wipe, and rebuild its cluster whenever a phase needs it, without backups, data
  migration, backward compatibility, or approval. Deploy each phase there once its
  local checks pass, and run its installed acceptance there.
- Keep the reference cluster stopped during editing, compilation, and image builds.
  Start only the isolated services a focused check requires. Start the full reference
  cluster for installed acceptance and stop it when those checks finish. Use node
  stop/start for routine development; delete and rebuild when the phase requires a
  reset. Reason stays off except for its separate acceptance, as the user requested;
  service batches need not deploy every server simultaneously. Check free disk space
  and expected build growth before large builds, because
  stopping nodes does not reclaim their volumes or build caches. Required installed
  and hardware GPU acceptance still runs against the services each check requires.
  A pass names its selected services and cannot establish untested GPU coexistence.
- During this plan, clean up completed experiment containers, verified-empty unused
  volumes, and superseded build outputs between steps. Preserve useful Rust and
  BuildKit caches. Before retiring Docker or OCI registry images, protect current
  installation and rollback references, reusable dependency images, and their full
  manifest and layer closure. Old simulation experiments may be removed. This
  resource discipline applies to this plan; it adds no repository-wide development
  rule or background cleanup service.
- Run Docker-backed native fixtures separately from image assembly and SBOM scanning.
  Concurrent source qualification exhausted fixture creation and cleanup deadlines
  on this host. Reuse the compiled test executables once publication has finished.
- Publish composed batches directly through `release images` when staging is not
  needed for an intermediate development deployment. Disk-pressure collection can
  reclaim runtime layers between staging and qualification; rebuilding those layers
  can change their digests even when Cargo reuses every compiled binary. A digest
  mismatch invalidates reuse of staged acceptance. Keep the failure visible and bind
  installed acceptance to the freshly qualified images.
- Before a fresh reference bootstrap, prepare the published images in the node cache
  sequentially before enabling application reconciliation. Include the rendered
  workload images and the runtime images selected through the installation locks.
  Confirm every reference in the node's CRI inventory before starting the workloads.
  Protect the selected images with containerd's supported pin during preparation and
  verify the CRI pinned flag. Keep their K3s import manifest on that node for restarts;
  remove superseded pins when the installation no longer needs those images.
  Cold image extraction must finish before database provisioning and application
  startup compete for the same disk. Preserve the failed attempt's diagnostics and
  clear its owned workloads before retrying.
- Read `AGENTS.md` and `docs/CODEMAP.md` before each phase.
- Work on `main` in coherent commits, one completed concern each. Commit after the
  batch's affected checks pass; commit size alone does not trigger another validation
  cycle. Run `cargo xtask enforce docs` for every documentation change.
- Internal names and formats change by hard cut. Do not add aliases, fallbacks, or
  readers for old identifiers. This applies to stored data and server/client contracts
  throughout this plan. Keep one current format, update its callers together and reset
  disposable reference state as needed. Do not add historical-data audits, conversion
  tools, dual readers, support windows or compatibility-only rollback qualification.
  Remove such work already introduced. Current-format recovery and transactional
  rollback remain required. This rule supersedes historical-data transition tasks
  recorded in earlier checkpoints and linked component designs for this plan.
- Re-verify the latest stable release of every new dependency at the moment you add it.
  Update the pin and this plan if it has moved.
- Delete superseded plans as you go, following Phase 0.
- GPU workloads request their device and fail closed. The embedding path has no CPU
  mode.
- Keep the standards registers current. Add a standard to them in the change that
  implements it, never earlier, because each register lists what Veoveo implements.
- Update `docs/CODEMAP.md` in the same change that adds, moves, or removes a crate,
  component, or document.
- Update the current status and remaining work when a batch lands, distinguishing
  source qualification from installed acceptance. Keep dated checkpoints and timings
  in [the progress log](PLATFORM_FOUNDATIONS_PROGRESS.md); do not prepend them here.

## Prerequisites And Deployment

The agent runs on the reference installation's GPU host, where the `k3d-veoveo-bioma`
Kubernetes context, the local registry, and a headed browser with hardware-backed
graphics are available. Without them, deployment and installed acceptance steps go to
[Deferred Work](#deferred-work) while the local work continues.

Deploy through the reference installation's own runbook: the Release publication
section of [`examples/bioma/README.md`](../examples/bioma/README.md) builds and pushes
images to the local registry, updates the release locks, and observes the rollout with
`cargo xtask smoke gitops-converge`. Flux reconciles veoveo.bioma.ai from `main` on
`origin`, so pushing `main` to `origin` is part of each deployment and is authorized
by this plan. Use [`docs/DEVELOPMENT_ITERATION.md`](DEVELOPMENT_ITERATION.md) for
affected-image staging. The "Create the local platform" section of the same README
rebuilds the cluster from scratch when a phase needs a clean installation.

## Standards Registers

Four documents list the standards and protocols Veoveo implements. Owning designs keep
their own Standards And Protocols sections, and these registers summarize them:

| Register | Scope |
|---|---|
| `README.md` § Standards And Protocols | Product-level areas |
| `docs/TECH_DESIGN.md` § Standards And Protocols | Cross-component standards and their supported subsets |
| `docs/ARCHITECTURE_DECISIONS.md` § Standards And Protocols | Architecture boundaries |
| `docs/architecture/catalogs/interfaces-and-protocols.csv` | Interface model; run `render.py` and `validate.py` after editing, as `docs/architecture/README.md` describes |

Each phase below names the register updates it owns.

## Phase 0: Retire Finished Plans

A plan is finished when its status says delivered, complete, or implemented. Before
deleting one, move each still-open item into the owning component: a `DESIGN.md`
status or limits section, or a `pending` entry with a reason in the server's
`AGENTS.md` Contract Compliance section. Then replace every link to the plan with a
link to the owning design, and remove its rows from `docs/CODEMAP.md` and
`docs/README.md`. Dated measurements disappear with the plan and stay in git history.

| Plan | Verdict | Open items go to |
|---|---|---|
| `RECORDING_CATALOG_HARD_CUT_PLAN.md` | Retired | `docs/RECORDINGS.md`, `servers/recording-mcp/DESIGN.md` |
| `RMCP_3_MIGRATION.md` | Retired | `mcp/contract/DESIGN.md` |
| `REACTIVE_UX_PLAN.md` | Retired | `apps/console/web` and `apps/workspace/DESIGN.md` |
| `SPEECH_PLAN.md` | Retired | `servers/speech-mcp/DESIGN.md` |
| `WORKSPACE_PLAN.md` | Retired | `apps/workspace/DESIGN.md` |
| `ARTIFACT_UPLOAD_PLAN.md` | Retired | `platform/artifacts/service/DESIGN.md`; replace the `docs/README.md` guide link with that design and `apps/console/web/src/uploads/DESIGN.md` |
| `AGENT_MANAGEMENT_PLAN.md` | Retired | `agents/manager/DESIGN.md` |
| `FORK_DEVELOPMENT_PLAN.md` | Retired | `docs/FORK_DEVELOPMENT.md`; drop the pointer at the top of `CONTRACT_EVOLUTION.md` |
| `COMPUTERS_PLAN.md` | Retired | `platform/computers/DESIGN.md`; rewrite the `CONTRACT_EVOLUTION.md` sentences that cite it |
| `PLATFORM_IMPROVEMENTS_PLAN.md` | Kept open cycle only | owning designs named in each cycle |
| `REPOSITORY_HARDENING_PLAN.md` | Kept open work; delivered sections retired | `tools/xtask`, `testing/` designs |
| `CAPABILITY_ADOPTION_PLAN.md` | Keep; its proposals are unapproved | none |

Acceptance: `cargo xtask enforce docs` passes, and `git grep` finds no link to a deleted
plan.

## Phase 1: Identifier Hard Cut

### Target forms

| Kind | Form | Example |
|---|---|---|
| MCP `_meta` keys and extension identifiers | `ai.veoveo/<kebab-name>` | `ai.veoveo/agent-message-targets` |
| OCI image labels | `ai.veoveo.<group>.<name>` | `ai.veoveo.build.mode` |
| Schema, format, evidence, and version tags | `veoveo.ai/<name>/v<N>` | `veoveo.ai/live-view/v4` |
| Kubernetes labels and annotations | `veoveo.ai/<name>` | `veoveo.ai/cancellation-phase` |
| JSON Schema `$id` | `https://veoveo.ai/contracts/<name>/v<N>` | `https://veoveo.ai/contracts/computer-storage/v1` |

Version numbers stay the same. Only the namespace changes.

### Inventory

`git grep -nE 'io\.veoveo|veoveo\.io|ai\.bioma\.veoveo'` finds 145 distinct
identifiers in 177 tracked files when this plan was written. Kubernetes chart labels,
selectors, and finalizers already use `veoveo.ai/`.

| Group | Count | Examples and defining locations |
|---|---|---|
| MCP `_meta` keys | 5 | `io.veoveo/agent-message-targets` (`mcp/apps-extension/src/models.rs:14`); `io.veoveo/app-resource-dependencies` and `io.veoveo/app-tool-dependencies` (`mcp/contract/src/gateway/server_config.rs:45-46`); `veoveo.io/gateway-discovery-degradation` (`mcp/contract/src/catalog.rs:7`) becomes `ai.veoveo/gateway-discovery-degradation`; `ai.bioma.veoveo/taskRetentionPin` (`platform/task-runtime/src/service.rs:19`, `servers/timeseries-mcp/src/bin/server.rs:95`, `sdk/python/src/veoveo_mcp/task_extension/models.py:23`) becomes `ai.veoveo/task-retention-pin` |
| Public payload schemas | about 12 | `live-view/v4` (`servers/uav-sim-mcp/src/contract/live_view.rs:12`), `hosted-mcp/v3` (`mcp/contract/src/lib.rs:9`), conformance profile and report (`mcp/conformance/src/profile.rs:10`, `report.rs:7`), recording catalog, projection, and playback tags, `map-route-handoff/v1` (Map-owned; UAV imports the contract), and the optimization problem tags (`servers/optimization-mcp/src/contract/mod.rs`) |
| Internal protocols | 4 | `cuopt-executor/v1` (Rust and Python), `uav-runtime-event/v2` (Rust and Python), `computer-storage/v1` with its `$id` (`platform/runtimes/computers/protocol/storage.json:3`), `computer-host/v1` |
| Persisted formats | about 9 | `travel-model-artifact/v1`, `recording-manifest/v9`, `retained-storage-host/v1`, `retained-home/v1`, `recording-journal-quarantine/v1`, `replacement-policy`, `computer-persistent-home/v1`, `computers-unconfigured/v1`, and the `simulation-view-desired-digest` values in migrations `0031` and `0032` |
| Deployment and installation formats | about 14 | `deployment/v8`, `deployment-lock/v8`, `local-registry/v1`, `image-release-evidence/v3`, `component-*`, `installed-deployment-unit/v1`, `computers-service/v3`, `computer-host/v1` config, and simulation lock and evidence tags |
| Evidence and report tags | about 55 | `testing/browser-smoke/src/main.rs:31`, `testing/deployment-smoke/src/gitops.rs:15`, `tools/xtask/src/commands/image.rs:43`, and siblings |
| OCI image labels | 19 | `io.veoveo.build.*` (`tools/xtask/src/commands/image.rs:44-48`, `image/normalized.rs:23-24`, `docker-bake.hcl`), `io.veoveo.simulation.*`, `io.veoveo.certification.source`, `io.veoveo.workload.role`, `io.veoveo.artifact.kind`, and the `veoveo.io/*` version labels in `platform/runtimes/simulation/Dockerfile:153-158`, which become `ai.veoveo.simulation.*` |
| Kubernetes annotation | 1 | `veoveo.io/cancellation-phase` (`testing/deployment-smoke/src/flux_cancellation/witness.yaml`) |
| Documentation only | 21 | historical names in docs; rename them or delete them with their plan |

### Consequences accepted by the cut

These changes alter stored or derived identities. The reference installation resets
its data, so none of them needs a migration:

- `computer-persistent-home/v1` feeds Computer template fingerprints stored in the
  store and in `examples/bioma/computers/computers.json`. Regenerate the fingerprints
  in that file.
- `computers-unconfigured/v1` derives a stored provider instance ID.
- Deployment-contract tags change every deployment digest. The next installation is
  a full rollout, and local receipts under `.git/veoveo-deployment/` become stale.
  Delete them.
- `map/source-feature-query/v2` invalidates outstanding pagination cursors.
- OCI label changes rebuild every image. Regenerate
  `platform/runtimes/simulation/simulation-runtime.lock.json`, the overlay
  `identity.json` files, and `deploy/local/k3d/registry.json`.
- Migrations `0031` and `0032` change in place. Their checksums change, so every
  existing store, including local development stores, must be recreated.

### Work

1. Rename every identifier in code, fixtures, tests, Helm templates, Dockerfiles,
   `docker-bake.hcl`, Python packages, and current-state docs in one commit series that
   builds at each step. Rename negative tests too, so they still reject an older
   version: `deploy/contract/tests/fork_installation.rs:270`,
   `deploy/contract/src/decoding.rs:34`,
   `apps/console/bff/src/recording_playback.rs:622`, and
   `servers/computers-mcp/tests/service.rs:383`.
2. Update committed fixtures: `deploy/contract/tests/fixtures/deployment-lock.json`,
   `showcase/sumo/deploy/deployment.json`,
   `testing/fixtures/{fork-installation,platform-selection}/deployment.json`,
   `examples/bioma/computers/{computers,host}.json`, and
   `mcp/conformance/profiles/hosted-server.example.json`.
3. Delete the unused `PREPARED_PROBLEM_VERSION` in
   `servers/optimization-mcp/src/problem_store.rs:18`.
4. Rename the architecture catalog entries in `docs/architecture/catalogs/*.csv`, then
   run `docs/architecture/tools/render.py` and `validate.py`. Render the PDF in a headed
   browser with hardware-backed graphics, following the GPU rules in `AGENTS.md`.
5. Add `cargo xtask enforce identifiers`. It scans tracked text files and fails on
   `io.veoveo`, `veoveo.io`, and `ai.bioma`. Its allowlist names only the documents
   that describe the cut: the Naming section of `AGENTS.md`, CE-10 in
   `docs/CONTRACT_EVOLUTION.md`, and this plan until its deletion. Historical records
   that stay in the repository are renamed like everything else. Add the target to the
   xtask surface listed in `AGENTS.md`.
6. Update the identifier rows in `mcp/contract/DESIGN.md`,
   `mcp/apps-extension/DESIGN.md`, and `deploy/contract/DESIGN.md`.
7. Rename the identifiers in the standards registers: the Optimization row in
   `README.md`, the cuOpt row in `docs/TECH_DESIGN.md`, the travel-model and executor
   references in `docs/ARCHITECTURE_DECISIONS.md`, and interfaces `VV-IF-041` and
   `VV-IF-042` in `interfaces-and-protocols.csv`.

### Reference installation reset

Write the reset procedure into `examples/bioma/README.md` using its existing runbook
commands. It stops the platform, deletes the SurrealDB volume, the Artifact object
storage, recording hub journals, and Computers retained homes and host journals, then
reinstalls from the new lock through GitOps. Run it without waiting for approval.

Acceptance:

- `cargo xtask enforce identifiers` passes, and the grep above finds matches only in its
  allowlist.
- `cargo test --workspace`, the Python SDK tests, and `cargo xtask enforce rust|python|docs` pass.
- The reference installation passes `installation-verify` from Phase 2 and live
  conformance certification after the reset.

## Phase 2: Installation Targets For Installed Smoke

Installed smoke scenarios default to the reference installation's cluster, ports, and
URL, and they hardcode its signing key ID, identity provider, tenant, deployment list,
and GPU count. A user with their own domain cannot run them. This phase makes the
installation an explicit input. The reference installation keeps running the same
checks from its own target file.

Work:

1. Implement `veoveo.ai/installation-target/v1` in `deploy/contract`, as specified in
   [its design](../deploy/contract/DESIGN.md#installation-target).
2. Add `examples/bioma/installation-target.json` with today's values: context
   `k3d-veoveo-bioma`, local base URL `http://127.0.0.1:8781`, public base URL
   `https://veoveo.bioma.ai`, the deployment list in
   `testing/smoke/src/bin/smoke/scenarios/bioma.rs:23`, its GPU minimum, its operator
   scopes, and its control-plane path.
3. Rename the scenario `bioma-verify` to `installation-verify` and the module
   `scenarios/bioma.rs` to `scenarios/installation.rs`. Each installed scenario takes
   a required `--installation <file>` and drops its default context and URLs
   (`testing/smoke/src/bin/smoke.rs:87-108`, `:538-540`, `:605-611`). Update
   `tools/xtask/src/commands/smoke.rs:266`.
4. Read the access-token key ID, the identity provider's authorization endpoint, and
   the tenant from the control plane named by the target. That replaces the
   `veoveo-bioma-2026-07` key ID, the Microsoft Entra host check, and the `"bioma"`
   tenant (`scenarios/artifact_consumers/python.rs:82`). Read deployments, GPU minimum,
   and operator scopes from the target.
5. In `scenarios/recording_catalog_sdk.rs`, remove the context check at line 17 and take
   the host mapping and Rerun endpoint from the target.
6. Make the generic chart assertions in `testing/deployment-smoke/src/helm_config.rs`
   run against `testing/fixtures/fork-installation` and
   `testing/fixtures/platform-selection`. Keep the reference-installation assertions as
   a separate module that renders `examples/bioma`.
7. Update the smoke command in `examples/bioma/README.md` (line 593) and any guide that
   names `bioma-verify`.

Acceptance:

- An installation target for the fork-installation fixture parses and validates.
- `installation-verify` passes against the reference installation with its target
  file.
- The recording catalog SDK smoke runs against an installation whose hostname and
  tenant differ from the reference, without editing Veoveo source. A disposable local
  profile from [`LOCAL_DEPLOYMENT_PROFILES.md`](LOCAL_DEPLOYMENT_PROFILES.md) serves as
  that installation.

## Phase 3: Resource Contract Corrections

The survey found existing violations of rules C04, C27, and C28. Fix each in its owning
server with a test that fails before the fix.

| Server | Violation | Fix |
|---|---|---|
| frames | `DESIGN.md` promises resource notifications, but `listen` passes no hub (`servers/frames-mcp/src/bin/server.rs:400`) | Emit world and list changes from a Store LIVE source through `SubscriptionHub` |
| duckdb | `listen` accepts resource filters that never fire (`server.rs:361`) | Reject resource filters and declare no resource subscriptions until a source exists |
| timeseries | same as duckdb (`server.rs:290-292`) | Same as duckdb |
| media | usage URIs are notified (`usage.rs:155`) but rejected by `listen` (`server.rs:809-830`) | Make usage URIs subscribable, or stop notifying them |
| map | spatial and raster derivations are notified but not subscribable (`mcp.rs:2487`) | Add them to the subscribable set |
| recording | the catalog URI is notified (`server.rs:131`) but not subscribable | Make it subscribable |
| map, media, optimization | per-process broadcast hubs do not survive restarts or reach other replicas | Feed their hubs from Store LIVE queries with change-feed recovery, as Time and Recording do; do not add outbox consumers |
| reason | the analyses index scans every analysis (`server.rs:624`) | Bounded cursor pages at the store |
| stream | run and session lists are unbounded | Bounded cursor pages |
| time | calendar, epoch, and event collections are unbounded (`catalog.rs:382,487`) | Bounded cursor pages |
| recording | the catalog caps at 500 with no cursor (`service.rs:284`) | Bounded cursor pages |
| uav-sim | `list_resources` enumerates every grant, plan, and task | List roots and templates only, with paged collections |
| speech | docs, contract, and artifact resources omit MIME types (`mcp.rs:143-148`, `:171-173`) | Declare MIME types |
| mcp/contract | `CHECKLIST_IDS` stops at C30 (`src/docs.rs:34`) while the checklist has C31 | Add C31, and declare it in every server's `AGENTS.md` |
| gateway | every resource read calls the upstream `list_all_resources` (`platform/gateway/src/mcp/resources.rs:372-400`) | Resolve the owning server from the cached discovery surface |

Acceptance: each server's tests cover its fix, and each server's `AGENTS.md` updates
the affected compliance entries.

### Modular Types And Server Contracts

[CE-13](CONTRACT_EVOLUTION.md#ce-13-modular-types-and-server-owned-contracts) accepts
this architecture across Veoveo. Build the foundation before continuing scope and
URI adoption. This is required Phase 3 work; the first converted servers establish
the pattern, and do not establish repository-wide completion.

| Owner | Target responsibility |
|---|---|
| `platform/types`, crate `veoveo-types` | Protocol-independent platform identity and attribution, validated names and resource URIs, focused builders, and public extension traits such as `ScopeDefinition` and `ResourceAddress` |
| Each server library's public `contract` module | Its scope enum, domain IDs, resource address variants, and request/response types |
| `mcp/contract` | MCP-specific traits that consume the foundational types, descriptors, discovery integration, protocol conversions, and hosted-server setup requirements |
| Server runtime and policy owners | Domain operations, current authorization, persistence, and protocol handlers |

The foundational crate has no dependency on RMCP, server implementations, database
clients, GPU libraries, or asynchronous runtimes. Keep domain vocabulary in its owning
library. The gateway accepts validated scope names and registration data without
exhaustive matches on server-specific scopes or resources.

Rust cross-server consumers depend on the owning library with
`default-features = false, features = ["contract"]`. That feature exposes the public
contract with foundational types and the required value, validation, and
serialization/schema support.
MCP integration and runtime modules have separate feature gates, and their Cargo
dependencies are optional. Binary targets require their runtime features. Adding a
feature name without removing runtime dependencies does not meet this requirement.
A separate contract crate needs a concrete dependency or independent release reason.

#### Review Each Contract Change

Review the affected contract before extending its implementation. Trace a representative
caller through construction, authorization, and persistence to identify where an API
loses domain types or requires knowledge of another component's internals. Apply the
following criteria throughout this plan, including audit, knowledge, and store work:

| Concern | Required result |
|---|---|
| Ownership | Name the library that owns the vocabulary and inspect the dependency direction. A cross-server consumer imports that owner's public contract; adding the domain must not add a dependency or registry entry to MCP core. |
| Typed interfaces | Keep specific scope, ID, resource, and query types through public and internal calls. Validate external values at admission and convert them to wire or driver representations at those adapters. |
| Construction | Prefer a typed constructor for a simple value. Add a focused builder when options or relationships make construction difficult. Required inputs retain their types; validation checks relationships before exposing a usable value. Use typestate only when it eliminates a concrete invalid sequence. |
| Contract adequacy | When the existing model cannot express the required invariant, change the owning contract and migrate its callers. Record the reason and material tradeoff in the owning design and this plan; update CE-13 when the ownership architecture changes. |
| Qualification | Demonstrate valid construction and rejection of invalid combinations, plus the affected domain behavior. Prove contract feature isolation with an independent consumer; qualify the current wire and stored formats with their consumers before claiming completion. |

Make these decisions within the accepted product architecture without waiting for the
user to identify each gap. Explain material decisions as work progresses. A new wrapper,
builder, or trait is complete only when the affected callers use it and its promised
invariants are checked. Record remaining adoption explicitly in the inventory below.

#### Implementation

1. Inventory current type ownership and Cargo dependency paths across servers,
   shared contracts, gateway, policy, SDKs, tests, and CLI tools. Record the scope and
   resource interfaces to migrate and their owning libraries. Identify contract
   modules that currently import runtime types, and resolve dependency cycles before
   extracting the foundation. Add its owning `DESIGN.md` and update the code map.
2. Extract `ScopeName`, `ResourceUri`, and the supporting types required by their
   actual consumers into `veoveo-types`. Keep the crate small; a shared use site alone
   does not transfer domain ownership. Move internal imports in a hard cut without
   aliases or duplicate definitions. Preserve wire spellings and current policy
   semantics. Change affected protocol and persisted formats by coordinated hard cut;
   qualify the current format with current consumers on a fresh installation.
3. Define small public, unsealed traits for scope-name conversion and typed resource
   address parsing/serialization. Each server implements them with its own enums
   and ID types. Define MCP-specific associations in `mcp/contract` using those
   traits and existing RMCP handlers. Shared constructors consume these interfaces;
   adding a server cannot require a core domain registry or a new match arm.
4. Use one server-owned declaration of each scope's wire spelling for authorization,
   serialization, configuration defaults, and advertised vocabulary where applicable.
   Domain helpers take the owning enum. Generic policy consumes validated names and
   continues to admit installation-defined scopes. A caller's full grant set may
   contain scopes outside a particular server's vocabulary.
5. Provide ergonomic builders that preserve the types of required IDs and query
   fields and validate field combinations before construction. Use typestate when it
   prevents a concrete invalid construction sequence. Resource variants own route
   shapes and supported parameters. The shared builder delegates component parsing
   and encoding to a maintained URI library; begin with the existing pinned `url`
   implementation and qualify its supported custom-scheme profile. Keep strings at
   serialization and driver bindings. Typed query APIs still apply current tenant,
   owner, context, labels, parent relationships, and filters in SQL before limits.
6. Add and qualify contract-only library features, then migrate all owned scope and
   resource paths in small commits. Map and Time are the first consumers. The known
   inventory also includes View and UAV scope helpers, configurable administrative
   middleware, gateway and policy literals, and Map URI builders and Store record-key
   APIs that erase domain types. Complete the inventory through other servers,
   clients, SDKs, templates, and CLI tools. Other languages use their native types and
   builders with the same ownership and wire contracts.
7. Extend shared conformance and template guidance. Reuse ordinary server libraries
   for domain tests and CLI operations. Hosted checks consume a profile and the
   discovered surface without importing server implementations. Traits establish
   API requirements; no marker trait claims behavioral compliance.

#### Acceptance

- An independently defined fixture server adds a new scope and resource family,
  implements the public traits, and passes applicable hosted conformance with only
  registration/configuration changes. Its implementation is not added to either
  foundational or MCP core source. A separate consumer imports only its public
  contract and constructs typed resource addresses.
- Isolated contract-only builds and Cargo dependency inspection prove that runtime,
  MCP integration, database, GPU, and provider dependencies are excluded. Run these
  checks outside a workspace feature-unified build that could conceal missing gates.
  Run supported runtime configurations separately to prove the gates compose.
- Compile-fail cases reject raw strings at domain authorization APIs, wrong-domain
  scope and ID types, and invalid required builder states. Runtime cases reject
  unknown domain scope spellings while allowing unrelated validated names in the
  caller's grant set. Grant decisions match the existing policy.
- URI tests cover build/parse round trips, existing published wire forms, custom
  schemes, reserved characters, percent encoding, repeated or unsupported parameters,
  fragments, normalization, and malformed IDs. Discovery templates agree with typed
  builders. Database tests reject incorrect parent combinations and apply visibility
  before limits; a URI's type alone grants no access.
- Applicable hosted conformance and owner-local lifecycle tests pass. Qualification
  keeps authentication, authorization, recovery, SQL selection, and required hardware
  checks distinct from compile-time structure. Update owning designs and compliance
  declarations with their actual implementation and qualification status.

#### Hosted Server Adoption

`veoveo_mcp_contract::hosting` hosts a server from its checked setup, a typed
`DomainServer` and optional `TaskSupport`. The contract's
[Hosting A Server](../mcp/contract/DESIGN.md#hosting-a-server) section is the
procedure. `servers/duckdb-mcp` is the reference for a server with durable tasks, and
`servers/frames-mcp` for one that also publishes resource changes. DuckDB, Timeseries,
Frames and Media are migrated; each migration removed between 480 and 650 lines.
`templates/rust-mcp` is the starting point for a new server. A migrated server can add
in-process router tests through the contract crate's `testing` feature.

Migrate each group below together, then run its grouped checks once: the affected
packages' tests and strict Clippy in one pass. Commits may stay split by server where
that keeps them coherent. Each migration deletes the server's `impl ServerHandler`,
`host.rs`, `internal_auth.rs`, `admin.rs`, `mcp_page` helper and identity extractors.
Each domain read names its cache policy through `DomainRead`; reads that set a zero
TTL today, such as knowledge-source members in Artifact, Knowledge and Reason, use
`DomainRead::no_store`. Behavior that the host now standardizes changes deliberately:
a missing Host authority is 400, an unparseable address is Invalid Params, and lists
are authenticated.

| Order | Servers left | Builder extension |
|---|---|---|
| 1 | time, view, uav-sim, optimization | Available: prompts and completion on `DomainServer` |
| 2 | stream, reason, speech, recording | Available: `ResourceSubscriptions` with `DurableTasksWithResources` |
| 3 | map, artifact, knowledge, computers | Needed first: domain hooks for dynamic `resources/list` and caller-filtered tool lists; computers and knowledge move their tools onto `#[tool_router]` |

Add each remaining extension to the host with a test before the servers that need it
migrate. A server-specific HTTP route uses `authenticated_routes`; a probe uses
`readiness`. A route that verifies its own caller, such as Media's signed provider
webhook, uses `public_routes`. A server whose integration tests include a server
module by path keeps Store-backed authorization in a free function over typed
addresses, as Media's `subscriptions::authorize` does.

#### Migration Inventory And Status

All sixteen Rust MCP server packages under `servers/` have library targets and define the
`contract` feature. Independent consumer qualification is recorded in each owning row.
Artifact, Computers and Speech also pass one separately resolved consumer and grouped
native default-feature tests; their runtime-only libraries compile. Existing libraries remain the
default owner; the inventory must not become a central domain-type registry.

| Surface | Current dependency or representation gap | Next owning change |
|---|---|---|
| Foundational primitives | `ScopeName`, `ResourceScheme`, `ResourceUri`, and `IdentifierError` are extracted into `platform/types`; direct callers use that crate. `ResourceUri` validates concrete RFC 3986 references through the existing URI library and returns redacted `ResourceUriError`; templates use `ResourceTemplateUri`. Generic references preserve network resource URLs with ports, while domain routes apply the stricter component profile. Native shared/server contract, independent consumer and Gateway tests and workspace-wide all-target strict Clippy pass. The 26-image release converges at `dc9cbf6f`; 91 public URI checks, Knowledge source-linked retrieval, Artifact delivery and Stream live GPU notifications pass | Complete the owner-specific builders and relationship admission listed below |
| Independent extension traits | `ScopeDefinition`, `ResourceAddress` and `TaskResourceAddress` are public and contain no domain variants; `McpServerContract` associates server-owned types with descriptors and documents. Artifact, Computers, Speech, Frames, Timeseries, Media, Time, UAV, Reason, DuckDB, Optimization, Recording, View, Stream and Map consume checked setup. The independently owned fixture passes hosted conformance, typed access/denial and contract-only consumption. Map's installed discovery and change/restart checks pass. Python defines nominal foundational types and open generic MCP associations; Datasheet consumes checked setup and the independent owner fixture passes | Preserve domain-owned authorization and complete the remaining owner-specific type gaps |
| Scope declarations | Every Rust server contract uses `scope_enum!`, including uninhabited empty vocabularies; checked setup consumes its `ALL` slice. Nonempty declarations reject invalid or duplicate spellings at compilation. Recording producer permissions also remain owner-local and typed through OAuth requests, discovery and Hub admission. The independent consumer covers all sixteen server libraries | Qualify installed producer admission with the next composed publication; wider field relationships remain in their owner rows |
| Resolved invocation authority | Capability and Work Context membership levels, invocation authority and output defaults belong to `veoveo-types`; callers import them directly. Five schemas, serialized authority bytes, nested identity admission and level ordering pass native and independent-consumer checks. MCP retains configuration and membership matching | Preserve complete authority when extracting domain contracts; qualify installed policy and composition consumers |
| Concrete URI components | `ResourceUriParts` validates the concrete profile with URL 2.5.8; `ResourceUriBuilder`, `UriAuthority`, and percent-encoding 2.3.2 encode typed scheme/authority and path/query components, preserve segment identity, and reject duplicate query names | Adopt through domain constructors with specific ID types; qualify each family's spelling and parameters |
| HTTPS network addresses | `HttpsUrl` owns canonical ASCII HTTPS syntax, immutable parsed access and redacted diagnostics. Source URLs retain signed query bytes and permit nondefault ports and repeated query names. DuckDB, Map and Timeseries preserve the type into the shared download runtime; host/DNS/redirect policy still controls access | Adopt this profile where other domain contracts require HTTPS; qualify installed source consumers |
| Resource templates | `ResourceTemplateUri` uses iri-string 0.7.14 with guards for RFC prefix bounds and dotted variable names; `McpResourceTemplate` prevents descriptor mutation. Time, UAV, Reason, DuckDB and the independent fixture consume checked template declarations. Native and isolated-consumer cases qualify syntax, expansion, existing addresses and error redaction | Extend checked declarations and domain-builder agreement across remaining servers |
| Gateway completion and audit targets | Completion and template discovery use `PolicyTarget::ResourceTemplate`. Stored policy events require the current v2 marker and typed event; the historical DTO adapter is removed. Native cross-connection reads and invalid-write checks pass | Qualify installed authorization and current-format audit reads; tighten the opaque resource validator after remaining URI families are inventoried |
| Platform identity and attribution | Principal, tenant, group, role, Work Context, delegation, data-label, and policy-version types, access subjects, and invocation provenance are extracted into `platform/types`; consumers import them directly; eleven baseline schemas, wire/profile tests, independent consumer tests, and strict workspace Clippy pass | Preserve these contracts during domain extraction; qualify installed identity and policy behavior with the affected services |
| Map | `MapScope` owns handler, Task, and default administrative scope spellings. Checked setup supplies startup and discovery with current-grant filtering and configured App metadata. Fixed resources, documents and direct authoring/catalog addresses use `MapResource`; builders require parent ID types and parsers return those IDs. Workers and tool links consume the builders. Authoring metadata pages use typed IDs and shared components; identity, Artifact metadata and geodetic IDs come from owner libraries. Contract-only consumption excludes runtime dependencies. The checked-setup and direct-address batch passes 35 installed source checks, including creation, changes, restart persistence and denial | Complete remaining paged-query addresses, DTO relationships and Store query IDs |
| Coordinate vocabulary | Map owns geodetic IDs; Frames owns worlds, conversions, and typed world/revision/frame addresses; RRD owns recorded frame/geofence metadata. Shared MCP coordinates are removed. Independent contract consumption and schema compatibility pass | Qualify installed consumers with the current absolute frame-ID profile |
| Map identity admission | Source, restriction, mobility, travel-model and six product-address families use canonical RFC UUIDv5/v7 IDs. Other domain IDs accept broader UUID-library spellings; Store authoring keys check only a prefix, byte bound, and slash exclusion | Apply the owner admission profile to remaining IDs and Store query APIs; qualify current-format installed consumption |
| Time | The contract feature excludes runtime dependencies; handlers, Tasks, and configuration defaults use `TimeScope`; `TimeResource` owns every URI family and the five collection cursor types. Checked server setup supplies startup, discovery and scope membership. Private runtime persistence owns SQL, mutation drafts and driver records; catalog calls retain domain IDs, versions, completion parents and cursors until driver conversion. Checked catalog decoding binds JSON identity, versions and indexed fields to the stored row; native corruption and immutable-acquisition checks pass | Complete broader DTO types and qualify current-format installed behavior; completion now requires the advertised reserved-expansion zone template |
| Time identity admission | Time owns both profiles: public IDs accept bounded prefixed names, including bootstrap authority references, while stored catalog keys require UUIDv7 suffixes. Persistence validates the stored profile without narrowing public provenance; Store has no Time query or draft API and owns the shared connection and migrations | Qualify installed admission; use the distinct current public and stored ID profiles when strengthening metadata construction |
| Time scalar admission | Clock policies have a checked builder. Metadata versions use `TimeVersion`, with a distinct zero-only source-creation input and current-format body decoding. `SubsecondNanoseconds` covers instants, expressions, cursors and persistence drafts. Total-coordinate conversion and NTP/UTC epoch arithmetic check seconds overflow; native boundary, transition and retained-row cases pass. JSON keeps its numeric shape. Requests and persistence keep typed guards; exhaustion checks preserve rows and atomically roll back failed retirement | Qualify installed numeric admission; finish remaining expression/projection scalar types and acquisition-state relationships |
| Time intervals | `TimeWindow` checks increasing bounds and a common authority at construction and decoding; accessors preserve these invariants. Algebra keeps endpoint uncertainty, selecting the maximum at tied coordinates. Schedule expansion clips to the horizon while preserving recurrence limits and labels. Native membership, metadata, authority and clipping cases pass; independent contract consumption qualifies the unchanged valid wire shape | Qualify current-format installed schedule Tasks and restart recovery |
| Time active-pointer admission | One SQL statement resolves each visible pointer to an active release with matching tenant, family and key. Pointer identity, history and versions are checked; activation rechecks parent and lifecycle relationships in the transaction. Native corruption, SQL payload exclusion and ten interleaved-mutation rollback cases pass | Qualify installed parent admission and transactional conflict rollback |
| Time authority contexts | Each engine request validates the joined active selection and provenance before reusing loaded files. Cache keys use the Store tenant type, engine epoch maps are independent, Store signals evict contexts and failures remove cached values. Native restart, isolation, disconnected-observation, provenance and file-recovery cases pass | Qualify installed restart/replica behavior and the declared coordinated upgrade |
| Time authority metadata | Checked references derive release identity from typed URIs. Effective pairs require the correct dataset roles and distinct IDs, and derive instant bindings. Wire adapters preserve valid fields; the runtime context keeps metadata and bindings private. Native retained-row and independent-consumer admission checks pass | Qualify current binding validation and installed startup |
| Time resolution metadata | `ResolveTimeOutput` admits matching instant/release pairs and protects them with read-only accessors; its wire adapter preserves flat projection fields. Engine epoch keys remain typed, relative calculations reject foreign authority and preserve uncertainty, and additional uncertainty checks overflow. Native and independent-consumer cases qualify these relationships | Qualify installed resolve/convert decoding and epoch behavior after authority activation; computed representations remain the engine's responsibility |
| Time activation preflight | A private draft carries the candidate and both admitted active families through file loading to SQL commit. SurrealDB 3.3.0 `FOR UPDATE` locks the selected pointer and release records, including absent identities. The shared tenant-write fence is removed. Native RocksDB contention, pointer/release repair and rollback checks pass | Qualify installed activation and concurrent conflict rollback |
| Digest wire profiles | Time's `AuthoritySourceDigest` preserves bare hexadecimal spelling through metadata, requests and typed persistence drafts; canonical content comparison and shared provenance use the foundational `sha256:` value. Native cases preserve uppercase retained data and idempotency while rejecting malformed matching rows. View still uses bare hexadecimal text | Qualify current digest admission in installed Time; migrate remaining owners and callers by hard cut |
| Computers | The isolated contract owns distinct Computer, execution, file-transfer, automation-grant, interactive-access, CLI-pairing and connection IDs, the complete resource vocabulary and an empty scope enum. Lifecycle and maintenance carry foundational TaskId through public receipts, worker queues and allocator handoff. Provider checkpoints use a separate correlation type for lifecycle and containment operations. Domain APIs, gateway routes, relays and generated browser schemas adopt those types. Checked hosted setup supplies startup and discovery. Owned and granted Computer reads admit identity, clearance and current policy before decoding; final SQL rechecks authority before public ordering and limits. Lifecycle and command/file Tasks resolve scoped policy inputs before reading private state. Browser/CLI grant reads bind credentials, sessions, parents and providers in SQL. Maintenance, reservation and execution receipts, claimed journal reads and Task-link repair apply SQL admission before decoding; closed typed Task payloads connect admission and workers Request, template and provider identities remain typed through the runtime and Store adapters. Checked completed-result constructors derive addresses and enforce identity, size and integrity relationships. Artifact references and session-family leases use their owner types; the grouped native and independent consumer checks pass | Complete remaining grant/input and state-view DTO relationships, including principal/client owner references; qualify the composed batch in the installation |
| Speech | The isolated contract owns distinct transcription/dictation IDs, every public resource family and an empty scope vocabulary. Checked hosted setup supplies initialization and discovery. Application execution, resource subscriptions, Gateway targets and Console routes retain the owner types; receipt decoding and completed output reads check parent identity. Independent consumers, compile-fail cases and native callers pass; generated browser schemas use the qualified shared converter | Qualify current-profile CUDA transcription/dictation and installed delivery; strengthen remaining transcript result relationships |
| Artifact plane model | `platform/artifacts/contract` owns occurrence identity, metadata, compliance, provenance, release state, grants, share-link values and byte handoffs. The separate plane service/client prevents placing their common model in the MCP server package without a Cargo cycle. Its server library exposes tool DTOs and typed `ArtifactResource` families through an isolated contract feature. Shared URI components build addresses from occurrence IDs or closed document variants. Checked hosted setup includes the Library App, index and embedded documents; both registrations declare revision 3. Independent consumption, direct consumer tests, and wire/schema qualification pass | Installed full/HEAD/range reads and anonymous sharing pass. Typed index cursor addresses are implemented; separate the remaining access/service request contracts. Upload ownership uses a typed SQL owner selector before decoding; the malformed foreign-row regression and all thirteen native service cases pass |
| Artifact identity and URI admission | `ArtifactId` checks version and RFC variant; `ArtifactUri` owns neutral/presented variants, preserves accepted URI spelling, and builds from typed IDs and schemes; metadata checks wire ID/URI agreement | Qualify installed consumption of the current identity and URI contract |
| Remaining Artifact references | Download URLs, some Store DTOs, and other domain URI fields still use broader string profiles | Migrate with each owning contract; distinguish Artifact identities from external fetch locations and declare persisted/profile changes |
| Artifact attribution construction | `ArtifactProvenance` uses foundational `InvocationProvenance`; a private wire adapter preserves valid flat metadata and requires each mode's identities in both decoding and schemas | Native publication/readback, independent consumption, schema/decoder parity, and compile-fail qualification pass; qualify installed metadata consumption during reference acceptance |
| Media public contract and hosted setup | The isolated library owns public requests and responses, registry entries, typed model/prediction identities, all hosted resource variants and an empty scope vocabulary. Checked MCP setup supplies startup and discovery; resource reads and subscription admission share the owner parser. Model IDs remain typed through provider submission and output attribution. Opaque prediction builders share the declared template encoding; model routes use reserved expansion. Provider HTTP URLs use component builders | Complete remaining DTO relationships and model catalog paging; qualify registration, installed behavior and provider recovery budgets |
| Frames hosted setup | The isolated server contract composes world, operation, usage, Artifact and document routes into `FramesResource`. Startup and discovery consume checked setup; reads and mutable subscription admission use owner parsing. No domain scopes are added | Qualify current resource admission and subscriptions through installed clients |
| Frames world reads | Frames owns typed reads over the existing Store client; SQL applies visibility and parent checks, world catalogs use typed keyset pages, and completion binds parents and matches before limits. Six isolated native cases pass. Discovery is static; private driver records and mutations also belong to Frames | Qualify installed paging and completion with current catalog consumers |
| Frames mutation inputs | Frames owns typed mutations, private driver records and world-event vocabulary. World publication checks owner, current labels and head agreement in the transaction; repeated writes settle from authorized matching state. Store has no world draft API | Qualify installed publication and concurrent replay under current writer policy |
| Frames world metadata construction | The shared domain crate below Frames and Recording runtimes owns the public model; the MCP contract feature re-exports it. Checked summaries, immutable revisions, and source references derive their repeated identities from typed URIs. The contract owns complete-tree validation and hashing; Store reads and UAV use it. Native corruption/visibility and independent schema/consumer checks pass | Qualify current world metadata with installed consumers |
| Frames operation references | Operation addresses use typed component builders; checked provenance derives its ID from the URI and rejects conflicting wire identity. Existing schema snapshots and independent contract consumption pass | Qualify installed consumption and current provenance checks |
| Frames stream references | `FrameStreamUri` applies the shared concrete URI profile; `FrameEntityPath` checks bounded producer selectors. Independent consumption, current schemas and native node qualification pass. Frames preserves source spelling and leaves route vocabulary with producers | Qualify installed behavior; complete each producer's typed builders |
| Frames usage visibility and pages | TaskRuntime applies current Task owner policy and linked-record agreement in SQL before grouping and limiting usage. Frames owns checked pages and typed Task cursors/URIs in its isolated contract feature; native denied-row and cursor cases pass | Qualify installed reads and subscriptions with current catalog consumers |
| Frames operation visibility | Frames owns SQL-scoped operation reads and transactional authority/immutable replay checks. Rust and Store require the authority object and profile; native caller, parent, schema and event-rollback cases qualify the current format | Qualify installed direct/Task operation reads and current-format recovery |
| Timeseries resource admission | The isolated library owns `TimeseriesResource`, closed document identities and typed Artifact handoff construction. Resource reads parse once; startup and discovery consume checked setup with unchanged task-only subscriptions. Parsers reject malformed route components and unsupported queries | Qualify current Artifact handoffs and installed resource consumption |
| Timeseries forecast admission | The forecast request uses DuckDB’s inline/HTTPS source profile and checked column names, horizon and filter values. Constructors and decoding reject invalid inputs before Task creation. Extraction applies finite-value and training filters in SQL. Native execution, schema/decoder admission, isolated consumers and strict Clippy pass | Qualify installed forecast admission and Artifact output; wider result relationships remain open |
| Timeseries usage | The library delegates pages and exact reads to TaskRuntime's SQL owner policy; typed usage URIs, cursors and checked pages belong to its isolated contract feature. Valid version 1 cursor bytes and response fields are preserved. Native denied-row, continuation, label-change and parent-metadata cases pass; the independent 60-package consumer and strict runtime/workspace Clippy pass | Qualify the coordinated replica replacement and installed reads |
| DuckDB usage and discovery | The library uses TaskRuntime SQL visibility for 100-entry usage pages and exact reads. Its isolated contract owns usage addresses, collection-bound cursors and checked pages; discovery declares roots/templates without scanning records. The unused unbounded Store usage catalog API is removed. Native reads and Spatial, the independent 59-package contract consumer, and strict runtime/workspace Clippy pass. Workbench cursor construction uses the browser URL API; headless navigation and reserved-character cases pass | Qualify installed page consumers and headed hardware Workbench acceptance |
| Optimization usage and contract | The library selects explicit owner-plus-Work-Context policy in TaskRuntime SQL before grouping and limits. Context records and stored authority must agree. Contract-only types preserve version 1 cursor bytes and report fields. Unscoped Store usage reads and Rust post-filter helpers are removed. Native owner/context selection and current-authority checks pass. The independent contract consumer preserves eleven solver schemas and excludes service dependencies | Qualify the coordinated control/executor replacement and installed reads |
| Optimization catalogs | Domain-owned readers match indexed and retained owner/context fields in SQL before pagination, exact lookup and completion. Optional tenants remain distinct; selected malformed records fail explicitly. Typed collection builders preserve version 1 cursor bytes. Native database checks, independent contract consumption and strict Clippy pass | Qualify current catalog permissions and installed consumers |
| Optimization resource admission | Typed domain builders, canonical output IDs and exhaustive resource dispatch replace string construction and prefix parsing for every hosted family. Checked MCP setup preserves discovery metadata and public schemas. The server declares no domain scopes and preserves gateway operation policy. Native template, setup and isolated-consumer checks pass; Map travel-model references use Map's contract feature | Qualify installed readiness and resource admission; finish DTO relationship checks |
| Map travel-model reads and shared references | Map owns typed model addresses and version 1 native Task cursors. Optimization imports the owner library's contract feature. SQL selects caller-owned successful results before limits and grouping, checking stored owner/request/result agreement. Native paging, clearance, malformed-row and isolated-consumer checks pass | Qualify installed page traversal with current consumers; complete DTO relationship admission |
| Map restriction reads | Map owns typed addresses, collection-bound cursors and checked compact pages. SQL applies tenant visibility before limits and operational time/family/withdrawal selection. Exact reads and pages reject indexed/document disagreement. Native page, validity, corruption, overflow and independent consumer checks pass; the unbounded Store list is removed | Qualify installed summary pages with current consumers; finish broader restriction DTO admission |
| Map product addresses | The contract owns typed release, source feature, raster, raster derivation, spatial derivation and route URIs. Builders and templates agree; exact reads and route subscriptions use owner parsing. View imports those types and checks source-feature release membership. Contract, native and independent-consumer checks pass | Type the remaining product DTO references and parent relationships; qualify current installed consumption |
| Map routing authority | One domain-owned SQL statement selects compatible enabled sources through tenant/dataset-matching active pointers and valid active releases. Typed release/family sets replace full-catalog scans and per-release lookups. Native tenant, lifecycle, parent, boundary-time and retained-document cases pass | Qualify installed routing; finish other internal release selections |
| Map source catalog | Map owns typed addresses, collection-bound cursors and checked public summaries. Exact reads, pages and completion select the tenant in SQL before limits; selected documents must agree with indexed metadata. Native isolation, continuation, redaction and corruption cases, isolated contract consumption and strict Clippy pass; Map Explorer walks the page envelope and the unbounded Store list is removed | Qualify current source-ID and document admission and installed page traversal |
| Map mobility catalogs | Map owns checked profile versions, exact and collection addresses, composite cursors and complete-profile pages. SQL selects tenant, parent, ID and numeric version before limits. Native isolation, ordering, completion and corruption cases, isolated contract consumption and strict Clippy pass; Map Explorer traverses pages and the unbounded Store read is removed | Qualify installed traversal with current consumers; UAV grants/handoff and the flight harness consume Map-owned addresses |
| UAV Map admission | Map-owned profile URIs and handoffs replace copied DTOs and manual parsing. SQL applies profile/advisory grants before selection and rechecks them during execution admission. Selected plans validate indexed metadata and physical identity; focused native policy, corruption and revocation checks pass | Qualify current adapter/Task restart recovery and installed grant/mission behavior |
| UAV execution exclusion | Admission guards a retained queued Task and writes its exact link, vehicle lease and plan in one transaction. Mission reads follow that link; unresolved outcomes retain their Task pin. Native correlation, cancellation/rollback and SQL cleanup-page tests cover the current admission model alongside contention and HTTP interruption | Retain complete observations across process loss; qualify current-format installed recovery and operator reconciliation |
| Media usage and prediction reads | Media owns current-owner and linked-record SQL selection before limits, typed 100-entry catalogs, static discovery and query-backed subscriptions. Billing selects unsettled jobs in SQL with Task/tenant/provider correlation. Native database and isolated contract checks pass; the prediction summary schema is preserved | Qualify current catalog consumers and installed subscription recovery |
| Media generation result | The isolated contract owns checked current generation results, typed addresses and output attribution. SQL selects successful linked Tasks under current owner policy. The old decoder, result conversion and rollback-only smoke option are removed. CLI downloads use the current result and verify resource equality | Qualify current-format installed delivery, caller isolation and restart behavior |
| Native Task identity | `veoveo-types` owns `TaskId`; consumers import it directly and Store's `task_record_id` performs database conversion. Native lifecycle, wire preservation, compile-fail, and independent consumption checks pass. Frames uses it in usage cursors without runtime dependencies; runtime external lookups still require v7 and opaque MCP handles keep their own profile | Native APIs and all workspace callers pass strict compilation; the shared runtime, changed admission adapters and isolated contract consumer pass native qualification. Qualify installed consumption |
| Shared public Task reads and notifications | Exact owner reads, subscription baselines and current-state delivery apply SQL visibility before decoding. Typed native IDs reach driver bindings. Public delivery uses event identities to select current Tasks; trusted internal replay preserves historical transitions. Native lifecycle, revocation, malformed-row and denied-page cases pass | Qualify current-format installed delivery; audit domain-specific Task adapters for their additional policy. Caller input-response transactions now recheck owner, context and operation selection; native rollback, malformed-row and competing-answer cases pass |
| DuckDB source contract | The isolated `contract` feature owns source vocabulary, read SQL helpers, checked resources/pages, Artifact origins and operation-usage metadata. Timeseries and the agent kernel consume that owner directly; MCP core has no domain dependency. Checked MCP setup owns startup and discovery. Database catalogs page over the current owner directory; owner keys distinguish optional tenant values. Execution keeps native Task IDs, direct calls carry no Task association, and publication checks capability/origin agreement. Export variants enforce selection/format agreement; query-result builders check row shape, count and output mode. Source URLs retain foundational HTTPS types through materialization; nonempty lists and neutral Artifact addresses are admitted before execution. Request builders check attachments, positive limits and output relationships; SQL/table/column text and reader options stay typed through execution. The inline/HTTPS profile is shared with Timeseries; DuckDB composes it with authenticated Artifact input. The grouped native and independent-consumer checks qualify 38 DuckDB schemas and schema/decoder agreement, with eleven DuckDB compile-fail examples | Qualify installed source consumption, catalog pages, the fresh owner-directory/metadata formats and Artifact publication/recovery |
| UAV contract | Contract-only imports expose public mission, grant and live-view v4 types without service dependencies. MCP core has no live-view exports; gateway owner conversion stays in the authenticated adapter. The independent consumer qualifies 96 schemas. Atomic Recording catalog state and result references use the shared Recording owner; native tests preserve physical settlement on invalid recording metadata. Checked MCP setup supplies startup, configuration, documents, templates and typed scope membership; native discovery and all 22 template expansions pass. Both gateway registrations declare revision 3 | Complete broader DTO relationships and cross-language construction; qualify current installed consumers, URI admission and recovery |
| UAV scopes | `UavScope` owns the four scope spellings and shared permission guards; tools and Tasks apply the same admission requirement, and flight preflight checks the installation-selected scopes through the owning vocabulary. Native routing, grant combinations, contract-only consumption and strict workspace Clippy pass | Qualify current scope enforcement on every installed UAV replica |
| Reason | Its isolated contract owns distinct catalog and analysis IDs, typed resources, cursors and an empty scope vocabulary. Checked MCP setup, SQL owner reads and Task-backed notifications are implemented. Current v1 results serve completion, Task reads and subscriptions. Grounding imports Stream contracts and carries input labels into output capabilities. Recording addresses in selections, results and analysis views use the Recording owner’s contract type | Complete broader result/reference typing; qualify current-format installed delivery, restart recovery and GPU behavior |
| Task-backed resource notifications | Domain-owned `TaskResourceAddress` implementations feed the shared `TaskResourceSubscriptions` adapter. One authorized Task subscription supplies explicit Task status and resource invalidations. Native independent-client, reconnect, revocation and official MCP cancellation cases pass. LIVE connection generations trigger a current-owner SQL baseline even after retained events expire; a TCP outage regression fails against the old watch. The adapter reuses Store changefeeds and LIVE wakes. Installed Stream checks qualify shared cross-replica completion and resource delivery | Adopt for other Task-backed domains while preserving their additional admission policy; qualify remaining domain consumers and coordinated replacement |
| Stream | Its isolated contract owns IDs, resources, cursors and response builders that check repeated identities and parent/output agreement. Checked MCP setup supplies static discovery; all 11 templates match builders, and both registrations declare revision 3 without list-change notifications. Run reads and subscription admission select caller-owned Tasks in SQL; isolated contracts qualify all 37 schemas. Recording addresses in selections, results and run views use the Recording owner’s contract type. Installed GPU replay passes cross-replica Task completion, canonical result reads, run invalidations, reconnect baselines and cancellation; public live-session subscriptions pass initial invalidations, advancing GPU results and preview, reconnect and cancellation | Strengthen remaining result relationships |
| Shared recorded video | `contract` owns selectors, timeline kinds and immutable source/snapshot builders with typed Recording IDs, positive byte counts, typed digests and checked layer/part relationships. JSON decoding applies the builders. The ordered source hash preserves its declared bytes. Materialization admits the public snapshot before extraction; Reason/Stream executors and Stream result validation check its selected Recording. Independent consumption excludes MCP, Store, Rerun and async dependencies | Finish selector construction and broader result relationships; qualify installed snapshot digests and consumers |
| View | Its isolated contract owns public scene types, scopes, Task kinds and typed resource addresses. Governed references import Map, Frames, Recording and Artifact URI types, and source features must belong to a declared Map release. Checked records validate parents, cameras, geometry and output bytes. Capture admission checks request revision and principal/tenant/Work Context before claiming. Task operations apply Work Context and operation selection in SQL; completed reads and subscription delivery validate saved requests, metadata, bytes and attribution before projection | Qualify installed consumers, cross-context Task delivery and GPU behavior, including GPU JPEG encoding |
| Recording | The shared domain crate below Hub and the MCP server owns public models, RFC UUIDv7 IDs, resource builders/parsers and catalog positions. The MCP library exposes the same types through its isolated contract feature. Hub ingest responses, Gateway policy targets, hosted reads/subscriptions/prompts and Video/Reason/Stream/UAV/View references use those types. Smoke clients reuse the owner for CLI admission, replay construction and capture results. SQL cursor conversion stays at the query call; MCP core imports neither package. Checked MCP setup owns static discovery and both registrations declare revision 3; typed `recording:seal` admission preserves gateway administrator restrictions. Public DTOs keep distinct Recording/dataset/layer/grant/projection IDs; catalog selections have a checked constructor, and Console imports shared playback DTOs. Projection reads, reservations and lifecycle transitions select current caller authority, source visibility and grant relationships in SQL before receipt decoding; writes admit and change state in one transaction. Typed requests bind idempotency to context, policy and inputs; native memory/RocksDB races and rollback pass. Grant creation admits every parent in its transaction; reuse checks current caller authority including Work Context in SQL. Redap grant classes are selected before decoding. Sealed playback manifests share typed lifecycle, time, Blueprint integrity and parent checks between the producer and Console; Rerun origins use typed URL components. Sealed query/request builders own projection bounds and metadata checks; RRD prepares upstream selectors before source materialization, and consumers share the domain model. Sealed result handles check request agreement and typed integrity facts; reuse verifies receipts and payloads. Scratch quotas include bounded metadata, preserve valid pairs across restarts and reclaim expired completed pairs on reservation. Typed Redap entry and segment addresses check parent identities; catalog responses use a checked immutable builder, and the supported address profile is qualified against Rerun. Public catalog, layer and seal models import Artifact owner references and typed integrity facts through immutable builders. Fallible Store conversion and seal ordering reject malformed metadata before lifecycle advancement or retry cleanup; SQL corruption cases pass. Projection coordinate references use the Frames owner’s revision-scoped type with bounded unique selection and request/result agreement. Playback plans, Blueprint validation, reader snapshots and cache APIs carry typed SHA-256 values; native byte, authorization and source-identity checks pass | Complete remaining Recording/RRD identity and publication adapters; qualify installed behavior |
| Shared consumers | Gateway, policy, Console BFF, Computers, conformance, smoke, and integration tests import foundational names | Keep imports direct and preserve authorization, identity serialization, and schemas |
| SDKs, clients, templates, and showcase servers | Python Task results use the shared result envelope and event version 3. `OwnerTaskQuery` composes typed operation names, UUIDv7 identities and optional Work Context predicates with current owner SQL selection. Exact reads, bounded pages, pending inputs and public notifications select before decoding; cancellation and input-response transactions recheck authority. Datasheet adopts that query for Tasks and usage. Report and usage catalogs have checked 100-item pages and distinct cursor types; exact usage and prefix completion select in SQL. Static discovery has no Task scans. URI variants use the shared checked component builder, nominal Artifact IDs and closed document IDs. Startup and discovery consume checked owner setup; Workbench follows page URIs. Both registrations declare revision 3. Identity reuse checks current security fields transactionally and preserves display names. RFC 9562 Task generation uses `uuid-utils` 1.0.0 and native UUID values. Native SDK/template checks pass 193 cases and the fork fixture passes eight; headless Workbench page navigation, Helm configuration and native Datasheet smoke pass. Installed Datasheet passes all 26 hosted checks and public Task, catalog and usage reads | Qualify the remaining installed multi-page consumers and complete wider owner-local SDK types |

`ResourceUri` admits concrete RFC 3986 references. Domain contracts apply the stricter
`ResourceUriParts` profile and their owner-specific route and ID checks; templates use
`ResourceTemplateUri`. The reference installation passes concrete admission checks
across all eighteen resource-serving servers. Gateway completion and audit targets use
the current typed format. Its policy selector matcher is separate from RFC 6570 expansion.
Foundation tests cover wire strings, schema descriptions, lexical rejection, independent
trait implementations, and compile-fail examples. A separately resolved consumer builds
without MCP, runtime, database, GPU, or provider dependencies. The shared MCP contract,
policy, and gateway library suites pass, as does strict workspace Clippy across all
targets and features. These checks qualify the extraction. All sixteen Rust server packages
expose isolated contract features. Datasheet consumes Python checked setup and passes
installed hosted and documentation-source qualification. Remaining domain builders and
field-relationship checks are tracked in the owner rows above.
Time's independent consumer passes its public-contract tests with only the Time
library, foundational types and URI libraries, serialization/schema support, and Chrono's
date/time types. The resolved graph excludes the clock feature as well as server, database,
GPU, and acquisition dependencies. Its runtime feature passes strict Clippy separately.
The default feature build, scope admission and configuration checks, public-ID
deserialization cases, and native catalog SQL tests pass. Shared-digest contract tests
pass with unchanged wire values. Scope schemas from independent modules stay distinct
even when their enums have the same name. Installed conformance is still pending.

The URI component tests cover all printable ASCII characters and Unicode, encoded
separators, malformed escapes and UTF-8, duplicate decoded query names, normalization,
and redacted validation errors. A regression exposed unescaped brackets from the URL
custom-scheme path setter; the percent-encoding step now covers its remaining non-URL
ASCII characters. Time release tests preserve supported ID spellings and string schemas,
reject wrong routes, IDs, queries, and encoded aliases, and prove the public trait round
trip. URL 2.5.8 and percent-encoding
2.3.2 were verified against their upstream stable release listings before adoption;
both versions were already present in the workspace lockfile.
The concrete profile requires unescaped authorities because URL's opaque-host
parser permits malformed percent escapes there. Current Time authorities and Artifact
UUID authorities fit that profile; encoded authority use must be inventoried before
wider adoption. Dynamic path and query components retain library encoding.

Time's resource migration uses one enum for parsing, building, and subscription
eligibility. Calendar, epoch, and event cursors preserve their v1 wire payloads and
carry distinct domain ID types. Catalog and admin APIs consume those cursor types;
event recovery follows the returned cursor directly. Native SQL cases round-trip the
emitted tokens before following pages. Time owns its persistence DTOs and carries domain
IDs through query construction until driver conversion. Expression/projection scalar types, zone fields outside
resource addresses and acquisition-state relationships still need review.

Authority-only root tests exposed an incorrect path-segment assumption in the
foundation helper. Roots now yield no segments; an explicit slash is distinct. Time
resource tests cover every family, malformed IDs and queries, cursor family mismatch,
version bounds, subscriptions, schemas, and compile-fail construction. Discovery's
zone template uses RFC 6570 reserved expansion to preserve slash-separated keys.
Time completion accepts only the advertised current zone template. The old adapter
and its support window are removed. Native discovery and typed-address checks pass;
qualify installed discovery on the rebuilt reference installation.

## Phase 4: Unified Audit Log

The initial survey behind [the audit design](AUDIT.md) identified these paths and
volumes. The baseline columns describe behavior before the audit implementation;
the target columns define the required results.

| Path | Baseline | Target |
|---|---|---|
| Gateway authentication (`platform/gateway/src/bin/gateway/auth.rs:98-380`) | One row per HTTP request, including every poll | Part of the request record; token lifecycle and credential denials only |
| Gateway policy (`platform/gateway/src/mcp/authorization.rs:329-367`) | One row per call, and one per item for discovery, repeated for every page and on each 5 s cache expiry; prompts are not cached (`prompts.rs:35`) | One record per request; one per list |
| Gateway tool call (`platform/gateway/src/mcp/tools.rs:410-432`) | Written after the effect, and a failed write errors the response | Completion record, retried, never fails the response |
| Admin outcome (`record_admin_operation_audit` in `platform/gateway/src/bin/gateway/audit.rs:236`) | Extra policy row with free-form metadata | Completion record |
| Artifact service (`platform/artifacts/service/src/service.rs:146-199`) | One row per authorization, including each range request; no trace ID; never deleted | Download windows; trace from the request context; retention |
| Upload completion (`platform/store/src/artifact_uploads/publication.rs:28-84`) | Same transaction as publication | Transactional writer |
| Live views (`servers/uav-sim-mcp/src/server/live_view_audit.rs`, `platform/store/src/live_views.rs`) | Best effort; failures only logged; never deleted | Issuance requires its record; close and expiry retried; retention |
| Refresh rotation (`platform/store/src/gateway_runtime.rs:504-620`) | Transactional | Transactional writer, `authentication` class |
| Speech (`platform/gateway/src/bin/gateway/speech/authority.rs:44-87`) | Authentication and policy rows per 1 s chunk, about 124 per minute | Session open, summary, and denials |
| Recording ingest denials (`platform/gateway/src/bin/gateway/recording_ingest.rs:449`) | Stored as authentication rows with a hard-coded `BearerJwt` method | Typed denial records |

| Action | Baseline rows | Required records |
|---|---|---|
| List tools with a cold cache | 1 + one per visible tool, above 100 | 1 |
| One tool call | 3 | 2 |
| One minute of dictation | about 124 | 2, plus denials |
| Video playback with 100 to 300 range requests | 300 to 900 | 1 per five-minute window |
| Agent episode with 10 turns and 8 tool calls | about 42, plus discovery on each connection rotation | one per request, plus one completion per tool call |

Other findings this phase fixes: `source_ip` is never written; trace IDs come in three
formats, so a request's authentication and policy rows never correlate; `request_id`
holds the event ID; rows without a tenant are invisible in the Console; the Console
stream scans the global changefeed and filters tenants in memory; CLI summaries read
every row without a tenant filter; `configs/deployments.json` `audit_event_days` is not
wired to the gateway's retention flag; and no code deletes `outbox_event` rows.

Work:

1. Upgrade SurrealDB from 3.2.4 to the latest 3.3 patch release in its own commit,
   before any audit change. Update the Helm image pin, every `surrealdb` crate pin, and
   the tests and documents that name the version. Check the store code against the 3.3
   behavior changes: locked reads through `SELECT … FOR UPDATE`, and `UPDATE` and
   `UPSERT` now evaluating `WHERE` before their data clauses. Qualify with the store and
   gateway test suites and all migrations on a fresh store. Run the installed smoke
   scenarios at the composed audit integration checkpoint; do not hold the broad
   audit implementation pass for a separate full-cluster deployment.
   Time currently uses a tenant fence. Generic exact-ID locked reads pass on 3.3.0;
   the domain pointer/release transaction still needs its replacement qualification.
   Qualify `FOR UPDATE` on both exact pointer IDs, including absent records, and
   their releases before replacing that fence; preserve the preflight snapshot checks.
   GitHub's latest-release API confirmed [SurrealDB 3.3.0](https://github.com/surrealdb/surrealdb/releases/tag/v3.3.0)
   on 2026-09-28, published at 11:40:24 UTC. Resolve the image digest and SDK provenance
   and qualify the release before committing the upgrade.
2. Add request timing before changing audit. Each request reports policy evaluation,
   audit commit, and upstream time in its trace, and the gateway exports them as
   histograms. Record a baseline for catalog lists, resource reads, and tool calls on the
   reference installation.
3. Define the record types in `platform/audit/contract`, and delete `AuditEvent`,
   `AuthAuditEvent`, and their metadata maps.
4. Add a migration that removes `audit_event` and creates `audit_record` with compound
   record IDs `[partition, uuidv7]`, record links for platform targets, `READONLY`
   fields, and a change feed without `INCLUDE ORIGINAL`; `audit_block`; and the
   `audit_daily` table view. Add only the secondary indexes the bounded queries use
   beyond the ID range. Existing audit rows are discarded, as CE-12 records.
5. Create `platform/audit` with the writer's transactional and group-commit modes, the
   sealer that follows the `audit_record` change feed under a store lease, block-based
   retention, the OCSF and OpenTelemetry exporters, and verification. The gateway hosts
   the sealer, exporter, and retention worker.
   Add the dedicated audit signing key to the installation secrets.
6. In the gateway, assign request IDs, establish W3C trace context, carry both in the
   signed request context to upstream servers, record source IP addresses, write one
   record per request, aggregate discovery lists, write tool and admin completion
   records, write token lifecycle records, type recording ingest denials, and add
   `gateway audit verify`. Write no per-request record for allowed artifact range
   requests and dictation chunks, because their window and session records own them. Make audit retention a required
   installation value, wired from Helm, and remove the 365-day default.
7. Cache discovery decisions by caller authority, policy revision, and catalog
   generation, and invalidate them on change events instead of the 5 s expiry. Cache
   prompt decisions the same way.
8. Move the Artifact service, Computers lifecycle changes, Work Context transitions, and
   live views onto the writer. Live-view issuance fails when its record cannot commit;
   close and expiry records are retried. For upload publication, evaluate a synchronous
   `DEFINE EVENT` that writes the record in the publication transaction. Keep it only if
   it is simpler than the writer call and passes the record-type tests, and record the
   result in the audit design.
9. Replace per-chunk speech records with session records.
10. Stop writing outbox events for audit records. Phase 5 removes the outbox itself.
11. Rebuild the Console's audit views on the paged, partition-scoped range query, the
    `audit_daily` view, and a LIVE query per partition with change-feed recovery after a
    reconnect. Remove the global change-feed scan in
    `platform/gateway/src/bin/gateway/admin/console/stream.rs`. Add server-side export,
    show the `installation` partition to installation administrators and auditors, and
    record audit-view access. Bound and scope the CLI summaries, and update smoke
    assertions that counted old rows.
12. Update the documents that describe the old behavior: the tool-call and discovery
    paragraphs in `mcp/contract/DESIGN.md`, the audit section of
    `servers/uav-sim-mcp/DESIGN.md`, `platform/gateway/src/bin/gateway/speech/DESIGN.md`,
    the audit retention section of `docs/TECH_DESIGN.md`, and gap G4 with its stale
    retention reference in `docs/REGULATED_READINESS.md`. Replace the 500 ms catalog
    target in `docs/DEVELOPMENT_ITERATION.md` with a pointer to the audit design's
    measurement section.
13. Update the standards registers: SurrealDB 3.3, OCSF export, and W3C Trace Context in
    `README.md`; OCSF 1.9.0, RFC 9162, RFC 8785, and S3 Object Lock in
    `docs/TECH_DESIGN.md`; and an audit export interface in
    `interfaces-and-protocols.csv`.

Acceptance:

- The SurrealDB 3.3 upgrade passes native client/server qualification before any audit
  commit lands. Installed upgrade and audit acceptance run together at the composed
  integration checkpoint.
- Tests count records for each action in the table above and match the target column,
  and a live-view renewal and an indexing window each produce the record the audit
  design specifies.
- A request fails when its record cannot commit, and a live-view authorization is not
  issued while the audit store is unavailable. An issued authorization keeps working.
- `gateway audit verify` detects an updated and a deleted sealed record, a back-dated
  inserted record, a removed block, and a bad signature, each made with database root
  credentials.
- A record committed after a later-stamped record is still sealed, because the sealer
  follows commit order.
- The gateway refuses to start without a retention value. Retention deletes every
  class, and with export configured it deletes only exported blocks.
- The exporter writes OCSF JSON Lines to the bundled S3-compatible store. Object Lock
  acceptance runs against a store that supports compliance mode, because the bundled
  RustFS does not (`docs/REGULATED_READINESS.md` gap G9).
- A repository test rejects any string field in a `detail` variant outside the
  identifier and reason-code allowlist.
- A warm catalog list evaluates no policy and writes one record.
- The Console's live audit view receives new records through its LIVE query without
  polling, and recovers missed records from the change feed after a reconnect.
- The before and after timing measurements are recorded in the audit design. Audit
  commit is no longer the dominant cost of catalog and read latency. This phase sets no
  fixed millisecond target.

## Phase 5: Store Simplification

Native changefeeds and LIVE wakeups replace the outbox. Parent-owned references
handle qualified cleanup, and immutable Computer request payloads live outside
changing journals. The release from `6ec504d8` passed installed certification,
Artifact delivery, Speech CUDA, Recording replay, Reason grounding and Stream
inference. Remaining domain, recovery and visual gates stay in the current status
and the Phase 3 inventory.

The [before/after measurements](../platform/store/measurements/native-after-2026-10-01.md)
and [separated-journal measurements](../platform/store/measurements/journal-separated-2026-10-01.md)
record the qualified write costs. Earlier checkpoints are in the
[progress log](PLATFORM_FOUNDATIONS_PROGRESS.md#phase-5-store-simplification).

Work:

1. Measure first. Record transaction conflicts, retries, and commit latency for outbox
   writers, and confirm how much of that cost the shared sequence causes.
2. Inventory every outbox writer and consumer with `git grep -n "outbox"`. Writers
   include `agents/runtime/src/runtime.rs`, `platform/store/src/artifact_access_requests.rs`,
   `map_authoring.rs`, `map_presentations.rs`,
   `servers/frames-mcp/src/state/worlds/`, and
   `servers/frames-mcp/src/state/operations/record.surql`.
   Consumers include the agent manager, the agent runtime, gateway agent events,
   `platform/task-runtime/src/runtime/subscriptions.rs`,
   `servers/artifact-mcp/src/bin/server/subscriptions.rs`,
   `servers/computers-mcp/src/protocol/subscriptions.rs`,
   `servers/media-mcp/src/bin/server/app_state.rs`, and the Map changeset projection.
3. Move each consumer to change feeds of the tables whose changes it needs. A consumer
   holds a LIVE query for push delivery and a persisted change-feed versionstamp cursor
   for recovery. Typed decoders in `platform/store` turn table changes into the events
   consumers act on, so each table's event vocabulary has one owner. A consumer whose
   cursor falls behind the change-feed retention reconciles from current table state.
   Migrate all consumers in one implementation pass with their typed decoders and
   recovery paths. Run their reactive tests as a grouped batch, collect independent
   failures, and commit coherent concerns after qualification.
4. Deliver delayed agent wakes from `wake.available_at` with a timer armed for the next
   due wake and re-armed by LIVE changes on `wake`. Do not poll.
5. Delete `outbox_event`, `outbox_checkpoint`, `platform_outbox_sequence`,
   `OutboxDraft`, and `platform/store/src/outbox.rs` once no consumer remains.
6. Review each changefeed that sets `INCLUDE ORIGINAL`.
   Keep it only where a consumer needs the prior state of a change, and record
   the reason in the owning design.
7. Survey code that deletes or repairs related records by hand, such as grants, shares,
   and relation edges. Replace each case with `REFERENCE` fields and `ON DELETE` rules
   where the database can enforce the same behavior, with a test per relationship.
8. Evaluate `DEFINE EVENT … ASYNC` with `RETRY` for projections that stay inside the
   database, starting with the Map changeset projection. Work that calls another
   service stays in that service.
9. Separate immutable encrypted Computer command/file request payloads from changing
   journal rows. Keep admission atomic, resolve typed private reads without exposing
   ciphertext to metadata consumers, preserve exact retries and restart recovery,
   and use a parent-owned reference for cleanup. Prove metadata changes do not copy
   request ciphertext into their feeds, then repeat the affected storage measurements.
10. Update `platform/store`, `platform/task-runtime`, `agents/runtime`, the Durable
   Platform Store section of `docs/TECH_DESIGN.md`, every other affected design, and
   the CODEMAP rows for `outbox.rs` and `changefeed.rs`.

Acceptance:

- `git grep outbox_event -- '*.rs' '*.surql' '*.py'` returns nothing, and every former consumer passes its
  reactive tests.
- No consumer polls. Review confirms each wait is a LIVE query, a change-feed read after
  a reconnect, or a timer for a known due time.
- Write amplification, conflicts, and commit latency are measured before and after and
  recorded in the platform store design.
- Each `REFERENCE` adoption has a test for its `ON DELETE` behavior, and each rejected
  candidate has its reason recorded.

## Phase 6: Extension Crate And Shared Plumbing

The extension, Rust/Python/Node adapters, audited reads, kernel provenance and
shared conformance checks are implemented. All eighteen participating sources pass
their declared installed checks. Docs-only sources skip change/search checks when
those capabilities are absent. The [progress log](PLATFORM_FOUNDATIONS_PROGRESS.md#phase-6-extension-crate-and-shared-plumbing)
keeps the implementation and qualification checkpoints.

1. Create `mcp/knowledge-extension` as a workspace crate with the models, server and
   client helpers, and docs collection listed in its design's implementation map.
2. Build the `{slug}.docs` collection into `veoveo_mcp_contract::docs`, so every Rust
   server that uses `server_docs!` declares it, and into `veoveo_mcp.contract.docs` in
   `sdk/python`, so `datasheet-mcp` in `templates/python-mcp` and fork Python servers
   declare it. Document revisions are SHA-256 digests computed at build time.
3. Add C32 to `CHECKLIST_IDS` and declare its applicability in each server's `AGENTS.md`.
   Knowledge publication is optional. A server that declares the extension must
   qualify its docs and domain collections; other servers mark C32 not applicable.
4. Add checks K01 through K08 to the conformance client and run them in certification
   for every server that declares the extension.
5. Add `platform/store/src/knowledge.rs` and the next ordered migration for catalog,
   chunk, and index-generation records.
6. In the gateway read path, declare the extension on upstream reads to declaring
   servers, attach the observation and read outcome to the read's audit record as a
   `detail` variant of the Phase 4 record type, commit that record before returning, and forward the observation only to declaring
   callers.
7. In `agents/kernel/src/resource.rs`, declare the extension on resource reads, keep
   the observation beside each admitted item, and render one provenance line per item
   inside the existing budgets.
8. Update the standards registers. Add `ai.veoveo/knowledge-source` to the agent and
   app interfaces row in `README.md`. Add a `docs/TECH_DESIGN.md` row for the extension
   with its RFC 9110 validator, RFC 9111 freshness, and RFC 8246 immutability
   semantics. Add the extension to the MCP row in `docs/ARCHITECTURE_DECISIONS.md`.
   Add an interface row for audited knowledge reads to
   `interfaces-and-protocols.csv`.

Acceptance:

- Gateway tests prove that a declared read's audit record carries its observation and
  outcome and commits before the result returns.
- A kernel test shows provenance lines within the byte budget.
- Existing Rust, Python and Node docs adapters pass K01 through K08 for the docs
  collections they declare. New servers need not adopt the extension.

## Phase 7: First Adoption Wave

Map and Artifact are the first new domain sources. Map makes places and authored
geography discoverable across tasks. Artifact lets users find stored outputs and follow
their provenance to the bytes. Keep the implemented Time collections and Chart docs.
Each adopted source fills observations from existing records and makes declared search
results resource links. When a domain has no revision, use the content digest as the
revision.

| Server | Collections | Existing provenance | Gaps to close |
|---|---|---|---|
| chart | `charts.docs` only | build-time document digests | Six installed documentation checks and revision-bearing audit reads pass at `67d75ae8`; immutable docs have no change or domain-search probe |
| time | events, calendar versions, epoch versions, acquired and packaged authority releases | stored creation tenant, owner and Work Context; content/access revisions, source digests and modification timestamps | Installed source conformance passes twenty-seven checks across docs and five domain collections, including event cancellation before and after a real service restart; source audit review passes |
| artifact | artifact metadata (`artifact://metadata/{id}`), never the bytes | compliance metadata: tenant, owner, Work Context, labels, provenance | Installed source conformance passes eleven checks, including grant changes across a real service restart; deadline qualification, K09/K10 review and source audit review pass |
| map | bounded summaries of feature layers, features, publications, locations, facilities and dataset releases | full source content digests, layer and feature revisions, Work Context, labels, stored timestamps and recorded modifying actors | All 35 installed source checks pass, including publication creation, four service restarts and search denial; source audit review and owned fixture cleanup pass. Archival preserves immutable publications |

Acceptance: each server passes K01 through K10 review and conformance, and the audit
log records the observed revision for reads of each collection.

The [source-owner review](PLATFORM_FOUNDATIONS_PROGRESS.md#phase-7-source-owner-review)
records K09/K10 provenance, revisions and change delivery, including Artifact deadlines.

## Phase 8: Knowledge Service

The installed nineteen-collection catalog, source-linked search, completion,
statistics, subscriptions, CUDA embedding and network isolation pass their recorded
acceptance. Knowledge restart preserves the active generation and current source
revisions. The controlled 0.6B/4B/8B comparison retains 0.6B; the
[measurement record](../platform/runtimes/embedding/verification/retrieval-2026-10-02.md)
states its corpus and measurement limits. [Earlier checkpoints](PLATFORM_FOUNDATIONS_PROGRESS.md#phase-8-knowledge-service)
record publication and qualification. Shared type and domain acceptance work remains
in the Phase 3 inventory.

1. Deliver the shared [embedding runtime](../platform/runtimes/embedding/DESIGN.md)
   before the knowledge service consumes it. Re-verify the latest stable vLLM release
   first, and keep one vLLM pin shared with `reason-mcp`. Do not add candle, fastembed,
   `ort`, or a Hugging Face client to any Veoveo service.
   1. Add the runtime to `deploy/helm/veoveo` following the `reason` values pattern:
      the official `vllm/vllm-openai` image run with `--runner pooling` and
      `--scheduling-policy priority`, a model-cache volume with the
      installation-supplied checkpoint at revision
      `97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3`, an init container that checks
      `platform/runtimes/embedding/checkpoint.sha256`, `HF_HUB_OFFLINE=1`, the `nvidia`
      runtime class, a `nvidia.com/gpu` request, an installation-set
      `--gpu-memory-utilization`, readiness on `/health`, the installation embedding
      API key Secret mounted into the platform workloads that use it, and a
      NetworkPolicy that admits platform-namespace pods except the `computer-host`
      component. Stage the checkpoint on the reference installation the way Reason's
      world-model checkpoint is staged, and add the runtime's GPU share and Deployment
      to `examples/bioma/installation-target.json`. Document it in
      `deploy/helm/veoveo/DESIGN.md`.
   2. Confirm that vLLM applies last-token pooling with L2 normalization for this
      checkpoint, and set the pooler configuration explicitly if it does not. Confirm
      that the embeddings route honors request priority on the pinned version, and
      record the result in the runtime design.
   3. Create `platform/runtimes/embedding/client` as the `veoveo-embedding-client`
      workspace crate: `embed_documents`, `embed_query`, priorities, request bounds,
      response validation, and `EmbeddingSpace` built from model discovery and deployment
      identity.
   4. Generate the reference vectors with the model card's `transformers` recipe
      through `uv run`, commit them as a fixture, and make the reference test pass
      against the runtime on a hardware GPU.
   5. Run the load test that shows interactive requests completing ahead of bulk work,
      and the network test that shows pods outside the platform namespace cannot
      connect.
2. Create `servers/knowledge-mcp` with `DESIGN.md` and `AGENTS.md`. Move the service
   sections of [`KNOWLEDGE.md`](KNOWLEDGE.md) into its design, keeping the
   cross-component flow in `KNOWLEDGE.md`.
3. Add typed knowledge approval entries to the control-plane contract in
   `mcp/contract/src/gateway/server_config.rs`, with validation. Register the service's
   machine client, grant it read access to approved collections only, and mark it as an
   indexing client so the gateway records its reads by collection window, as
   [the audit design](AUDIT.md#event-selection) specifies.
4. Implement discovery, the catalog resources, enumeration, change subscriptions,
   reconciliation, chunking, and index generations keyed by embedding space, with
   indexing at bulk priority. Add the `embed` tool for agents and external MCP hosts.
5. Implement `search` with BM25, HNSW, reciprocal rank fusion, and query embedding at
   interactive priority. Narrow candidates inside the SurrealDB query by tenant, the
   caller's Work Contexts and grant subjects, and clearance labels, over-fetch, and
   decide each candidate with `veoveo_mcp_contract::access::decide` from
   `mcp/contract/src/access.rs`, the predicate the Artifact service uses. Do not copy
   it.
6. Add the `knowledge-mcp` Helm chart, gateway registration, and offline image entries
   for it and the embedding runtime. `knowledge-mcp` requests no GPU.
7. Build an evaluation set from the Phase 7 collections, and record recall at 10 for
   the chosen chunk settings in the index generation. Measure indexing throughput with
   concurrent searches, then qualify `Qwen3-Embedding-4B` and `8B` against 0.6B as the
   runtime's [Model Selection](../platform/runtimes/embedding/DESIGN.md#model-selection)
   describes, and record the choice in the runtime design.
8. Update the standards registers. Add a knowledge area to `README.md` naming W3C DCAT
   3 and `Qwen/Qwen3-Embedding-0.6B` served by vLLM. Add `docs/TECH_DESIGN.md`
   rows for DCAT 3, SurrealDB `FULLTEXT` and `HNSW` indexes, and the vLLM embedding
   runtime with its internal OpenAI Embeddings API profile. Add the embedding runtime
   to `software-components.csv` with its internal interface. Add `knowledge-mcp` to `software-components.csv` and its search,
   catalog, and source-read interfaces to `interfaces-and-protocols.csv`.

Acceptance:

- K01 through K10 pass for `knowledge.docs`.
- Access tests prove that no result, title, or snippet escapes the caller's effective
  access.
- The embedding runtime passes its verification on a hardware GPU: readiness, digest
  and CUDA refusal, reference vectors at cosine similarity of at least 0.999, priority
  under load, and network isolation.
- Indexing throughput with concurrent searches and the 0.6B, 4B, and 8B comparison are
  recorded.
- The `embed` tool is authorized and audited through the gateway, and an agent's
  episode budget counts it.
- Invalidation and reconciliation tests pass.
- The reference installation indexes the approved Phase 7 collections, and a search
  returns links an agent can read.

## Phase 9: Reusable Reason Analyses

Installed Reason findings pass publication, result grants, revocation and separate
Reason/Knowledge restart checks. [Earlier checkpoints](PLATFORM_FOUNDATIONS_PROGRESS.md#phase-9-reusable-reason-analyses)
record those runs. Broader result/reference typing remains in the Phase 3 inventory.
Reason stays off during other service batches and starts for its own acceptance.

Adopt Reason's completed analyses and results so users can find earlier findings and
read their grounding without rerunning inference. Observations carry stored owner,
Work Context, labels, model provenance and modification metadata. Return bounded
analysis text and link result Artifacts for larger content. Keep source authorization
in SQL before pagination and decoding.

The existing analysis and result resources use Task-owner admission. Reusable findings
must derive visibility from the published result Artifact's current grants, selected
Work Context, labels and retention. Keep Task control owner-scoped. Select successful
analyses and their readable result Artifacts together in SQL before decoding or LIMIT;
an indexing-client registration cannot bypass source authorization. Reuse Artifact's
admission contract, and carry its access changes into Reason observations and collection
invalidations. Summary resources must link larger result content to the owning Artifact.

Acceptance: Reason passes K01 through K10 for these collections. The reference
installation indexes approved analyses and results, and the evaluation set includes
queries that retrieve prior findings with their source and revision. Denied analyses
must not expose a title, snippet or result link.

### Postponed Adoption Outside This Plan

These candidates require a demonstrated user need before implementation. They are
excluded from this plan's completion criteria and its Deferred Work register.

| Server | Postponed domain collections |
|---|---|
| optimization | problems, runs, solutions |
| frames | worlds and immutable revisions |
| recording | recordings and layers |
| reason | pipeline and model catalogs |
| stream | runs and results |
| speech | transcripts |
| view | compositions |
| uav-sim | control grants and mission plans |
| duckdb | database schemas |

Media, Timeseries, Computers and Datasheet receive no new domain collections in this
plan. Their existing documentation support can stay. Chart stays documentation-only.
`showcase/sumo` has no well-known surface and is outside the reference installation.

## Accepted Risks

The user accepted these risks on 2026-09-26. They do not block completion, and they
are recorded in the owning designs instead of Deferred Work:

| Risk | Handling |
|---|---|
| The bundled RustFS store has no Object Lock, so compliance-mode export cannot be qualified on the reference installation (`docs/REGULATED_READINESS.md` gap G9) | Export to the bundled store is still required. When no compliance-mode store is available, record the unqualified Object Lock path in the audit design's status and in gap G9 |
| vLLM may not honor request priority on the embeddings route | The client caps bulk requests in flight, as the embedding runtime design specifies, and the runtime design records which mechanism shipped |
| The plan spans ten phases | The Status line and Deferred Work record progress; phases land and deploy one at a time |

## Deferred Work

The implementing agent adds a row for every step it defers, and removes the row when
the step lands. Each row matches a `TODO(foundations)` comment in the code. The plan is
not complete while a row remains.

| Phase and step | Code path | What remains | Why it was deferred |
|---|---|---|---|
| Phase 1 reference reset | `testing/flight-smoke/src/domain.rs` | Diagnose Rerun timeline initialization and finish composed playback/timing and landing visual acceptance | At `bf0fe31d`, the selected 24-service batch passes Stream App capture and Recording replay. Rerun receives frames but has no healthy timeline at the failing checkpoint; the capture acknowledgement correctly fails and cleanup lands the aircraft. Continue independent contract work before repeating this gate. Preserve freshness, spatial-content, flight-health and hardware requirements; Reason stays in its separate acceptance batch |
