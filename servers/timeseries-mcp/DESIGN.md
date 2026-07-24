# Timeseries MCP Server

Forecasting server: `forecast` materializes a typed DuckDB source, fits the
configured method per series, logs observed rows, forecast quantiles, and
provenance into a Rerun RRD artifact on the shared artifact plane, and returns
structured output.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with one task-capable tool, resources and templates, structured content, and usage resources. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Forecast source, mapping, horizon, output, and app-call argument contracts. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; forecasting executes through durable create, status, cancellation, result, and subscription operations. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; the self-contained `ui://timeseries/forecast.html` view uses the sandboxed host bridge. |
| CSV, JSON/NDJSON, and Apache Parquet | Governed inline, HTTPS, or artifact sources are materialized through the shared DuckDB source contract. |
| [Rerun](https://rerun.io/docs/) RRD | Full-resolution observations, forecast quantiles, and provenance are encoded into an immutable recording artifact. |
| SVG | The MCP App renders its bounded preview as inline vector graphics without external network access. |

## MCP surface

| Kind | Name | Notes |
|---|---|---|
| tool | `forecast` | task-capable; structured output `TimeseriesForecastOutput` |
| resource | `ui://timeseries/forecast.html` | MCP App view (see below) |
| resource | `timeseries://usage` | usage ledger index |
| resource template | `timeseries://usage/task/{task_id}` | per-task usage rows |
| resource template | `timeseries://artifact/{artifact_id}` | immutable RRD artifact blob |

Structured output carries three layers:

- `forecast` — the summary (method, horizon, per-series row counts).
- `preview` — downsampled chartable series (observed points plus
  mean/q10/q90 forecast steps, capped at 500 points per series by
  `PREVIEW_POINTS_PER_SERIES`). This exists so app views and other clients
  can chart without re-reading the RRD.
- `artifact` — metadata for the full-resolution Rerun recording.

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
