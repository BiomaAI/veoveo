# Shared Native Test Fixtures

## Standards And Protocols

| Boundary | Profile |
|---|---|
| SurrealDB `3.3.0` | Existing qualified server image pinned by OCI digest, WebSocket Rust client, complete platform migrations |
| Docker Engine CLI | Tokio child processes with cancellation and deadlines; local disposable container lifecycle; loopback-only ephemeral port; no installed volumes or credentials |
| Linux cgroup v2 | Optional storage measurements read the owned container's `io.stat` write counters, keyed by block-device major/minor |
| Rust test harness | Source module reused by owning integration tests; no separate smoke process or assertion framework |

`store.rs` owns one disposable database and two independent database-editor clients.
It generates ephemeral credentials, applies the platform catalog as fixture admin,
and transfers cleanup ownership only after setup succeeds. Dropping the fixture force-removes
its named container, including the container's writable layer. `new` selects memory;
`with_backend(StoreBackend::RocksDb)` selects an owned `/tmp` database with a 64 MiB
block cache and two 16 MiB write buffers. Both profiles use the same image, migrations
and database-editor clients. RocksDB qualification uses the installation's storage
engine without creating a host volume. The fixture owns no data that needs a graceful flush;
this avoids waiting for server shutdown while the fixture still holds client sockets. Computers admission and shared Task recovery use the same setup,
which avoids maintaining divergent copies of database lifecycle and cleanup code.
The fixture never connects to the installation database. It proves store behavior,
not public deployment or provider execution.

`with_backend_and_schema` applies an owning test's additional schema as fixture
admin before creating runtime clients. Its ten-second deadline covers that setup.
This permits sequence and schema measurements while keeping test operations on the
database-editor role used by services.

## Catalog Registration

`catalog_registry.rs` reuses the production registration source in
[`platform/gateway/catalog`](../../platform/gateway/catalog/DESIGN.md). Tests therefore
exercise the same kernel and owner declarations as installation binaries without
maintaining another registration list. Source reuse lets an owner test use its own
library types without a reverse package dependency on the installation recipe.

`registry()` clones one binding within the test executable. `fresh_registry()` creates
an independent binding for rejection tests: admitted handles and targets from one
registry cannot authorize requests through another. `catalog_admission.rs` supplies
the gateway admission wrapper over these declarations. Domain assertions stay in
the owning suites; the fixtures provide no policy decisions or installation credentials.

## Lifecycle Bounds

`store/container.rs` owns Docker creation, startup, port admission and removal. Each
startup command has 30 seconds; a timed-out child has two seconds to die and be
reaped. Docker stdout is limited to 4096 bytes with a two-second read deadline.
Diagnostics identify the stage, fixture name and exit code or I/O category, and omit
arguments, environment and child output. Test-only Tokio process/I/O features keep
this lifecycle out of contract consumers' dependency graphs.

Cleanup ownership starts before `docker create`. Cancellation kills the CLI child.
If the daemon has not confirmed creation, cleanup probes only the allocated name
for up to 120 seconds, then removes the observed container. An expired reconciliation
reports an unknown creation outcome and fails the test; an early removal of a missing
name cannot count as cleanup. Failure to spawn the CLI proves that creation was never
dispatched and requires no reconciliation. Creation and startup are separate commands,
so a late creation response cannot also start a workload. The guard transfers to
`TestDb` after setup succeeds. Drop runs removal on a separate thread with its own
Tokio runtime, allowing the same cleanup on current-thread tests and during runtime
shutdown. Removal has ten seconds plus the child-reaping/output bounds. Cleanup
failure fails the test and names the owned resource for inspection; during panic
unwinding it prints the same diagnostic without a second panic.

Migration/readiness has 60 seconds. Runtime credential creation and each client
connection have ten seconds. Domain tests can put setup before their SQL assertion
deadline, as Reason's index tests do. The fixture uses the cached digest with
`--pull never`, so missing images fail without fetching storage during a test.

`platform/store/tests/fixture_lifecycle.rs` injects CLI failures, hanging commands,
caller cancellation, late daemon creation, unresolved creation, missing CLI, malformed
published ports and cleanup timeout. Its Linux shell
stand-ins hold no database data. Native Store and owner suites exercise the same
lifecycle against the pinned database image; the command tests alone prove no SQL
or installed behavior.

## Storage Measurements

`store/io.rs` reads write bytes and operation counts from the cgroup of the PID
reported by Docker for the owned container. The measurement requires a local Unix
Docker socket, a Linux cgroup-v2 hierarchy and readable I/O counters. It preserves
separate block-device counters because summing stacked devices can double-count
writes. [Linux documents the accounting profile](https://docs.kernel.org/admin-guide/cgroup-v2.html).

The owning workload invokes `compact` before and after its timed writes. The helper
connects with the isolated fixture-admin credentials and gives
[`ALTER DATABASE COMPACT`](https://surrealdb.com/docs/reference/query-language/statements/alter/database)
30 seconds to finish. Runtime writes and replay continue through database-editor
clients. A host `sync` followed by two seconds of stable counters drains kernel
writeback; that observation has a 30-second deadline. Compaction, writeback settling
and Docker inspection are outside the latency window.

The difference measures container-attributed block writes for the workload plus
requested compaction. It does not establish steady-state write amplification or
SSD NAND writes. Docker's `SizeRw` supplies the logical size of the writable layer,
including engine files. The fixture removes that layer after the observation.

## Input Admission

`tool_inputs.rs` shares pure owner-owned input fixtures between contract consumers
and authenticated hosted tests. It checks fixture integrity, decodes serialized
bytes, and mutates controlled objects, required fields and variant tags. Owners
select their actual request types and assert domain state; the helper owns no
transport, lifecycle, provider or domain catalog.
