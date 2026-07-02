# Technical Design: an all-in MCP architecture

This document explains the design strategy behind `veoveo-media-mcp` — and
specifically why it looks different from most MCP servers you'll find in the wild.

## The premise: we own both ends

Most MCP servers are written defensively, for the lowest common denominator of clients.
Because typical hosts only reliably surface *tools*, servers compensate by flattening
everything into tools: `search_models`, `get_schema`, `check_status`, `get_result`,
`list_jobs`... The protocol's richer surfaces — resources, templates, completions,
subscriptions, tasks, notifications — sit unused because "clients don't support them."

We reject that premise. **We define the required MCP capability profile and test it.** That inverts the
design pressure: instead of dumbing the server down to weak clients, we push every concern
to the protocol feature that was designed for it, and require compatible MCP clients to
consume it. The client CLI in this repo is not a product client — it is the conformance
test. If a protocol surface exists on the server, the CLI exercises it end to end against
real traffic.

The payoff is a server whose *entire* API is one tool, yet loses nothing:

| Concern | Weak-client answer | Our answer |
|---|---|---|
| "what models exist?" | `search_models` tool | resource `media://models` |
| "what are this model's params?" | `get_schema` tool | resource template `media://model/{model_id}` |
| "autocomplete a model id" | fuzzy tool + prompt engineering | `completion/complete` on the template argument |
| "is my job done?" | `check_status` tool, agent poll-spam | MCP tasks: `tasks/get`, `tasks/result` |
| "tell me when it's done" | impossible; poll | `resources/subscribe` → `notifications/resources/updated` |
| "show progress" | log lines in tool output | `notifications/progress` + task `statusMessage` |
| "abort it" | `cancel_job` tool | `tasks/cancel` |
| namespacing | `media_run`, `media_search`... | server identity (`serverInfo.name = media`) + `media://` URI scheme; tool is just `run` |

Nothing above is exotic. It's all in the spec. Being "all in" simply means using it.

## One tool: `run(model, input)`

The media provider exposes ~1000 models (image, video, audio, 3D, LLM), each with its own input
schema. The classic failure modes for wrapping such a catalog:

- **988 generated tools** — blows every context window, makes `tools/list` useless.
- **A mega-tool with a union schema** — unvalidatable, undiscoverable.
- **A vague pass-through** — "input: object, see docs" — pushes schema discovery out of band.

Our answer is a single task-required tool whose *discovery story lives in the protocol*:

1. `run`'s description points at `media://models` and the model template.
2. The provider registry publishes a real JSON Schema per model.
   We re-publish it, verbatim, as a resource. The client reads the schema and builds input.
3. The server validates `input` against that same schema **before** submitting — precise,
   immediate errors ("`quality` must be one of low|medium|high") instead of a burned
   round-trip or wasted credits. Validation at the boundary is correctness, not client
   babysitting; the client still owns schema-driven construction.

New provider model? Zero code changes anywhere. The registry is cached (1h TTL) and the
same cache backs the catalog resource, the per-model resources, and completions.

## Long-running work: tasks + webhooks

Generation takes seconds to minutes (gpt-image-2 edits run ~2 minutes). Blocking a
`tools/call` for that long fights every transport timeout in the chain. Two async systems
solve the two halves of the problem, and the server fuses them:

