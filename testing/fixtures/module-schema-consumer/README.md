# Module Schema Consumer

This independent Rust workspace composes all seventeen owner declarations through
`schema` features with default features disabled. Native tests pin the current
166 table names, fourteen functions and two analyzers to their target owners,
check the declared dependency order, and qualify optional selection through the
shared registry. The generated plan fixture is compared with those actual owner
exports. These checks do not approve the current mixed migration SQL.

The gateway source producer generates the plan fixtures from the selection inputs.
Their reference image binding qualifies source rendering; it does not attest that the
previously published reference image contains the new command. Locked installation
compilation runs the exact newly published image. Tasks and Optimization have additive version-zero lanes; the other owner lanes are empty.

Run the consumer independently of workspace feature unification:

```sh
CARGO_TARGET_DIR=target cargo test --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml
cargo tree --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml --edges normal,build
```

The normal/build graph consists of this fixture, the eleven schema-declaring crates,
and the optional checked plan serialization graph. `veoveo-modules` default declarations
still have no dependencies. Contract libraries, database drivers, parser/runner dependencies,
MCP hosting and runtimes must not enter through a schema dependency. JSON decoding reuses
qualified `serde_json =1.0.151`; generated plans preserve their camelCase wire profile
and decimal-string generation without any HTTP schema or runtime dependency.
