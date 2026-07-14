# Veoveo

Veoveo is a self-hosted MCP platform for governed tools, durable work, artifacts,
recordings, and autonomous agents. Each installation is owned and operated by the
organization that deploys it. Veoveo has no vendor control plane, required hosted
service, or required domain name.

`veoveo.bioma.ai` is one deployment example under `examples/bioma`; it is not a
product dependency or canonical hostname.

## What It Provides

- A typed MCP gateway that aggregates hosted servers into policy-scoped profiles.
- Full MCP surfaces where the domain fits: tools, resources and templates, prompts,
  completions, final durable tasks, subscriptions, notifications, structured
  content, and URI identities.
- OIDC/OAuth browser login, PKCE, client credentials, ID-JAG, signed access tokens,
  durable refresh-token rotation, encrypted duplicate delivery, and replay-family
  revocation.
- Short-lived Ed25519 gateway-to-service identity assertions. Hosted servers receive
  only public verification keys.
- A required SurrealDB `3.2.0` platform store for identity, policy, control revisions,
  tasks, artifacts, recordings, agents, audit, and the transactional outbox.
- A shared artifact plane with opaque UUIDv7 occurrence identities, tenant-local
  deduplication, user/group grants, and expiring revocable anyone-with-link shares.
- Arbitrary DuckDB SQL inside an owner-scoped, resource-bounded container sandbox.
- Durable Rerun recording ingestion and an authorized recording MCP projection.
- Local recorded-video perception through a provider-neutral MCP server backed by
  NVIDIA DeepStream and TensorRT, with no LLM or hosted inference dependency.
- A durable autonomous-agent runtime with task detach/resume, wakes, budgets, local
  analytical memory, and Rerun episode recording.
- An authenticated operations console for health, tasks, artifacts, agents,
  recordings, MCP topology, policy, audit, and installation state.
- Equivalent Docker Compose and Helm installation shapes, plus a verified offline
  bundle path.

The normative product boundaries are in
[`docs/ARCHITECTURE_DECISIONS.md`](docs/ARCHITECTURE_DECISIONS.md).

## Architecture

```text
Browser / MCP client
        |
        v
  installation ingress
   |       |        |
   |       |        +--> /s/{token} -> artifact-service
   |       +-----------> /console, /auth -> console-bff
   +-------------------> /mcp/*, /oauth/*, /admin/*,
                           /artifacts/* -> mcp-gateway
                                           |
                  gateway-signed identity  |
             +-------------+---------------+----------------+
             |             |               |                |
          media-mcp    duckdb-mcp     recording-mcp    perception-mcp
             |             |               |                |
             +-------------+--------+------+----------------+
                                    |
                         artifact-service / recording-hub
                                    |
                  +-----------------+-----------------+
                  |                                   |
           SurrealDB 3.2.0                      S3 / RustFS bytes
```

SurrealDB is the durable coordination authority. DuckDB is an analytical runtime,
not the platform database. RRD segments are the durable time-and-space record. S3
compatible storage owns artifact bytes while SurrealDB owns their governed identity
and authorization records.

## Hosted Servers

The canonical control plane defines twelve server identities:

| Server | Main capability |
|---|---|
| `media` | provider-neutral media catalog, schemas, generation, webhook completion |
| `timeseries` | typed forecasting and durable RRD output |
| `duckdb` | arbitrary query/execute/ingest/export SQL in bounded workspaces |
| `optimization` | deterministic planning and artifact output |
| `frames` | WGS84, ECEF, ENU, and NED frame conversion and durable batches |
| `map` | Earth geography, governed map releases, restrictions, and logistics routing |
| `datasheet` | dataset preview, column statistics, and task-based profiling (Python template) |
| `artifact` | artifact discovery, metadata, grants, release, and sharing |
| `recording` | governed recording discovery, query, subscription, and publication |
| `perception` | governed Rerun video extraction, local detection/tracking, and derived annotations |
| `charts` | chart rendering projected through the gateway |
| `rerun` | bridged Rerun viewer MCP surface |

