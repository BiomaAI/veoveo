# Knowledge Sources And Identifier Cut Plan

Status: approved for implementation on 2026-09-26. No phase has started.

This plan tells an implementing agent how to deliver four changes. The first moves
every repository-owned identifier onto the `veoveo.ai` domain in one hard cut. The
second makes installed smoke checks run against any installation, not only the Bioma
reference installation. The third fixes resource contract violations found while
surveying the servers. The fourth makes every server a knowledge source and adds the
`knowledge-mcp` catalog, index, and ledger.

The target contracts live in the owning documents:

- [Knowledge source extension](../mcp/knowledge-extension/DESIGN.md)
- [Knowledge sharing](KNOWLEDGE.md)
- [MCP server contract](../mcp/contract/DESIGN.md), rule C32
- [Contract evolution](CONTRACT_EVOLUTION.md), CE-10 and CE-11
- [Deployment contract](../deploy/contract/DESIGN.md#installation-target)
- [Naming rules](../AGENTS.md#naming)

Delete this plan, and its CODEMAP row, in the change that completes the last phase.
Update each owning design when its phase lands, because the designs hold the current
state after this file is gone.

## Standards And Protocols

| Standard or protocol | Role in this plan |
|---|---|
| MCP `2026-07-28` extensions and `_meta` key rules | Identifier forms and the `ai.veoveo/knowledge-source` extension |
| RFC 9110, RFC 9111, RFC 8246 | Revision, freshness, and immutability semantics for knowledge reads |
| W3C DCAT 3 and PROV-O | Catalog and ledger models in `knowledge-mcp` |
| SurrealDB 3.2.4 | Ledger, catalog, `FULLTEXT` BM25, and `HNSW` indexes |
| fastembed `7.1.0`, candle `0.11.0`, `Qwen/Qwen3-Embedding-0.6B` | Embedding on a hardware GPU |
| `veoveo.ai/installation-target/v1` | Installation input for installed smoke scenarios |

## Working Rules

- Read `AGENTS.md` and `docs/CODEMAP.md` before each phase.
- Work on `main` in small commits, one concern each. Run the native checks each commit
  touches, and run `cargo xtask enforce docs` for every documentation change.
- Internal names and formats change by hard cut. Do not add aliases, fallbacks, or
  readers for old identifiers.
- Ask the user before any command that deletes data on a running installation.
- Re-verify the latest stable release of every new dependency at the moment you add it.
  Update the pin and this plan if it has moved.
- Delete superseded plans as you go, following Phase 0.
- GPU workloads request their device and fail closed. The embedding path has no CPU
  mode.
- Keep the standards registers current. Add a standard to them in the change that
  implements it, never earlier, because each register lists what Veoveo implements.

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
| `RECORDING_CATALOG_HARD_CUT_PLAN.md` | Delete | `docs/RECORDINGS.md`, `servers/recording-mcp/DESIGN.md` |
| `RMCP_3_MIGRATION.md` | Delete | `mcp/contract/DESIGN.md` |
| `REACTIVE_UX_PLAN.md` | Delete | `apps/console/web` and `apps/workspace/DESIGN.md` |
| `SPEECH_PLAN.md` | Delete | `servers/speech-mcp/DESIGN.md` |
| `WORKSPACE_PLAN.md` | Delete | `apps/workspace/DESIGN.md` |
| `ARTIFACT_UPLOAD_PLAN.md` | Delete | `platform/artifacts/service/DESIGN.md`; replace the `docs/README.md` guide link with that design and `apps/console/web/src/uploads/DESIGN.md` |
| `AGENT_MANAGEMENT_PLAN.md` | Delete | `agents/manager/DESIGN.md` |
| `FORK_DEVELOPMENT_PLAN.md` | Delete | `docs/FORK_DEVELOPMENT.md`; drop the pointer at the top of `CONTRACT_EVOLUTION.md` |
| `COMPUTERS_PLAN.md` | Delete | `platform/computers/DESIGN.md`; rewrite the `CONTRACT_EVOLUTION.md` sentences that cite it |
| `PLATFORM_IMPROVEMENTS_PLAN.md` | Delete when every cycle record is delivered; otherwise keep the open cycles only | owning designs named in each cycle |
| `REPOSITORY_HARDENING_PLAN.md` | Keep while any item is open; delete delivered sections | `tools/xtask`, `testing/` designs |
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
| Public payload schemas | about 12 | `live-view/v4` (`mcp/contract/src/live_view.rs:12`), `hosted-mcp/v3` (`mcp/contract/src/lib.rs:9`), conformance profile and report (`mcp/conformance/src/profile.rs:10`, `report.rs:7`), recording catalog, projection, and playback tags, `map-route-handoff/v1` (map and `servers/uav-sim-mcp/src/contract.rs:388`), and the optimization problem tags (`servers/optimization-mcp/src/domain/mod.rs:18-21`) |
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
   `io.veoveo`, `veoveo.io`, and `ai.bioma`. Add the target to the xtask surface listed
   in `AGENTS.md`.
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
reinstalls from the new lock through GitOps. Show the user the exact commands and wait
for approval before running them.

Acceptance:

- `cargo xtask enforce identifiers` passes, and the grep above returns nothing.
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
  tenant differ from the reference, without editing Veoveo source.

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
| map, media, optimization | per-process broadcast hubs do not survive restarts or reach other replicas | Feed their hubs from the Store outbox or Store LIVE, as Time and Recording do |
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

## Phase 4: Extension Crate And Shared Plumbing

1. Create `mcp/knowledge-extension` as a workspace crate with the models, server and
   client helpers, and docs collection listed in its design's implementation map.
2. Build the `{slug}.docs` collection into `veoveo_mcp_contract::docs`, so every Rust
   server that uses `server_docs!` declares it. Document revisions are SHA-256 digests
   computed at build time.
3. Add C32 to `CHECKLIST_IDS` and declare it in every server's `AGENTS.md`.
4. Add checks K01 through K08 to the conformance client and run them in certification
   for every server that declares the extension.
5. Add `platform/store/src/knowledge.rs` and the next ordered migration for
   `knowledge_observation`, `knowledge_read`, `knowledge_derivation`,
   `knowledge_invalidation`, catalog, chunk, and index-generation records.
6. In the gateway read path, declare the extension on upstream reads to declaring
   servers, write observation and read records before returning, and forward the
   observation only to declaring callers.
7. In `agents/kernel/src/resource.rs`, keep the observation beside each admitted item
   and render one provenance line per item inside the existing budgets.
8. Update the standards registers. Add `ai.veoveo/knowledge-source` to the agent and
   app interfaces row in `README.md`. Add a `docs/TECH_DESIGN.md` row for the extension
   with its RFC 9110 validator, RFC 9111 freshness, and RFC 8246 immutability
   semantics. Add the extension to the MCP row in `docs/ARCHITECTURE_DECISIONS.md`.
   Add an interface row for gateway-ledgered knowledge reads to
   `interfaces-and-protocols.csv`.

Acceptance:

- Store tests cover observation deduplication, supersession, and every read outcome.
- A gateway test proves that records commit before the result returns.
- A kernel test shows provenance lines within the byte budget.
- Every server passes K01 through K08 for its docs collection.

## Phase 5: First Adoption Wave

Each server below declares its collections, fills observations from existing records,
and makes its search results resource links. When a domain has no revision, use the
content digest as the revision.

| Server | Collections | Existing provenance | Gaps to close |
|---|---|---|---|
| chart | `charts.docs` only | none | Implement the docs declaration in `server.mjs` |
| time | events, calendar versions, epochs, authority releases | `record_version`, calendar `version`, `source_digest`, admin timestamps; owner only in the store | Surface owner and Work Context in observations; paging from Phase 3 |
| optimization | problems and solutions (immutable), runs | `digest_sha256`, `authority`, timestamps, engine digest | Surface labels in observations; restart-safe hub from Phase 3 |
| artifact | artifact metadata (`artifact://metadata/{id}`), never the bytes | compliance metadata: tenant, owner, Work Context, labels, provenance | Cursor paging for `artifact://index`; `modifiedBy` from the occurrence record |
| map | feature layers, features, publications, locations, facilities, dataset releases | layer and feature revisions, `created_by`, Work Context, labels, changeset sequence, source digests | Return resource links from `search_locations`; declare the other collections from its templates |

Acceptance: each server passes K01 through K10 review and conformance, and the
gateway ledger records reads of each collection.

## Phase 6: Knowledge Service

1. Create `servers/knowledge-mcp` with `DESIGN.md` and `AGENTS.md`. Move the service
   sections of [`KNOWLEDGE.md`](KNOWLEDGE.md) into its design, keeping the
   cross-component flow in `KNOWLEDGE.md`.
2. Add typed knowledge approval entries to the control-plane contract in
   `mcp/contract/src/gateway/server_config.rs`, with validation. Register the service's
   machine client, and grant it read access to approved collections only.
3. Implement discovery, the catalog resources, enumeration, change subscriptions,
   reconciliation, chunking, and index generations.
4. Add fastembed with default features disabled and the `qwen3` and `cuda` features.
   Package the `Qwen/Qwen3-Embedding-0.6B` files pinned by revision and SHA-256. The
   builder stage needs the CUDA toolkit for candle's kernels. Record the `ort`
   pre-release pin beside the dependency.
5. Implement `search` with BM25, HNSW, reciprocal rank fusion, and effective-access
   filtering. Share the effective-access predicate that `platform/store/src/artifacts.rs`
   applies. Do not copy it.
6. Add the Helm chart, GPU request, gateway registration, and offline image entry.
7. Build an evaluation set from the Phase 5 collections, and record recall at 10 for
   the chosen chunk settings in the index generation.
8. Update the standards registers. Add a knowledge area to `README.md` naming W3C DCAT
   3, W3C PROV-O, and `Qwen/Qwen3-Embedding-0.6B` on CUDA. Add `docs/TECH_DESIGN.md`
   rows for DCAT 3, PROV-O, SurrealDB `FULLTEXT` and `HNSW` indexes, and the fastembed
   and candle profile. Add `knowledge-mcp` to `software-components.csv` and its search,
   catalog, and source-read interfaces to `interfaces-and-protocols.csv`.

Acceptance:

- K01 through K10 pass for `knowledge.docs`.
- Access tests prove that no result, title, or snippet escapes the caller's effective
  access.
- A test on a hardware GPU proves that startup fails without CUDA and that the loaded
  model matches its digests.
- Invalidation and reconciliation tests pass.
- The reference installation indexes the approved Phase 5 collections, and a search
  returns links an agent can read.

## Phase 7: Second Adoption Wave

| Server | Collections | Prerequisite |
|---|---|---|
| frames | worlds, immutable revisions | Stamp Work Context on worlds from invocation authority; today owner and labels live only in the store |
| recording | recordings, layers | Recording URIs in catalog entries; owner in observations |
| reason | analyses and results; pipelines and models | Owner and labels in observations; link result artifacts instead of inlining them |
| stream | runs and results | Owner in observations |
| speech | transcripts | Transcript index and completion |
| view | compositions | Frames and tiles are not knowledge |
| uav-sim | control grants, mission plans | Live simulation state is not knowledge |
| duckdb | database schemas with `indexing: metadata` | Table contents stay behind the `query` tool |

Media, timeseries, and sumo declare only their docs collection until they hold records
worth sharing.

Acceptance: each server passes K01 through K10, and `knowledge-mcp` indexes its
approved collections.
