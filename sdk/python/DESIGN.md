# Python Hosted Server Contracts

The Python SDK supplies common types and MCP integration for independently owned
servers. Each server package defines its scope vocabulary, resource variants and
domain admission. The [SDK guide](README.md) describes identity verification, Tasks,
Artifact transport and development commands.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| MCP `2026-07-28`, Python SDK `mcp==2.0.0` | Hosted stateless Streamable HTTP, Discover, resource descriptors and request-scoped subscriptions; the [hosted contract](../../mcp/contract/DESIGN.md) defines the required surface |
| JSON Schema 2020-12 | Complete tool schemas through `schema.py`; Pydantic validates wire models |
| RFC 3986, [rfc3986 2.0.0](https://pypi.org/project/rfc3986/2.0.0/) | Concrete hierarchical URI syntax with input spelling preserved; owner resource components reject credentials, ports, fragments, malformed UTF-8 and repeated query names |
| RFC 6570, [uri-template 1.3.0](https://pypi.org/project/uri-template/1.3.0/) | ASCII resource templates with a fixed scheme; component builders use simple, reserved, path and query expansion. Library-specific defaults, array notation and variable aliases are rejected. Partial expansion is not used |
| RFC 6749 scope-token | Closed owner scope enums; external `ScopeName` values follow Veoveo's printable identifier profile |
| RFC 9562 UUIDv7 | Task identities and nominal Artifact identities at wire admission |
| `ai.veoveo/knowledge-source` | Embedded document collections, content digests, negotiated observations and conditional reads |

## Type Ownership

`veoveo_mcp.types` has no MCP or runtime imports. `ScopeName`, `ResourceScheme`,
`ResourceUri` and `ResourceTemplateUri` are distinct checked string types. Their
constructors and Pydantic adapters apply the same validation. JSON serialization
preserves their wire strings. Generic network references may include ports and
fragments; `ResourceUri.components()` applies the stricter domain address profile.

Owners define enums derived from `ScopeEnum`. Enum construction rejects invalid
OAuth spelling and aliases. `ScopeDefinition` exposes each variant's validated
name. Parsing an external name establishes syntax, while the owner's enum identifies
a permission known to its implementation.

`ResourceAddress` exposes `to_uri()`. Owner variants carry specific identities and
cursor types. Their constructors check those types; owner parsers reconstruct the
variants and reject unsupported route shapes. `ResourceUriBuilder` accepts checked
scheme, authority and segments. The URI libraries own escaping and component parsing.
The builder rejects duplicate query names before expansion can collapse them.

`CheckedText` lets an owner define additional nominal identities with shared
Pydantic integration. `contract.artifacts.ArtifactId` uses it for canonical UUIDv7
spelling. Wider identity models and runtime contracts keep their owning modules.

## Checked MCP Setup

`contract.server.McpServerContract` associates a server's scope and resource types
with its implementation identity, documents and descriptors. The protocol contains
no server registry. `McpResource` and `McpResourceTemplate` builders bind descriptors
to their checked addresses and reject an attempted address replacement.

`McpServerSetup` checks scope membership, duplicate declarations, typed resource
round trips, required documents and the docs/contract roots. It attaches document
collection metadata to the declared document template. Handlers receive sorted
copies of the validated descriptors, so changing a response cannot mutate setup.
`configure()` checks the actual MCP server identity and required resource handlers
before declaring the knowledge extension.

`has_scope()` accepts the owner's scope type and authenticated grant names. It
rejects another owner's enum and refuses undeclared permissions. The owner still
checks tenant, Work Context, resource grants and other current domain policy.

Servers construct setup before connecting Stores or resuming workers. Datasheet's
`server/contract.py` supplies the working template. Importing that module validates
its declarations before `serve()` starts dependencies. Resource reads authenticate
the request, parse the owner address, and apply the existing SQL owner query.

Python protocols provide annotations for static consumers; constructor checks and
native tests establish runtime declaration consistency. Hosted conformance,
authorization, recovery and GPU acceptance require their own applicable checks.

## Qualification

Run `uv run --locked --all-extras pytest` from this directory. The independent owner
fixture exercises setup without importing Datasheet. It checks descriptor tampering,
missing and duplicate declarations, permission type mismatches and discovery-copy
isolation. URI cases exercise reserved characters, malformed escapes, template
separation and nominal wire decoding. A fresh process proves that foundational
imports do not load MCP, SurrealDB or the Task runtime.

Datasheet qualifies template expansion against its resource variants and exercises
the handlers over MCP. Its installed acceptance uses the reference installation's
hosted and knowledge-source suites. The unrelated fork workload also runs against
the shared SDK to detect wire and packaging regressions.

## Internal Identity Assertions

The receiver requires `veoveo.ai/gateway-internal-assertion/v2` on the Ed25519 JWT
and `veoveo.ai/gateway-request-context/v2` on its signed request context. Missing or
unsupported formats fail verification. Hosted receivers and the gateway require a
coordinated upgrade and drain; this SDK admits one internal format.

`AuditManagedExecution` carries checked instance and optional UUIDv7 episode identity
with positive unsigned 64-bit generation and dispatch counters. The closed access-token
model rejects the public `managed_agent` claim in internal assertions. Only an automated
service without a browser session may carry execution attribution. The receiver checks
actor, source principal, client, tenant, context, invocation provenance and source-token
expiry before delivery. Current registration and domain permissions remain the server's
responsibility.

## Task Query Assets

The Task runtime stores complete SurrealQL statements under
`src/veoveo_mcp/tasks/queries/`. Owner, Work Context, operation-type and cursor
choices select complete files before execution. Authorization predicates run in
SQL before limits and decoding. The package loader caches each UTF-8 resource
through `importlib.resources`; wheels carry the same assets. The native SHOW
changefeed template substitutes only validated numeric cursor and limit values.

Test mutations live under `tests/queries/`. The native fixture starts an isolated,
digest-pinned SurrealDB 3.3 container and uses a prebuilt Gateway composition
binary to generate an empty optional-module selection, prepare the installation
and migrate its planned kernel lanes. `VEOVEO_TEST_GATEWAY_BIN` selects that
executable; the default is the repository's `target/debug/gateway`. Tests fail
with a build prerequisite diagnostic when it is unavailable. They neither build
the binary nor publish an installation control plane.
