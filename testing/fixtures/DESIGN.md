# Shared Native Test Fixtures

## Standards And Protocols

| Boundary | Profile |
|---|---|
| SurrealDB `3.2.4` | Existing qualified server image pinned by OCI digest, WebSocket Rust client, complete platform migrations |
| Docker Engine CLI | Local disposable container lifecycle; loopback-only ephemeral port; no installed volumes or credentials |
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
