# Shared Declaration Macros

This crate owns compile-time expansion for owner-admitted identities, typed resource routes, closed domain
vocabularies and embedded UTF-8 documents. `veoveo-types` re-exports these macros alongside the ordinary traits
that their generated code implements. Each consuming library owns its domain declarations and validation.

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| Rust 2024, toolchain 1.99.0 | Attribute declarations for nongeneric identity newtypes and resource structs/enums, a unit-vocabulary derive, and a function-like document macro |
| Serde 1.0.229 | Unit-enum serialization for ordinary vocabularies and string serialization for scopes; identity and address owners declare their serialization profile |
| JSON Schema 2020-12, schemars 1.2.2 | Owner metadata and existing vocabulary schemas; identity and address profiles select generated metadata or an owner schema |
| RFC 3986 and WHATWG URL resource components | Resource routes delegate concrete parsing and encoding to the foundational URI profile; network adapters keep their own profiles |
| RFC 6570 URI Templates | Resource declarations support literal scheme/authority, literal or scalar path segments, one final `{+tail}`, and a final optional `{?query,...}` expression; the attribute does not implement all template operators |
| OAuth 2.0, RFC 6749 section 3.3 | Code-owned scope-token syntax; dynamic installation scopes keep `ScopeName` admission |
| Veoveo Task operation names | The `TaskTypeName` lexical profile, with distinct spellings |
| SurrealDB Rust SDK 3.3.0 | Optional consumer-only `SurrealValue` delegation preserves the owner's literal-string mapping |
| SHA-256, FIPS 180-4 | Digest of the embedded document's original UTF-8 bytes |

## Id

`#[veoveo_types::id(...)]` accepts a nongeneric single-field tuple newtype.
The `text`, `hex`, `prefixed` and `uuid` forms select shared mechanics; an ordinary
owner `IdProfile` supplies its error, admission, generation and wire/schema policy.
Prefixes stay beside each concrete declaration. A family can share its profile
without repeating validators, standard derives or serialization implementations.

```rust
#[veoveo_types::id(prefixed(MapIds, "dataset-"), fresh, stable)]
pub struct DatasetId(String);
```

`parse` and `FromStr` apply owner admission. String IDs also expose `as_str`,
`AsRef<str>` and checked String conversions. UUID-backed IDs expose a copied
`as_uuid`; shared `UuidIdentity` and `StableKeyIdentity` capabilities support generic
consumers. The optional `fresh` capability supplies `new` and `Default`; `stable`
uses the owner's UUIDv5 namespace. Generation and admission are separate policies:
a v7 generator does not imply that the owner rejects other UUID versions.
Generated values pass admission before an ID is returned.

Text profiles call complete owner validators. Shared UUID and hex helpers handle
version, variant, spelling, length and alphabet policy. Error context can vary per
declaration without matching Rust type names at runtime. The profile contains no
server registry, and independent libraries may implement `Identity` directly.

Profiles select String or inner-UUID Serde behavior, including binary formats.
Generated schema helpers preserve declaration metadata; a schema identity override
changes only identity. An owner callback supplies genuinely different schema bodies.
Standard derives come from the attribute. Unrelated derives remain available, while
duplicate generated derives and incompatible capabilities fail compilation.
Identity and address frontends share the standard-derive conflict checker in
`src/derive_policy.rs`; each frontend supplies its additional reserved derives.

Secret declarations keep deliberate text exposure and owner-redacted formatting.
Their validators sanitize returned errors; these mechanics provide no zeroization.
`custom` delegates unusual storage or admission to explicit owner hooks through the
same emitter. Nonidentity labels and quantities stay ordinary checked newtypes.

`const_uuid` is available only when the owner admits arbitrary UUID values. The
consumer-only `surreal = "table"` capability uses that profile for existing native
UUID record IDs and emits SDK delegation in the consumer. Neither the macro crate
nor `veoveo-types` depends on the database SDK.

## ResourceAddress

`#[veoveo_types::resource_address(...)]` implements the public `ResourceAddress`
trait for nongeneric structs and enums. Each declaration selects an ordinary owner
`ResourceProfile` and its representation:

