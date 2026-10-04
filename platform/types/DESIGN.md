# Veoveo Foundational Types

## Standards And Protocols

| Standard or format | Implemented profile |
|---|---|
| JSON | Identifiers and resource references serialize as strings; deserialization applies their constructors. Access subjects use `kind` and `id`; invocation provenance uses `mode` with the required attribution fields. |
| JSON Schema 2020-12 | Schemars emits string schemas for identifiers and tagged unions for subjects and provenance; runtime validators apply the lexical rules below |
| [OAuth 2.0 scope tokens, RFC 6749 section 3.3](https://www.rfc-editor.org/rfc/rfc6749#section-3.3) | `Vocabulary` with the `scope` hook declarations follow the scope-token grammar. Dynamic `ScopeName` preserves the broader repository profile of nonempty text without whitespace or controls. Neither type establishes a grant. |
| Resource reference syntax | Lowercase URI scheme followed by `://` and a nonempty suffix without whitespace or controls; an opaque reference can include a completion template, and this validator does not establish full URI or template conformance |
| [WHATWG URL Standard](https://url.spec.whatwg.org/) | Concrete hierarchical address components use [`url` 2.5.8](https://docs.rs/url/2.5.8/url/). The profile rejects parser violations and normalization, requires an unescaped authority, and excludes credentials, ports, fragments, and unexpanded templates. This is a Veoveo resource profile, not support for every URI scheme. |
| [RFC 3986 URI syntax](https://www.rfc-editor.org/rfc/rfc3986) | `ResourceUri` uses the existing `iri-string` 0.7.14 parser for concrete ASCII absolute references with a lowercase scheme and authority syntax. An authority or path must be nonempty. It preserves spelling, admits network ports and fragments, and rejects malformed escapes and unexpanded templates. Domain routes apply the stricter component profile separately. |
| HTTPS URL profile | `HttpsUrl` uses the same URL parser for canonical ASCII HTTPS addresses without credentials or fragments. Ports and repeated query names are admitted, and input query bytes are preserved. Runtime network policy owns host and address authorization. |
| Percent encoding and form query encoding | [`percent-encoding` 2.3.2](https://docs.rs/percent-encoding/2.3.2/percent_encoding/) decodes UTF-8 components and encodes path characters left unescaped by the URL setter. URL query pairs use form semantics: `+` represents space and `%2B` represents plus. |
| [RFC 6570 URI Templates](https://www.rfc-editor.org/rfc/rfc6570) | `ResourceTemplateUri` uses [`iri-string` 0.7.14](https://docs.rs/crate/iri-string/0.7.14) for all four expression levels and expansion. The profile admits ASCII literals with a literal lowercase scheme, `://`, and a nonempty suffix. Local guards enforce prefix lengths `1..=9999` without leading zeroes and nonempty dotted variable-name components. Expanded results must also pass the concrete resource profile. |
| Rust extension interfaces | Public `Identity`, `AccessGrant`, `ScopeDefinition`, `TaskTypeDefinition`, `ResourceAddress` and `TaskResourceAddress` traits permit implementations in independent libraries |
| Task operation names | Veoveo names contain 1–128 ASCII bytes: a lowercase initial letter followed by lowercase letters, digits, dots, underscores or hyphens. Validation establishes syntax, not implementation or authority. |
| SHA-256 provenance strings | `Sha256Digest` serializes as `sha256:` followed by 64 lowercase hexadecimal digits. Explicit `sha256_hex` Serde adapters serve fields whose owner declares bare lowercase hex. The type validates supplied digests and performs no hashing. |
| Native Task UUIDs, RFC 9562 | `TaskId` generates UUIDv7 and preserves the UUID parser and Serde profile from `uuid` 1.25.0; parsing does not establish a version, Task existence, or authority |

## Ownership And Dependencies

`veoveo-types` owns `ScopeName`, `ResourceScheme`, `ResourceUri`, `IdentifierError`,
`Sha256Digest`, `Sha256DigestError`, `HttpsUrl` and `HttpsUrlError`. A digest can be constructed from a checked
32-byte SHA-256 output without parsing text; the type performs no hashing. `ResourceUriParts`, `ResourceUriBuilder`,
`UriAuthority`, `UriSegment`, and `ResourceUriError` implement concrete component handling.
`ResourceUriBuilder::from_parts` extends checked addresses while preserving their
existing query pairs and rejecting duplicate added names.
`ResourceSelector`, `ResourceUriPrefix`, `ResourceUriTemplate` and `ResourceSelection`
own lexical URI selection below protocol and persistence adapters. `ResourceSelection`
intersects an owning scheme with the profile's selectors; constructing it confers no
permission. The policy evaluator supplies authorized selections.

`ResourceTemplateUri` and `ResourceTemplateError` own template admission and expansion.
`expand_scalars` accepts a standard string map for scalar variables and delegates to
the same iri-string expansion engine. Consumers need no URI-library context for this
case. Names keep their RFC spelling, unreferenced entries are ignored, and absent
variables remain undefined. Domain builders still validate required IDs and parents.
Platform identity belongs here too: `PrincipalId`, `TenantId`, `WorkContextId`,
`DelegationId`, `GroupId`, `RoleId`, `DataLabelId`, `PolicyVersion`, `TokenIssuer`
and `TokenSubject` are distinct
validated newtypes. `AccessSubject` identifies a principal or group. `InvocationMode`
and `InvocationProvenance` describe direct, delegated, or automated attribution.
`TaskId` identifies a native platform Task independently of its database record or MCP handle.
It depends on Serde, Schemars, URL, percent encoding, iri-string, UUID and Chrono date/time values. It contains no protocol transport, asynchronous
runtime, database client, provider integration, or server vocabulary.

Resolved invocation values also belong here: `InvocationAuthority`,
`WorkContextMembershipLevel`, `WorkContextOutputPolicy`, `WorkContextGrant` and
`AccessLevel`. They carry the selected Work Context, membership, output defaults and
provenance across protocols and domain contracts. Their string enums, field names,
defaults and order comparisons form one shared model. MCP owns Work Context
configuration and membership matching against authenticated principals; the gateway
resolves and signs authority, and existing policy owners enforce it. Possessing or
deserializing these values establishes no grant.

Identity syntax and attribution establish no authority. Authentication, policy
evaluation, Work Context membership, and access decisions stay with their existing
owners. Artifact metadata and coordinate vocabularies have domain owners; their use
by several servers does not make them foundational identity types.

`AccessGrant` exposes a domain grant's subject, access level and expiry to the shared
access evaluator. Artifact grants and Knowledge read grants implement it in their own
contracts. The evaluator needs no invented domain ID and does not import either grant
record shape. Chrono supplies date/time values with its clock feature disabled here.

A server library owns its scope enum and resource variants. Its `ScopeDefinition`
implementation maps a domain value to a validated name. Its `ResourceAddress`
implementation parses and serializes its domain address, including route and ID
validation. Neither trait requires a central registry or confers authority. The
policy owner compares requested names with current authenticated grants and performs
the other authorization checks.

`TaskTypeDefinition` gives a domain-owned enum its complete operation list and maps
validated names or incoming wire strings into that enum. `Vocabulary` with the `task_type` hook generates
the enum, list and names from one declaration. Invalid names and duplicate spellings
fail constant evaluation. Handlers match the parsed enum exhaustively; adding a variant
requires implementing its dispatch. Shared crates store `TaskTypeName` without knowing
the domain variants. Constructors and Serde validate names, and JSON preserves their
string spelling. TaskRuntime and Store carry this type through admission and snapshots;
the database adapter wraps its Serde representation without adding a database dependency
here. Name validation does not register a handler or authorize execution.

`TaskResourceAddress` extends a domain resource address with its backing `TaskId`.
Owners implement it only for routes whose contents follow one native Task. The
Task runtime uses that relationship to share an authorized observation source across
Task status and resource notifications. This trait supplies identity, not permission;
domain admission and current SQL owner checks still apply.

`mcp/contract` consumes these types for protocol descriptors, identity, policy, and
discovery. Consumers import the foundational types directly from this crate. It
provides no server-specific enums or blanket implementation that makes an arbitrary
name or reference a domain contract.

## Identity Mechanics

The public `Identity` trait in `src/id.rs` exposes owner admission through
`parse_identity` and deliberate text exposure through `identity_text`. Its associated
error type belongs to the owner. An independent library can implement the trait
without a derive or an edit to the foundation. Raw String has no blanket `Identity`
implementation. Admission establishes syntax; authentication and domain policy
establish authority.

The re-exported `Id` derive calls these ordinary mechanics. Owner functions choose
UUID versions, accepted aliases, text limits, generation and stable-key namespaces.
The String validation hook preserves the owned input allocation. Optional parser,
wire-conversion, generation and schema hooks remove plumbing while leaving these
rules with their owner. Generated values pass owner admission before construction.
The [macro design](../macros/DESIGN.md#id) defines the hook interface.

Serde and Schemars declarations belong to each wrapper. A transparent UUID wrapper
keeps UUID's binary behavior, while an owner-declared String conversion keeps string
serialization in binary formats too. Schema derives retain their metadata and identity;
the optional owner schema hook retains its declared shape and Schemars' default
identity. The shared mechanics impose no universal UUID or schema profile.

`identity_text` can expose secret material even when Display and Debug redact it.
Owners must make that exposure deliberate and sanitize errors at secret admission.
Gateway refresh tokens preserve their explicit text and wire access while replacing
rejected input in `IdentifierError` with `[REDACTED]`. The derive adds no zeroization.
Nonidentity display metadata, such as `PrincipalDisplayName`, uses an ordinary checked
newtype. Database adapters and SDK derives stay in their consumers; the foundation
knows neither Store tables nor server-owned IDs.

## Wire Behavior

Claim identities (`PrincipalId`, `TenantId`, `DelegationId`, `GroupId`, and `RoleId`)
require nonempty text without control characters. They preserve Unicode and whitespace.
`DataLabelId` and `PolicyVersion` also reject whitespace. `WorkContextId` accepts
nonempty lowercase ASCII letters, digits, hyphens, and underscores. These profiles
preserve supplied spelling and impose no byte limit. Domain and transport rules may
require additional validation at their own inputs.

The public `identifier_syntax` module supplies these lexical validators for other
owners' newtypes. It does not expose an interchangeable generic identity. Constructors
and Serde use the same validator; the distinct types survive until serialization.
`AccessSubject` requires the corresponding principal or group ID. Direct provenance
requires an initiator, delegated provenance also requires a delegation ID, and
automated provenance carries neither field. Services must authenticate this attribution
before trusting it.

`TaskId` holds a UUID. Its parser admits simple, hyphenated, braced, and lowercase-prefix
URN spellings, including uppercase hexadecimal digits and existing non-v7 values.
Display and JSON serialization emit lowercase hyphenated UUIDs. Its string schema
describes these admitted spellings. Serde delegates to UUID, preserving that library's
behavior for binary and other deserializers. `new` and `default` generate v7 values.
The Task runtime checks its narrower v7 profile at external lookup admission.
Store's `task_record_id` adapter binds the value to the `task` table with a UUID key.
The foundational type has no database traits or table knowledge. Opaque MCP Task
handles and gateway route identities keep their separate protocol contracts.

UUID is pinned to the already-qualified 1.25.0 parser used by native Tasks and Map.
The [upstream registry](https://crates.io/crates/uuid) reports 1.26.1 as stable on
2026-09-27. Retaining 1.25.0 keeps the ownership extraction independent from a parser
upgrade; dependency currency work must qualify newer admission and serialization
behavior before changing that pin.

`Vocabulary` with the `scope` hook implements a domain enum from its owner's variant-to-spelling declaration.
Its parser, serializer, schema, display, and `ScopeDefinition` implementation use that
same declaration. Duplicate spellings and values outside the OAuth scope-token grammar
fail compilation. Consumers need Serde and Schemars. A domain may also
implement the public trait directly.
Empty declarations produce uninhabited enums, reject every parsed name, and emit
the JSON Schema `false` schema. Servers expose the generated `ALL` slice to checked
MCP setup, including when they declare no domain permissions.
Schema identities include the declaring module, so independently owned enums with
the same Rust name keep distinct definitions in a combined schema.

`ScopeName` preserves its input spelling. Its constructor accepts scopes unknown to
Veoveo core; the owning domain determines which names its code understands.
`ResourceScheme` requires a lowercase ASCII letter followed by lowercase ASCII
letters, digits, `+`, `-`, or `.`.

`ResourceUri` validates concrete absolute references through the URI library in both
construction and Serde decoding. It preserves the supplied spelling and rejects
unexpanded templates, malformed percent escapes, raw Unicode and relative references.
Errors omit the supplied value because network references may contain credentials.
`ResourceTemplateUri` carries template declarations through completion and discovery.

The generic reference also carries network resource identities, including OAuth
protected-resource URLs with ports. `ResourceUri::components` applies the stricter
server-owned address profile and returns `ResourceUriParts`; callers may also parse
external text directly into those parts. Domain parsers validate route meaning,
supported query names, IDs and parent relationships. A valid generic reference does
not establish a domain address or permission to read it.

## Task Operation Names

`TaskTypeName` applies the operation-name grammar to dynamic input and Serde decoding.
Code-owned declarations use a constant constructor that rejects invalid literals at
compile time. `TaskTypeDefinition` lets a server-owned enum supply its operation names.
Stream and Reason expose these enums through their contract-only libraries. The shared
runtime accepts their names in `OwnerTaskQuery`; it contains no domain variants.
Selected names convert to text when binding to the database driver.

## HTTPS Network URLs

`HttpsUrl` owns the separate network-address profile. Construction rejects parser
violations and any spelling the URL library would normalize. An authority-only HTTPS
root includes its trailing slash. The type preserves encoded path and query bytes,
query ordering, repeated names and nondefault ports. Debug output and admission errors
omit the URL because a query may contain credentials. `as_url` exposes an immutable
parsed URL to HTTP adapters without reparsing or erasing its type.

The type validates syntax only. A loopback URL can pass this constructor, and a download
policy must still reject forbidden addresses after DNS resolution and every redirect.
Server contracts own source-list cardinality and supported source kinds. This
foundational type contains no DuckDB, Map or Timeseries vocabulary.

## Concrete Resource Components

`ResourceUriParts` preserves the supplied spelling. It rejects malformed percent
escapes, invalid UTF-8, decoded control characters, relative addresses, and input
the URL parser would normalize. It decodes each path segment separately, keeping
an escaped slash inside its original segment. Query names are decoded before
duplicate detection; empty names are invalid. The owning domain rejects unsupported
names and validates IDs, route shapes, and field combinations.
An authority-only root yields no path segments. An explicit trailing slash yields
an empty segment, which lets an owner distinguish those spellings.
The authority holds an unescaped declared name or ID. Dynamic text that requires
encoding belongs in the path or query. The URL library's opaque-host parser permits
malformed percent escapes, so the resource profile rejects escapes in the authority.

`ResourceUriBuilder` starts from a declared route or typed scheme and `UriAuthority`,
then appends decoded `UriSegment`
values through the URL path setter. A segment rejects empty text, dot-only relative
components, and controls. The percent-encoding library escapes the non-URL ASCII
characters that the custom-scheme setter leaves literal. Query pairs use the URL
serializer and require unique nonempty names. Bases cannot already carry a query.
`build` validates the resulting concrete profile before returning a wire reference.
`from_components` uses URL setters for dynamic schemes and authorities. It selects a
fixed base for the URL standard's special schemes because those setters cannot cross
between special and ordinary schemes. An authority that needs normalization for the
chosen scheme is rejected. Component helpers contain no domain vocabulary.

Domain constructors expose their specific ID types and convert them to segments at
this serialization step. `UriSegment` is an encoding helper and cannot establish a
domain identity. A domain with one canonical ID spelling compares parsed output with
its builder to reject encoded aliases. Foundation parsing itself preserves valid
escape spelling and does not equate differently spelled references.

`ResourceUriError` carries a failure category without copying input. Its display,
debug, and source chain cannot expose credentials or query values from a rejected
address. The older `IdentifierError` contract below has a different representation.

`IdentifierError` carries an invalid public identifier and a static validation rule.
Callers must not put credentials or secret material into it. Its display format is
shared with identifier validation in higher layers.

`Sha256Digest` preserves the canonical prefixed spelling and emits the corresponding
JSON Schema pattern. Its `from_hex` constructor accepts the unprefixed lowercase
digest produced by hashing libraries; default serialization includes the `sha256:` prefix.
The `sha256_hex` module supplies required and optional Serde field adapters for
contracts that declare 64 lowercase hex digits without a prefix. These adapters reject
prefixed or uppercase input; they do not guess a format. Such fields declare their
bare-hex JSON Schema alongside the adapter.

## Resource Templates

`ResourceTemplateUri` preserves a validated template's spelling and serializes as a
JSON string. Its constructor and deserializer apply the same RFC 6570 parser. A
literal-only template is valid, and repeated variable occurrences preserve their
names and order. URI templates can describe fragments or addresses whose authority
depends on variables; validating their syntax does not prove that every expansion
will satisfy a server's concrete resource profile.

`expand` accepts the upstream library's `Context` interface and produces a
`ResourceUri` only after `ResourceUriParts` accepts the result. Owners keep required
parameters typed in their constructors and validate the resulting domain route.
RFC 6570 permits undefined variables; expansion alone cannot establish required IDs,
parent relationships, query policy, or authorization. The library handles reserved,
path, query, list, associative, prefix and explode expansion. The concrete profile
can reject valid RFC expansions, including fragments and repeated query keys.
Errors retain no template text, binding values, or upstream error payload.

The parser accepts ASCII template literals; percent-encoded literals and Unicode
binding values are supported. Admission additionally rejects zero, leading-zero and
five-digit prefix modifiers and empty dotted variable-name components. The pinned
library accepts those spellings, and some five-digit values overflow its expansion
parser's internal integer conversion. The guards check modifiers and names after the
library validates expression structure. Remove each guard when a qualified upstream
release rejects its retained negative cases. These are consumer-side admission checks;
the dependency source is unmodified.

The exact iri-string pin is the stable `0.7.14` release verified against the
[upstream registry](https://crates.io/crates/iri-string) on 2026-09-27. URL does not
parse RFC 6570 templates, so this dependency supplies that missing parser and
expansion engine with only its standard-library feature. Concrete component handling
continues to use the qualified URL profile. The gateway's existing
`ResourceUriTemplate` policy selector accepts a narrower pattern language and has
its own matching semantics. Neither that selector nor an opaque historical
`ResourceUri` is implicitly converted to `ResourceTemplateUri`.

## Qualification

Native tests check string serialization and schemas, invalid input rejection, scope
vocabulary independence, and resource-reference spelling. The consumer integration
test implements both traits with an independent domain. Compile-fail examples reject
generic names and references where domain implementations are required. Component
tests cover every printable ASCII character, Unicode, encoded separators, duplicate
query names, normalization, templates, and malformed input. Time's contract consumes
the builder and implements `ResourceAddress` for its authority-release URI.
Template tests cover RFC operators and modifiers, malformed expressions, string
schemas, variable spelling, and concrete-profile rejection after expansion. Time
compares every declared template with its typed resource builders, including reserved
zone names and all collection cursors.
Identity tests compare all eleven schemas with a fixture captured before extraction,
preserve the three lexical profiles and tagged JSON forms, and reject invalid nested
IDs and missing attribution. Compile-fail examples reject crossed identity types,
incorrect subject variants, and incomplete delegated provenance.

These tests qualify foundational types and extension points. Hosted-server behavior,
SQL visibility, authorization decisions, and runtime isolation require their owning
tests and conformance profiles.

## Shared Installation Names

The foundational library owns opaque gateway Task routes, server and profile names, OAuth client and refresh-family
identities, agent definition/model/template/instance names, and authentication methods
and reason codes. MCP re-exports these types for its protocol consumers. The audit
contract imports them directly without depending on MCP or the gateway. A managed-agent
instance is a validated installation name; it is not assumed to be a UUID.

## Resource Selection

`ResourceSelector` admits a scheme, a literal prefix, or a restricted template.
`ResourceUriTemplate` accepts simple lowercase `{variable}` identifiers separated by
literal text. Each variable consumes nonempty text up to the first occurrence of the
next literal. The matcher does not retry later occurrences when the first match leaves
an empty variable or an unmatched suffix. A trailing variable consumes the rest.
For example, `example://item/{id}-end` matches `example://item/a-end` and rejects
`example://item/a-end-end`.

`literal_segments` exposes the checked template as bound data for database adapters.
It includes the initial literal and one following literal per variable; an empty final
literal denotes a trailing variable. Adapters qualify parity with `matches_uri` before
using this representation for admission. Prefixes preserve their literal spelling,
including encoded delimiters. This selection language has separate semantics from
RFC 6570 expansion through `ResourceTemplateUri`.

## Closed Vocabulary Mechanics

The public `Vocabulary` trait exposes a domain-owned variant slice and spelling
lookup. An independent contract may implement it directly. The re-exported derive
uses these ordinary mechanics and the existing scope and Task traits; the foundation
owns no domain variants. The [macro design](../macros/DESIGN.md) defines declaration
syntax, Serde profiles and consumer-only database delegation. Existing schemas and
wire spellings are preserved during mechanical adoption.
