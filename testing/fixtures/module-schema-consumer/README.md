# Module Schema Consumer

This independent Rust workspace composes all nineteen owner declarations through
`schema` features with default features disabled. Native tests pin the current
166 table names, 39 functions and two analyzers to their target owners,
check the declared dependency order, and qualify optional selection through the
shared registry. The generated plan fixture is compared with those actual owner
exports. Declaration checks do not replace complete SQL admission or native execution
qualification of each selected owner body.

The gateway source producer generates the plan fixtures from the selection inputs.
Their reference image binding qualifies source rendering; it does not attest that the
previously published reference image contains the new command. Locked installation
compilation runs the exact newly published image. Every registered owner has a
version-zero lane containing its current schema; kernel owners install their
declared Identity, Task, Gateway and Artifact SQL APIs.
Owner lookup tables belong to Map, UAV, Reason, Stream, Optimization and Media.

Run the consumer independently of workspace feature unification:

```sh
CARGO_TARGET_DIR=target cargo test --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml
cargo tree --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml --edges normal,build
```

The normal/build graph includes the owner declarations, checked plan serialization,
and the foundational `Vocabulary` derive with its Serde and Schemars support.
`veoveo-modules` default declarations have no dependencies. Domain contract libraries,
database drivers, parser/runner dependencies, MCP hosting and asynchronous runtimes
must not enter through a schema dependency. JSON decoding reuses qualified
`serde_json =1.0.151`; generated plans preserve their camelCase wire profile and
decimal-string generation.
