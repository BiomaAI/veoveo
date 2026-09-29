# Recording Domain Contract

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| RFC 6749 section 3.3 | The closed `RecordingScope` enum defines `recording:seal`; names describe permissions and confer no authority. |
| RFC 9562 UUIDv7 | Recording, dataset, layer, read-grant and projection identities require the RFC UUID variant and lowercase hyphenated spelling. |
| RFC 3986 and RFC 6570 | Foundational component parsing/building and discovery templates for Recording resources. The domain fixes each route and its parameter types. |
| RFC 3339 and SHA-256 | Playback and projection expiry timestamps decode to UTC values; Blueprint and projection digests use the foundational digest type and lowercase 64-character hex on the wire. |
| JSON and JSON Schema Draft 2020-12 | Public Recording views, seal requests/results, playback manifest v9, catalog grants and Arrow projection models. |
| Recording catalog cursor version 1 | Collection-bound JSON encoded as lowercase hexadecimal, at most 2048 input bytes, with a timestamp and Recording ID. |
| Veoveo Recording resources | `recording://recordings/{UUIDv7}`, its `layers` child, the catalog, well-known documents and the Recording Explorer address. These are domain declarations; the crate implements no MCP transport. |

## Ownership And Dependencies

This crate owns Recording's public IDs, resource addresses, cursors, sealing scope and data models.
It depends on foundational types, Serde, JSON Schema support, UUIDs and clock-free
date/time values. It imports no server, Store, async runtime, Rerun or GPU library.

Hub produces the same Recording identity that playback, analysis, UAV and acceptance
clients consume.
Recording MCP depends on Hub for Blueprint validation, live-message handling and
publication. Placing the shared contract below both packages permits Hub to build
public addresses without a Cargo cycle. This dependency requirement justifies a
separate domain crate. The MCP server's library exposes these same types through its
`contract` feature; it defines no second model. Generic MCP infrastructure imports
neither the domain crate nor the server's vocabulary.

`scopes.rs` declares the sealing permission used by runtime checks and MCP setup.
`ids.rs` defines distinct Recording, dataset, layer, read-grant and projection IDs.
They share UUID admission mechanics without allowing implicit conversion between domains.
`resources.rs` builds and admits domain routes
through the foundational URI library. `cursor.rs` owns public catalog positions;
the service converts them to Store's query types at the persistence call. `uris.rs`
declares fixed discovery roots and templates. `catalog.rs` owns grants. Its catalog-request constructor admits 1–500 input Recording IDs and produces
a sorted unique selection. Dataset membership remains a Store admission check.
The crate root owns recording views, layer views and sealing. `playback.rs` owns the
closed Recording lifecycle, manifest schema, typed timestamps and Blueprint integrity
fields. `PlaybackManifestBuilder::build` checks the archive's dataset, Recording and
catalog revision against the manifest. It admits one playback plane according to the
lifecycle, checks capture layer names against their ordinals and rejects reversed
capture timestamps. Blueprint revision and byte length are nonzero types.

`PlaybackManifest` is sealed and exposes immutable field access. JSON decoding runs
the same builder checks, including unknown-field rejection. The Blueprint digest uses
`Sha256Digest` internally with the field's lowercase hex wire profile. The model checks
shape and relationships; token signatures, expiry against the current clock and source
authorization belong to the service. Console imports this model through the server's
contract-only feature and checks that it names the requested Recording before forwarding
it. The contract requires no runtime or clock source.

`projection/query.rs` owns the checked query and its published limits: 64 entity paths,
64 component identifiers, 10,000 samples, 10,000 rows and 32 MiB. Selectors are unique,
nonempty text of at most 1024 bytes without control characters. Sampling requires an
ordered range or a strictly increasing explicit grid within the requested sample limit.
Temporal values exclude Rerun's reserved static marker, `i64::MIN`.
`projection/request.rs` builds a request from the checked query, distinct dataset and
Recording IDs, a deadline of 1–15,000 ms and bounded metadata. Unit keys must name a
selected component. Both query and request expose immutable field access; JSON decoding
runs the same builders and rejects unknown fields. The request wire shape stays flat.
`query_identity` supplies typed serialization inputs in the owner's field order, excluding
the caller's idempotency key. Exhaustive field binding requires an explicit identity
decision when the request gains a field. The service hashes those inputs without
editing an untyped JSON object.

The lightweight query checks bounds and relationships without importing Rerun grammar.
RRD prepares an `ArrowProjectionQuery` with the pinned upstream entity, component and
timeline parsers before any source loading. It also rejects distinct spellings that
resolve to the same entity. The service performs this step before Artifact materialization.
Authorization, configured lower limits, concurrency and scratch admission belong to the
service. `projection/result.rs` owns the sealed result handle and its builder. It checks
sample-grid ordering, row/omission totals, output bounds and metadata shape on JSON
admission. `build_for` also checks the requested parents, timeline, grid, units, frame
references and limits; a consumer with the request uses `validate_request` for those
same relationships. Digests use `Sha256Digest` through RRD, construction and download,
with bare lowercase hex at the declared wire fields. Byte length is nonzero and expiry
is a UTC value. The service checks query identity and expiry against the SQL-admitted
receipt, then verifies payload length and SHA-256 before reuse. The model alone does
not prove file contents or grant authority. Coordinate-frame reference interpretation
is tracked in the foundations plan.

Address decoding rejects alternate spellings, fragments, unsupported or duplicate
query parameters, malformed IDs and wrong resource parents. Errors omit submitted
values. An admitted URI grants no access. Service owners check current authorization
and operational bounds, while SQL selects visible records before limits and decoding.
Store adapters require native RFC UUIDv7 record keys with the declared table. String
record keys are not part of the current storage profile. Remaining address-field admission is
tracked in the [foundations plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

## Qualification

`cargo test -p veoveo-recording-contract` checks wire/schema shapes, UUID admission,
every resource family's round trip, malformed cursors, discovery template expansion
and compile-time rejection of raw or wrong-domain IDs. Independent consumers must
resolve without service features. The MCP facade test proves public type identity;
Hub and Gateway native tests qualify their adapters. Installed authorization, playback
and GPU acceptance belong to the owning services.
