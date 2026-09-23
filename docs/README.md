# Veoveo Documentation

Find the guide for the work you need to do in the table below. Veoveo is the
platform. [Bioma](../examples/bioma/README.md) is one installation of it and also
serves as the acceptance environment. People use Workspace for daily chat with each
other and with agents, and administrators use Console to manage the installation, its
services, and its agents. Computers is part of every standard release.

## Find A Guide

| Task | Start here |
|---|---|
| Understand the product | [Repository overview](../README.md), [architecture decisions](ARCHITECTURE_DECISIONS.md), [technical design](TECH_DESIGN.md) |
| Work on shared chat and agent feedback | [Workspace client](../apps/workspace/DESIGN.md), [Workspace delivery](WORKSPACE_PLAN.md), [reactive UX delivery](REACTIVE_UX_PLAN.md) |
| Add agent creation and management | [Agent-management plan](AGENT_MANAGEMENT_PLAN.md): API/Console authoring, delegated Workspace creation and managed UAV-style agents |
| Use or develop Speech | [Delivery and qualification](SPEECH_PLAN.md), [Speech domain](../servers/speech-mcp/DESIGN.md) |
| Work on Computers | [Domain design](../platform/computers/DESIGN.md), [delivery and qualification](COMPUTERS_PLAN.md) |
| Install and operate Veoveo | [Enterprise deployment](ENTERPRISE_DEPLOYMENT.md), [Helm installation](../deploy/helm/veoveo/README.md), [offline delivery](../deploy/offline/README.md) |
| Iterate on code and deploy a change | [Development iteration](DEVELOPMENT_ITERATION.md), [image builds](IMAGE_BUILDS.md), [local deployment profiles](LOCAL_DEPLOYMENT_PROFILES.md) |
| Run checks and understand GitHub status | [Local evidence workflow](CONTINUOUS_INTEGRATION.md#local-evidence-workflow) |
| Find implementation ownership | [Code map](CODEMAP.md), [contributor instructions](../AGENTS.md) |
| Implement or integrate an MCP server | [Server contract](../mcp/contract/DESIGN.md), [fork development](FORK_DEVELOPMENT.md), [Python template](../templates/python-mcp/README.md) |
| Build or host a capability UI | [MCP Apps contract](../mcp/apps-extension/DESIGN.md), [Map integration](MAP_APP_INTEGRATION.md), [Console development](../apps/console/web/README.md) |
| Understand access and output ownership | [Work Context governance](WORK_CONTEXT_GOVERNANCE.md), [shared policy](../platform/policy/DESIGN.md) |
| Upload and consume files | [Artifact uploads](ARTIFACT_UPLOAD_PLAN.md), [Artifact service](../platform/artifacts/service/DESIGN.md), [Python SDK](../sdk/python/README.md) |
| Ingest and use recordings | [Recording model](RECORDINGS.md), [Rerun client guide](RERUN_RECORDINGS.md), [producer ingest](RECORDING_INGEST.md), [Recording MCP](../servers/recording-mcp/DESIGN.md) |
| Configure GPU placement | [GPU placement](GPU_PLACEMENT.md), [simulation runtime](../platform/runtimes/simulation/DESIGN.md) |
| Run a showcase or inspect the UI | [Showcases](../showcase/README.md), [screenshot gallery](screenshots/GALLERY.md), [architecture views](architecture/README.md) |
| Connect a third-party system | [Connector catalog](connectors/README.md) |

## Current Guidance And Historical Records

Repository-wide requirements are in [Architecture Decisions](ARCHITECTURE_DECISIONS.md)
and the accepted [Contract Evolution](CONTRACT_EVOLUTION.md) decisions. The
[MCP server contract](../mcp/contract/DESIGN.md) defines what every hosted server must
implement. Each component keeps a `DESIGN.md` beside its code that describes what it
does today and what is still missing. [CODEMAP.md](CODEMAP.md) tells you which
component owns a change.

Plans keep their original design and their dated delivery records. When a plan's
status line disagrees with an older future-tense passage in the same file, trust the
status line. An approved plan may not be implemented or released yet. An acceptance
result applies only to the revision and installation it names.

### Delivery And Remaining Work

This table was last checked against the repository on September 18, 2026. Later
proposals carry their own dates. Each linked record states exactly what its acceptance
covered.

| Record | State |
|---|---|
| [Speech](SPEECH_PLAN.md) | Deployed September 22. CUDA dictation, recording Tasks, reload recovery, playback, and downloads passed installed browser acceptance. Physical-microphone, scale, and broader readiness testing are still open. |
| [Agent management](AGENT_MANAGEMENT_PLAN.md) | Delivered September 20. Authoring, publication, revision adoption, and managed lifecycle are deployed at `veoveo.bioma.ai` and pass installed checks with four UAV pilots. Still open: a completed simulator flight, managed templates that admit Computer tools, load measurements, and a second-person usability session. |
| [Workspace](WORKSPACE_PLAN.md) | First web release deployed September 16. Later reactive behavior is tracked in the Reactive UX row. The plan lists follow-ups for distinct-person testing and production Task input. |
| [Reactive UX](REACTIVE_UX_PLAN.md) | Deployed and verified September 17. Status display for a cold catalog and large-scale performance experiments are still open. |
| [Computers](COMPUTERS_PLAN.md) | Deployed on Bioma, where browser access, the stock CLI, retained files, and named agent authority are verified. A clean offline release and broader performance testing are still open. |
| [Artifact uploads](ARTIFACT_UPLOAD_PLAN.md) | First release deployed and verified September 9, including a resumed 10 GiB browser upload. Larger uploads and comparative transfer measurements are still open. |
| [MCP migration](RMCP_3_MIGRATION.md) | Migration delivered. September 17 qualification uses the RMCP 3.4.0 and Rig 0.42.0 forks pinned in Cargo; older pin tables preserve migration history. |
| [Recording catalog](RECORDING_CATALOG_HARD_CUT_PLAN.md) | Request 016 implemented, activated and accepted. |
| [Platform improvements](PLATFORM_IMPROVEMENTS_PLAN.md) | Requests 001–013 closed; requests 014–023 have mixed delivery states and phase-specific gates. |
| [Repository hardening](REPOSITORY_HARDENING_PLAN.md) | Partially delivered. Its delivery table lists the implemented tooling and the remaining governance work. |
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
[regulated readiness](REGULATED_READINESS.md) are explorations. They describe ideas,
not shipped features, and they do not certify an installation.

The publication sources and their PDF-generation instructions are listed in
[the code map](CODEMAP.md#documentation-index). Published figures and PDFs are
snapshots. For current behavior, read the component designs.

## Keep Documentation Current

Keep a component's design beside its code. Put cross-component architecture and
operating guides here, and update this index and CODEMAP when ownership or locations
change. Link to the command or contract that defines a behavior instead of copying a second
version of it into a plan.

When a feature lands, update the plan's status, its implementation map, and the
documentation index in the same change. Keep dated results together with the revision
they were measured on and the limits of what they tested. When describing future work,
say what remains rather than rewriting the record of what was delivered.

Check local links, section anchors, and example paths after editing. Check command
examples against the source of the CLI that runs them. Documentation edits do not
require rebuilding or deploying Veoveo, except for documents embedded in a binary or
served as a resource, which are build inputs of their component.