The SUMO showcase adds a provider-neutral `sumo` traffic-world server without
changing platform contracts. See [`showcase/sumo/README.md`](showcase/sumo/README.md).

## Durable Tasks

Long operations use the shared `TaskRuntime` and the final Veoveo task extension.
Task IDs are UUIDv7; creation, leases, transitions, cancellation, results, retention
pins, and outbox events are durable and atomic.

Recovery is declared per operation:

- `resume`: deterministic work may resume after lease expiry.
- `webhook_wait`: a submitted provider job waits only for a signed webhook.
- `interrupted_indeterminate`: mutating work is failed and never replayed.

Provider completion is webhook-only. There is no provider status polling or polling
fallback.

Cancelling a submitted media task makes the local task result permanently cancelled and
records a durable provider-cancellation request plus its outcome. The provider request is
best effort: acknowledgement does not guarantee that compute stopped or that billing was
refunded. A later signed terminal webhook may update the provider job and reconcile actual
billing, but it cannot create artifacts or replace the cancelled task result.

## DuckDB SQL

DuckDB intentionally accepts arbitrary SQL. Flexibility is the product feature; the
security boundary is the execution sandbox:

- owner-derived database paths and one canonical workspace registry;
- locked DuckDB settings and extension policy;
- memory, thread, spill, row, byte, and execution limits;
- governed artifact/ingest inputs and explicitly authorized HTTPS attachment;
- container filesystem, capability, process, and network restrictions;
- `query` and `export` may resume; mutating `execute` and `ingest` become
  `interrupted_indeterminate` once execution may have begun.

## Artifacts And Sharing

Every occurrence receives a new `artifact://{uuidv7}` identity. Hashes are integrity
and tenant-local deduplication data, never public addresses.

Two sharing modes are separate and explicit:

1. Grant `read`, `write`, or `admin` to an authorized user or group. Tenant and label
   policy still applies.
2. Mark an artifact `releasable`, then create a read-only anyone-with-link bearer.
   Links default to seven days, may not exceed thirty days, can have a download limit,
   and can be revoked.

Authorized large downloads pass through the gateway policy/audit boundary before a
short-lived object-store redirect is returned. Public links use `/s/{token}`; only a
token hash is stored. The default Caddy edge suppresses these bearer paths from
access logs. Helm isolates `/s` in a dedicated Ingress with an explicit access-log
disable annotation; operators using another controller must replace it with that
controller's equivalent and apply the same suppression in APM/WAF/tracing. Domain
servers expose no independent byte routes.

## Operations Console

The first console screen is the live installation, not a landing page. The React UI
and Rust BFF support:

- service and MCP health;
- task progress, recovery class, and cancellation;
- artifact download, release state, grants, link creation, and revocation;
- agents, wakes, recordings, policies, and audit evidence.

The BFF performs authorization-code PKCE, keeps access and rotating refresh tokens in
an encrypted HttpOnly cookie, and enforces CSRF on mutations. Browser JavaScript never
receives a gateway bearer token. A short gateway delivery window lets concurrent
stateless BFF requests receive the identical rotated successor; use of the consumed
token after that window is replay and revokes the family.

## Install With Compose

Prerequisites are Docker with Compose v2 and enough resources to build the Rust
workspace. The canonical Compose topology also includes `perception-mcp`; its
Ubuntu runtime requires an NVIDIA driver, NVIDIA Container Toolkit, NGC access,
and a GPU supported by DeepStream 9. macOS is a development host for the Rust
layers, not an NVIDIA runtime. Copy and populate the installation environment:

```bash
cp .env.example .env
```

Required values include SurrealDB bootstrap/runtime credentials, object-store
credentials, the gateway Ed25519 private key and public JWKS, authorization-server
signing material, OIDC client secret, a distinct 32-byte gateway refresh-delivery key,
console session key, media webhook secret, and `PUBLIC_BASE_URL`. Generate the refresh
delivery key with `openssl rand -base64 32`; the decoded value must be exactly 32 bytes.
Update `configs/gateway.local.json` for the installation's OIDC issuer, tenant mapping,
public origin, and client registrations. Set `PERCEPTION_CONFIG_DIR` and
`PERCEPTION_MODEL_DIR` to the model-specific DeepStream configuration and TensorRT
engine roots described in
[`docs/PERCEPTION_MCP_DESIGN.md`](docs/PERCEPTION_MCP_DESIGN.md).

