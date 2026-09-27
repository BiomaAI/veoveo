# Veoveo Foundational Types

## Standards And Protocols

| Standard or format | Implemented profile |
|---|---|
| JSON | Names and resource references serialize as strings; deserialization applies their constructors |
| JSON Schema 2020-12 | Schemars emits string schemas; runtime validators apply the lexical rules below |
| OAuth 2.0 scope values | `ScopeName` carries the repository's nonempty token profile without whitespace or control characters; it does not enumerate an authorization server's vocabulary or establish a grant |
| Resource reference syntax | Lowercase URI scheme followed by `://` and a nonempty suffix without whitespace or controls; an opaque reference can include a completion template, and this validator does not establish full URI or template conformance |
| Rust extension interfaces | Public `ScopeDefinition` and `ResourceAddress` traits permit implementations in independent libraries |

## Ownership And Dependencies

`veoveo-types` owns `ScopeName`, `ResourceScheme`, `ResourceUri`, and `IdentifierError`.
It depends on Serde and Schemars. It contains no protocol transport, asynchronous
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

`ScopeName` preserves its input spelling. Its constructor accepts scopes unknown to
Veoveo core; the owning domain determines which names its code understands.
`ResourceScheme` requires a lowercase ASCII letter followed by lowercase ASCII
letters, digits, `+`, `-`, or `.`.

`ResourceUri` preserves an opaque absolute reference. Its lexical validator accepts
the current completion-template strings used by gateway policy and stored audit
targets. It does not validate percent encoding, path structure, query arguments, or
domain IDs. Domain parsers must establish their own resource shape. The required
separation of concrete addresses and templates, shared URI builders, and server
contract features are tracked in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

`IdentifierError` carries an invalid public identifier and a static validation rule.
Callers must not put credentials or secret material into it. Its display format is
shared with identifier validation in higher layers.

## Qualification

Native tests check string serialization and schemas, invalid input rejection, scope
vocabulary independence, and resource-reference spelling. The consumer integration
test implements both traits with an independent domain. Compile-fail examples reject
generic names and references where domain implementations are required.

These tests qualify foundational types and extension points. Hosted-server behavior,
SQL visibility, authorization decisions, and runtime isolation require their owning
tests and conformance profiles.
