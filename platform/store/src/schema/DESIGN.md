# Kernel Module Declarations

## Standards And Protocols

The declarations implement the internal Rust `ModuleSetup` contract from
[`veoveo-modules`](../../../modules/DESIGN.md). Schema-only builds require no
protocol, database driver or runtime. The executing Store uses the qualified
SurrealDB 3.3.0 SurrealQL migration profile documented by
[Store migrations](../migrations/DESIGN.md); these declarations contain no SQL.

## Owners And Ordering

`schema` exports seven independent setup functions, each accepting the composition
root's checked `LaneExecution`. The required order is Store, Identity, Gateway,
Artifacts, Tasks, Audit and Knowledge. Each declaration requires its predecessor;
the registry resolves transitive dependencies. This declares the target order and
does not certify the present mixed schema against that order.

| Owner | Table claims | Function claims |
|---|---|---|
| Store | `changefeed_checkpoint`, `platform_schema_migration`, `platform_downstream_migration`, `platform_module_lane`, `platform_module_migration` | None |
| Identity | `tenant`, `enterprise`, `principal`, `principal_group`, `membership`, `work_context`, `oauth_client` | None |
| Gateway | `gateway_*`, `policy_revision`, `profile`, `profile_server`, `mcp_server`, `mcp_interaction` | None |
| Artifacts | `artifact_*`, `share_link` | `fn::artifact_upload_profile_digest`, `fn::artifact_upload_authority_matches` |
| Tasks | `task`, `task_input`, `task_idempotency`, `task_produced_artifact`, `task_used_artifact`, `provider_job`, `provider_event`, `domain_usage` | None |
| Audit | `audit_*` | `fn::append_audit`, `fn::append_audit_indexing` |
| Knowledge | `knowledge_*` | None |

Store also claims the shared `platform_search` analyzer, and Knowledge claims
`knowledge_text`. Their exact analyzer names are separate from table and function
claims.

Tasks uses explicit claims because Frames owns `task_used_frame`. Store owns the
named-lane history tables as infrastructure; application migrations cannot mutate
runner bookkeeping. Claims identify logical owners, not tenant isolation.

## Features And Execution

The default `runtime` feature activates the existing Store implementation and its
dependencies. `--no-default-features --features schema` exposes only these declarations
and `veoveo-modules` with default features disabled. Runtime integration test targets
require `runtime`.

Every lane is empty. The composition root supplies an execution image and argument
vector, and owns command availability and qualification. The existing gateway image
is the initial composition host candidate because its binary links the Store. Its
`installation-prepare` command applies the mixed catalog without publication.
`module-migrate` executes one selected lane; runtime-authenticated publication follows
completed preparation and lane checks. Installed image and Job qualification is tracked
in the active contract plan. Execution declarations alone do not establish that an
installation's selected image supports these commands.

Each kernel owner has one declaration in `schema/<owner>.rs`. Owner migrations and
queries belong together in the corresponding owner folder when persistence is placed
there. The compiled mixed catalog and its numeric histories remain the current
migration implementation; empty declarations do not redistribute or duplicate it.

## Current Association Constraints

Tasks references Artifacts through `task.result_artifact`; Artifacts references Tasks
through `artifact_occurrence.task`. Tasks references Gateway profiles and servers,
while Gateway references Tasks through `gateway_task_route.source_task` and
`mcp_interaction.task`. Audit's frozen `audit_record.target_ref` includes Computers.
These associations need owner-approved changes before the existing schema can satisfy
the declared order and exclude optional-module references from kernel lanes. Moving
only link-table definitions does not remove the embedded associations.
