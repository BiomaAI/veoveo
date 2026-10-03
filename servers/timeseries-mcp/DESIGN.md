# Timeseries MCP Server

Forecasting server: `forecast` materializes a typed DuckDB source, fits the
configured method per series, logs observed rows, forecast quantiles, and
provenance into a Rerun RRD artifact on the shared artifact plane, and returns
structured output.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Protocol version `2026-07-28`; JSON-RPC 2.0 over stateless Streamable HTTP with Discover, one task-capable tool, resources and templates, structured content, and usage resources. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Forecast source, mapping, horizon, output, and app-call argument contracts. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; forecasting executes through durable create, status, cancellation, terminal `tasks/get` payloads, and `subscriptions/listen`. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; the self-contained `ui://timeseries/forecast.html` view uses the sandboxed host bridge. |
| CSV, JSON/NDJSON, and Apache Parquet | Inline CSV and HTTPS sources are materialized through the shared DuckDB runtime; forecast does not materialize Artifact inputs. |
| [Rerun 0.38.1](https://rerun.io/docs/) RRD | Full-resolution observations, forecast quantiles, and provenance are encoded into an immutable recording artifact. |
| SVG | The MCP App renders its bounded preview as inline vector graphics without external network access. |
| Veoveo MCP server contract | Revision 3, including canonical result handoff, bounded discovery, and the 8 MiB final serialized-response cap. |
| Veoveo usage resource profile | `timeseries://usage` pages and native UUIDv7 Task addresses; the shared URI component profile and version 1 Base64 cursor described under Usage Reads. |

The forecast request imports `DuckDbTabularSource` and read SQL rendering from the DuckDB
server library with only its `contract` feature enabled. Timeseries owns source
materialization and forecasting; this dependency provides no DuckDB hosted runtime.
The source type and schema admit inline CSV and HTTPS inputs. Artifact input belongs
to DuckDB's broader source profile and is rejected during forecast decoding.
Source URLs use foundational `HttpsUrl`, and URL lists use DuckDB's nonempty source
type. Forecast decoding applies those profiles before the worker creates a workspace.
Provenance keeps the same types until serialization. The [shared runtime](../../platform/runtimes/duckdb/DESIGN.md)
still enforces allowed hosts, public DNS addresses, redirects and byte/time limits.
Reader options also use the DuckDB contract's checked names and closed value enum.
Forecast decoding rejects malformed options before materialization, and the worker
renders admitted options through the same infallible SQL helper.

Forecast construction requires `TimeseriesForecastHorizon` with 1–100,000 steps and
`DuckDbColumnName` for mapped and filtered columns. Column names preserve their spelling
and reject blank values or NUL. Filter numbers must be finite; `in` lists and predicate
lists require at least one member. Builders and JSON decoding enforce these rules before
Task admission, and JSON Schema declares the same bounds. The extraction query selects
non-null finite observations and applies the training filter in SQL while preserving
each observation's original row position.

## Library Features

The library's `contract` feature exposes forecast DTOs, the complete `TimeseriesResource`
vocabulary, typed Artifact and usage addresses, cursor positions, and pages. Consumers disable default features. Its dependencies
contain serialization, schema and foundational values plus DuckDB's contract feature;
they exclude the MCP runtime, database engines, network clients and Rerun runtime.
`runtime` adds forecasting, Artifact access, owner models and the usage reader.
`mcp` adds the hosted binary and is enabled by default.

## Hosted Resource Admission

`TimeseriesArtifactUri::new` requires an Artifact occurrence ID and delegates component
construction to the Artifact contract. The decoder requires the Timeseries scheme and
rejects escaped aliases, extra segments, queries and fragments. The forecast producer
constructs its result address from the returned occurrence ID. Public resource dispatch
parses `TimeseriesResource` once and matches its closed route variants; embedded document
IDs use `TimeseriesDocument`.

These routes use the foundational URL 2.5.8 concrete-resource profile and their declared
RFC 6570 templates. `TimeseriesContract` implements `McpServerContract` and validates
startup configuration, resource descriptors and templates before Store initialization.
MCP initialization and discovery consume that setup. The empty `TimeseriesScope` enum
declares no additional permissions; gateway policy, Task owner selection and the Artifact
service continue to authorize requests. Native template expansion and an independent
contract consumer qualify route admission without a hosted service.

## MCP surface

| Kind | Name | Notes |
|---|---|---|
| tool | `forecast` | task-capable; structured output `TimeseriesForecastOutput` |
| resource | `ui://timeseries/forecast.html` | MCP App view (see below) |
| resource | `timeseries://usage` | usage ledger index |
| resource template | `timeseries://usage{?cursor}` | bounded usage-ledger page selected by an opaque cursor |
| resource template | `timeseries://usage/task/{task_id}` | per-task usage rows |
| resource template | `timeseries://artifact/{artifact_id}` | immutable RRD artifact blob |

Structured output carries three layers:

- `result_uri` — the canonical `timeseries://artifact/{artifact_id}` handoff
  for the immutable full-resolution product.
- `forecast` — the summary (method, horizon, per-series row counts).
- `preview` — downsampled chartable series (observed points plus
  mean/q10/q90 forecast steps, capped at 500 points per series by
  `PREVIEW_POINTS_PER_SERIES`). This exists so app views and other clients
  can chart without re-reading the RRD.
- `artifact` — metadata for the full-resolution Rerun recording.

Human content contains a short identity-free completion status and one resource
link for `result_uri`. `resources/list` advertises the stable usage root and
templates without enumerating task records. Usage reads return at most 100
authorized task identities in stable task order, with a versioned opaque cursor;
the per-task URI remains an exact lookup. The shared transport discards any
final serialized JSON response larger than 8 MiB and returns the canonical
response-budget diagnostic without partial content.

`subscriptions/listen` admits task IDs only. A resource-only filter is rejected,
and a mixed filter acknowledges only its task IDs. The server advertises neither
resource subscriptions nor resource-list changes because it has no resource event
source.

## Usage Reads

`TimeseriesUsage` binds reads to the Timeseries Task runtime. TaskRuntime selects
usage under the caller's principal, profile, optional tenant and complete label
clearance in SQL. The usage row and its Task must agree on server and tenant;
the Task record and stored owner envelope must agree on identity. These checks
precede grouping, ordering and the 101-row lookahead that supplies each 100-entry
page. Exact reads apply the same predicate in one query. A missing or inaccessible
Task produces the same resource-not-found response. This usage policy does not
require an additional Work Context match.

`contract::usage` owns `TimeseriesTaskUsageUri`, `TimeseriesUsageIndexUri`,
`TimeseriesUsageCursor`, entries and pages. Constructors use the foundational URI
component builder and require native UUIDv7 Task identities. Parsing rejects aliases,
fragments and unknown or duplicate query parameters. Entries derive their Task ID
from the URI; decoding rejects conflicting identities. Pages enforce ascending unique
Task IDs, the fixed limit, and agreement between a continuation cursor and the last
entry of a full page.

The cursor uses URL-safe unpadded Base64 over the version 1 JSON object
`{"version":1,"task_id":"<uuid>"}`. It is a position, and every subsequent query
checks current visibility. The page fields are `usage`, `limit` and optional
`next_cursor`; terminal pages omit the cursor. The URI profile is a Veoveo extension
over the shared [resource component profile](../../platform/types/DESIGN.md).

### Usage Deployment And Qualification

Deploy Timeseries replicas and usage-page consumers together. The foundations rollout
uses a fresh disposable installation and the current request/source format. Installed
acceptance exercises multiple pages with denied rows, exact usage reads, renewed
authority and current-format recovery. Historical data conversion and mixed-format
replicas are outside this rollout.

`tests/usage_contract.rs` checks wire preservation, typed construction and malformed
inputs. `tests/usage.rs` uses the isolated pinned Store fixture to check page filling,
cursor continuation, current labels and parent metadata through the library reader.
TaskRuntime owns the complete shared owner-policy matrix. Contract isolation is
qualified with an independent consumer; runtime-only builds check feature composition.

## MCP App (ext-apps "2026-01-26")

The server declares `io.modelcontextprotocol/ui` in its capabilities
(`veoveo-mcp-apps-extension`) and ships one app view:

- `ui://timeseries/forecast.html`, MIME `text/html;profile=mcp-app`, embedded
  via `include_str!` from `assets/forecast-app.html` — a fully self-contained
  HTML document (no external fetches; enforced by `forecast_app_is_self_contained`).
- The `forecast` tool carries `_meta.ui = {resourceUri, visibility: ["model","app"]}`,
  so hosts render the view alongside tool results and the view itself may
  re-invoke `forecast` (e.g. with a different horizon) through the host bridge.
- The view renders the `preview` series as an SVG line chart (observed solid,
  forecast mean dashed, 10–90% band shaded; validated categorical palette,
  light/dark from `hostContext.theme`) and reports its height via
  `ui/notifications/size-changed`.

The gateway manifest must set `resource_projection: server_owned` and
`capabilities.apps: true` (validated: apps ⇒ resources + server-owned), expose
`ui://timeseries/` to profiles, and grant `resource_schemes: ["timeseries","ui"]`
in policy — see `configs/gateway.local.json`.

## App security posture (host contract)

- The app is self-contained by contract; hosts apply a deny-all frame CSP
  (`default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline';
  img-src data:`). The `_meta.ui.csp` allow-lists are intentionally unused by
  first-party servers.
- The console host renders the view in an opaque-origin iframe
  (`sandbox="allow-scripts"`, no `allow-same-origin`): no cookies, storage, or
  network. Its only capability is the postMessage bridge; `tools/call` is
  proxied through the console BFF, which allows only app-visible tools of this
  server linked to this view, and the gateway re-authorizes every call under
  the operator's policy. Worst case for malicious view HTML is calling this
  server's policy-allowed tools as the signed-in operator — the same power the
  model already has.
- Size caps: app HTML ≤ 2 MiB (host-enforced and locally tested), call
  arguments ≤ 256 KiB, call results ≤ 2 MiB (console BFF).
