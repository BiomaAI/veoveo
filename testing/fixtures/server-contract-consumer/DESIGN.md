# Independent Server Contract Consumer

## Standards And Protocols

Cargo resolver 3 selects Artifact, Computers, Speech, Frames, Timeseries and Media libraries with default
features disabled and `contract` enabled. The fixture calls their public Rust APIs;
it implements no transport or installed service protocol.

## Ownership And Qualification

This separate workspace prevents service test features from hiding a dependency leak.
The consumer constructs requests with Artifact identity shared across Speech and the
Artifact MCP library, imports the Computers public projection, and builds Frames and
Timeseries resource addresses from their owner types. Media consumers construct model
requests and Artifact addresses through the same isolated contract. Its graph test
rejects Veoveo implementation crates, RMCP, databases and asynchronous runtimes.

Run the isolated checks with one shared build directory:

```sh
cargo test --locked --offline --manifest-path testing/fixtures/server-contract-consumer/Cargo.toml --target-dir target
```

The checks require the qualified Rust toolchain and fetched dependencies. They perform
no network, service, identity or GPU operations. Cargo owns compilation and process
cleanup. Test execution is finite and creates no installation fixtures. Service and
GPU acceptance belong to the owning harnesses. Recheck each runtime configuration
separately when changing a server feature gate.