Validate before startup:

```bash
just gateway-validate
just deployments-validate
just smoke-compose-config
```

Start the canonical single-host installation:

```bash
just compose-up
just compose-ps
just health
```

The local edge binds to `127.0.0.1:8780`. Public exposure belongs to the installation
operator's ingress. The canonical stack does not start a tunnel.

Useful entrypoints are:

```text
{PUBLIC_BASE_URL}/console/
{PUBLIC_BASE_URL}/mcp/operator
{PUBLIC_BASE_URL}/mcp/admin
{PUBLIC_BASE_URL}/healthz
{PUBLIC_BASE_URL}/readyz
```

Direct hosted MCP ports are loopback development targets and are blocked at the public
edge. Provider webhooks and curated provider-fetchable media remain plumbing routes.

## Install With Helm

The Helm chart is under `deploy/helm/veoveo`. It uses one SurrealDB 3.2.0 RocksDB
StatefulSet, separate bootstrap/runtime Secrets, default-deny NetworkPolicy, optional
strict service-mesh mTLS, a singleton persistent DuckDB workspace, governed recording
storage, and operator-supplied telemetry/SIEM configuration.

```bash
just helm-check
```

See [`deploy/helm/veoveo/README.md`](deploy/helm/veoveo/README.md) for required
Secrets, ConfigMaps, object-store ingress, and offline values.

## Offline Installation

Create the bundle on a connected build host, then verify and import it on the isolated
host:

```bash
just offline-bundle
just offline-load output/veoveo-offline-0.1.0.tar.gz docker /opt/veoveo
```

The bundle contains pinned runtime images, Veoveo images, Compose and Helm material,
typed configuration schemas, checksums, resolved image identities, and SPDX SBOMs.
Loading retains all verification evidence. See
[`deploy/offline/README.md`](deploy/offline/README.md).

## Development And Verification

The workspace is pinned by `rust-toolchain.toml` and uses Rust edition 2024. Building
it natively needs a C/C++ toolchain, `cmake`, `pkg-config`, and SQLite development
files, because `proj-sys` compiles PROJ from source for Map and its dependents
(the conformance client and the gateway):

```bash
sudo apt-get install build-essential cmake pkg-config sqlite3 libsqlite3-dev
```

GitHub-hosted CI runners and the server Dockerfiles already carry these packages.
Docker is required for every SurrealDB-backed test and smoke, and [`uv`](https://docs.astral.sh/uv/)
runs the Python platform package and the datasheet template.

Common checks are:

```bash
just fmt
just check
just test
just test-python
just test-perception
just smoke-gateway
just smoke-hub
just smoke-datasheet
just smoke-agent-kernel
just showcase-sumo-smoke
```

All smoke orchestration is Rust. The `Justfile` only builds or dispatches human-facing
commands.

The whitepaper and harness PDFs render from their canonical `docs/*-print.html`
sources with `just docs-pdf`, which drives headless Chrome. Pass `chrome=` when the
browser binary has a different name. Never edit the PDFs directly.

## Repository Map

```text
agents/                     agent kernel and durable runtime
apps/console/               browser BFF and React operations UI
mcp/                        shared MCP contracts, extensions, and bridges
platform/                   gateway, persistence, task, artifact, recording, and query runtimes
servers/                    hosted MCP servers, including artifact and recording projections
testing/                    protocol conformance and Rust multi-process smoke harnesses
sdk/python/                 Python platform package for hosted MCP servers
templates/python-mcp/       canonical Python server template (datasheet)
showcase/sumo/              real SUMO world, simulator image, and showcase MCP server
configs/                    canonical typed installation configuration
deploy/                     Helm and offline installation material
examples/bioma/             optional Entra and Cloudflare deployment example
```

Detailed ownership and call paths are in [`docs/CODEMAP.md`](docs/CODEMAP.md) and
[`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md).
