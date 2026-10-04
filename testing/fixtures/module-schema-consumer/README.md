# Module Schema Consumer

This independent Rust workspace composes all sixteen owner declarations through
`schema` features with default features disabled. Native tests pin the current
166 table names, fourteen functions and two analyzers to their target owners,
check the declared dependency order, and qualify optional selection through the
shared registry. They do not approve current migration SQL or establish that a
lane execution command is implemented.

The logical `gateway` host and `module-migrate` arguments are a declaration fixture.
Command/image availability needs separate implementation and qualification. Every
owner's current lane is empty.

Run the consumer independently of workspace feature unification:

```sh
cargo test --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml
cargo tree --manifest-path testing/fixtures/module-schema-consumer/Cargo.toml --edges normal,build
```

The normal/build graph consists of this fixture, the ten schema-declaring crates,
and dependency-free `veoveo-modules`. Contract libraries, database drivers, MCP hosting
dependencies and runtimes must not enter through a schema dependency.
