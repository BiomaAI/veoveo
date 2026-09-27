# Veoveo Foundational Types

## Standards And Protocols

| Standard or format | Implemented profile |
|---|---|
| JSON | Names and resource references serialize as strings; deserialization applies their constructors |
| JSON Schema 2020-12 | Schemars emits string schemas; runtime validators apply the lexical rules below |
| [OAuth 2.0 scope tokens, RFC 6749 section 3.3](https://www.rfc-editor.org/rfc/rfc6749#section-3.3) | `scope_enum!` declarations follow the scope-token grammar. Dynamic `ScopeName` preserves the broader repository profile of nonempty text without whitespace or controls. Neither type establishes a grant. |
| Resource reference syntax | Lowercase URI scheme followed by `://` and a nonempty suffix without whitespace or controls; an opaque reference can include a completion template, and this validator does not establish full URI or template conformance |
| [WHATWG URL Standard](https://url.spec.whatwg.org/) | Concrete hierarchical address components use [`url` 2.5.8](https://docs.rs/url/2.5.8/url/). The profile rejects parser violations and normalization, requires an unescaped authority, and excludes credentials, ports, fragments, and unexpanded templates. This is a Veoveo resource profile, not support for every URI scheme. |
| Percent encoding and form query encoding | [`percent-encoding` 2.3.2](https://docs.rs/percent-encoding/2.3.2/percent_encoding/) decodes UTF-8 components and encodes path characters left unescaped by the URL setter. URL query pairs use form semantics: `+` represents space and `%2B` represents plus. |
| Rust extension interfaces | Public `ScopeDefinition` and `ResourceAddress` traits permit implementations in independent libraries |
| SHA-256 provenance strings | `sha256:` followed by 64 lowercase hexadecimal digits; the value validates a supplied digest and performs no hashing |

## Ownership And Dependencies

`veoveo-types` owns `ScopeName`, `ResourceScheme`, `ResourceUri`, `IdentifierError`,
`Sha256Digest`, and `Sha256DigestError`. `ResourceUriParts`, `ResourceUriBuilder`,
`UriSegment`, and `ResourceUriError` implement concrete component handling.
It depends on Serde, Schemars, URL, and percent encoding. It contains no protocol transport, asynchronous
runtime, database client, provider integration, or server vocabulary.

A server library owns its scope enum and resource variants. Its `ScopeDefinition`
implementation maps a domain value to a validated name. Its `ResourceAddress`
implementation parses and serializes its domain address, including route and ID
validation. Neither trait requires a central registry or confers authority. The
policy owner compares requested names with current authenticated grants and performs
the other authorization checks.

`mcp/contract` consumes these types for protocol descriptors, identity, policy, and
discovery. Consumers import the foundational types directly from this crate. It
provides no server-specific enums or blanket implementation that makes an arbitrary
name or reference a domain contract.

## Wire Behavior

`scope_enum!` generates a domain enum from its owner's variant-to-spelling declaration.
Its parser, serializer, schema, display, and `ScopeDefinition` implementation use that
same declaration. Duplicate spellings and values outside the OAuth scope-token grammar
fail compilation. Consumers need Serde and Schemars. A domain may also
implement the public trait directly.
Schema identities include the declaring module, so independently owned enums with
the same Rust name keep distinct definitions in a combined schema.

`ScopeName` preserves its input spelling. Its constructor accepts scopes unknown to
Veoveo core; the owning domain determines which names its code understands.
`ResourceScheme` requires a lowercase ASCII letter followed by lowercase ASCII
letters, digits, `+`, `-`, or `.`.

`ResourceUri` preserves an opaque absolute reference. Its lexical validator accepts
the current completion-template strings used by gateway policy and stored audit
targets. It does not validate percent encoding, path structure, query arguments, or
domain IDs. `ResourceUri::components` applies the concrete profile and returns
`ResourceUriParts`; callers may also parse external text directly into those parts.
This additional validation does not narrow historical wire decoding. The required
separation of concrete addresses and templates at gateway policy and audit boundaries,
builder adoption, and server contract features are tracked in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

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

`ResourceUriBuilder` starts from a declared route and appends decoded `UriSegment`
values through the URL path setter. A segment rejects empty text, dot-only relative
components, and controls. The percent-encoding library escapes the non-URL ASCII
characters that the custom-scheme setter leaves literal. Query pairs use the URL
serializer and require unique nonempty names. Bases cannot already carry a query.
`build` validates the resulting concrete profile before returning a wire reference.

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
digest produced by hashing libraries; serialization includes the `sha256:` prefix.

## Qualification

Native tests check string serialization and schemas, invalid input rejection, scope
vocabulary independence, and resource-reference spelling. The consumer integration
test implements both traits with an independent domain. Compile-fail examples reject
generic names and references where domain implementations are required. Component
tests cover every printable ASCII character, Unicode, encoded separators, duplicate
query names, normalization, templates, and malformed input. Time's contract consumes
the builder and implements `ResourceAddress` for its authority-release URI.

These tests qualify foundational types and extension points. Hosted-server behavior,
SQL visibility, authorization decisions, and runtime isolation require their owning
tests and conformance profiles.
