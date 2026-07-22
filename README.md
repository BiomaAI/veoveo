# Veoveo

Veoveo is a self-hosted MCP platform for governed tools, durable work, artifacts,
recordings, and autonomous agents. Each installation is owned and operated by the
organization that deploys it. Veoveo has no vendor control plane, required hosted
service, or required domain name.

`veoveo.bioma.ai` is one deployment example under `examples/bioma`; it is not a
product dependency or canonical hostname.

## What It Provides

- An MCP gateway that aggregates hosted servers into policy-scoped profiles.
- Full MCP surfaces where the domain fits: tools, resources and templates, prompts,
  completions, final durable tasks, subscriptions, notifications, structured
  content, and URI identities.
- OIDC/OAuth browser login, PKCE, client credentials, ID-JAG, signed access tokens,
  durable refresh-token rotation, encrypted duplicate delivery, and replay-family
  revocation.
- Short-lived Ed25519 gateway-to-service identity assertions. Hosted servers receive
  only public verification keys.
- A required SurrealDB `3.2.1` platform store for identity, policy, control revisions,
  tasks, artifacts, recordings, agents, audit, and the transactional outbox.
- A shared artifact plane with opaque UUIDv7 occurrence identities, tenant-local
  deduplication, Work Context ownership, user/group grants, governed access
  requests, and expiring revocable anyone-with-link shares.
- Arbitrary DuckDB SQL inside an owner-scoped, resource-bounded container sandbox.
- Durable Rerun recording ingestion and an authorized recording MCP projection.
- Local recorded-video perception through a provider-neutral MCP server backed by
  NVIDIA DeepStream and TensorRT, with no LLM or hosted inference dependency.
- A durable autonomous-agent runtime with task detach/resume, wakes, budgets, local
  analytical memory, and Rerun episode recording.
- An authenticated operations console for health, tasks, artifacts, agents,
  recordings, MCP topology, policy, audit, and installation state.
- One Kubernetes and Helm installation shape, a GPU-capable k3d development
  profile, and a verified offline bundle path.

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
           SurrealDB 3.2.1                      S3 / RustFS bytes
