# Veoveo Documentation

Start with the guide for the work you need to do. Veoveo is the platform;
[Bioma](../examples/bioma/README.md) is one installation configuration and acceptance
environment. Workspace is the daily productivity client. Console administers the
installation, its services and agents. Computers is a core platform capability.

## Find A Guide

| Task | Start here |
|---|---|
| Understand the product | [Repository overview](../README.md), [architecture decisions](ARCHITECTURE_DECISIONS.md), [technical design](TECH_DESIGN.md) |
| Work on shared chat and agent feedback | [Workspace client](../apps/workspace/DESIGN.md), [Workspace delivery](WORKSPACE_PLAN.md), [reactive UX delivery](REACTIVE_UX_PLAN.md) |
| Work on Computers | [Domain design](../platform/computers/DESIGN.md), [delivery and qualification](COMPUTERS_PLAN.md) |
| Install and operate Veoveo | [Enterprise deployment](ENTERPRISE_DEPLOYMENT.md), [Helm installation](../deploy/helm/veoveo/README.md), [offline delivery](../deploy/offline/README.md) |
| Iterate on code and deploy a change | [Development iteration](DEVELOPMENT_ITERATION.md), [image builds](IMAGE_BUILDS.md), [local deployment profiles](LOCAL_DEPLOYMENT_PROFILES.md) |
| Run checks and understand GitHub status | [Local evidence workflow](CONTINUOUS_INTEGRATION.md#local-evidence-workflow) |
| Find implementation ownership | [Code map](CODEMAP.md), [contributor instructions](../AGENTS.md) |
| Implement or integrate an MCP server | [Server contract](../mcp/contract/DESIGN.md), [external extensions](EXTERNAL_EXTENSIONS.md), [integration runbook](EXTERNAL_REPOSITORY_INTEGRATION.md), [Python template](../templates/python-mcp/README.md) |
| Build or host a capability UI | [MCP Apps contract](../mcp/apps-extension/DESIGN.md), [Map integration](MAP_APP_INTEGRATION.md), [Console development](../apps/console/web/README.md) |
| Understand access and output ownership | [Work Context governance](WORK_CONTEXT_GOVERNANCE.md), [shared policy](../platform/policy/DESIGN.md) |
| Upload and consume files | [Artifact uploads](ARTIFACT_UPLOAD_PLAN.md), [Artifact service](../platform/artifacts/service/DESIGN.md), [Python SDK](../sdk/python/README.md) |
| Ingest and use recordings | [Recording model](RECORDINGS.md), [producer ingest](RECORDING_INGEST.md), [Recording MCP](../servers/recording-mcp/DESIGN.md) |
| Configure GPU placement | [GPU placement](GPU_PLACEMENT.md), [simulation runtime](../platform/runtimes/simulation/DESIGN.md) |
| Run a showcase or inspect the UI | [Showcases](../showcase/README.md), [screenshot gallery](screenshots/GALLERY.md), [architecture views](architecture/README.md) |
| Connect a third-party system | [Connector catalog](connectors/README.md) |

## Current Guidance And Historical Records

Repository-wide requirements live in [Architecture Decisions](ARCHITECTURE_DECISIONS.md)
and the accepted [Contract Evolution](CONTRACT_EVOLUTION.md) decisions. The
[MCP server contract](../mcp/contract/DESIGN.md) governs the hosted protocol.
Each component's adjacent `DESIGN.md` describes its implemented boundary and declared
gaps. [CODEMAP.md](CODEMAP.md) routes a change to those owners.

Plans retain their original design and dated delivery evidence. Their status and
current checkpoint take precedence over an older future-tense passage in the same
file. Approval of a plan does not establish implementation or release qualification.
An installed acceptance result proves the revision and installation it names.

### Delivery And Remaining Work

Status reviewed against the repository on September 18, 2026. Follow the linked
record for the precise acceptance limits.

| Record | State |
|---|---|
| [Workspace](WORKSPACE_PLAN.md) | First web release deployed September 16; later reactive behavior is recorded separately below. Distinct-person and production Task-input follow-ups remain documented. |
| [Reactive UX](REACTIVE_UX_PLAN.md) | Deployed and verified September 17. Cold-catalog status presentation and large-scale performance experiments remain. |
| [Computers](COMPUTERS_PLAN.md) | Core capability deployed with the Bioma configuration; browser, stock CLI, retained files and named agent authority have installed evidence. Clean/offline release closure and broader performance qualification remain separate gates. |
| [Artifact uploads](ARTIFACT_UPLOAD_PLAN.md) | First release deployed and verified September 9, including a resumed 10 GiB browser upload. Larger-capacity and comparative transfer measurements remain. |
| [MCP migration](RMCP_3_MIGRATION.md) | Migration delivered. September 17 qualification uses the RMCP 3.4.0 and Rig 0.42.0 forks pinned in Cargo; older pin tables preserve migration history. |
| [Recording catalog](RECORDING_CATALOG_HARD_CUT_PLAN.md) | Request 016 implemented, activated and accepted. |
| [Platform improvements](PLATFORM_IMPROVEMENTS_PLAN.md) | Requests 001–013 closed; requests 014–023 have mixed delivery states and phase-specific gates. |
| [Repository hardening](REPOSITORY_HARDENING_PLAN.md) | Partially delivered; use its delivery table for implemented tooling and remaining governance work. |
| [Capability adoption](CAPABILITY_ADOPTION_PLAN.md) | Weather, tabular prediction and skills tracks are recorded proposals; none is approved or started. |

### Measurements And Investigations

| Record | How to use it |
|---|---|
| [Development iteration](DEVELOPMENT_ITERATION.md) | Current operating workflow, known costs and active follow-ups |
| [Build/deploy audit](BUILD_DEPLOY_ITERATION_AUDIT.md) | Dated observations and experiments; commands and pins belong to their recorded revision |
| [Image performance](IMAGE_BUILD_PERFORMANCE.md) | Reproducibility and cold/warm measurements with their test conditions |
| [MCP Apps audit](MCP_APPS_LIVE_AUDIT.md) | Findings from the dated live investigation; each issue needs its own retest |
| [Artifact preview and App handoff](ARTIFACT_PREVIEW_AND_APP_HANDOFF.md) | Investigation and open design questions |

[Software-factory isolation](FACTORY_ISOLATION.md),
[self-improving harnesses](SELF_IMPROVING_HARNESS.md),
[model post-training](HARNESS_MEDIATED_MODEL_POST_TRAINING.md), and
[regulated readiness](REGULATED_READINESS.md) are explorations. They do not advertise
shipped capabilities or certify an installation.

The publication sources and their PDF-generation instructions are indexed in
[the code map](CODEMAP.md#documentation-index). Published figures and PDFs are
snapshots; component designs govern current implementation details.

## Keep Documentation Current

Keep a component's design beside its code. Put cross-component architecture and
operating guides here, and update this index and CODEMAP when ownership or locations
change. Link to the authoritative command or contract instead of copying a second
version of it into a plan.

When a feature lands, update the plan's status, its current implementation map and
the documentation index together. Preserve dated evidence with its revision and
qualification limits. Future work should identify what remains without reopening
completed delivery.

Check local links, section anchors and example paths after editing. Check command
examples against the owning CLI source. Ordinary documentation edits do not require
rebuilding or deploying Veoveo; a document embedded in a binary or served resource
also participates in that component's build inputs.
