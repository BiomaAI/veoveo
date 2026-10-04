# Independent Server Contract Consumer

## Standards And Protocols

Cargo resolver 3 selects all sixteen Rust MCP server libraries under `servers/` with
default features disabled and `contract` enabled. The fixture calls their public Rust APIs;
it implements no transport or installed service protocol.
JSON Schema Draft 2020-12 checks use jsonschema 0.51.0 with network and file retrieval
disabled, and serde_json 1.0.151. These exact test-only pins reuse the workspace
qualification profile. Its independent codec error uses thiserror 2.0.20, matching
the workspace-qualified static-error implementation rather than upgrading it
alongside error plumbing.
Its `knowledge` feature also consumes Map and Reason collection descriptors through
their lightweight knowledge features.

## Ownership And Qualification

This separate workspace prevents service test features from hiding a dependency leak.
The consumer constructs requests with Artifact identity shared across Speech and the
Artifact MCP library, constructs Computers identities and resource addresses, and builds Frames and
Timeseries resource addresses from their owner types. Map callers construct direct addresses with typed parent and member identities. Media consumers construct model
requests and Artifact addresses through the same isolated contract. Its graph test
rejects Veoveo implementation crates, RMCP, databases and asynchronous runtimes.
The shared `veoveo-macros` proc-macro crate supplies compile-time public declarations
and is an allowed foundation dependency. The runtime and database exclusions still
apply. An independently implemented component codec qualifies generated route patterns
against typed builders, admitted percent-encoding aliases, query values and tails with
a real JSON Schema validator. Nonempty and empty components have distinct profiles;
structural patterns do not claim domain relationships or duplicate-query rejection.
Scope checks consume every library's declaration, reject foreign names, and round-trip
all nonempty vocabularies. Recording producer permissions remain distinct from sealing.
The analytical consumer builds a Timeseries forecast from DuckDB's tabular source
profile and column types, checked forecast steps, and a nonempty training filter.
It also promotes that source into DuckDB's complete input type.
Map handoff construction and Computers grant addresses use their owner libraries'
checked builders without pulling either host runtime into the consumer.

Run the isolated checks with one shared build directory:

```sh
cargo test --locked --offline --manifest-path testing/fixtures/server-contract-consumer/Cargo.toml --target-dir target
cargo test --locked --offline --manifest-path testing/fixtures/server-contract-consumer/Cargo.toml --target-dir target --features knowledge
```

The checks require the qualified Rust toolchain and fetched dependencies. They perform
no network, service, identity or GPU operations. Cargo owns compilation and process
cleanup. Test execution is finite and creates no installation fixtures. Service and
GPU acceptance belong to the owning harnesses. Recheck each runtime configuration
separately when changing a server feature gate.
