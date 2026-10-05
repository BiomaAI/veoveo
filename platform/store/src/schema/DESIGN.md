# Kernel Module Declarations

## Standards And Protocols

The declarations implement the internal Rust `ModuleSetup` contract from
[`veoveo-modules`](../../../modules/DESIGN.md). Schema-only builds require no
protocol, database driver or runtime. The private lane runner parses and executes
its admitted SurrealDB 3.3.0 SurrealQL profile. The installation plan uses
`veoveo.ai/module-plan/v1`; migration identity binds each owner's numbered body
and declaration to its recorded SHA-256 checksum.

## Owners And Ordering

`schema::kernel_modules` exports seven always-selected kernel owners. Each setup
accepts the composition root's checked `LaneExecution`. Their execution order is
Store, Identity, Gateway, Artifacts, Tasks, Audit and Knowledge. Every owner declares
its predecessor, and the registry resolves transitive requirements before effects.

| Owner | Table claims | Function claims |
|---|---|---|
| Store | `changefeed_checkpoint`, `platform_module_lane`, `platform_module_migration`, `platform_module_installation` | None |
| Identity | `tenant`, `enterprise`, `principal`, `principal_group`, `membership`, `work_context`, `oauth_client` | `fn::kernel::identity::actor_admitted_v1`, `fn::kernel::identity::identity_enabled_v1`, `fn::kernel::identity::principal_summaries_v1`, `fn::kernel::identity::search_enabled_users_v1`, `fn::kernel::identity::enabled_user_v1`, `fn::kernel::identity::identity_labels_v1` |
| Gateway | `gateway_*`, `policy_revision`, `profile`, `profile_server`, `mcp_server`, `mcp_interaction` | `fn::kernel::gateway::task_retention_route_v1` |
| Artifacts | `artifact_*`, `share_link` | `fn::artifact_upload_profile_digest`, `fn::artifact_upload_authority_matches`, `fn::kernel::artifacts::read_v1` |
| Tasks | `task`, `task_input`, `task_idempotency`, `task_produced_artifact`, `task_used_artifact`, `provider_job`, `provider_event`, `domain_usage` | `fn::kernel::tasks::selection_v1`, `fn::kernel::tasks::lifecycle_v1`, `fn::kernel::tasks::release_retention_v1` |
| Audit | `audit_*` | `fn::append_audit`, `fn::append_audit_indexing` |
| Knowledge | `knowledge_*` | None |

Store claims the `platform_search` analyzer; Knowledge claims `knowledge_text`.
Runner bookkeeping belongs to Store infrastructure and cannot be mutated by an
application migration. Claims identify logical owners, not tenant isolation.

## Fresh Installation And Execution

Every kernel lane installs its complete current schema at version zero. Optional
owners supply their current schemas from their own crates. The composition selects
those owners and prepares every selected SQL body before database writes. The runner
records append-only lane and migration identities, rejects drift and preserves
known disabled-owner histories when a selection changes.

`PlatformStore::connect` authenticates and inspects the selected database without
applying schema. Either `platform_schema_migration` or
`platform_downstream_migration` marks an unsupported mixed-catalog database and
produces an actionable fresh-installation error. This profile supplies no historical
catalog migration or backfill path.

The gateway composition provides `installation-prepare`, `module-migrate` and
`module-status`. Preparation establishes the generation and database-scoped runtime
credentials. Each migration command applies one selected owner lane after its
prerequisites. Publication and serving require completed preparation and current
selected lanes. The composition owns command/image qualification; declarations alone
do not certify an installed Job or image.

`--no-default-features --features schema` exposes these declarations and the
dependency-free module API. The default `runtime` feature activates Store persistence and
the database driver. Composition and native fixtures activate the runner separately.
Each owner's SQL lives in `schema/<owner>/migrations`;
queries remain with the persistence implementation that executes them.

## Schema Associations

Plain typed field links among selected kernel owners may be reciprocal. For example,
Tasks links to Artifact results, Artifact occurrences link to Tasks, and Gateway
routes link to their source Tasks. The runner checks every referenced table owner
while keeping migration execution dependencies acyclic. This allowance applies only
to owned field types without `REFERENCE`; executable reads and writes still obey
owner and dependency admission.

Audit stores `audit_record.target_ref` as an optional opaque native record link.
Its field assertion checks a nonempty table name without enumerating optional owner
tables. The installation's Audit target registry admits each owner payload and
lookup reference, and Store verifies their agreement when encoding and decoding rows.
Registering a read codec neither selects that owner's workload nor installs its schema.

## Versioned Kernel SQL APIs

