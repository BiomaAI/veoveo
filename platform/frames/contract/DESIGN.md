# Frames Domain Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| JSON and JSON Schema Draft 2020-12 | Frames IDs, world trees, immutable revisions, conversion requests/results, provenance, catalog and Task usage models. |
| RFC 3986 and RFC 6570 | The foundational URL 2.5.8 component builder implements Frames' hierarchical custom-scheme routes and declared query parameters. |
| RFC 3339 | Clock-free Chrono 0.4.45 values for creation, revision and usage timestamps. |
| SHA-256 | Canonical world-tree serialization hashes to the foundational `sha256:` plus lowercase hex representation. |
| WGS84 and EPSG:4978 | Geodetic and Earth-centered Cartesian value models; conversion execution belongs to the Frames runtime. Map owns CRS, datum and ellipsoid names. |
| Veoveo Frames resources | World, immutable revision, revision-scoped frame, operation, usage and collection addresses. These are repository-owned routes, not MCP transports. |

## Ownership And Dependencies

This crate owns Frames' public spatial values, IDs, resource addresses, tree admission,
metadata and provenance. The MCP library re-exports this model through its isolated
`contract` feature. Clients may import that feature; producers below the server
runtime import this crate directly. The dependency graph contains foundational and
Artifact types, Map's contract, serialization, hashing and date/time libraries.
It excludes Store, MCP, RRD, async runtimes and GPU engines.

Frames runtime consumes RRD, which consumes the Recording domain. Recording projections
carry Frames references. Placing the Frames contract below those runtime adapters
resolves that dependency cycle without copying frame vocabulary into Recording or MCP
core. This crate introduces no process or deployment boundary.

## Construction And Admission

`ids.rs` owns distinct world, revision, frame and operation IDs. Each admits 1–128
ASCII letters, digits, underscores, hyphens, dots or colons, excluding the complete
values `.` and `..`. JSON carries strings. `uris.rs` builds addresses from these IDs:
a world builds a revision, and a revision builds a frame. `WorldFrameUri` always pins
an immutable world revision. Parsers reject credentials, ports, escaped aliases,
queries on singular resources and incorrect routes.

`tree.rs` validates a complete rooted tree, including its 10,000-frame bound, canonical
ordering and digest. `metadata.rs` checks repeated identity, root, head and digest facts
before exposing immutable world summaries, revisions and source references. `streams.rs`
admits concrete producer addresses and bounded entity paths without owning producer
routes. `catalog.rs` and `usage.rs` own typed cursors and page models.

An admitted address does not establish existence, membership, content or access.
[Frames runtime](../../../servers/frames-mcp/DESIGN.md) authorizes current records in SQL,
resolves transforms and checks referenced data. A consumer that only carries an address
must not claim to have resolved its frame or transformed its coordinates.

## Qualification

The owning native tests qualify schemas, address construction, rejected inputs, world
metadata, tree limits, canonical hashing and current UAV scenario values. The complete
UAV tree has a pinned digest and passes repeated JSON value and byte round trips. Compile-fail
examples reject raw or wrong-domain construction. The MCP facade test proves public
type identity. Independent consumers must resolve without runtime dependencies.
Installed publication, Task delivery and conversion acceptance belong to the server.

## Identity Declaration Mechanics

Frame identities use `Id` with the coordinate owner’s lexical validator. The owner retains its relative-component rejection, accepted colon spelling and String schema profile. URI admission and frame-world relationships are separate checks.

## Resource Address Declarations

Frame world, revision and operation addresses declare typed routes with the shared
`ResourceAddress` derive. World-frame addresses preserve their composed revision getter
through an ordinary adapter to a private typed route. Cached fields are private; checked
constructors rebuild and admit the same route. World and revision input hooks reject
escaped wire components before identifier admission, preserving their Route error
profile. A wrong route shape is rejected before typed fields are parsed. Usage routes use
explicit Task and cursor codecs. Cursor payload bytes and existing address schemas stay
owner declarations.
