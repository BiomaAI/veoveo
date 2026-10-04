# Module Declarations And Lane Runner

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Module declarations | Repository-owned typed Rust API; the default library has no dependencies |
| SurrealQL | Private parser and AST adapter pinned together to `surrealdb-syn =3.3.0` and `surrealdb-sql =3.3.0`; admitted subset described below |
| Database transport | SurrealDB Rust SDK `=3.3.0`, WebSocket with Rustls, explicit native transactions |
| Generated installation plan | Repository-owned `veoveo.ai/module-selection/v1` and `veoveo.ai/module-plan/v1` JSON; camelCase fields, positive u64 generation encoded as canonical decimal text |
| Migration identity | SHA-256 over the owner body and recorded declaration identity; append-only numbered lane history |

## Declaration Ownership

`ModuleSetup` declares a module's layer, exact table, function and analyzer claims,
table/function prefixes, dependency requirements, extension bindings and migration
lane. Checked builders reject overlapping claims and malformed names. The complete
registry rejects duplicate modules, cross-owner overlaps, dependency cycles and
kernel dependencies on optional modules. Typed analyzer ownership covers the shared
search analyzer and Knowledge's text analyzer.

The declaration graph sits below the foundational types and their procedural macros.
Its ordinary nominal types and contextual standard-library errors avoid a dependency
cycle. Schema-only owner features expose declarations without pulling in contracts,
server runtimes, database drivers or GPU libraries.

`ObservationTable` combines a checked `TableName` with either LIVE-only delivery or
a declared changefeed retention duration. Optional owners supply their own closed
observation enums and convert them into this descriptor; shared infrastructure does
not list their tables. The declaration does not prove table installation or retention.
Native owner schema checks establish those properties. Record replay rejects
LIVE-only sources, while resource invalidation can use them with current-state
reconciliation. The [Store observation design](../store/src/changefeed/DESIGN.md)
defines delivery and checkpoint behavior.

Execution declarations bind a logical image target to executable argv. Image targets
contain ASCII letters, digits, dot, underscore, slash and hyphen, with a 256-byte limit.
Composition resolves that target to its qualified OCI digest and supplies the command;
the declaration does not certify that an image contains the executable. One composition
image may execute several owners' lanes.

## Catalog And Selection

The registry contains every compiled module. Selection includes all kernels and the
transitive prerequisites of enabled optional modules. A known optional lane's persisted
history remains valid while the module is disabled. Histories absent from the complete
catalog are unknown. Ordered selection describes execution dependencies; the executor
also checks database completion before a dependent lane advances.

Migration numbering begins at zero. An empty lane has no latest version and needs an
explicit initialized header. Each migration carries its checked name, generated filename,
SQL body and fixed prerequisite declarations. Later migrations cannot remove or lower
a prior prerequisite. A concrete minimum must exist in the dependency's compiled lane.
`Satisfied` requires the dependency's entire compiled lane to be current before pending
work starts. Historical receipts capture what satisfied a fixed migration rather than
rewriting that migration when its dependency later grows.

## SQL Admission

`prepare` parses and admits every selected body before creating a private immutable
`PreparedInstallation`. Only that value exposes database execution. A malformed body
in another selected lane therefore prevents even bookkeeping initialization.

The adapter bounds bodies to 1 MiB, expression depth to 64, object depth to 32, query
depth to 20 and visited expressions to 100,000 per body. It rejects transaction/session
statements and privileged objects. Supported definitions cover NORMAL or ANY tables, fields with builtin reference cleanup,
indexes, analyzers without callbacks, synchronous events and functions whose bodies
pass recursive admission. Table alteration and owned table/field/index/event/function/
analyzer removal use the same ownership policy. Unsupported variants fail before execution.

Schema mutation and data writes require the current owner. Kernel reads may use declared
dependencies; optional reads may use declared optional dependencies. Typed record links and analyzer references may use declared dependencies
without granting data reads. Optional modules cannot directly read kernel data. Analyzer MAPPER filters are rejected because they load external files. The
current adapter rejects custom calls until their stored bodies and versioned kernel API
profiles are qualified. Its small builtin allowlist excludes network, file, scripting,
provider and dynamic functions.

Every supported expression-bearing child is visited, including defaults, assertions,
permissions, comments, nested objects, event/function bodies and cast types. Unproven
record dereferences, graph traversal and dynamic targets fail admission. This subset
does not admit the entire production schema; later ownership work must qualify additional
constructs or rewrite their owners before production lanes replace the current bootstrap.
Admission errors identify the module, filename and construct or object without printing
SQL bodies or bound values.

## History And Execution

Store owns the reserved `platform_module_lane`, `platform_module_migration` and
`platform_module_installation` tables.
The runner alone mutates them; owner SQL cannot access these tables. Logical ownership
checks constrain admitted SQL independently of the database user's privileges. A database
EDITOR or system user does not provide per-module table isolation.

The executor applies each body and its history in one native transaction and explicitly
cancels failed transactions. It compares fixed migration identity, rejects gaps and drift,
and reads persisted history to distinguish a matching concurrent winner from a failure.
Read-only status supports publication readiness without trusting Job launch order.
Catalog inspection compares parsed table, field and index declarations before accepting
bookkeeping infrastructure; `IF NOT EXISTS` and an empty SELECT do not prove its shape.

