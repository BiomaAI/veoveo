# Connector Recipes

A Veoveo installation operates beside the platforms an enterprise already
trusts. Connector recipes bring those platforms into the same agentic
surface. Each recipe is a guide a coding agent can execute: it installs the
vendor's MCP server beside the Veoveo connector, authenticates against the
platform, and proves the connection with a real call before any work depends
on it. The catalog below records the verified install surface for every
entry so an agent can begin from this table alone while per-platform recipe
files land in this directory.

Every entry was verified against vendor documentation on 2026-07-23. The MCP
ecosystem moves quickly, so re-verify an entry before relying on it and
update the date when you do.

## Two Ways To Connect

A client-side connector is the default. The coding agent adds the vendor's
server to its own MCP client configuration next to the installation's
`/mcp/{profile}` endpoint and pairs tools across both from the first
session. No cluster change is required, and the platform's own auth and
permissions continue to govern its side.

A governed upstream is the deeper integration. The vendor's server registers
in the gateway control plane as a `streamable_http` upstream, which places
it inside the installation's identity, policy, audit, and task boundary.
Servers that ship as stdio processes reach the gateway through
[`mcp/bridges/stdio`](../../mcp/bridges/stdio/), which re-exposes a child
process over streamable HTTP on an internal network. Register the server and
a profile entry in the control plane document, then validate with
`cargo run -p veoveo-mcp-gateway --bin gateway -- validate --control-plane
<file>`. Choose this path for platforms whose calls should appear in the
installation's audit trail, such as databases with write access or systems
that act on the physical world.

## Catalog

Status meanings: **Official** is a vendor-published server in general
availability. **Official beta/preview** ships from the vendor with a
stability caveat. **Admin gated** means a platform administrator must enable
the surface before an agent can connect. **Community** is a maintained
third-party server, named so the provenance stays visible.

### Geospatial and Earth observation

These pair with the `map`, `frames`, `view`, and `perception` servers:
imagery tasking follows a map release, and matched traces land as governed
feature layers.

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Mapbox | `https://mcp.mapbox.com/mcp` or `npx @mapbox/mcp-server` | OAuth or access token | Official |
| TomTom | `https://mcp.tomtom.com/maps` | API key | Official |
| CARTO | `https://{region}.api.carto.com/mcp/{account_id}` | OAuth or API token | Official |
| SkyFi | `https://mcp.skyfi.com/mcp` | OAuth | Official |
| Planet | `pip install planet-mcp` | Planet SDK login | Official beta |
| NASA Earthdata | `https://cmr.earthdata.nasa.gov/mcp/v1` | None | Official |
| Google Maps | `https://mapstools.googleapis.com/mcp` | API key or OAuth | Official |
| OpenStreetMap | `npx -y @cyanheads/openstreetmap-mcp-server@latest` | None | Community |

### Weather and airspace

Operational weather and live airspace feed mission planning in `uav-sim`,
`time`, and `timeseries` workflows.

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Tomorrow.io | Vendor MCP endpoint over the core weather APIs | API key | Official |
| Meteomatics | `https://mcp.meteomatics.com/mcp` | OAuth with PKCE | Official |
| Flightradar24 | `npx @flightradar24/fr24api-mcp@latest` | `FR24_API_KEY` | Official |

### Data and analytics

External warehouses complement the sandboxed `duckdb`, `datasheet`, and
`timeseries` servers. Exported GeoParquet and RRD-derived datasets remain
governed artifacts on the Veoveo side.

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Databricks | `https://{workspace}/api/2.0/mcp/sql` and sibling managed endpoints | Databricks OAuth | Official |
| Snowflake | `CREATE MCP SERVER`, then the account's `mcp-servers` endpoint | OAuth or access token | Official |
| MotherDuck | `uvx mcp-server-motherduck` or `https://api.motherduck.com/mcp` | Service token | Official |
| ClickHouse | `uv run --with mcp-clickhouse` locally or `https://mcp.clickhouse.cloud/mcp` | Env credentials or OAuth | Official |
| Google BigQuery | `https://bigquery.googleapis.com/mcp` | Cloud IAM | Official |
| PostgreSQL | `uvx postgres-mcp --access-mode=restricted` | `DATABASE_URI` | Community |
| DataHub | `uvx mcp-server-datahub@latest` or the DataHub Cloud tenant endpoint | Personal access token | Official |

### Observability and security

Dashboards, incidents, and security posture for the infrastructure an
installation runs on, alongside the `recording` and `timeseries` evidence
plane.

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Grafana | `docker run grafana/mcp-grafana` | Service account token | Official |
| Datadog | `https://mcp.datadoghq.com/v1/mcp` | OAuth | Official |
| Dynatrace | `https://{env}.apps.dynatrace.com/platform-reserved/mcp-gateway/v0.1/servers/dynatrace-mcp/mcp` | Platform token or OAuth | Official |
| Splunk | Splunkbase app 7931, then `https://{host}:8089/services/mcp` | Splunk RBAC token | Official, admin gated |
| Sentry | `https://mcp.sentry.dev/mcp` | OAuth | Official |
| CrowdStrike | `uvx falcon-mcp` | API client ID and secret | Official preview |
| Tenable | `https://cloud.tenable.com/mcp/` | API key header | Official |

### Industrial operations and fleet