```

SurrealDB is the durable coordination authority. DuckDB is an analytical runtime,
not the platform database. RRD segments are the durable time-and-space record. S3
compatible storage owns artifact bytes while SurrealDB owns their governed identity
and authorization records.

## Hosted Servers

The canonical control plane defines fifteen server identities:

| Server | Main capability |
|---|---|
| `media` | provider-neutral media catalog, schemas, generation, webhook completion |
| `timeseries` | forecasting with governed artifacts and durable RRD output |
| `time` | authority-bound time resolution, calendars, clock quality, timelines, and events |
| `duckdb` | arbitrary query/execute/ingest/export SQL in bounded workspaces |
| `optimization` | deterministic planning and artifact output |
| `frames` | WGS84, ECEF, ENU, and NED frame conversion and durable batches |
| `map` | Earth geography, governed map releases, restrictions, and logistics routing |
| `datasheet` | dataset preview, column statistics, and task-based profiling (Python template) |
| `artifact` | artifact discovery, metadata, grants, release, and sharing |
| `recording` | governed recording discovery, query, subscription, and publication |
| `perception` | governed Rerun video extraction, local detection/tracking, and derived annotations |
| `reason` | governed semantic and temporal reasoning over recorded video with audited world-model output |
| `charts` | chart rendering projected through the gateway |
| `rerun` | bridged Rerun viewer MCP surface |
| `view` | GPU-backed 3D Tiles views and reproducible offscreen frame capture |

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

Every task, recording, agent, and artifact belongs to a Work Context. The gateway
resolves direct, delegated, or automated invocation authority, then hosted services
retain that trusted provenance and apply the context's output ownership, initial
grants, classification, and labels. See
[`docs/WORK_CONTEXT_GOVERNANCE.md`](docs/WORK_CONTEXT_GOVERNANCE.md) for the
neutral enterprise model and rollout contract.

Two sharing modes are separate and explicit:

1. Grant `read`, `write`, or `admin` to an authorized user or group. Tenant and label
   policy still applies.
2. Mark an artifact `releasable`, then create a read-only anyone-with-link bearer.
   Links default to seven days, may not exceed thirty days, can have a download limit,
   and can be revoked.

Authorized large downloads pass through the gateway policy/audit boundary before a
short-lived object-store redirect is returned. Public links use `/s/{token}`; only a
token hash is stored. The chart isolates `/s` in a dedicated Ingress with an explicit access-log
disable annotation; operators using another controller must replace it with that
controller's equivalent and apply the same suppression in APM/WAF/tracing. Domain
servers expose no independent byte routes.

## Operations Console

The first console screen is the live installation, not a landing page. The React UI
and Rust BFF support:

- service and MCP health;
- task progress, recovery class, and cancellation;
- artifact download, release state, grants, link creation, and revocation;
- effective artifact access, invocation provenance, and access-request review;
- agents, wakes, recordings, policies, and audit evidence.

The BFF performs authorization-code PKCE, keeps access and rotating refresh tokens in
an encrypted HttpOnly cookie, and enforces CSRF on mutations. Browser JavaScript never
receives a gateway bearer token. A short gateway delivery window lets concurrent
stateless BFF requests receive the identical rotated successor; use of the consumed
token after that window is replay and revokes the family.

## GPU Execution Contract

Veoveo treats hardware GPU access as part of the execution contract for simulation,
perception, 3D rendering, Rerun, and visual acceptance. Required workloads request an
NVIDIA device and fail closed when CUDA, Vulkan, WebGPU, or WebGL cannot reach hardware.
Software rendering is not a supported fallback. Browser-driven verification must prove
hardware WebGPU and WebGL before it interacts with the product, and it must stop if
either context becomes unavailable.

## Develop With k3d

Local container development uses k3d and the same Helm chart as a fielded
installation. The pinned profile currently uses k3d 5.9.0, Kubernetes 1.36.2,
kubectl 1.36.2, and Helm 4.2.3. Its custom K3s node installs the NVIDIA Container
Toolkit and fails closed when the host GPU is unavailable.

```bash
just k3d-node-build
just profile-cluster-up showcase/sumo/deploy/deployment.json
just sumo-k3d-status
```

Disposable showcase workloads are selected by typed local deployment profiles. Images
publish once under a full Git revision and move through the shared local OCI registry
by layer. The SUMO profile is the complete local proof:

```bash
REVISION=$(git rev-parse HEAD)
just profile-validate showcase/sumo/deploy/deployment.json
just profile-publish showcase/sumo/deploy/deployment.json "$REVISION"
just profile-up showcase/sumo/deploy/deployment.json "$REVISION"
just showcase-sumo-verify
```

The cluster maps its canonical loopback ingress to `http://localhost:8780` and
SUMO MCP verification to `127.0.0.1:8895`. See
[`deploy/local/k3d/README.md`](deploy/local/k3d/README.md) for GPU validation,
profile isolation, and registry delivery. The reusable contract is documented in
[`docs/LOCAL_DEPLOYMENT_PROFILES.md`](docs/LOCAL_DEPLOYMENT_PROFILES.md).

The Bioma example deploys the complete installation in the `veoveo-bioma`
cluster and connects `veoveo.bioma.ai` through Cloudflare Tunnel. It publishes
immutable OCI charts, bootstraps enterprise-owned Argo CD separately, and reconciles
the platform plus an independently packaged MCP extension from Git. See
[`docs/ENTERPRISE_DEPLOYMENT.md`](docs/ENTERPRISE_DEPLOYMENT.md) for the neutral
contract and [`examples/bioma/README.md`](examples/bioma/README.md) for the executable
installation and acceptance sequence.

## Install With Helm

The Helm chart is under `deploy/helm/veoveo`. It uses one SurrealDB 3.2.1 RocksDB
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

The bundle contains pinned runtime images, Veoveo images, Helm material,
versioned configuration schemas, checksums, resolved image identities, and SPDX SBOMs.
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
configs/                    canonical installation configuration
deploy/                     Helm and offline installation material
examples/bioma/             enterprise GitOps reference installation
tools/screenshots/          repeatable authenticated Console and Rerun capture tool
docs/screenshots/           screenshot catalog, gallery, and capture runbook
```

Detailed ownership and call paths are in [`docs/CODEMAP.md`](docs/CODEMAP.md) and
[`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md). The current product views are collected
in the [`docs/screenshots` gallery](docs/screenshots/GALLERY.md).
