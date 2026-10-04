# Shared Declaration Macros

This crate owns compile-time expansion for owner-admitted identities, typed resource routes, closed domain
vocabularies and embedded UTF-8 documents. `veoveo-types` re-exports these macros alongside the ordinary traits
that their generated code implements. Each consuming library owns its domain declarations and validation.

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| Rust 2024, toolchain 1.99.0 | Procedural derives for nongeneric identity newtypes, resource structs/enums and unit vocabularies, plus a function-like document macro |
| Serde 1.0.229 | Unit-enum serialization for ordinary vocabularies and string serialization for scopes; identity and address owners declare their serialization profile |
| JSON Schema 2020-12, schemars 1.2.2 | Owner metadata and existing vocabulary schemas; identity and address owners keep their standard derives or supply an explicit schema hook |
| RFC 3986 and WHATWG URL resource components | Resource routes delegate concrete parsing and encoding to the foundational URI profile; network adapters keep their own profiles |
| RFC 6570 URI Templates | Resource declarations support literal scheme/authority, literal or scalar path segments, one final `{+tail}`, and a final optional `{?query,...}` expression; this derive does not implement all template operators |
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

## ResourceAddress

`#[derive(ResourceAddress)]` implements the existing public address trait for
nongeneric structs and enums. A struct declares `#[resource(template = "…",
error = OwnerError, route_error = owner_mapping)]`; an enum declares its error and
mapping on the type and a template on each variant. Every dynamic component maps
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
query values for owner-internal encoding. Public constructors return checked owner
values through `resource_from_parts`. Owners retain their
public constructors and delegate usable-value construction to the checked helper.
Manual owner constructors must maintain the cache and typed-field invariant. Every field of a cached struct must be private to its owner module.
Cached enum variants fail expansion because their components cannot be externally private.
Cached owners also ensure that component types and accessors cannot mutate admitted
values through interior mutability and invalidate their wire agreement. Field privacy
prevents direct external assignment; it does not prove arbitrary codec types immutable.

An explicit `accessor = method_name` generates a borrowed getter. Option fields return
`Option<&T>`; `copy_accessor` requests a getter returning the field by value and requires
its Rust type to support that operation. Existing owner getters can stay handwritten,
and enum accessors remain owner-defined. This preserves borrowed and copied API choices
instead of imposing one getter shape. Domain convenience methods and Display stay
with the owner.

`RESOURCE_ROUTES` drives parsing and building. `RESOURCE_TEMPLATES` comes from the
same parsed declarations. Structs also expose `RESOURCE_ROOT` and `RESOURCE_TEMPLATE`; enums expose named
`RESOURCE_TEMPLATE_<VARIANT>` constants in upper snake case. Existing discovery constants
can alias these strings. Manually declared descriptors expose their template only
through `discovery_template`, which checks agreement with their component route.

`wire` emits checked String conversions for an owner's existing Serde declaration.
Serde and Schemars derives remain owner choices. A `schema` hook can delegate an
existing manual schema, with optional `schema_inline`; it keeps Schemars' default
schema identity. A descriptor's `wire_pattern` describes encoded component structure,
including percent-encoded delimiters and declared query policies. It is a permissive
schema pattern rather than a pasted domain-ID regex. An owner must qualify any schema
change separately from mechanical adoption. Ordinary `ResourceAddress` implementations
remain available for composed-domain and network adapters.

`resource_wire_patterns(ResourcePatternSpelling)` composes
optional codec `encoded_pattern` hooks with shared route mechanics. A hook returns
`ResourceEncodedPattern { pattern, allows_empty }` for the requested encoding and
canonical or admitted spelling. Owners declare empty query and tail admission explicitly.
The derive does not rewrite ID regexes or change an owner's selected JsonSchema policy.

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

## Macro Catalog And Enforcement

`cargo xtask enforce rust` checks every tracked Rust source against this catalog
before its native checks. `--macros-only` runs the same catalog check without the
workspace suites. The parser visits file, module and function bodies, including
lexically present cfg branches. Comments, string fixtures and macro invocation tokens
are opaque. Invalid Rust or an unsupported declaration fails the check. Git's index
selects source paths, including newly staged files, and excludes build outputs.

| Definition | Kind and path | Owner reason |
|---|---|---|
| `Id`, `ResourceAddress`, `Vocabulary` | Derives in `platform/macros/src/lib.rs` | Shared nominal declaration mechanics delegate to ordinary foundation traits |
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
