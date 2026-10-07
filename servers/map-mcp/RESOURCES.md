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
| Veoveo Map cursor profiles | Mobility and travel-model pages use version 2 camelCase JSON envelopes. The unchanged metadata, source, restriction and ID-only operational envelopes use version 1; each binds its admitted collection and parent. |

## Resource Surface

Root resources are:

```text
map://sources
map://datasets
map://locations
map://facilities
map://mobility-profiles
map://restrictions
map://routes
map://matrices
map://travel-models
map://feature-layers
map://publications
map://layer-products
map://compositions
map://rasters
map://raster-derivations
map://spatial-derivations
```

Resource templates are:

```text
map://source/{source_id}
map://sources{?cursor}
map://datasets{?cursor}
map://dataset/{dataset_id}{?cursor}
map://dataset/{dataset_id}/release/{release_id}
map://source-feature/{release_id}/{source_feature_id}
map://raster/{raster_id}
map://raster-derivation/{derivation_id}
map://spatial-derivation/{derivation_id}
map://location/{location_id}
map://facility/{facility_id}
map://mobility-profile/{profile_id}/{profile_version}
map://mobility-profiles{?cursor}
map://restriction/{restriction_id}
map://restrictions{?cursor}
map://routes{?cursor}
map://matrices{?cursor}
map://acquisitions{?cursor}
map://acquisition/{acquisition_id}
map://route/{route_id}
map://matrix/{matrix_id}
map://travel-model/{travel_model_id}
map://travel-models{?cursor}
map://artifact/{artifact_id}
map://feature-layers{?cursor}
map://publications{?layer_id,cursor}
map://layer-products{?publication_id,cursor}
map://compositions{?cursor}
map://feature-layer/{layer_id}
map://feature-layer/{layer_id}/schema/{schema_version}
map://feature-layer/{layer_id}/style/{style_version}
map://feature-layer/{layer_id}/features{?publication_id,bbox,datetime,geometry_type,filter,limit,cursor,minimum_commit_sequence}
map://feature-layer/{layer_id}/feature/{feature_id}
map://feature-layer/{layer_id}/feature/{feature_id}/revision/{feature_revision}
map://feature-layer/{layer_id}/changeset/{changeset_id}
map://feature-layer/{layer_id}/publication/{publication_id}
map://feature-layer/{layer_id}/publication/{publication_id}/product/{product_id}
map://composition/{composition_id}
map://composition/{composition_id}/revision/{composition_revision}
```

Source resources present public source fields. Routes and matrices are owner
scoped. Travel models are filtered by principal, gateway profile, tenant,
labels, and Work Context from their durable task owner. Dataset, geography,
profile, and restriction resources are tenant scoped. Authored layers,
publications, products, and compositions are Work Context scoped and filtered
by the caller's data labels. Map repository queries apply those predicates in SQL before returning
layer and composition records. Publication and product queries select visible parent
layers in SQL, including direct product reads. Raster derivation resources are confined to their
creating Work Context, while the immutable source raster remains tenant
scoped. Spatial derivations are also confined to their creating Work Context.

`map://datasets` and `map://dataset/{dataset_id}` return release pages with
`items`, `limit: 100`, and `next_cursor`. Items are release documents in ascending
release-ID order. The root includes all tenant-visible datasets. A dataset URI
selects its parent in SQL before the keyset predicate and limit. A version-1
hex-encoded cursor binds the release collection and optional dataset ID; it does
not confer access. Each page applies the current tenant again. These reads observe
current committed rows, without a snapshot across page requests. An empty first
page for a dataset returns not-found; an exhausted continuation returns an empty
page. Exact release and geographic product addresses declare their routes beside their
owner IDs in `contract/product_uri.rs`. The shared `ResourceAddress` derive uses
those declarations for parsing and typed constructors, while the owner keeps its
cached wire, accessors, string schemas and invalid-address errors. Source and source
collection addresses use the same mechanics with the existing opaque source cursor
codec. Discovery templates are checked against these route declarations. An exact
release URI binds both dataset and release IDs in the database.
A layer-product URI likewise binds its layer, publication, and product IDs in SQL
alongside current layer visibility.

Authoring metadata collections use the same 100-item envelope, ordered by immutable
domain ID. Layer and composition pages exclude archived rows. Publication and product
pages select current parent-layer visibility and may read immutable products of an
archived layer. Their optional `layer_id` and `publication_id` filters apply in SQL
before keyset selection and limits. Cursors bind those filters and reapply current
tenant, Work Context, and label access on every request. These pages observe current
committed rows rather than a snapshot across requests. The App follows all pages
before publishing a refreshed collection, preserving its previous view on failure.

Route, matrix, and acquisition indexes return the same page envelope with up to
100 items ordered by immutable domain ID. Their cursors bind the collection and
are valid only at version 1. Map repository queries apply tenant and owner predicates before
keyset selection and limits, including for direct reads. Route and matrix items
contain status or profile metadata and a `resourceUri` for the complete document;
index queries omit route geometry and matrix cells. Matrix reads and completion
select only rows containing a matrix document. Acquisition pages contain job
records, and acquisition updates enforce ownership in the same SQL write as their
revision check.

Map owns the coordinated collection-page transition. Installations drain Map and
replace its binary and packaged Map Explorer together. Clients must consume the
page envelope and follow `next_cursor`; grouped dataset objects and bare arrays
for releases, routes, matrices, acquisition jobs, or authoring metadata are unsupported. Route and
matrix consumers read each summary's `resource_uri` when they need the payload.
Rollback restores the previous binary and App together; this response change does
not convert persisted records. Qualification covers multiple pages, foreign
tenants and owners, mismatched parents, and rejected cursors. No mixed-version
response adapter is supported during this installation upgrade.

Raster and spatial derivation indexes return up to 100 summaries with resource
links, a `limit`, and an optional `next_cursor`. Page templates accept the version-1
hex-encoded cursor; its kind must match the requested collection. A page request
always applies the current tenant and Work Context in SQL before the cursor and
limit. Direct resources return the complete immutable document. Completion applies
the search term in SQL and selects at most 101 IDs to determine `hasMore`.

Both indexes accept resource subscriptions. Raster index admission requires
`map:dataset:read`; spatial index admission additionally requires
`map:spatial:derive`, matching their resource reads.

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
