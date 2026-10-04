# Map Resource Contracts

Map owns its public identifiers, resource addresses, scope vocabulary and library
features. The MCP adapter exposes these contracts through checked declarations.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| RFC 3986 and the foundational URL 2.5.8 component profile | Concrete Map addresses use checked scheme, authority, path and query components. Direct addresses require the spelling produced by their owner builder. |
| RFC 6570 | Advertised templates use the shared `ResourceTemplateUri` validator and scalar expansion. Domain parsers validate the expanded identity and route. |
| Veoveo MCP server contract revision 3 | `McpServerContract` and `McpServerSetup` validate startup and discovery declarations for MCP `2026-07-28`. |
| RFC 9562 | Map identity types own their generated UUIDv7 and stable UUIDv5 profiles. |
| JSON Schema Draft 2020-12 | Public Rust types supply the schemas used by the MCP adapter. |
| Veoveo Map cursor version 1 | Hex-encoded typed JSON binds metadata and operational continuations to their collection and selected parent. |

## Public Types And Authorization

`contract/resources.rs` owns fixed discovery addresses, the closed document vocabulary,
and direct acquisition, dataset, location, facility, matrix and authoring addresses.
`MapResource` keeps each parent and member ID in its owning type. Shared URI components
encode those values; parsing rejects extra paths, escaped aliases and unsupported queries.
Resource readers receive the parsed IDs, and authoring mutations and workers use the
same builders for links and notifications.

`contract/address.rs` composes the owner parsers into `MapAddress`, the host's complete
resource admission type. Its private construction pairs the validated URI with a
`MapTarget` for direct resources, products, pages, filtered feature queries, Artifact
references or knowledge. Readers dispatch that target and pass its typed fields to the
domain reader. The retained URI supplies the response address without reparsing it.

`contract/catalog_pages.rs` owns `MapCatalogPage` for route, matrix, acquisition,
release and derivation pages. Each continuation carries its specific domain ID;
release continuations also bind the optional dataset parent. Parsing rejects unknown
query fields, duplicate parameters, invalid IDs, other collections and cursor versions
other than 1. Reader APIs retain the position type until the Store call binds its key.
Cursor JSON preserves each collection's version-1 field profile. Deployments need no
persisted conversion; discarding a cursor restarts its collection traversal.

`MapContract` implements `McpServerContract` in `src/mcp/setup.rs`. Startup validates
the server identity, scopes, resources, templates and embedded documents before opening
Store or analytical engines. Initialization and discovery consume that setup. Resource
discovery selects checked addresses under the caller's current Map scopes and adds the
configured basemap origin to the App descriptor. These checks establish declaration
consistency; domain readers still enforce SQL visibility and parent relationships.

`contract/product_uri.rs` owns the six exact Map product address families consumed by
View. Builders require each segment's domain ID, and parsers admit the complete route
through the foundational URI library. Dataset release and source feature addresses
retain their typed parent identities. Reads use those identities when calling the
domain readers; SQL owns tenant, caller and parent selection. Derivation and route
producers use the same builders as discovery. Their broader result DTO relationship
checks remain tracked in the [consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md).

The server's public contract owns `MapScope`, its closed authorization vocabulary.
Handlers and Tasks require that enum and share one grant check. Configurable
administrative admission accepts a validated `ScopeName` and defaults to `MapScope::Admin`.
Unrelated server scopes in the caller's grants remain valid. `MapAccessContext`
carries database identity and is separate from this vocabulary.

`MapMetadataRequest` implements the foundational `ResourceAddress` trait for authored
layer, publication, product, and composition collections. Variants carry their specific
ID types and optional parent selection. The shared URI library parses and encodes query
components. `MapMetadataCursor` validates its version-1 envelope and typed position,
then checks the collection and parent again when resumed. Serialization emits a hex
string. A cursor grants no access; every page applies current database visibility.

Artifact references in releases, raster products and derivations, publications,
layer products, compositions, and travel-model outputs use the Artifact owner's
[`ArtifactUri`](../../platform/artifacts/contract/DESIGN.md#wire-and-construction).
Release and raster validators require the neutral plane variant. Publication and
product Store inputs preserve that type until driver serialization. Stored JSON keeps
the same string fields; the Artifact owner's compatibility profile defines admission
and retained-data handling. These types confer no access to bytes or parent records.
Retained Map documents must also pass their domain validators before an upgrade;
these enforce the neutral-only rule beyond the shared Artifact address profile.

The library exposes `contract`, `knowledge`, `runtime`, and `mcp` features. Cross-server consumers
use `default-features = false, features = ["contract"]`. This builds the public model,
including CRS, datum, and ellipsoid IDs, without MCP, database, GPU, network client,
or async runtime dependencies. Geometry validation uses the workspace's geo 0.32.0
profile with default features disabled. Chrono supplies date/time values without its
clock feature; UUID supplies the existing generated and stable ID profiles.
The runtime owns engines and persistence. The MCP adapter owns App HTML and hosted
protocol wiring; the default `mcp` feature includes the runtime.

Remaining DTO relationships and typed Store query keys are work in the
[consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md#modular-types-and-server-contracts).

The MCP adapter separates resource reads in `src/mcp/resources.rs` from descriptors
and templates in `src/mcp/discovery.rs`. `src/mcp/owned.rs` handles operational pages;
`src/mcp/metadata.rs` handles authored records and metadata pages. Both receive the
selection already admitted by the public contract and enforce current domain policy.