Edge devices, frontline manufacturing, network automation, and vehicle
fleets connect physical operations to the same surface that records and
reasons over them.

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Litmus Edge | `docker run -p 8000:8000 ghcr.io/litmusautomation/litmus-mcp-server:latest` | Litmus Edge device credentials | Official |
| PTC ThingWorx | MCP Server entity embedded in ThingWorx 10.1 | OAuth (RFC 9728) | Official preview, admin gated |
| Tulip | `npx @tulip/mcp-server` | API key and secret with scopes | Official |
| Itential | `uvx itential-mcp` | Platform basic, OAuth, or JWT | Official |
| Geotab | Managed connector enabled from MyGeotab | MyGeotab login | Official |
| Armis Centrix | Tenant endpoint from the Centrix console | Console API secret | Official, admin gated |

### Defense and government

| Platform | Connect | Auth | Status |
|---|---|---|---|
| Palantir Foundry | `npx -y palantir-mcp --foundry-api-url https://{enrollment}.palantirfoundry.com` | `FOUNDRY_TOKEN`, Control Panel enablement | Official, admin enabled |
| GovTribe | `https://govtribe.com/mcp` | API key bearer | Official |
| GovSpend | `https://mcp-spark-prod.govspend.com/mcp` | OAuth, subscription entitled | Official |
| GovInfo (GPO) | Public preview, see govinfo.gov | None | Official preview |

### Engineering and simulation

| Platform | Connect | Auth | Status |
|---|---|---|---|
| MATLAB | `matlab-mcp-server` release binary (stdio) | Local MATLAB R2021a+ | Official |
| ROS | `robotmcp/ros-mcp-server` over rosbridge | rosbridge endpoint | Community |
| Autodesk | Revit 2027 MCP server; ACC through the APS server examples | Autodesk account or service account | Official |
| AWS IoT SiteWise | `uvx awslabs.aws-iot-sitewise-mcp-server@latest` | AWS credentials | Official |

### Cloud control planes

| Platform | Connect | Auth | Status |
|---|---|---|---|
| AWS | `https://aws-mcp.us-east-1.api.aws/mcp` through `uvx mcp-proxy-for-aws` | IAM SigV4 | Official |
| Azure | `npx -y @azure/mcp@latest server start` | Entra ID via `az login` | Official |
| Cloudflare | `https://mcp.cloudflare.com/mcp` and sibling hosted servers | OAuth | Official |

### Work and incident

| Platform | Connect | Auth | Status |
|---|---|---|---|
| PagerDuty | `https://mcp.pagerduty.com/mcp` or `uvx pagerduty-mcp` | User API token | Official |
| Slack | `https://mcp.slack.com/mcp` | OAuth, workspace admin approval | Official |
| Atlassian | `https://mcp.atlassian.com/v1/mcp` | OAuth 2.1 | Official |
| Linear | `https://mcp.linear.app/mcp` | OAuth | Official |
| GitHub | `https://api.githubcopilot.com/mcp/` | OAuth or PAT | Official |

## Watch List

These platforms have announced or gated MCP surfaces that are not yet
recipe-grade. Re-check them before each catalog revision.

| Platform | State as of 2026-07-23 |
|---|---|
| Esri ArcGIS | Location Platform MCP in gated beta, no public endpoint |
| NVIDIA Isaac Sim | Official docs-search MCP; live sim control remains community |
| Inductive Automation Ignition | MCP Module in early access, GA planned for the 8.3.5 line |
| AVEVA PI | Announced at AVEVA World 2026, nothing shipping |
| Salesforce | Hosted server GA but Enterprise Edition and admin enabled |
| ServiceNow | Included in Now Assist SKUs, instance admin configured |
| Bentley | `openstaad-mcp` shipped; iTwin MCP remains roadmap |
| SentinelOne | `purple-mcp` official and read-only, early |
| Palo Alto Cortex | Open beta, package downloadable only inside a tenant |
| Wiz | Preview, customer gated |

## Known Dead Ends

Do not install these. They circulate in older guides and agent training
data.

| Do not use | Why | Use instead |
|---|---|---|
| `@modelcontextprotocol/server-postgres` | Archived July 2025 after a SQL injection finding | `crystaldba/postgres-mcp` |
| Anthropic reference Slack and Google Maps servers | Archived | First-party remote endpoints above |
| `https://mcp.asana.com/sse` | v1 endpoint shut down May 11, 2026 | `https://mcp.asana.com/v2/mcp` |
| Opsgenie anything | Product end of life | Atlassian |
| `@dynatrace-oss/dynatrace-mcp-server` | Deprecated by Dynatrace | Hosted gateway endpoint above |
| `genai-toolbox` | Renamed | `googleapis/mcp-toolbox` |
| "Lattice MCP" servers | They target Lattice the HR platform, not Anduril Lattice | Nothing exists for Anduril |
| Datadog `/api/unstable/` MCP path | Superseded | `https://mcp.datadoghq.com/v1/mcp` |

## Gaps The Platform Already Fills

No recipe-grade MCP server exists for MAVLink or PX4 vehicle control, for
Rerun recordings, for Foxglove, or for OPC UA data access. The `uav-sim`,
`recording`, and `perception` servers are the governed answer to the first
three, which is worth stating plainly when a prospect asks how those systems
connect.

## Recipe Format

Each platform receives one file in this directory following
[`TEMPLATE.md`](TEMPLATE.md). A recipe leads with the client-side install
for Claude Code, proves itself with a runnable verification step, pairs the
vendor's tools with Veoveo tools in worked prompts, and adds the governed
upstream registration when the platform warrants it. Recipes begin with read
access and widen scopes only on explicit request. Community servers say so
in the first line.