- **MCP tasks (SEP-1319)** solve *client ↔ server* async: `tools/call` with `task`
  metadata returns a durable `CreateTaskResult` immediately; the client polls `tasks/get`
  (honoring the server's `pollInterval`), fetches the payload via `tasks/result`, can
  `tasks/cancel` at any time, and survives disconnects because the task id is durable.
- **Provider webhooks** solve *provider -> server* async: we submit with
  `?webhook=<public-url>/webhooks/{provider}`; the provider POSTs the terminal prediction,
  HMAC-SHA256-signed (`{webhook-id}.{webhook-timestamp}.{body}`, `v3,<hex>` header,
  constant-time verified against the account secret).

The fuse is a oneshot channel keyed by prediction id: the tool's task future awaits it,
the webhook handler fires it. When the callback lands, the task completes and the client
learns about it through *protocol events*, not polling luck:

```
Provider ──POST /webhooks (signed)──▶ ingest_prediction()
                                          ├─ resolve oneshot ─▶ task future completes
                                          │     ├─ notifications/tasks/status (Completed)
                                          │     └─ tasks/result payload ready
                                          └─ notifications/resources/updated ─▶ subscribers
```

There is no provider polling path. Missing webhook delivery is an operational failure:
the task eventually fails rather than silently switching to a second provider-status path.

We implement the `tasks/*` handlers manually against our own task store rather than using
rmcp's stock `OperationProcessor`, because we key tasks to provider prediction ids and
want mid-flight `statusMessage` updates ("submitted; prediction X; subscribe
media://prediction/X for updates"). That message is load-bearing: it's how the client
learns the prediction URI to subscribe to *while the task is still running*.

## The URI scheme is the namespace

```
media://models                          catalog index (id, type, description, price)
media://model/{model_id}                full input schema + pricing   (completable)
media://prediction/{id}                 live prediction state         (subscribable)
media://artifact/{sha256}               server-owned output artifact
media://usage                           usage resource index
media://usage/task/{task_id}            estimates/actuals for one task
```

Tool names are scoped per-connection in MCP; hosts that aggregate servers do their own
prefixing (`mcp__media__run`). Prefixing tool names server-side just stutters. The
`media://` scheme is where the namespace actually belongs, and it gives every noun in
the system a stable, linkable identity: task status messages reference prediction URIs,
tool results carry `ResourceLink` blocks, subscriptions target them.

## Results are structured, twice

A completed `run` returns a `CallToolResult` carrying:

- a human-readable text block (model, output count, timing),
- one `ResourceLink` per output (`media://artifact/{sha256}` + mime type) — outputs are
  addressable without exposing provider CDN URLs,
- `structuredContent`: the provider prediction JSON plus artifact metadata for
  programmatic consumers.

Provider output URLs are copied once into the server-owned artifact store, then redacted
from the client-facing result. Inputs the provider must fetch are served from the server's
own `/files/*` static route through the public tunnel URL — the same single process
handles MCP, webhooks, artifacts, and media.

## The client surface is MCP, only MCP

The server binds one HTTP listener, but its routes serve three different parties — and
only one of them is a contract:

- `/mcp` — the protocol's transport. This is the *entire* client surface.
- `/webhooks/{provider}`, `/files/*` — internal necessities for parties that cannot speak
  MCP: providers must POST callbacks somewhere and GET input media from somewhere. These
  routes are plumbing, undocumented for clients, and never carry anything client-facing.
- `/healthz` — ops.

There is deliberately **no client-facing REST API** — no endpoints that list, query, or
mutate anything. Anything a client needs to *know* is reachable through the protocol:
artifacts via `resources/read`, usage via resources, chaining by passing resource URIs in
tool input (the server rewrites them to provider-fetchable URLs internally). Two ways to
learn the same facts means two contracts to version, secure, and keep consistent — and
the HTTP one always wins by convenience until the protocol surface rots. It's the
tool-flattening failure mode wearing a different hat.

**A content host is not a REST API.** MCP is not built for large binaries: SDK transports
cap messages around 4MB, embedded/blob resources are guided to ~1MB, base64 adds 33%, and
the core protocol has no ranged reads, chunking, or resume (SEP-1597/1708/2356 are open
precisely because of this). The spec's own idiom for large content is link-not-blob:
`ResourceLink` blocks may carry any URI, including custom schemes. Artifact results carry
both identities: `media://artifact/{sha256}` as the canonical, protocol-readable one
(blob via `resources/read`), and a `https://…/artifacts/{sha256}` content URL in artifact
metadata for bulk retrieval. That route is GET-only, immutable, content-addressed —
functionally a private CDN. Clients never discover anything there; every URL they touch
was handed to them by the protocol. The moment it grows a second verb, a listing, or any
fact not already in the protocol, it has become an API and broken the rule.

## What "all in" costs, and why it's worth it

The honest trade-off: this server is a poor citizen in a lowest-common-denominator host.
A client that only does `tools/list` + `tools/call` cannot even invoke `run` (task-required
returns `-32601` on plain calls — per spec). We consider that a feature: it makes protocol
support non-optional for our clients, so the capabilities we rely on can never silently rot.

What we get in return:

- **Tiny, stable tool surface.** One tool forever, regardless of catalog growth.
- **Push, not poll.** The webhook's arrival propagates to the client as protocol events
  in the same second.
- **Self-describing.** Schemas, pricing, and live state are all readable, completable,
  and subscribable resources — no out-of-band docs required to drive any model.
- **Symmetric conformance.** The generic contract conformance CLI exercises the same
  protocol contract every Veoveo server is expected to expose.

## Standardization layer: rmcp below, Veoveo policy above

`rmcp` remains the MCP protocol SDK. It gives us protocol types, handler traits,
transport implementation, routing, task request/response models, resources, templates,
and notifications. We do not hide that behind a second generic MCP framework.

The reusable `veoveo-mcp-contract` crate has a narrower job: encode Veoveo's policy layer for
resilient provider-backed generation servers. That layer should standardize the parts
`rmcp` deliberately does not own:

- provider webhook completion,
- durable task recovery across restarts,
- consistent task lifecycle for long-running provider jobs,
- artifact ingestion into a server-owned store,
- `{scheme}://artifact/{sha256}` plus `/artifacts/{sha256}` download URLs,
- usage estimates, actuals, and usage resources,
- URI conventions across providers,
- TTL/GC policy,
- feature extension names such as `ai.veoveo/artifacts` and `ai.veoveo/usage`.

This is not a rule that every MCP server must use tasks. It is a rule that any Veoveo MCP
server wrapping long-lived provider jobs must expose those jobs through MCP tasks, and any
server creating durable artifacts or billable usage must use the standard artifact and
usage surfaces. Fast metadata, search, config, or read-only resource servers can remain
plain `rmcp` tools/resources.

## Veoveo production gateway

The Veoveo platform should provide a first-class MCP gateway inspired by the way larger
MCP platforms assemble registry, auth, policy, hosted runtimes, and observability around
individual servers. The gateway is our product boundary, not a dependency on an external
orchestrator. The first shipped gateway must be dynamic, self-hosted, and secure by
default; "first slice" does not mean anonymous, static, tool-only, or local-dev-only.

The gateway speaks MCP outward and MCP inward: external MCP clients connect to one gateway
profile, while the gateway connects only to hosted Veoveo MCP servers in the first shipped
version.

```
MCP client
  |
  |  MCP over streamable HTTP
  v
Veoveo gateway profile (/mcp/{profile})
  |-- media-mcp
  |-- simulation-mcp
  |-- rl-mcp
  |-- optimization-mcp
```

The first shipped version explicitly excludes third-party or remote MCP servers. Every
upstream server is a Veoveo-hosted server with a typed server manifest, a known URI scheme,
a known mount path, and conformance coverage from `veoveo-mcp-contract`. Direct server
endpoints such as `/media/mcp` remain valid contract targets for internal testing and
service composition, but external clients should normally use the gateway profile endpoint.

The gateway must preserve the full MCP contract. It is not a tool-only aggregator. It must
forward or aggregate the protocol surfaces our servers rely on:

- `tools/list` and `tools/call`,
- `resources/list`, `resources/templates/list`, `resources/read`, and
  `resources/subscribe`/`resources/unsubscribe`,
- `prompts/list` and `prompts/get`,
- `completion/complete`,
- `tasks/list`, `tasks/get`, `tasks/result`, and `tasks/cancel`,
- server notifications such as `tasks/status`, `progress`, `resources/updated`,
  and list-changed notifications.

Profiles and policies serve different jobs. A gateway profile is a curated static MCP
surface such as `default`, `media`, `research`, or `ops`; it decides which hosted servers,
tools, resources, prompts, and schemes are even exposed. Policy is the runtime decision
layer; it decides whether a specific principal may perform a specific action on a specific
tool, task, resource, artifact, or data label at request time. Unknown servers, tools,
resources, prompts, profiles, principals, or data labels are denied.

Because the gateway collapses multiple MCP servers into one outward MCP server, gateway
tool names must be namespaced at the gateway boundary. Direct servers should keep concise
local names such as `run`; the gateway can expose canonical names such as `media__run`.
Resource URIs stay server-owned (`media://artifact/{sha256}`, `media://usage/task/{task_id}`)
because URI schemes are already the resource namespace.

Authentication and authorization are part of the first shipped gateway, not a later add-on.
The gateway must implement MCP-compatible HTTP authorization:

- OAuth 2.0 Protected Resource Metadata for each gateway profile.
- `WWW-Authenticate` challenges that point clients at the profile's protected-resource
  metadata and requested scopes.
- OAuth 2.1/OIDC authorization-code + PKCE for browser-based enterprise SSO.
- MCP Enterprise-Managed Authorization using the
  `io.modelcontextprotocol/enterprise-managed-authorization` extension and ID-JAG exchange.
- MCP OAuth Client Credentials for headless/service principals, preferably with
  private-key JWT client authentication.
- Audience/resource-bound access tokens scoped to one gateway profile.

Reference: [Enterprise-Managed Authorization: Zero-touch OAuth for MCP](https://blog.modelcontextprotocol.io/posts/enterprise-managed-auth/)
announces the stable MCP Enterprise-Managed Authorization extension and frames the IdP as
the centralized policy and audit authority for enterprise MCP access.

The gateway maps authenticated claims to strongly typed Veoveo principals, tenants,
groups, roles, scopes, and data labels. Hosted servers should receive a short-lived
gateway-issued internal token or signed identity assertion, not raw external IdP tokens by
default. Servers remain responsible for enforcing the Veoveo contract on task ownership,
artifact reads, usage reads, and regulated-data labels; gateway policy reduces exposure but
does not replace server-side checks.

Gateway data must be split by sensitivity and lifecycle:

- **Control data**: hosted server manifests, gateway profiles, profile assignments,
  policy sets, environment definitions, tenant records, identity-provider metadata,
  resource authorization server metadata, OAuth client registrations, data-label
  definitions, and secret references. This data is dynamic and durable. It should start as
  typed files plus hot reload or an explicit admin API, then move to a control-plane store
  without changing the MCP contract.
  The file-backed gateway exposes an authenticated admin reload path for mounted profiles;
  the set of mounted profile route ids is immutable during reload and changing it is a
  deploy operation.
  OAuth client registrations are typed control data: each advertised profile auth mode must
  have a matching registered client grant, and each client must explicitly allow the scopes
  required by the profile and its policy rules.
  The enterprise identity provider and the MCP resource authorization server are separate
  control-plane objects: the IdP handles SSO and ID-JAG issuance, while the resource
  authorization server is the issuer/JWKS authority for profile-scoped MCP access tokens.
- **Secret data**: provider API keys, webhook secrets, OAuth client secrets, gateway
  signing keys, JWKS private keys, and token-exchange credentials. Store secret references
  in control data, never secret values. Local development may use `.env`; enterprise
  deployments should use Vault, HCP Vault, cloud secret managers, KMS-backed stores, or
  their approved secret infrastructure.
  The gateway has a typed secret resolver boundary. `env` secrets resolve from the named
  variable, and `vault`/`hcp_vault` secrets resolve from HashiCorp Vault KV v2 locators in
  the form `kv2://{mount}/{path}#field` with optional `?version={n}`. Vault-backed
  resolution requires explicit `VAULT_ADDR` and `VAULT_TOKEN`; no local Vault default is
  accepted. Secret-manager sources that are not implemented fail closed.
- **Runtime state**: gateway task id to upstream task id mapping, subscription ownership,
  request correlation ids, token revocation entries, replay-protection ids, OAuth state,
  ID-JAG exchange state, and short-lived session metadata. This state is operationally
  durable and must survive process restarts.
- **Audit and evidence**: authentication outcomes, policy decisions, tool calls,
  resource reads, task reads/results/cancels, artifact reads, usage reads, admin changes,
  credential resolution outcomes, and security-relevant failures. Audit records must carry
  principal, tenant, profile, method, target server, action, decision, reason code, policy
  version, trace id, and timestamp. They must not contain raw prompts, provider payloads,
  bearer tokens, secrets, signed URLs, artifact bytes, or webhook bodies.
- **Analytics**: usage, cost, latency, error rates, policy-denial rates, and access
  patterns. DuckDB is appropriate for local/server analytics and exportable reporting; it
  is not the secret store.

Regulated-data support is a design requirement. Policy must be able to express and enforce
access for CUI, ITAR, PII, customer-confidential data, export-control labels, tenant
boundaries, project boundaries, user/service principals, group membership, citizenship or
US-person requirements when provided by the IdP, environment, model/provider allowlists,
artifact ownership, usage visibility, and retention state. Classified deployments require
an accredited deployment environment, approved identity provider, approved cryptography,
approved storage, approved network boundary, and approved operations process; the Veoveo
software must provide the hooks and enforcement model, while the deployment proves the
classification boundary.

The shipped gateway must enforce these hard requirements:

- fail closed for unknown profiles, servers, tools, resources, prompts, tasks, artifacts,
  principals, scopes, policy versions, labels, and token issuers,
- authenticate every gateway request except explicitly documented health/readiness probes,
- validate JWT signature, issuer, audience/resource, expiration, not-before, scopes, and
  replay identifiers where applicable,
- bind access tokens to exactly one gateway profile resource,
- use per-method policy checks for `tools/*`, `resources/*`, `prompts/*`, `completion/*`,
  `tasks/*`, artifact reads, usage reads, and admin operations,
- require server-side policy checks inside hosted MCP servers for task ownership, artifact
  access, usage access, and regulated labels,
- issue short-lived internal gateway-to-server tokens or signed assertions; do not pass raw
  external IdP tokens to hosted servers by default,
- support internal mTLS or equivalent authenticated service-to-service transport in
  enterprise deployments,
- export audit and telemetry to platform logs/OpenTelemetry/SIEM without leaking protected
  content,
- support secret rotation by reference, not by redeploying code,
- keep provider completion webhook-only for long-running provider jobs.

The implementation plan is production-gateway-first:

1. Add typed server manifest, gateway profile, principal, tenant, scope, data-label,
   secret-reference, policy, policy-decision, audit-event, token-subject, and runtime-state
   models to `veoveo-mcp-contract`.
2. Create a `mcp-gateway` crate that loads dynamic typed control data and connects to the
   hosted upstreams enabled for a profile, starting with `media-mcp` but not hard-coding
   media into the architecture.
3. Add gateway durability for runtime state and audit/analytics. Use DuckDB where it fits
   analytics and local durability, but keep secret values in `.env` for local development
   and secret-manager references for enterprise deployments.
4. Route `/mcp/{profile}` to the gateway in Compose while keeping provider plumbing such as
   `/media/webhooks`, `/media/files`, and `/media/artifacts` owned by the media server.
5. Implement protected-resource metadata, `WWW-Authenticate` challenges, JWT/JWKS
   validation, audience/resource binding, profile-scoped policy, and structured audit
   events before exposing the gateway as the default client entrypoint.
6. Add OAuth/OIDC authorization-code + PKCE, MCP Enterprise-Managed Authorization / ID-JAG,
   and MCP OAuth Client Credentials. These are required auth modes, not optional demos.
7. Add gateway-to-server internal tokens or signed assertions, server-side verification,
   and policy enforcement inside `media-mcp`.
8. Add conformance modes for direct servers and gateway profiles. Both paths must exercise
   tools, resources, prompts, completions, tasks, usage, artifacts, notifications, auth
   failures, policy denials, and audit emission.
9. Add self-hosted deployment profiles for local, enterprise, and regulated environments.
   Each profile must declare required external services, secret sources, object stores,
   telemetry sinks, allowed ingress, allowed egress, and data-retention behavior.

## Enterprise deployment and pluggable infrastructure

Enterprise deployments should be able to bring their own object store, state store, and
observability stack without changing MCP server code. The server depends on narrow
infrastructure ports, not on a specific local service:

```
Client
  |
MCP over streamable HTTP
  |
MCP server container
  |-- rmcp protocol handlers
  |-- veoveo-mcp-contract policy: tasks, artifacts, usage, recovery
  |-- provider adapter: current media provider, Replicate, OpenAI media, ...
  |
  |-- per-server SQL durable state
  |-- S3-compatible artifact store
  |-- structured logs / OpenTelemetry sink
```

The contract layer should define shared data models and protocol semantics, not duplicate
backend traits already owned by focused crates. Artifact bytes use
`object_store::ObjectStore`; the media server layers Veoveo content addressing, artifact
URIs, DuckDB metadata, and compliance labels over `Arc<dyn ObjectStore>`.

The contract layer should define shared types/services such as:

```rust
trait UsageLedger {
    async fn record_estimate(...);
    async fn record_actual(...);
    async fn query(...);
}

trait EventSink {
    fn emit_task_event(...);
    fn emit_artifact_event(...);
    fn emit_usage_event(...);
}
```

The artifact store contract should target S3-compatible APIs so deployments can use
RustFS locally, AWS S3, Cloudflare R2, Ceph/RGW, MinIO, or another compatible service
without changing MCP behavior.

For regulated data, the important separation is bytes vs. metadata. Artifact bytes live
behind the injected object store; task, prediction, artifact, and usage metadata live in
per-server DuckDB by default. The shared contract owns the DuckDB usage analytics schema
so every MCP server records estimates and actual billing rows the same way. Artifact metadata already has optional classification,
tenant, owner, and retention fields. Server logs must avoid prompts, webhook bodies,
provider output URLs, signed URLs, and raw provider payloads; log only correlation ids
such as `task_id`, `prediction_id`, `artifact_sha256`, `model_id`, and future `tenant_id`.

For logging and observability, MCP servers should emit structured JSON logs to stdout and
OpenTelemetry traces/metrics/logs where configured. Events must carry stable correlation
fields: `task_id`, `prediction_id`, `artifact_sha256`, `provider`, `model_id`, and
eventually `tenant_id`. Enterprise operators can route those signals into Datadog,
Splunk, ELK, Loki, Honeycomb, CloudWatch, or another collector without server changes.

Docker Compose is the local and self-hosted reference deployment, not a hard dependency.
Each MCP server runs as its own container. Shared crates such as `veoveo-mcp-contract` are
compiled into those servers, not deployed as a runtime service. The default Compose stack
should include batteries-included infrastructure:

- one container per MCP server (`veoveo-media-mcp`, future provider servers),
- a mounted DuckDB volume per server for durable task/prediction metadata and shared usage analytics,
- RustFS as the default S3-compatible artifact store,
- an OpenTelemetry collector,
- optional Loki/Grafana or equivalent local log UI.

Enterprise deployments replace defaults by configuration: omit RustFS and point S3
settings at the enterprise object store; omit local logging UI and point OTEL export at
the enterprise collector; provide secrets through their secret manager instead of `.env`.
Compose profiles should make that explicit:

- `default`: MCP servers plus bundled RustFS, state store, OTEL collector, and local logs UI,
- `enterprise`: MCP servers only, expecting external state/object/observability endpoints,
- `dev`: local helpers such as static input files, tunnels, and test fixtures.

The design rule is simple: MCP servers may depend on per-server SQL durability,
S3-compatible artifact storage, and standard telemetry. They must not depend specifically
on RustFS, Loki, or any other default Compose service.

## Verified behavior

Both paths were proven against a production media provider, through a real cloudflared tunnel:

1. `openai/gpt-image-2/edit` — input image served via `/files`, 122.9s inference,
   completed by provider webhook.
2. a text-to-image model — webhook **verified and pushed**,
   task completed in ~2s; client received `resources/updated` and `tasks/status`
   notifications live, then downloaded outputs from the resource links.

Plus: schema validation rejects bad input before submission, `tasks/cancel` aborts
in-flight work, and completions rank prefix matches across the full 988-model registry.
The local workspace tests cover the current artifact/usage URI contract, DuckDB-backed
state and analytics helpers, webhook signature verification, schema extraction, and conformance CLI
build.

Media server retention is enforced locally: terminal task metadata, provider prediction
rows, usage analytics rows, artifact metadata, artifact owners, and artifact bytes are
pruned by configured non-zero retention windows on startup and hourly thereafter. Artifact
metadata records carry `retention_expires_at` evidence, and object bytes are deleted
through the configured `object_store` backend. Gateway audit retention is enforced on the
same startup/hourly cadence for auth audit rows, policy audit rows, expired authorization
records, and expired JWT revocations.

## Known gaps

- **Provider billing timing is asynchronous.** The usage ledger records estimates at submit
  time and provider-confirmed actual billing rows after completion through billing
  reconciliation keyed by the completed prediction id.
- **Tasks are an evolving extension.** SEP-1319 (2025-11-25) is what rmcp 2.0 ships; the
  2026-07-28 spec moves tasks to an extension with `tasks/update` for mid-flight input.
  Owning both ends means we migrate both sides in one commit when rmcp does.