Each public leaf declares its complete definition, introducing version, signature,
read tables and effect profile. The module runner checks these declarations before
effects. Optional callers need the owner's dependency and a per-migration minimum
covering introduction; callers inspect every argument under the read-only profile.

| API | Owned reads and effect |
|---|---|
| `fn::kernel::identity::actor_admitted_v1` | Reads `tenant`, `work_context` and `principal`; checks supplied context digest and optional human requirement |
| `fn::kernel::identity::identity_enabled_v1` | Reads those same Identity tables for current enabled identity |
| `fn::kernel::identity::principal_summaries_v1` | Reads `principal`; returns tenant-scoped ID/name pairs for at most 256 supplied IDs, including disabled and service actors |
| `fn::kernel::identity::search_enabled_users_v1` | Reads `principal`; filters enabled human users by tenant and lowercase name substring before ordering by name and limiting to 20 |
| `fn::kernel::identity::enabled_user_v1` | Reads `principal`; returns whether one supplied principal is an enabled human in the supplied tenant |
| `fn::kernel::identity::identity_labels_v1` | Reads `tenant`, `work_context` and `principal`; returns person, tenant name and context title when both records belong to that tenant, otherwise `NONE` |
| `fn::kernel::gateway::task_retention_route_v1` | Reads one `gateway_task_route` and returns its source Task and server; missing route throws |
| `fn::kernel::artifacts::read_v1` | Reads one `artifact_occurrence` and its `artifact_grant` edges; applies current scope and a supplied database timestamp, returning admitted opaque metadata, observation material and the next future expiry |
| `fn::kernel::tasks::selection_v1` | Reads one `task`; returns the admitted object or `NONE` |
| `fn::kernel::tasks::lifecycle_v1` | Trusted service selection of one Task by exact record, server, tenant and operation; returns lifecycle and typed ownership metadata plus whole-result agreement, without payloads or caller authorization |
| `fn::kernel::tasks::release_retention_v1` | Reads one `task` and updates only its `retention_pins`; checks expected server and existing pin |

Identity's metadata leaves use specific tenant, principal and Work Context record
parameters. Principal summaries reject arrays above 256 entries before reading and
preserve unspecified result order. Duplicate IDs produce one row; missing and foreign
tenant IDs produce none. Historical attribution does not require an enabled principal.
Identity labels check record existence and both tenant relationships. These read-only
leaves neither authenticate their supplied scope nor call other exports. Workspace
owns human search input validation and all chat, invitation and run predicates. Its
invitation transaction calls `enabled_user_v1` before mutations, and inbox metadata
resolves only the inviters selected by Workspace's pending-invitation query.

Task selection takes thirteen typed bindings: Task, server, tenant, owner, profile,
request keys, labels, optional operation types and an optional Work Context triple.
It preserves persisted owner/request agreement, clearance and context predicates.
An absent context requires all three context inputs absent. Raw optional tenant
identity stays distinct from the effective installation tenant. Open input/result
payloads confer no record traversal permission; consumers guard and inspect them
before domain predicates, grouping or limits.

Tasks stores the required owner snapshot in `owner_context`, separately from the
`owner` principal record link. The request declares its input, status message and
timing fields; only the input accepts arbitrary domain JSON. Store-owned driver
types validate the controlled objects. Rust and Python readers check that the
snapshot agrees with the indexed identity references and current Task authority.
Transactions comparing an expected request also compare the expected owner snapshot.
Snapshot clearance labels and output-policy labels have separate meanings and fields.

Retention release uses the explicit `OwnedUpdate` profile for `task.retention_pins`.
Agents first reads the Gateway leaf and then calls the Task leaf for a present source
Task, within its existing result-consumption transaction. The leaves neither call
foreign private functions nor commit independently. Supplied scope originates in
service admission; these functions do not authenticate arbitrary caller values.

Artifacts exports `fn::kernel::artifacts::read_v1` for exact occurrence admission.
Its typed inputs separate principal and group subjects and include one database
query timestamp. The leaf owns tenant, labels, classification, context, current
grants and retention checks. Denial returns NONE; admitted reads return opaque
metadata, grant-sensitive observation material and the earliest future expiry.
Ordinary Artifact exact/page reads use this same admission before pagination.
Optional owners compare whole metadata receipts without importing their provenance
vocabulary into the kernel or querying paths inside opaque metadata.

## Closed Object Assertions

SurrealDB optional child declarations can materialize absent fields as `NONE`
before validating their parent object. Closed objects reject unknown keys and
check each required value against `NONE`; key presence alone does not prove a
required value exists. Tagged objects permit declared inactive children only
when their values are `NONE`, while requiring the active variant's fields.
