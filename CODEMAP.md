# Veoveo Code Map

Snapshot: `8fa9b24` on 2026-07-03.

This map explains what each area of the repository is for, where the code volume is
concentrated, and which parts look like technical debt or still-open product gaps.

## High-level shape

Veoveo is currently a Rust workspace with five crates:

| Crate | Rust lines | Purpose |
|---|---:|---|
| `mcp-gateway` | 14,522 | Public MCP gateway, auth, policy, admin, audit, and upstream MCP forwarding. |
| `mcp-contract` | 9,507 | Shared Veoveo contract: typed IDs, gateway config, policy, internal auth, artifacts, usage, deployment, schemas, pagination, telemetry. |
| `media-mcp` | 4,009 | Hosted media MCP server: provider-backed generation, webhook completion, artifacts, usage, prompts, resources, tasks. |
| `smoke` | 3,881 | Rust smoke-test harness for process orchestration and end-to-end checks. |
| `mcp-conformance` | 3,233 | CLI that exercises MCP surfaces and emits contract schemas/test tokens/fake services. |

Total Rust source is about 35k lines. Including root docs, JSON config, Compose, Dockerfiles,
and Justfile, the counted source/docs/config total is about 38k lines.

Runtime flow:

```text
external MCP client
  -> Cloudflare tunnel / enterprise edge
  -> Caddy edge (:8780)
  -> mcp-gateway (:8788, /mcp/default)
  -> media-mcp (:8787, /media/mcp)
  -> provider API
  <- provider webhook to /media/webhooks
```

The gateway is the external client contract. `media-mcp` is still a valid hosted MCP
server internally and for conformance, but public clients should not connect to
`/media/mcp` directly.

## Root files

| Path | Purpose |
|---|---|
| `Cargo.toml` | Workspace membership, Rust 2024 edition, shared dependency versions. |
| `Cargo.lock` | Pinned dependency graph. |
| `README.md` | Operator-facing architecture, setup, run, routing, logs, and local process notes. |
| `TECH_DESIGN.md` | Design rationale: protocol-rich MCP, webhook-only provider completion, gateway/auth/security direction. |
| `AGENTS.md` | Engineering rules for this repo: hard cut, no fallbacks, strong types, module boundaries, Justfile discipline. |
| `Justfile` | Human command dispatcher for build/test/smoke/compose/e2e. It should not become a test framework. |
| `compose.yaml` | Local stack: RustFS, media, gateway, edge, OTEL. |
| `compose.tunnel.yaml` | Cloudflare named tunnel service. |
| `configs/Caddyfile` | Public edge routing from one origin to gateway and media provider/content routes. |
| `configs/gateway.local.json` | Local gateway control plane for Compose. |
| `configs/gateway.smoke.json` | Test gateway control plane for smoke scenarios. |
| `configs/jwks.smoke.json` | Local test JWKS trusted by smoke/local headless auth. |
| `configs/deployments.json` | Typed self-hosted deployment profile examples. |
| `configs/otel-collector.yaml` | Local OpenTelemetry collector config. |

## `mcp-contract`

This is the policy and schema layer above `rmcp`. It should contain only provider-neutral
contract concepts that future Rust/Python/TypeScript MCP servers must share.

Important files:

| Path | Purpose |
|---|---|
| `src/lib.rs` | Public re-export surface. This is intentionally broad because other crates consume the contract from here. |
| `src/gateway.rs` | Gateway control-plane aggregate type and validation entrypoint. |
| `src/gateway/ids.rs` | Strong newtypes for IDs: profiles, servers, tools, tenants, JWT IDs, scopes, etc. |
| `src/gateway/server_config.rs` | Server manifests, upstream transport/security, profile exposure, OAuth client config. |
| `src/gateway/auth_config.rs` | IdP, authorization server, JWKS, OIDC/OAuth config types. |
| `src/gateway/policy.rs` | Policy actions, targets, effects, decisions, reason codes. |
| `src/gateway/validation.rs` | Cross-reference and security validation for control-plane config. |
| `src/gateway/runtime_state.rs` | Durable projection records: task mappings, subscriptions, OAuth state, revocations. |
| `src/gateway/wire.rs` | JSON wire-shape helpers for contract models. |
| `src/gateway/error.rs` | Typed validation errors. Large because validation is explicit and fail-closed. |
| `src/gateway/tests.rs` | Contract validation tests. Large but valuable. |
| `src/deployment.rs` | Typed deployment profile model: object store, state store, telemetry, network boundaries, regulated-data controls. |
| `src/internal_auth.rs` | Gateway-to-server internal signed identity assertions. |
| `src/analytics.rs` | Shared DuckDB usage analytics schema. |
| `src/storage.rs` | Artifact metadata and compliance metadata. |
| `src/usage.rs` | Usage record/report contract. |
| `src/tasks.rs` | Task store helpers, timestamps, progress/status notifications, related task metadata. |
| `src/uri.rs` | Canonical `{scheme}://...` resource URI parsing and helpers. |
| `src/host.rs` | Host header validation helpers. |
| `src/telemetry.rs` | Shared tracing/OpenTelemetry setup. |