| Form | Representation and defaults |
|---|---|
| `cached_checked(Profile)` | Private typed fields and String or ResourceUri cache; checked constructor and ordered traits |
| `cached(Profile)` | Private typed fields and cache; constructor for already-admitted components |
| `components(Profile)` | Typed tuple or named fields; direct construction from admitted components |
| `routes(Profile)` | Enum with a template on each variant; owner-defined variant construction |
| `custom(...)` | Explicit mechanics for representations outside these forms |

The parser admits owner, variant and field options once into a shared declaration.
Both compact forms and custom declarations use that model. Complete route admission
precedes normalization into typed field plans for cache, component role, constructor
argument and accessor mode. One field walk emits parsing, encoded patterns,
constructors and accessors from those plans. Optional query values and optional scalar
storage with an owner codec have distinct roles. The same component fragments build
addresses from parsed fields and constructor arguments; compact forms preserve their
cache spelling profile and custom hooks preserve qualified String caches.

A struct declares `template = "…"` on the attribute. The profile maps route errors;
URI errors use that mapping unless the owner overrides it. Every dynamic component maps
to one concrete field. Named fields use their Rust spelling unless
`#[resource(variable = "…")]` specifies the template variable. Tuple fields require
an explicit variable. Templates with overlapping component shapes fail expansion,
including a literal route that intersects a scalar sibling or two routes distinguished
only by query names. Query errors and failed field admission cannot select another
variant.

Scalar fields use `IdentityResourceCodec` by default. Owners may declare
`codec = OwnerCodec` and `error = owner_mapping` on a field to preserve version,
configuration or cursor admission. Optional queries require `Option<T>`. A final
`{+variable}` requires `tail` and an explicit `ResourceTailCodec<T>`; its owner admits
the already-decoded components and chooses their segmentation when building.
Encoding and parsing use `ResourceRoute` in the foundation. These hooks do not change
opaque cursor payloads or codecs.

The optional type-level `input = owner_fn` hook receives `ResourceUriParts` after shape
selection and component capture, before typed field admission. Checked construction
reparses the built URI through that hook. Variants cannot override it; duplicate hooks
and unsupported attribute placements fail expansion. Owners use it for raw-wire
admission rules whose error profile precedes decoded identifier checks.

The type-level `validate` hook checks relationships on each admitted value and before
output. Variant hooks are limited to `template`, `route_error`, `canonical` and
`allow_empty_query`. They inherit the owner's mapping and canonical policy unless
overridden. Unsupported placements fail compilation. An undeclared query on a
non-query route produces `UnexpectedQuery`; malformed declared query data produces
`Query`. Owners map these categories to their existing errors.

Canonical parsing compares the input with a URI rebuilt from admitted typed fields.
`canonical = false` admits component aliases under the owner's codecs. Query-order
and empty-query policy stay explicit. A private `#[resource(cache)]` field may hold
ResourceUri or String; output preserves that stored wire spelling. Generated
`resource_from_parts` takes owned concrete fields, builds through the descriptor and
reapplies parsing, relationship checks and canonical policy before returning an owner
value. The private `resource_build_uri` helper takes borrowed fields and optional borrowed
query values for owner-internal encoding. Checked constructors return admitted values through `resource_from_parts`.
Component wrappers can construct directly from typed fields when no input,
relationship or additional admission hook must run. The macro rejects a direct
wrapper that would bypass one of those hooks. Manual owner constructors must
maintain the cache and typed-field invariant. Every field of a cached struct must be private to its owner module.
Cached enum variants fail expansion because their components cannot be externally private.
Cached owners also ensure that component types and accessors cannot mutate admitted
values through interior mutability and invalidate their wire agreement. Field privacy
prevents direct external assignment; it does not prove arbitrary codec types immutable.

An explicit `accessor = method_name` generates a borrowed getter. Option fields return
`Option<&T>`; `copy_accessor` requests a getter returning the field by value and requires
its Rust type to support that operation. Explicit clone or consuming getters handle non-Copy fields. Enum accessors and
domain convenience methods stay owner-defined. Forms generate ordinary formatting,
parsing and construction; declarations specify only deviations they require.

`RESOURCE_ROUTES` drives parsing and building. `RESOURCE_TEMPLATES` comes from the
same parsed declarations. Structs also expose `RESOURCE_ROOT` and `RESOURCE_TEMPLATE`; enums expose named
`RESOURCE_TEMPLATE_<VARIANT>` constants in upper snake case. Existing discovery constants
can alias these strings. Manually declared descriptors expose their template only
through `discovery_template`, which checks agreement with their component route.

