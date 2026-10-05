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

`prepare` parses each selected body once and admits it before creating a private
immutable `PreparedInstallation`. Its private AST context records ordered function
definitions, their introducing migrations, explicit overwrites and removals. Only the
prepared value exposes database execution. A malformed body in another selected lane
therefore prevents even bookkeeping initialization.

The adapter bounds bodies to 1 MiB, expression depth to 64, object depth to 32, query
depth to 20 and visited expressions to 100,000 per body. It rejects transaction/session
statements and privileged objects. Supported definitions cover NORMAL and ANY tables, relations with static endpoints,
owned materialized views, field literal and structural kinds, builtin reference cleanup,
indexes, analyzers without callbacks, synchronous events and functions whose bodies
pass recursive admission. Table alteration and owned table/field/index/event/function/
analyzer removal use the same ownership policy. Unsupported variants fail before execution.

Schema mutation and data writes require the current owner. Kernel reads may use declared
dependencies; optional reads may use declared optional dependencies. Typed record links and analyzer references may use declared dependencies
without granting data reads. Plain `DEFINE FIELD TYPE record<table>` declarations
on owned tables may reference any selected kernel owner when the declaring owner is
also a kernel. These schema links may cycle because selection includes every kernel;
the migration execution graph stays acyclic. The visitor carries this permission
through union, array and set field types and validates every static table claim.
A generic `record` kind permits opaque storage only in an owned field type without
`REFERENCE`; it supplies no executable target, cast, signature or dereference privilege.
Fields with `REFERENCE` use declared dependencies, as do optional owners and
`table<table>` types. Casts, variable and function types, record literals and every
executable field child use the ordinary admission policy. Runner history tables
cannot appear in either profile. Optional modules cannot directly read kernel data. Analyzer MAPPER filters are rejected because they load external files. Private calls within one owner inspect the complete callee body and every transitive
call under the callee's ownership. Calls between optional owners require a declared
dependency, a concrete minimum covering introduction and a read-only callee. Optional
calls into a kernel use declared versioned `KernelSqlApi` leaves. The builtin profile
excludes network, file, scripting and provider effects. `record::exists` checks the
same admitted target and read policy as SELECT. Static `type::record` constructors
validate the table claim and every key expression.

Every supported expression-bearing child is visited, including defaults, assertions,
permissions, comments, nested objects, event/function bodies and cast types. Unproven
record dereferences, graph traversal and dynamic targets fail admission. The selected production owner lanes use this supported profile. Additional
SurrealQL constructs require complete child inspection and owner/effect qualification
before the runner admits them.
Admission errors identify the module, filename and construct or object without printing
SQL bodies or bound values.

Deferred bodies may bind an initially forward-declared same-owner helper. Immediate
calls require the actual definition at that statement, including transitive calls.
Recursion, unresolved calls and mutation in any function argument fail admission.
Effect analysis memoizes exact definition versions, statement positions, read-only
contexts and traversal depths. Every function change rechecks surviving functions,
fields, events, table permissions, views and COUNT-index predicates. A body admitted as
read-only cannot acquire writes through a later callee overwrite. Removing a referenced
function requires removal or replacement of its surviving callers first.

Schema declarations and removals are migration top-level statements. Conditional table,
field, event, function and index declarations fail admission because they cannot prove
which stored body survives. ALTER TABLE permission changes replace the stored permission
profile while preserving its other deferred properties.

Native object parameters, explicitly typed object locals and iterators over explicitly
typed arrays of objects permit single-field extraction. Other locals need a positive
`type::is_object` guard. Owned field expressions admit single-field `$this` access and
scalar `$value` methods whose declared kind proves the receiver. The admitted methods
are length, array/set distinct and array/set all with a fully inspected read-only
closure. Event before/after rows support single-field extraction. Record traversal,
unproven collection methods and computed targets fail admission.


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
own installed checks. Current kernel and optional schemas live in their owning lanes.
Source and native qualification do not establish installed image or Job qualification;
that acceptance is tracked in the active contract plan.

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
The composition hashes its complete plan and runtime account name under
`veoveo.ai/installation-preparation/v1` into a preparation identity. The plan binds
the compiled owner bodies and execution declarations. The generation is an installation-owned
positive integer, independent of chart release metadata. A changed preparation identity
requires a higher generation; the same generation with conflicting identity fails.

Prepared execution initializes the reserved infrastructure, then claims the generation
before runtime-account provisioning and owner-lane execution. A delayed older preparer cannot rotate the newer account.
Completion rotates a validated database editor and writes the completion marker in one
owned native transaction. Already completed keys do not repeat rotation. The API accepts
only credentials for this fixed operation and has no arbitrary-SQL execution hook.
A live Tokio runtime supports cancellation cleanup as described above.

Installation lane execution compares the completed preparation key inside every
header and migration transaction. A current lane also verifies its key before returning.
Absent or unfinished preparation may wait within the caller's deadline; malformed
markers, conflicting identities and superseding generations fail admission.

## Kernel SQL APIs

`ModuleOwnership` checks owner identity, layer and object claims without an execution
host. A runtime contribution binds its table against this declaration; that check
neither installs the table nor proves registry agreement. `ModuleSetup` uses the same
value and adds composition-supplied execution, dependencies and lane history.

A kernel owner exports an exact versioned `KernelSqlApi`. The default effect profile
is read-only. An explicit `OwnedUpdate` profile names one owned table and its checked
top-level fields. The declaration names the
introducing migration, parameter and return types, owned read tables and complete
function definition. Supported signatures use strings, booleans, objects, specific
record tables, arrays and optional values. The private parser requires the definition
once in its introducing entry and compares its full AST and signature. Declaring an
export does not waive inspection of its body. Read-only leaves reject writes. Updating
leaves admit only `UPDATE SET` assignments to their listed fields; whole-record changes,
nested targets, creation, deletion and DDL fail admission. Both profiles reject custom
calls, scripts, foreign reads and nondeterministic built-ins. A typed record parameter
may supply a SELECT or UPDATE target inside a leaf when its table passes the declared
read or update profile. Ordinary owner SQL cannot use a parameter as a dynamic target.
Updating leaves run in their caller's transaction; they do not commit independently.
Permission predicates and schema comments are read-only contexts in every owner. They
reject direct mutation and calls to updating exports, even on an updating leaf.

The visitor permits a single field on a native object parameter or an explicitly
typed object local. Other locals require a positive branch of
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
argument and rejects argument-side mutation, including calls to updating exports. Private functions and undeclared versions
cannot become callable through a namespace prefix. Runtime services bind admitted
caller scope through their owner contracts; the SQL API does not authenticate an
arbitrary supplied scope.

Index columns have a separate schema proof. Every intermediate in a multipart column
must have a preceding `object` or `option<object>` field declaration on that table.
The visitor remembers these declarations across the owner's lane entries. Executable
queries still use the guarded local-object profile above. Table, field and index
shape definitions, removals and alterations must be migration top-level statements;
conditional blocks and deferred function/event bodies cannot grant or erase proof.
Conditional field and index definitions cannot certify the stored shape and fail
admission. Field/table
removals, redefinitions and wildcard declarations invalidate the relevant proof;
existing dependent indexes must be removed before a shape change. Redefining a parent
object cannot silently certify its previously declared descendants.
