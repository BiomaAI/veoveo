# Shared Declaration Macros

This crate owns compile-time expansion for closed domain vocabularies and embedded
UTF-8 documents. `veoveo-types` re-exports these macros alongside the ordinary traits
that their generated code implements. Each consuming library owns its enum variants.

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| Rust 2024, toolchain 1.98.1 | Procedural derives for nongeneric unit enums and a function-like document macro |
| Serde 1.0.229 | Unit-enum serialization for ordinary vocabularies; existing string serialization for scopes |
| JSON Schema 2020-12, schemars 1.2.2 | Owner metadata, module-qualified schema identities and the existing enum schemas |
| OAuth 2.0, RFC 6749 section 3.3 | Code-owned scope-token syntax; dynamic installation scopes keep `ScopeName` admission |
| Veoveo Task operation names | The `TaskTypeName` lexical profile, with distinct spellings |
| SurrealDB Rust SDK 3.3.0 | Optional consumer-only `SurrealValue` delegation preserves the owner's literal-string mapping |
| SHA-256, FIPS 180-4 | Digest of the embedded document's original UTF-8 bytes |

## Vocabulary

`#[derive(Vocabulary)]` accepts unit variants without generics or explicit numeric
discriminants. A variant uses snake_case unless `#[vocabulary(rename = "…")]`
specifies its spelling. Empty and duplicate spellings fail compilation. The derive
implements the public `Vocabulary` trait, `ALL`, `as_str`, `Display`, `FromStr`,
Serde and JSON Schema. Owners continue to choose other standard derives themselves.

`#[vocabulary(scope)]` also implements `ScopeDefinition`, conversion from a checked
`ScopeName`, and conversion back to that name. It applies the ordinary foundation's
scope-token helper during constant evaluation. Empty scope enums admit no values
and emit the `false` schema. Existing scope schemas do not gain documentation fields.

`#[vocabulary(task_type)]` implements `TaskTypeDefinition`; constant evaluation calls
`TaskTypeName::from_static` for every spelling. A Task vocabulary must contain an
operation. Scope and Task profiles cannot be combined.

Ordinary vocabulary serialization delegates to an internal Serde-derived enum with
the owner's enum name, spellings and variant order. This preserves Serde's unit
variant token and ordinal, including nonhuman formats. Schema generation delegates
to schemars with owner documentation and schema attributes. The helper keeps the
owner's schema name and module-qualified identity. Scope serialization delegates to
its existing checked-name contract instead.

`#[vocabulary(surreal)]` emits a private SDK-derived enum into the consuming crate
and delegates `SurrealValue` to it. The helper's literal annotations come from the
same spelling declaration used by every other surface. Neither this crate nor
`veoveo-types` depends on SurrealDB. The hook requires the consumer's existing
`surrealdb` dependency; unsupported SDK combinations fail compilation.

## Embedded Documents

`embedded_document!("relative/path.md")` reads under the consumer's manifest
directory, validates UTF-8 and hashes the original bytes during compilation. It
emits `include_str!` alongside the digest, so rustc tracks document changes.
`server_docs!` stays in `mcp/contract` because its declaration selects the calling
server's documents. Document content and discovery remain owned by their consumers.

## Dependencies And Qualification

The macro implementation reuses the existing locked syn 3.0.4, quote 1.0.47,
proc-macro2 1.0.107 and sha2 0.11.0 implementations. The crates.io registry reports syn 3.0.6 as stable; 3.0.4 preserves the parser
already used by the document macro while this batch changes ownership and derives.
A parser upgrade requires separate qualification of diagnostics and expansion behavior.
quote and proc-macro2 match the registry's stable releases. These workspace pins are
exact and do not change the resolved implementations.
No additional third-party dependency implements these declarations.

Foundation tests qualify independent trait implementations, explicit spellings,
empty scopes, invalid declarations and same-named owners. Audit tests compare every
migrated enum against its previous Serde/schemars declaration and verify unit-variant
ordinals. Store and Time compare SDK kinds, conversion, rejection and JSON values
against their previous declarations. Contract-only consumers qualify dependency
isolation. The active [plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#phase-0-core-macros-and-shared-building-blocks)
tracks the remaining Id, ResourceAddress, checked-model and cursor concerns.