Wire-enabled forms generate Serde through checked String conversions. Schema
selection supports declaration metadata, String schemas and owner callbacks.
`schema = owner` requires a schema-bearing profile at compile time. Generated
helpers retain schema attributes; the emitted owner does not retain helper-only
attributes. Duplicate or conflicting options fail expansion. A descriptor's `wire_pattern` describes encoded component structure,
including percent-encoded delimiters and declared query policies. It is a permissive
schema pattern rather than a pasted domain-ID regex. An owner must qualify any schema
change separately from mechanical adoption. Ordinary `ResourceAddress` implementations
remain available for composed-domain and network adapters.

`resource_wire_patterns(ResourcePatternSpelling)` composes
optional codec `encoded_pattern` hooks with shared route mechanics. A hook returns
`ResourceEncodedPattern { pattern, allows_empty }` for the requested encoding and
canonical or admitted spelling. Owners declare empty query and tail admission explicitly.
The attribute does not rewrite ID regexes or change an owner's selected JsonSchema policy.

## Vocabulary

`#[derive(Vocabulary)]` accepts unit variants without generics or explicit numeric
discriminants. A variant uses snake_case unless `#[vocabulary(rename = "…")]`
specifies its spelling. Empty and duplicate spellings fail compilation. The derive
implements the public `Vocabulary` trait, `ALL`, `as_str`, `Display`, `FromStr`,
Serde and JSON Schema. Owners continue to choose other standard derives themselves.
Generated return types use fully qualified `core::result::Result`; an owner's local
error alias cannot change the trait signatures.

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

## Macro Catalog And Enforcement

`cargo xtask enforce rust` checks every tracked Rust source against this catalog
before its native checks. `--macros-only` runs the same catalog check without the
workspace suites. The parser visits file, module and function bodies, including
lexically present cfg branches. Comments, string fixtures and macro invocation tokens
are opaque. Invalid Rust or an unsupported declaration fails the check. Git's index
selects source paths, including newly staged files, and excludes build outputs.

| Definition | Kind and path | Owner reason |
|---|---|---|
| `id`, `resource_address` | Attributes in `platform/macros/src/lib.rs` | Compact owner declarations delegate to ordinary foundation traits |
| `Vocabulary` | Derive in `platform/macros/src/lib.rs` | One owner spelling declaration supplies vocabulary traits |
| `embedded_document` | Function-like proc macro in `platform/macros/src/lib.rs` | Compile-time UTF-8 embedding and hashing |
| `server_docs` | Declarative macro in `mcp/contract/src/docs.rs` | Document selection must expand in the calling server crate |
| `impl_scoped_redap_service` | Declarative macro in `servers/recording-mcp/src/playback.rs` | One owner authorization policy implements the generated third-party gRPC service trait |

The checker matches path, public macro name, Rust entrypoint name and kind, and
requires one definition for every catalog entry. Adding an entry requires owner
rationale and parser tests. The policy lives in
[`tools/xtask/src/commands/macro_policy.rs`](../../tools/xtask/src/commands/macro_policy.rs).
Production scalar and error helpers use ordinary owner functions and nominal types;
test repetition uses generic functions.

## Dependencies And Qualification

The macro implementation uses locked syn 3.0.6, quote 1.0.47, proc-macro2 1.0.107
and sha2 0.11.0. The macro crate enables syn's `full` expression parser for owner
hooks. The source-policy checker uses the same pin with `full`, `visit` and
`parsing`; independent consumers do not depend on feature unification from a runtime.
Owner errors use the exact workspace thiserror 2.0.21 pin. No additional third-party dependency
implements these declarations.

Foundation tests qualify independent trait implementations, explicit spellings,
empty scopes, invalid declarations and same-named owners. Audit tests compare every
migrated enum against its previous Serde/schemars declaration and verify unit-variant
ordinals. Store and Time compare SDK kinds, conversion, rejection and JSON values
against their previous declarations. Contract-only consumers qualify dependency
isolation. The active [plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#phase-0-core-macros-and-shared-building-blocks)
tracks owner qualification and the remaining implementation phases.
