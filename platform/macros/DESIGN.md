# Shared Declaration Macros

This crate owns compile-time expansion for owner-admitted identities, closed domain
vocabularies and embedded UTF-8 documents. `veoveo-types` re-exports these macros alongside the ordinary traits
that their generated code implements. Each consuming library owns its domain declarations and validation.

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| Rust 2024, toolchain 1.98.1 | Procedural derives for nongeneric tuple newtypes and unit enums, plus a function-like document macro |
| Serde 1.0.229 | Unit-enum serialization for ordinary vocabularies and string serialization for scopes; identity owners declare string conversion or inner-value serialization |
| JSON Schema 2020-12, schemars 1.2.2 | Owner metadata and existing vocabulary schemas; identity owners keep their standard derives or supply an explicit schema hook |
| OAuth 2.0, RFC 6749 section 3.3 | Code-owned scope-token syntax; dynamic installation scopes keep `ScopeName` admission |
| Veoveo Task operation names | The `TaskTypeName` lexical profile, with distinct spellings |
| SurrealDB Rust SDK 3.3.0 | Optional consumer-only `SurrealValue` delegation preserves the owner's literal-string mapping |
| SHA-256, FIPS 180-4 | Digest of the embedded document's original UTF-8 bytes |

## Id

`#[derive(Id)]` accepts a nongeneric single-field tuple newtype. The owner declares
its error type and exactly one admission function. `admit = owner_fn` calls
`fn(&str) -> Result<Inner, Error>`. For a String wrapper, `string` with
`validate = owner_fn` calls `fn(&str) -> Result<(), Error>` and keeps an admitted
owned String without copying it again. The derive implements the public `Identity`
trait and `FromStr`; the foundation contains no registry of ID forms or domains.

The `string` hook supplies a checked `new`, `as_str`, `AsRef<str>`,
`TryFrom<String>` and conversion into String. `constructor = parse` chooses a
parsing constructor name. An inner-value wrapper may use `constructor` separately
and opt into `wire_string` conversions through `Identity`. Those conversions do not
choose a Serde representation. Each owner declares its Serde derives or implementations,
including whether a binary serializer sees a string or the inner UUID bytes.

`generate = owner_fn` supplies `new` and `Default`. It applies the declared text
projection to the generated inner value and reapplies admission before returning the
wrapper. String validators check the owned generated value without copying it. A generator that
produces an inadmissible value panics with a fixed diagnostic. Generation rules,
UUID versions and namespaces belong to the owner. A String wrapper with generation
must name a separate parsing constructor.

`identity_text` returns `Cow<str>` through the owner’s `text` hook when supplied.
String wrappers otherwise borrow their inner text; other wrappers format the inner
value. Display writes borrowed String text without padding, uses an explicit text hook when declared,
and otherwise delegates to the inner formatter. `no_display` leaves
Display to the owner, and the derive does not generate Debug. Secret owners explicitly
expose text and keep redacted formatters. Gateway refresh-token admission also
sanitizes its returned error so rejected bearer material cannot enter diagnostics.
These mechanics make no zeroization guarantee.

Owners normally retain their standard Schemars declaration or manual implementation.
An optional `schema = owner_fn` calls
`fn(&mut SchemaGenerator) -> Schema` and implements `JsonSchema` with the type’s name.
`schema_inline` preserves an owner’s inline profile. This hook keeps Schemars' default
schema identity; it does not impose a module-qualified identity or invent metadata,
patterns or limits. Admission, serialized forms and schema precision remain owner
choices. Checked display labels and other values that do not identify an entity stay
ordinary newtypes.

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
exact and do not change the resolved implementations. The macro crate explicitly enables
syn’s `full` expression parser for owner closure hooks; independent consumers do not
rely on another dependency to supply that feature.
No additional third-party dependency implements these declarations.

Foundation tests qualify independent trait implementations, explicit spellings,
empty scopes, invalid declarations and same-named owners. Audit tests compare every
migrated enum against its previous Serde/schemars declaration and verify unit-variant
ordinals. Store and Time compare SDK kinds, conversion, rejection and JSON values
against their previous declarations. Contract-only consumers qualify dependency
isolation. The active [plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#phase-0-core-macros-and-shared-building-blocks)
tracks owner qualification and the ResourceAddress, checked-model and cursor concerns.
