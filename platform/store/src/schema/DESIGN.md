# Kernel Module Declarations

## Standards And Protocols

The declarations implement the internal Rust `ModuleSetup` contract from
[`veoveo-modules`](../../../modules/DESIGN.md). Schema-only builds require no
protocol, database driver or runtime. The executing Store uses the qualified
SurrealDB 3.3.0 SurrealQL migration profile documented by
[Store migrations](../migrations/DESIGN.md); the Tasks lane exports a guarded, read-only selection function.

## Owners And Ordering

`schema` exports seven independent setup functions, each accepting the composition
root's checked `LaneExecution`. The required order is Store, Identity, Gateway,
Artifacts, Tasks, Audit and Knowledge. Each declaration requires its predecessor;
the registry resolves transitive dependencies. This declares the target order and
does not certify the present mixed schema against that order.

| Owner | Table claims | Function claims |
|---|---|---|
| Store | `changefeed_checkpoint`, `platform_schema_migration`, `platform_downstream_migration`, `platform_module_lane`, `platform_module_migration`, `platform_module_installation` | None |
| Identity | `tenant`, `enterprise`, `principal`, `principal_group`, `membership`, `work_context`, `oauth_client` | None |
| Gateway | `gateway_*`, `policy_revision`, `profile`, `profile_server`, `mcp_server`, `mcp_interaction` | None |
| Artifacts | `artifact_*`, `share_link` | `fn::artifact_upload_profile_digest`, `fn::artifact_upload_authority_matches` |
| Tasks | `task`, `task_input`, `task_idempotency`, `task_produced_artifact`, `task_used_artifact`, `provider_job`, `provider_event`, `domain_usage` | `fn::kernel::tasks::selection_v1` |
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

Tasks has an additive version-zero lane. The other kernel lanes are empty. The composition root supplies an execution image and argument
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
migration implementation; these declarations do not redistribute or duplicate it.

## Current Association Constraints

Tasks references Artifacts through `task.result_artifact`; Artifacts references Tasks
through `artifact_occurrence.task`. Tasks references Gateway profiles and servers,
while Gateway references Tasks through `gateway_task_route.source_task` and
`mcp_interaction.task`. Audit's frozen `audit_record.target_ref` includes Computers.
These associations need owner-approved changes before the existing schema can satisfy
the declared order and exclude optional-module references from kernel lanes. Moving
only link-table definitions does not remove the embedded associations.

## Task Selection SQL API

`tasks/migrations/0000_selection_v1.surql` installs the versioned read-only
`fn::kernel::tasks::selection_v1` function. It accepts a specific `record<task>` and
thirteen scalar scope bindings including that record: server, tenant, principal,
profile, their request keys, labels, optional operation types and an optional Work
Context triple. The function reads the persisted row and preserves the runtime's
owner/request agreement, label clearance and context predicates. An absent context
requires all three context inputs absent. Raw optional tenant identity stays distinct
from the runtime's effective installation tenant.

Nested positive object guards precede every local field access. Malformed or
record-valued policy intermediates cannot contribute visibility. Denied or missing
Tasks return `NONE`. An admitted selection contains id, creation time, operation type,
status and the domain's opaque input/result payloads. Consumers guard these open
payloads before their own SQL predicates and apply those predicates before grouping or
limits. The supplied scope comes from service admission; this function does not
authenticate caller-supplied values.

The module runner checks the declared function signature, exact definition and owned
read profile before effects. Installation preparation first applies the current mixed
catalog, then selected lanes install this additive API. Existing Task indexes and
mixed histories keep their current owner until the persistence cut.
