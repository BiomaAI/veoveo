# Recording Domain Contract

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| RFC 6749 section 3.3 | The closed `RecordingScope` enum defines `recording:seal`; names describe permissions and confer no authority. |
| RFC 9562 UUIDv7 | Recording, dataset, layer, read-grant and projection identities require the RFC UUID variant and lowercase hyphenated spelling. |
| RFC 3986 and RFC 6570 | Foundational component parsing/building and discovery templates for Recording resources. The domain fixes each route and its parameter types. |
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
declares fixed discovery roots and templates. `catalog.rs` owns grant and projection
models. Its catalog-request constructor admits 1–500 input Recording IDs and produces
a sorted unique selection. Dataset membership remains a Store admission check.
The crate root owns recording views, layer views, sealing and playback models.
Console imports playback models through the server's contract-only library feature;
it checks the requested Recording and related archive fields before forwarding a manifest.

Address decoding rejects alternate spellings, fragments, unsupported or duplicate
query parameters, malformed IDs and wrong resource parents. Errors omit submitted
values. An admitted URI grants no access. Service owners check current authorization
and operational bounds, while SQL selects visible records before limits and decoding.
Store adapters require native RFC UUIDv7 record keys with the declared table. String
record keys are not part of the current storage profile. Broader playback/projection
field relationships are
tracked in the [foundations plan](../../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

## Qualification

`cargo test -p veoveo-recording-contract` checks wire/schema shapes, UUID admission,
every resource family's round trip, malformed cursors, discovery template expansion
and compile-time rejection of raw or wrong-domain IDs. Independent consumers must
resolve without service features. The MCP facade test proves public type identity;
Hub and Gateway native tests qualify their adapters. Installed authorization, playback
and GPU acceptance belong to the owning services.