Why it is large:

- Strong typed IDs and enums replace loose strings.
- Gateway/auth/policy config has many cross references that are validated at startup.
- Regulated-data and self-hosted deployment concerns are encoded as types instead of prose.
- JSON Schema export depends on the contract being explicit.

## `mcp-gateway`

This is currently the largest crate. It is both an MCP server outward and an MCP client
inward. It owns the public MCP profile, auth, policy, audit, admin control-plane changes,
and protocol-preserving forwarding to hosted MCP servers.

Important library areas:

| Path | Purpose |
|---|---|
| `src/lib.rs` | Public gateway library exports. |
| `src/catalog.rs` | Loaded/validated control-plane index and lookup helpers. |
| `src/catalog/tests.rs` | Catalog/profile/policy validation tests. |
| `src/auth.rs` | Auth facade and tests for JWT/client assertion/ID-JAG/OIDC verification. |
| `src/auth/*` | Split auth implementations: access token, client assertion, ID-JAG, OIDC, principal extraction. |
| `src/mcp.rs` | Gateway MCP handler skeleton and upstream lifecycle. |
| `src/mcp/*` | Protocol surface forwarding/aggregation: tools, resources, prompts, completions, tasks, authz, upstream HTTP/cache/progress. |
| `src/policy.rs` | Runtime policy request construction and helper mapping from MCP methods/URIs. |
| `src/mcp_support.rs` | MCP utility conversions/errors/pagination support. |
| `src/metadata.rs` | Protected-resource and authorization-server metadata. |
| `src/secrets.rs` | Secret reference resolution, env/Vault support, redaction. |
| `src/state.rs` | Gateway DuckDB state facade. Large and should keep shrinking into `src/state/*`. |
| `src/state/*` | State schema, auth state, task mapping, subscriptions, audit persistence. |
| `src/principal_audit.rs` | Audit metadata extraction from principals. |
| `src/tool_name.rs` | Gateway tool-name projection, e.g. `media__run`. |

Important binary areas:

| Path | Purpose |
|---|---|
| `src/bin/gateway.rs` | CLI entrypoint and subcommands. Should stay thin. |
| `src/bin/gateway/server.rs` | Axum route wiring and streamable HTTP MCP service setup. |
| `src/bin/gateway/runtime.rs` | Runtime state structs, HTTP client construction, readiness, GC loop. |
| `src/bin/gateway/auth.rs` | HTTP auth middleware and metadata/JWKS endpoints. |
| `src/bin/gateway/oauth.rs` | Token endpoint grant dispatch. |
| `src/bin/gateway/oauth_client_credentials.rs` | Client credentials with private-key JWT. |
| `src/bin/gateway/oauth_grants.rs` | Authorization-code token exchange. |
| `src/bin/gateway/oauth_grants/id_jag.rs` | MCP Enterprise-Managed Authorization / ID-JAG exchange. |
| `src/bin/gateway/oauth/*` | Browser OAuth authorize/callback flow. |
| `src/bin/gateway/admin/*` | Authenticated control-plane reload/apply and JWT revocation admin operations. |
| `src/bin/gateway/audit.rs` | Audit recording for admin/auth/policy operations. |
| `src/bin/gateway/http_util.rs` | HTTP/JWKS/secret utility code. |
| `src/bin/gateway/tokens.rs` | Gateway access-token issuance and JWKS from signing key. |

Why it is large:

- It is not a tool-only proxy. It preserves tools, resources, templates, prompts,
  completions, tasks, subscriptions, notifications, artifacts, and usage.
- Auth is enterprise-grade by design: protected-resource metadata, browser OAuth,
  client credentials, private-key JWT, ID-JAG, JWKS, revocation, audit.
- It records durable runtime state and policy/audit evidence.

## `media-mcp`

This is the first hosted domain server. It should stay provider-neutral externally while
using provider-specific implementation internally.

Important files:

