# Shared DuckDB Runtime

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| DuckDB 1.5.5 SQL and C API | The workspace's pinned Rust adapter runs SQL in caller-selected read-only or writable connections. Services own their SQL and database files. |
| HTTPS and HTTP redirects | Service-side downloads use reqwest with automatic redirects disabled. Each request checks an allowed host, resolves public addresses and pins the selected address while TLS authenticates the host. |
| WHATWG URL Standard | The foundational `HttpsUrl` uses URL 2.5.8. Initial source URLs and resolved redirects admit HTTPS without credentials or fragments. The runtime applies network policy separately. |
| JSON and Base64 | Query cells convert into JSON-compatible values; binary values use Base64. This is an internal Rust result representation. |
| DuckDB Spatial | A caller may configure the pinned trusted Spatial extension and its native or GeoJSON longitude/latitude axis policy. Startup verifies effective settings. |

## Ownership

`engine.rs` owns connection configuration, file-access restrictions, trusted extension
loading, statement validation and query limits. `source.rs` owns request directories
and service-side materialization of inline bytes, authorized Artifact bytes and HTTPS
sources. The library has no MCP handler, source registry, credential store or domain
scope vocabulary. DuckDB, Map and Timeseries own those policies and their source models.

The [DuckDB server design](../../../servers/duckdb-mcp/DESIGN.md) defines hosted database
ownership, tools and publication. [Map](../../../servers/map-mcp/DESIGN.md) owns registered
acquisition sources. [Timeseries](../../../servers/timeseries-mcp/DESIGN.md) owns forecast
materialization and output.

## Engine Access

Callers provide a database path, read mode, attachments and `EngineSettings`. File access
is denied or confined to a supplied service root or request directory. Spill files use
a separate directory. The runtime disables engine network access and configuration
changes after loading the caller's explicitly selected trusted extensions. SQL receives
no credential or download authority.

`QueryLimits` controls row and byte materialization and whether to count remaining rows.
Interactive reads stop at the first limit; their observed row count is a lower bound
when truncated. The result includes column descriptions, complete materialized rows,
the observed count and the truncation flag. Owners validate their public result types.

## Source Materialization

Download entrypoints accept `HttpsUrl`. The type's [foundational profile](../../types/DESIGN.md#https-network-urls)
preserves signed query spelling and supplies a parsed URL. A typed URL grants no network
access. `HttpsSourcePolicy` still requires an allowed host and rejects private or reserved
DNS answers. Each redirect is resolved with the URL library, admitted under the HTTPS
profile and checked against the same host/address policy. The client carries credential
headers only to the original host. Request-failure diagnostics omit source URLs because
their query parameters may carry credentials.

The policy caps connect time, total time, redirects and downloaded bytes. Optional media
type restrictions apply before writing response bytes. Failed downloads remove their
partial file. `RequestWorkspace` owns and removes request and spill directories. Services
using the directory-based materializers own that directory's lifecycle.

`AuthorizedArtifact` accepts bytes obtained after the caller's Artifact-plane read
authorization. Local materialization applies the byte cap and filename checks; it does
not perform Artifact authorization itself.

## Qualification

Native tests cover sandbox settings, statement handling, row/byte limits, file isolation,
download denial, private-address rejection, redirect profiles and Artifact byte limits.
A compile-fail example rejects an unchecked string at the download API. Tests use
temporary directories and perform no visual or GPU workload. Domain services qualify
their policy and public contracts separately.