Operation waits default to 30 seconds and cancellation to 10 seconds. Checked overrides
cap them at 300 and 60 seconds. Owned Tokio tasks explicitly cancel transactions when
an operation times out or its awaiter is dropped; their cleanup requires the runtime to
stay alive. A lost begin response has no addressable transaction handle, and a lost
commit response cannot be treated as revoked. Callers must preserve the unresolved
outcome, observe history and perform explicit session cleanup before further mutations.

## Qualification And Integration

Declaration and parser fixtures cover nonempty lanes, cross-owner and recursive effects,
links, analyzers, malformed SQL and unsupported syntax. The independent schema consumer
composes all actual owner exports through schema-only features. Executor fixtures use
isolated owned database containers and the existing SDK transaction profile.

The parser packages are upstream internal APIs without a stability guarantee. Their
[upstream documentation](https://docs.rs/surrealdb-syn/3.3.0/surrealdb_syn/) schedules
replacement by a new parser. Both exact pins move together only after AST-child coverage,
fail-closed policy and native transaction fixtures pass. Replace this private adapter
when upstream offers a maintained equivalent that preserves complete inspection.

The deployment harness qualifies rendered Jobs and the digest-pinned composition
image's commands through `module-installation-verify`. Its isolated namespace covers
fresh preparation, lane completion, runtime-authenticated publication, credential
rotation and stale-generation rejection. It applies selected chart resources directly;
full Helm rollback, hosted workload startup and managed-agent recovery require their
own installed checks. Production schemas and histories use the mixed Store catalog
until ownership moves into the declared lanes.

## Generated Composition Plans

The optional `serialization` feature adds checked JSON selection and plan documents.
The default declaration graph has no dependencies. A composition calls actual owner
exports and generates dependency-ordered lane descriptors, complete module names and
host predicates. Every present key in a host predicate must be enabled; separate rows
express alternative consumers. An enabled host requires its selected schema lane,
while selection alone never starts a workload.

The composition identity is a locked OCI digest supplied by installation compilation.
It is not a binary self-attestation. Running the exact locked image and binding the
rendered objects establishes provenance. Consumers regenerate the entire plan from
compiled declarations before effects; selected module names remain open validated types.

## Preparation Generation

`PreparationKey` and `InstallationGeneration` are dependency-free checked types.
The composition hashes its complete plan, compiled mixed-schema identity and runtime
account name into a preparation identity. The generation is an installation-owned
positive integer, independent of chart release metadata. A changed preparation identity
requires a higher generation; the same generation with conflicting identity fails.

Prepared execution initializes the reserved infrastructure, then claims the generation
before mixed-schema work. A delayed older preparer cannot rotate the newer account.
Completion rotates a validated database editor and writes the completion marker in one
owned native transaction. Already completed keys do not repeat rotation. The API accepts
only credentials for this fixed operation and has no arbitrary-SQL execution hook.
A live Tokio runtime supports cancellation cleanup as described above.

Installation lane execution compares the completed preparation key inside every
header and migration transaction. A current lane also verifies its key before returning.
Absent or unfinished preparation may wait within the caller's deadline; malformed
markers, conflicting identities and superseding generations fail admission.

## Read-Only Kernel SQL APIs

`ModuleOwnership` checks owner identity, layer and object claims without an execution
host. A runtime contribution binds its table against this declaration; that check
neither installs the table nor proves registry agreement. `ModuleSetup` uses the same
value and adds composition-supplied execution, dependencies and lane history.

A kernel owner exports an exact versioned `KernelSqlApi`. The declaration names the
introducing migration, parameter and return types, owned read tables and complete
function definition. Supported signatures use strings, booleans, objects, specific
record tables, arrays and optional values. The private parser requires the definition
once in its introducing entry and compares its full AST and signature. Declaring an
export does not waive inspection of its body. Its read-only leaf profile rejects writes,
DDL, custom calls, scripts, foreign reads and nondeterministic built-ins.

The visitor permits a single field on a local object only in a positive branch of
`IF type::is_object($local)`. It inspects the guard argument and every branch. Proofs
do not cross an ELSE or sibling branch, and exported functions cannot redefine a
parameter or local. Only declared parameters and previously bound locals are available;
ambient session parameters cannot supply object proofs. Nested fields require another
local and another guard. Bare row
fields are available only inside an owned SELECT. This profile prevents record-link
traversal through flexible payloads without treating an owner declaration as a proof
of stored object shape.

A migration calling an API must declare its owner's dependency and an explicit
per-migration minimum covering introduction. Admission checks the exact API name,
argument count and proven types, including specific record tables. It inspects every
argument and rejects argument-side mutation. Private functions and undeclared versions
cannot become callable through a namespace prefix. Runtime services bind admitted
caller scope through their owner contracts; the SQL API does not authenticate an
arbitrary supplied scope.

Index columns have a separate schema proof. Every intermediate in a multipart column
must have a preceding `object` or `option<object>` field declaration on that table.
The visitor remembers these declarations across the owner's lane entries. Executable
queries still use the guarded local-object profile above. Table, field and index
shape definitions, removals and alterations must be migration top-level statements;
conditional blocks and deferred function/event bodies cannot grant or erase proof.
`IF NOT EXISTS` field definitions do not certify a preexisting shape. Such index
definitions retain earlier dependencies rather than replacing them. Field/table
removals, redefinitions and wildcard declarations invalidate the relevant proof;
existing dependent indexes must be removed before a shape change. Redefining a parent
object cannot silently certify its previously declared descendants.
