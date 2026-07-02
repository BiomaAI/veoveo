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

The contract layer should define ports such as:

```rust
trait ArtifactStore {
    async fn put(...);
    async fn get(...);
    async fn head(...);
}

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
per-server SQLite by default. Artifact metadata already has optional classification,
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
- a mounted SQLite volume per server for durable task/prediction metadata,
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
The local workspace tests cover the current artifact/usage URI contract, SQLite-backed
state helpers, webhook signature verification, schema extraction, and conformance CLI
build.

## Known gaps

- **Provider actual billing is not available yet.** The usage ledger records provider-accepted
  estimates from registry pricing/formula metadata. It does not invent actual usage rows
  until a provider payload exposes billable actuals.
- **Subscription identity is coarse.** Unsubscribe clears all peers for a URI — fine for
  owned single-client deployments, wrong for multi-tenant.
- **No task/artifact GC.** Completed task entries and artifacts need explicit retention
  policy enforcement.
- **Tasks are an evolving extension.** SEP-1319 (2025-11-25) is what rmcp 2.0 ships; the
  2026-07-28 spec moves tasks to an extension with `tasks/update` for mid-flight input.
  Owning both ends means we migrate both sides in one commit when rmcp does.