| Path | Purpose |
|---|---|
| `src/lib.rs` | Media library module exports. |
| `src/provider.rs` | Internal provider API client and provider model/prediction types. |
| `src/artifacts.rs` | S3-compatible artifact persistence through `object_store`. |
| `src/state.rs` | Media DuckDB state: predictions, artifacts, task owners, usage. |
| `src/uris.rs` | `media://...` resource URI helpers. |
| `src/webhook.rs` | Provider webhook signature verification. |
| `src/bin/server.rs` | Axum + rmcp server wiring and MCP handler. Still dense. |
| `src/bin/server/config.rs` | CLI/env config. |
| `src/bin/server/generation_task.rs` | Webhook-only provider task execution. |
| `src/bin/server/ownership.rs` | Principal/task/artifact ownership and regulated-label checks. |
| `src/bin/server/usage.rs` | Estimate and actual usage recording/reconciliation. |
| `src/bin/server/prompts.rs` | MCP prompts for model selection/edit/video/task review. |
| `src/bin/server/internal_auth.rs` | Gateway-signed internal token verification. |
| `src/bin/server/retention.rs` | Task/artifact/usage retention GC. |
| `src/bin/server/outputs.rs` | Client-facing prediction/artifact output shaping. |
| `src/bin/server/host.rs` | Host header enforcement. |
| `src/bin/server/app_state.rs` | Shared runtime state. |

Why it is not tiny:

- A single `run` tool is hiding a lot of protocol surface: resource catalog, model schemas,
  task lifecycle, webhook fuse, artifact ingest, usage, prompts, completions, ownership,
  retention, and internal auth.
- The code is intentionally not flattening this into many tools.

## `mcp-conformance`

This is the generic MCP client/test CLI. It is not product runtime.

Important files:

| Path | Purpose |
|---|---|
| `src/bin/conformance.rs` | CLI dispatch and shared imports. |
| `src/bin/conformance/cli.rs` | Command-line shape. |
| `src/bin/conformance/client.rs` | rmcp client connection setup. |
| `src/bin/conformance/mcp_commands.rs` | Human/conformance commands: info, resources, prompts, complete, run, usage, artifact. |
| `src/bin/conformance/auth_discovery.rs` | Protected-resource/auth metadata checks. |
| `src/bin/conformance/tokens.rs` | Test JWKS, private key, client assertions, ID-JAG, token exchange helpers. |
| `src/bin/conformance/fake_services.rs` | Fake provider/IdP/hosted MCP/OTLP services for smoke tests. |
| `src/bin/conformance/control_plane.rs` | Test control-plane generation helpers. |
| `src/bin/conformance/schema.rs` | JSON Schema export for external implementations. |

Why it is large:

- It is exercising protocol surfaces that normal MCP examples ignore.
- It also contains fake services so smoke tests do not depend on real IdPs/providers.

## `smoke`

This is the Rust smoke-test framework. It exists because the Justfile should not carry
complex process lifecycle, retries, assertions, JSON parsing, or cleanup.

Important files:

| Path | Purpose |
|---|---|
| `src/bin/smoke.rs` | CLI dispatch for smoke scenarios. |
| `src/bin/smoke/scenarios.rs` | Scenario module wiring. |
| `src/bin/smoke/scenarios/basic.rs` | Compose/config/schema basics. |
| `src/bin/smoke/scenarios/media.rs` | Direct media server auth/task smoke tests. |
| `src/bin/smoke/scenarios/gateway/http.rs` | Gateway HTTP boundary, auth discovery, browser OAuth. |
| `src/bin/smoke/scenarios/gateway/authenticated.rs` | Gateway-to-media auth, policy, admin flows. |
| `src/bin/smoke/scenarios/gateway/two_servers.rs` | Gateway namespacing/routing with two upstreams. |
| `src/bin/smoke/scenarios/gateway/task_run.rs` | Full gateway task run with webhook completion and usage. |
| `src/bin/smoke/scenarios/secrets.rs` | Secret resolution/Vault behavior. |
| `src/bin/smoke/support/*` | Process lifecycle, HTTP, MCP, auth, usage, control-plane, assertions. |

## Where most code is

Largest areas:

| Area | Rust lines | Reading |
|---|---:|---|
| `mcp-gateway` | 14,522 | Biggest runtime surface: full MCP gateway plus enterprise auth/policy/audit/admin. |
| `mcp-contract/src/gateway` | 5,317 | Typed control plane, policy, auth config, validation, tests. |
| `mcp-gateway/src/bin/gateway` | 4,709 | HTTP routes, OAuth, admin, audit, runtime wiring. |
| `smoke` + `mcp-conformance` | 7,114 | Test/conformance machinery, not production server runtime. |
| `media-mcp` | 4,009 | Domain server, provider integration, artifacts, usage, ownership. |

Largest individual files:

| Lines | File | Reading |
|---:|---|---|
| 1,677 | `mcp-contract/src/gateway/tests.rs` | Large validation test suite. Acceptable, though can be split if navigation hurts. |
| 994 | `mcp-gateway/src/catalog/tests.rs` | Large catalog/policy tests. Acceptable but near split threshold. |
| 969 | `media-mcp/src/bin/server.rs` | Runtime file near the repo's 1,000-line extraction threshold. |
| 967 | `mcp-contract/src/deployment.rs` | Dense typed deployment model and validation. Split soon. |
| 966 | `mcp-gateway/src/state.rs` | State facade plus tests; should continue splitting. |
| 897 | `mcp-contract/src/gateway/validation.rs` | Dense validation rules. Reasonable, but should not absorb unrelated policy logic. |
| 773 | `mcp-conformance/src/bin/conformance/fake_services.rs` | Fake services. Acceptable test support, but could split by fake service type. |
| 692 | `mcp-contract/src/gateway/error.rs` | Many explicit validation errors. Verbose but useful. |
| 654 | `media-mcp/src/state.rs` | Media DuckDB state. Watch as features grow. |
| 634 | `mcp-contract/src/gateway/server_config.rs` | Typed config. Watch as server manifests grow. |

## Technical debt and cleanup targets

1. `media-mcp/src/bin/server.rs` is still too dense.
   It wires routes and implements a lot of MCP handler behavior. Next extraction should split
   direct handler surfaces into `resources`, `tasks`, `tools`, and `http_routes` modules.

2. `mcp-gateway/src/state.rs` is near the size limit.
   It already has `src/state/*`, but the facade still contains many tests and helpers. Move
   table-specific tests next to table modules and keep `state.rs` as the public facade.

3. `mcp-contract/src/deployment.rs` is a dense model file.
   Split into `deployment/types.rs`, `deployment/validation.rs`, and `deployment/tests.rs`
   before adding more deployment concepts.

4. `configs/gateway.local.json` and `configs/gateway.smoke.json` duplicate most of their
   shape.
   This is manageable now, but it will drift. Prefer a typed config builder, generator, or
   smaller overlays before adding more server profiles.

5. `configs/gateway.local.json` still contains `idp.example.com` placeholders for the
   enterprise IdP.
   That is acceptable as a local placeholder, but risky if treated as deployable config.
   We should split example/template enterprise IdP config from local runnable config.

6. The Justfile is still 264 lines.
   It is mostly dispatch now, but Cloudflare tunnel configuration remains shell-heavy. If it
   grows again, move tunnel/admin orchestration into a Rust admin/devops helper instead of
   expanding Justfile logic.

7. `mcp-conformance` overlaps with `smoke` in auth/test support.
   This is acceptable today because conformance is a CLI and smoke is process orchestration,
   but shared test support may need a small crate if duplication increases.

8. Enterprise concepts are present before a full product UI/control plane exists.
   `tenant`, `data_label`, policies, secret refs, and deployment profiles are justified by
   CUI/ITAR/PII goals, but they need a product-level control-plane story so they do not become
   static JSON ceremony.

## Gaps

1. Production IdP integration is not proven against a real enterprise IdP in this repo.
   The code supports OIDC/OAuth/JWKS/ID-JAG paths and fake services test the flows, but real
   Okta/Entra/Auth0-style deployments need integration tests and docs.

2. Multi-replica gateway state is not solved by local DuckDB.
   DuckDB is good for local analytics/state, but a horizontally scaled gateway will need a
   shared transactional control/runtime store or a single-writer deployment model.

3. Production mTLS/service-mesh enforcement is modeled but not fully wired.
   The contract declares `tls`, `mutual_tls`, and `service_mesh_mtls`; Compose uses internal
   HTTP plus signed assertions. Regulated deployments need tested cert/service-mesh wiring.

4. Python/TypeScript contract support is schema-first, not SDK-first.
   JSON Schema export exists, but there are no generated Python/TypeScript packages yet.

5. Dynamic control-plane UX is API/Postgres-backed, not a product console.
   Admin apply exists, but server/profile/policy management still needs a product-grade UI/API.

6. Authorization policy is strongly typed but still basic ABAC/RBAC.
   It checks principals, scopes, profiles, tools/resources/tasks/artifacts, tenants, and data
   labels, but richer enterprise policy features may need a policy language or external PDP.

7. Observability is wired for logs/traces and audit summaries, but SIEM/export packaging is
   not a full deployment product yet.

## Is this over-engineered?

Some of it is intentionally ahead of the current one-server product:

- A full gateway is heavier than needed for only `media-mcp`.
- Enterprise auth and policy create a lot of code before multiple real tenants/users exist.
- Typed deployment/network/regulated-data models are more ceremony than a small local app needs.

But this is mostly aligned with the stated product direction: self-hosted, secure MCP gateway
and hosted servers for confidential/regulatory workloads. The risk is not the direction; the
risk is letting early static config and dense files harden into the architecture.

The pragmatic path is:

1. Keep the crate split.
2. Split the few dense files before adding features to them.
3. Generate or compose config instead of duplicating giant JSON files.
4. Add real IdP and mTLS integration tests before claiming enterprise readiness.
5. Keep future servers small by using `mcp-contract` for common artifact/usage/auth/task
   pieces instead of copying media server code.
