# Technical Design: an all-in MCP architecture

This document explains the design strategy behind `wavespeed-mcp-server` — and specifically
why it looks different from most MCP servers you'll find in the wild.

## The premise: we own both ends

Most MCP servers are written defensively, for the lowest common denominator of clients.
Because typical hosts only reliably surface *tools*, servers compensate by flattening
everything into tools: `search_models`, `get_schema`, `check_status`, `get_result`,
`list_jobs`... The protocol's richer surfaces — resources, templates, completions,
subscriptions, tasks, notifications — sit unused because "clients don't support them."

We reject that premise. **We build both the server and the clients.** That inverts the
design pressure: instead of dumbing the server down to weak clients, we push every concern
to the protocol feature that was designed for it, and require our clients to consume it.
The client in this repo is not a demo — it is the conformance test. If a protocol surface
exists on the server, the client exercises it, end to end, against real traffic.

The payoff is a server whose *entire* API is one tool, yet loses nothing:

| Concern | Weak-client answer | Our answer |
|---|---|---|
| "what models exist?" | `search_models` tool | resource `wavespeed://models` |
| "what are this model's params?" | `get_schema` tool | resource template `wavespeed://model/{model_id}` |
| "autocomplete a model id" | fuzzy tool + prompt engineering | `completion/complete` on the template argument |
| "is my job done?" | `check_status` tool, agent poll-spam | MCP tasks: `tasks/get`, `tasks/result` |
| "tell me when it's done" | impossible; poll | `resources/subscribe` → `notifications/resources/updated` |
| "show progress" | log lines in tool output | `notifications/progress` + task `statusMessage` |
| "abort it" | `cancel_job` tool | `tasks/cancel` |
| namespacing | `wavespeed_run`, `wavespeed_search`… | server identity (`serverInfo.name = wavespeed`) + `wavespeed://` URI scheme; tool is just `run` |

Nothing above is exotic. It's all in the spec. Being "all in" simply means using it.

## One tool: `run(model, input)`

WaveSpeed exposes ~1000 models (image, video, audio, 3D, LLM), each with its own input
schema. The classic failure modes for wrapping such a catalog:

- **988 generated tools** — blows every context window, makes `tools/list` useless.
- **A mega-tool with a union schema** — unvalidatable, undiscoverable.
- **A vague pass-through** — "input: object, see docs" — pushes schema discovery out of band.

Our answer is a single task-required tool whose *discovery story lives in the protocol*:

1. `run`'s description points at `wavespeed://models` and the model template.
2. WaveSpeed's registry (`GET /api/v3/models`) publishes a real JSON Schema per model.
   We re-publish it, verbatim, as a resource. The client reads the schema and builds input.
3. The server validates `input` against that same schema **before** submitting — precise,
   immediate errors ("`quality` must be one of low|medium|high") instead of a burned
   round-trip or wasted credits. Validation at the boundary is correctness, not client
   babysitting; the client still owns schema-driven construction.

New WaveSpeed model? Zero code changes anywhere. The registry is cached (1h TTL) and the
same cache backs the catalog resource, the per-model resources, and completions.

## Long-running work: tasks + webhooks, fused

Generation takes seconds to minutes (gpt-image-2 edits run ~2 minutes). Blocking a
`tools/call` for that long fights every transport timeout in the chain. Two async systems
solve the two halves of the problem, and the server fuses them:

- **MCP tasks (SEP-1319)** solve *client ↔ server* async: `tools/call` with `task`
  metadata returns a durable `CreateTaskResult` immediately; the client polls `tasks/get`
  (honoring the server's `pollInterval`), fetches the payload via `tasks/result`, can
  `tasks/cancel` at any time, and survives disconnects because the task id is durable.
- **WaveSpeed webhooks** solve *provider → server* async: we submit with
  `?webhook=<public-url>/webhooks/wavespeed`; WaveSpeed POSTs the terminal prediction,
  HMAC-SHA256-signed (`{webhook-id}.{webhook-timestamp}.{body}`, `v3,<hex>` header,
  constant-time verified against the account secret).

The fuse is a oneshot channel keyed by prediction id: the tool's task future awaits it,
the webhook handler fires it. When the callback lands, the task completes and the client
learns about it through *protocol events*, not polling luck:

```
WaveSpeed ──POST /webhooks (signed)──▶ ingest_prediction()
                                          ├─ resolve oneshot ─▶ task future completes
                                          │     ├─ notifications/tasks/status (Completed)
                                          │     └─ tasks/result payload ready
                                          └─ notifications/resources/updated ─▶ subscribers
```

A slow poll of the WaveSpeed API (30s) backstops lost callbacks — or runs standalone when
no public URL is configured. This isn't hypothetical: during E2E, a stale webhook secret
caused every signature check to fail, and the run still completed via fallback. Push is
the fast path; poll is the guarantee.

We implement the `tasks/*` handlers manually against our own task store rather than using
rmcp's stock `OperationProcessor`, because we key tasks to WaveSpeed prediction ids and
want mid-flight `statusMessage` updates ("submitted; prediction X; subscribe
wavespeed://prediction/X for updates"). That message is load-bearing: it's how the client
learns the prediction URI to subscribe to *while the task is still running*.

## The URI scheme is the namespace

```
wavespeed://models                      catalog index (id, type, description, price)
wavespeed://model/{model_id}            full input schema + pricing   (completable)
wavespeed://prediction/{id}             live prediction state         (subscribable)
```

Tool names are scoped per-connection in MCP; hosts that aggregate servers do their own
prefixing (`mcp__wavespeed__run`). Prefixing tool names server-side just stutters. The
`wavespeed://` scheme is where the namespace actually belongs, and it gives every noun in
the system a stable, linkable identity: task status messages reference prediction URIs,
tool results carry `ResourceLink` blocks, subscriptions target them.

## Results are structured, twice

A completed `run` returns a `CallToolResult` carrying:

- a human-readable text block (model, output count, timing),
- one `ResourceLink` per output (CDN URL + mime type) — outputs are addressable, not blobs,
- `structuredContent`: the full prediction JSON for programmatic consumers.

Base64 output is deliberately off (WaveSpeed doesn't support it with webhooks anyway);
media moves by URL in both directions. Inputs the provider must fetch are served from the
server's own `/files/*` static route through the public tunnel URL — the same single
process handles MCP, webhooks, and media.

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
- **Symmetric conformance.** Client and server share one lib crate and one SDK (rmcp);
  every feature is tested by actually being used.

## Verified behavior

Both paths were proven against production WaveSpeed, through a real cloudflared tunnel:

1. `openai/gpt-image-2/edit` — input image served via `/files`, 122.9s inference,
   completed via **poll fallback** (webhook signature was failing on a stale secret —
   exactly the failure the fallback exists for).
2. `wavespeed-ai/flux-schnell` — corrected secret, webhook **verified and pushed**,
   task completed in ~2s; client received `resources/updated` and `tasks/status`
   notifications live, then downloaded outputs from the resource links.

Plus: schema validation rejects bad input before submission, `tasks/cancel` aborts
in-flight work, and completions rank prefix matches across the full 988-model registry.

## Known gaps

- **State is in-memory.** Tasks, predictions, and subscriptions die with the process.
  Task ids are durable *within* a server lifetime only. Next step: sled/SQLite store.
- **Subscription identity is coarse.** Unsubscribe clears all peers for a URI — fine for
  owned single-client deployments, wrong for multi-tenant.
- **No task GC.** Completed task entries accumulate until restart (TTL is accepted from
  clients but not yet enforced).
- **Tasks are an evolving extension.** SEP-1319 (2025-11-25) is what rmcp 2.0 ships; the
  2026-07-28 spec moves tasks to an extension with `tasks/update` for mid-flight input.
  Owning both ends means we migrate both sides in one commit when rmcp does.
