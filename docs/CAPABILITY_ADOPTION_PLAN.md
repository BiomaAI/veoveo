# Capability Adoption Plan: Weather, Tabular Prediction, And Skills

Status: recorded. No track is approved or started. Each track opens through an
explicit approval and lands as its own hard cut with contract conformance,
acceptance evidence, and documentation in the same change.

Baseline: Veoveo main `b3813e50` on 2026-08-25.

This plan records the implementation shape for three capabilities: a hosted
weather domain server, a hosted tabular prediction server, and adoption of the
MCP skills extension. It preserves the hosted-server contract, the one-domain
rule per server, provider-neutral user-facing naming, and the existing GPU
placement and artifact provenance boundaries.

## Standards And Protocols

| Standard or profile | Plan boundary |
|---|---|
| [Model Context Protocol](../mcp/contract/DESIGN.md) `2026-07-28` | Every new server complies with hosted-server contract revision 3 and is discovered by the existing `servers/*-mcp/` conformance glob. |
| [MCP skills extension, SEP-2640](https://github.com/modelcontextprotocol/experimental-ext-skills) | Experimental draft. Skills are served through the Resources primitive with no new protocol methods. Adoption pins the exact draft revision and records it in the contract standards table. |
| [`templates/python-mcp`](../templates/python-mcp/) | Initial implementation path for both new servers, following the `datasheet` precedent. |
| JSON Schema 2020-12 | Closed schemas for every tool input, tool output, and artifact payload the tracks introduce. |
| Artifact plane and provenance | Weather products and predictions land as immutable artifacts whose provenance names sources, inputs, and backend identity. |
| GPU placement policy ([`GPU_PLACEMENT.md`](GPU_PLACEMENT.md)) | Prediction inference requests its accelerator and fails closed without it. Weather serving is CPU-bound and requests none. |

## Track A: Weather Domain Server

Purpose: operational weather as a governed capability. Mission planning in
`uav-sim`, `map`, `time`, and `timeseries` workflows currently has no hosted
weather surface, and external weather MCP endpoints reach installations only
as ungoverned client-side connectors.

Shape:

- `servers/weather-mcp`, slug `weather`, scheme `weather://`, built from the
  Python server template.
- Provider adapters remain internal. User-facing tools stay provider-neutral:
  current conditions, point and route forecasts, active alerts, and aviation
  weather for named stations. Route forecasts accept the map server's
  canonical route handoff.
- Resources expose stations, zones, and retained weather products. Weather
  products land as artifacts with source, query, and validity provenance.
- Historical observations flow into Timeseries through its ordinary ingestion
  surface rather than a private store.

Integration points: route weather over `veoveo.io/map-route-handoff/v1`
inputs, wind and visibility context for UAV mission admission, validity
windows through Time, and archived observations for forecasting.

Interim posture: a community weather MCP endpoint may join an installation as
a governed `streamable_http` upstream through the existing control plane
while this track lands. That registration is installation configuration, not
part of this plan.

Acceptance: contract conformance passes by discovery, one acceptance scenario
records a route forecast informing a UAV mission admission decision as
evidence, and the smoke harness covers the server lifecycle.

## Track B: Tabular Prediction Server

Purpose: in-context prediction over governed tables. The platform can query,
transform, and forecast operational data, but it has no capability that
classifies or regresses over an arbitrary governed table.

Shape:

- `servers/predict-mcp`, slug `predict`, scheme `predict://`, built from the
  Python server template with GPU inference.
- The domain contract is backend-neutral: an in-context tabular classifier
  and regressor over mixed numeric and categorical columns. Tabular
  foundation model backends load behind that contract, and the exact backend
  identity is recorded in every prediction's provenance.
- Inputs are governed tables: DuckDB result artifacts and tabular artifacts
  from the artifact plane. Context-row selection is bounded and recorded in
  provenance, because which rows formed the in-context training set is part
  of what makes a prediction defensible.
- Tools: task-augmented prediction over a table, holdout evaluation, and
  backend inspection. Predictions and evaluation results land as artifacts.
- Holdout evaluation results are shaped so a later scorer under
  [`SELF_IMPROVING_HARNESS.md`](SELF_IMPROVING_HARNESS.md) can consume them
  without rework.

Integration points: DuckDB result tables as prediction inputs, Timeseries
features joined into tables, and mission-outcome tables derived from
recordings and episodes as the first operational subjects.

Acceptance: contract conformance passes by discovery, one acceptance scenario
classifies a mission-outcome table derived from existing recordings and lands
the prediction artifact with full provenance, and GPU admission fails closed
without the accelerator.

## Track C: MCP Skills Extension

Purpose: servers shipping structured how-to knowledge that teaches agents how
to orchestrate tools across the catalog, discovered and governed like every
other capability.

Shape:

- `mcp/skills-extension`, a contract crate following the
  [`mcp/apps-extension`](../mcp/apps-extension/DESIGN.md) precedent: typed
  skill records, validation, and the pinned SEP-2640 draft revision.
- Skills are resources. Each hosted server may ship skills beside its
  existing well-known docs and contract resources, under its own canonical
  URI scheme.
- The gateway projects skills per profile with the same admission semantics
  as tools: a skill is visible only when every tool it orchestrates is
  admitted by the caller's profile, and discovery failure follows the
  profile's declared discovery failure mode.
- Agents load skills through the governed current-profile resource adapter
  with its existing byte, count, and admitted-text bounds. Agent manifests
  declare admitted skills.
- Conformance extends the existing docs checks: every shipped skill
  references only tools that exist in the owning server's surface.

Skills become an improvable surface under the scorer primitive once measured
acceptance lands, in the same row as tool descriptions and prompts.

Acceptance: the contract crate validates the pinned draft, one existing
server ships one skill, the gateway projects it under profile admission, and
an agent exercises the skill in an acceptance scenario recorded as evidence.

Gate: SEP-2640 is a draft in pull-request form. This track opens either when
the extension stabilizes upstream or through an explicit early-adoption
decision recorded with the exact pinned revision.

## Sequencing

The tracks are independent. Weather and tabular prediction share the Python
template path and may proceed in either order. Skills adoption waits on its
gate. No track starts before its approval is recorded in this status line,
and each track updates [`CODEMAP.md`](CODEMAP.md), the README capability
surfaces, and its server documents in the same change that lands the code.
