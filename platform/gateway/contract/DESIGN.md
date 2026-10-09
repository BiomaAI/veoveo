# Gateway Contract

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema draft 2020-12 | Gateway identities, secret references, HTTP/TLS endpoints, catalog declarations, App dependency DTOs and discovery failures; camelCase configuration keys, snake_case controlled values and explicit schemars type names |
| OAuth 2.0 RFC 6749, PKCE RFC 7636 and OpenID Connect Core 1.0 | Admitted authorization codes, client and identity-provider state, OIDC nonces and S256 challenge/verifier values shared with storage and protocol adapters. The contract supplies values and admission, while the gateway performs the exchanges. |
| WHATWG URL and HTTP/TLS configuration | The qualified URL library checks HTTP(S) hosts, credentials, query and fragments. Plain HTTP endpoints carry TLS, CA and secret references without an MCP transport selector. |
| `ai.veoveo/app-resource-dependencies` and `ai.veoveo/app-tool-dependencies` | Gateway-projected metadata for caller-visible App dependencies |
| `ai.veoveo/gateway-discovery-degradation` | Typed discovery failures consumed by the MCP adapter |

## Ownership And Dependencies

This crate owns transport-independent OAuth continuity and PKCE values, principal display metadata, authorization-server and protected-resource identities, secret-reference configuration, HTTP/TLS endpoint declarations, catalog registration mechanics, App dependency values, discovery failure vocabularies and their metadata keys. Typed fields reuse `veoveo-types` admission.
Gateway runtime, MCP adapters and the Console browser contract import these values
directly. This crate owns the complete discovery degradation value and its sorted,
deduplicated constructor and merge behavior. MCP supplies the MetaObject conversion
trait; consumers import the value from this transport-independent owner.

A separate crate prevents a dependency cycle: MCP contract already feeds gateway
runtime, while browser schema generation must not enable MCP transports. This library
depends only on foundational types, Serde, JSON Schema support, typed error derivation, UTC timestamp support and the qualified URL library. The extraction preserves the
published spellings and schema names. Derived schema collision IDs follow the current
owning Rust module; they do not change serialized JSON or schema definition names.


## Catalog Declarations

A publisher binds one immutable registry before admitting a control-plane revision.
Action keys carry registry identity and a closed owner vocabulary. Descriptors declare
all six selector requirements, supported target kinds, server capabilities and audit
access. Owner sections validate against neutral facts from that revision and return
protected-resource and persisted-object descriptors. Target codecs admit an owner
value and compute its checked server/resource audit address before the core uses it.
The installation recipe composes these declarations through owner contract features.
Neither declarations nor the recipe import MCP, a database driver or async runtime.

`OAuthGrantType` and `OAuthClientAuthMethod` supply the shared grant and client
authentication vocabularies. MCP configuration re-exports these definitions. Owner
admission receives each client's complete grant and authentication sets, typed
default Work Context and invocation mode, credential configuration flags and
compatibility settings. Work Context facts identify its tenant. These facts describe
the current revision; the OAuth runtime still checks current membership and policy.

Contributed objects cannot use core catalog object kinds. Their `(kind, id)` pair
must be unique within a revision, matching the database's publication key; tenant
metadata does not create another identity namespace. Admission rejects these
conflicts before a publisher constructs the database transaction.

The protocol schema adapter composes registered actions, targets and sections through
the schema generator's type identities. Display names cannot identify an extension
codec because independent owners can use the same name. Fields typed as kernel
actions keep their closed kernel vocabulary when a schema also includes registered
policy actions.

`HttpUpstreamEndpoint` describes plain HTTP and rejects unknown fields, including
`transport`. MCP owns its separate endpoint with the required MCP transport selector.
Both profiles reuse the same URL, TLS, CA and secret-reference types. Decoding and
schema generation preserve existing URL and certificate-path admission.

## Tool Names

`GatewayToolName` composes typed server and local tool names through `from_parts`.
Its parser admits nonempty lowercase ASCII letters, digits, hyphens and underscores,
including existing unqualified names. Serde uses the same admission and the public
schema keeps its string shape and `GatewayToolName` title. Invalid names return the
owner error type with the existing identifier diagnostic.

## Console Bootstrap And Health

Gateway Contract owns authenticated Console bootstrap, installation presentation,
session identity and available-tenant records. Its transport, HTTP route-purpose
and health vocabularies supply the wire values used by Console producers and
clients. The JSON profile preserves camelCase fields, declared omissions and
RFC 3339 UTC timestamps. Installation inventory and event rows belong to the
Console BFF contract because they compose optional domain contracts.
