# Shared Native Test Fixtures

## Standards And Protocols

| Boundary | Profile |
|---|---|
| SurrealDB `3.2.4` | Existing qualified server image pinned by OCI digest, WebSocket Rust client, complete platform migrations |
| Docker Engine CLI | Tokio child processes with cancellation and deadlines; local disposable container lifecycle; loopback-only ephemeral port; no installed volumes or credentials |
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
