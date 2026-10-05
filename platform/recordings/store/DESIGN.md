# Recording Persistence

## Standards And Protocols

This library uses the SurrealDB Rust SDK `3.3.0` over the shared Store WebSocket
connection. Private records use typed `SurrealValue` encoding. The internal
`veoveo-modules` declaration API owns schema claims and lane identity; it is separate
from Recording's public MCP, resource and playback contracts.

## Ownership

`RecordingRepository` holds a shared authenticated `PlatformStore` connection.
It owns Recording persistence identities, driver records, dataset and layer lifecycle,
read grants, projection receipts, ingest streams and Blueprint revisions. Queries
live in `src/queries`; callers bind admitted values and retain transaction decoding.
Native fixture statements live in `tests/queries/`, with complete static files for
finite grant and Recording corruption cases.
`PlatformStore` supplies the connection and kernel owner APIs, and does not define
Recording's domain records. The Recording service owns current caller admission,
Artifact transport, playback and native Redap integration.

## Schema And Features

The `schema` feature exports the optional `recordings` module without a database
driver or runtime. It claims `recording_*` and `recording`, and requires Tasks and
its earlier kernel requirements. Version zero installs the complete current schema
from `src/schema/migrations/0000_current.surql`. Composition supplies execution;
repository construction and connection startup apply no schema.

The default `persistence` feature exposes repositories and activates the database
client. Schema-only consumers disable defaults. Owner observation declarations
identify native LIVE and changefeed sources without granting read authority.

## Qualification

The `recording_catalog`, `recording_grants` and `recording_projections` integration
suites use isolated native fixtures and current selected lanes. They check database
behavior rather than playback, public authorization or installed image/Job operation.
Those acceptance requirements belong to the
[Recording service design](../../../servers/recording-mcp/DESIGN.md).
