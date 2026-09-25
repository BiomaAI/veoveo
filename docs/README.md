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
| Work on shared chat and agent feedback | [Workspace client](../apps/workspace/DESIGN.md) |
| Create and manage agents | [Agent manager](../agents/manager/DESIGN.md), [agent authoring UI](../apps/console/web/src/agent-management/DESIGN.md), [agent runtime](../agents/runtime/DESIGN.md) |
| Use or develop Speech | [Speech server](../servers/speech-mcp/DESIGN.md) |
| Work on Computers | [Computers domain](../platform/computers/DESIGN.md) |
| Install and operate Veoveo | [Enterprise deployment](ENTERPRISE_DEPLOYMENT.md), [Helm installation](../deploy/helm/veoveo/README.md), [offline delivery](../deploy/offline/README.md) |
| Iterate on code and deploy a change | [Development iteration](DEVELOPMENT_ITERATION.md), [image builds](IMAGE_BUILDS.md), [local deployment profiles](LOCAL_DEPLOYMENT_PROFILES.md) |
| Run checks and understand GitHub status | [Local evidence workflow](CONTINUOUS_INTEGRATION.md#local-evidence-workflow) |
| Find implementation ownership | [Code map](CODEMAP.md), [contributor instructions](../AGENTS.md) |
| Implement or integrate an MCP server | [Server contract](../mcp/contract/DESIGN.md), [fork development](FORK_DEVELOPMENT.md), [Python template](../templates/python-mcp/README.md) |
| Build or host a capability UI | [MCP Apps contract](../mcp/apps-extension/DESIGN.md), [Map integration](MAP_APP_INTEGRATION.md), [Console development](../apps/console/web/README.md) |
| Understand access and output ownership | [Work Context governance](WORK_CONTEXT_GOVERNANCE.md), [shared policy](../platform/policy/DESIGN.md) |
| Upload and consume files | [Artifact uploads](ARTIFACT_UPLOAD_PLAN.md), [Artifact service](../platform/artifacts/service/DESIGN.md), [Console uploads](../apps/console/web/src/uploads/DESIGN.md), [Python SDK](../sdk/python/README.md) |
| Ingest and use recordings | [Recording model](RECORDINGS.md), [Rerun client guide](RERUN_RECORDINGS.md), [producer ingest](RECORDING_INGEST.md), [Recording MCP](../servers/recording-mcp/DESIGN.md) |
| Configure GPU placement | [GPU placement](GPU_PLACEMENT.md), [simulation runtime](../platform/runtimes/simulation/DESIGN.md) |
| Run a reference integration or inspect the UI | [Reference integrations](../showcase/README.md), [screenshot gallery](screenshots/GALLERY.md), [architecture views](architecture/README.md) |
| Connect a third-party system | [Connector catalog](connectors/README.md) |

## Where Things Are Defined

[Architecture Decisions](ARCHITECTURE_DECISIONS.md) and
[Contract Evolution](CONTRACT_EVOLUTION.md) hold the repository-wide rules. The
[MCP server contract](../mcp/contract/DESIGN.md) defines what every hosted server must
implement. Each component keeps a `DESIGN.md` beside its code that describes what it
does today and what is still missing. [CODEMAP.md](CODEMAP.md) tells you which
component owns a change.

Plans, measurements, investigations, and explorations are listed in the
[code map's documentation index](CODEMAP.md#documentation-index). They record how the
system was built and tested. For current behavior, read the component designs.

## Keep Documentation Current

Keep a component's design beside its code. Put cross-component architecture and
operating guides here, and update this index and CODEMAP when ownership or locations
change. Link to the command or contract that defines a behavior instead of copying a second
version of it into a plan.

When a feature lands, update the component's design and, if you keep a plan for it,
the plan's status in the same change. Keep dated results in plans and measurement
records, together with the revision they were measured on. Guides and READMEs describe
the current state only.

Check local links, section anchors, and example paths after editing. Check command
examples against the source of the CLI that runs them. Documentation edits do not
require rebuilding or deploying Veoveo, except for documents embedded in a binary or
served as a resource, which are build inputs of their component.
