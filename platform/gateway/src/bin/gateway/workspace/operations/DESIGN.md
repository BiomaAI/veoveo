# Workspace MCP Operations

## Standards And Protocols

This module is a native MCP `2026-07-28` client using the repository's qualified
RMCP pin. Discover negotiates the official SEP-2663 Tasks extension. Tool calls
use the SDK's single-round API; SEP-2322 continuation state stays on the server.
Task reads, updates and cancellation use `tasks/get`, `tasks/update` and
`tasks/cancel`. Request-scoped `subscriptions/listen` carries Task-ID filters.
The browser receives a separate generated HTTP JSON projection, contentless Task
detail invalidations and actor-private typed personal events. It does not receive
bearer credentials or request state.
Elicitation forms use the MCP primitive schema profile, validated with the
repository's qualified JSON Schema implementation. Sampling and roots requests
are not advertised or executed by this client.

## Authority And Dispatch

Workspace routes admit a current direct human and Work Context through the shared
gateway checks. MCP requests carry that person's bearer to the fixed loopback
gateway endpoint with the installation's public Host header. The endpoint is
constructed from the gateway listener; request bodies cannot choose a destination,
profile, identity or credential. Redirects are disabled. A new native client
per request avoids retaining credentials beyond the bounded operation lifetime.

The private [operation store](../../../../../../store/src/workspace/operations/DESIGN.md)
commits a stable dispatch identity before `tools/call`. Sixteen process slots bound
concurrent dispatch. Only the receipt claimant sends the call. The HTTP response
returns the receipt immediately while the owned worker records the native outcome.
An 85-second wait or shutdown records an unconfirmed outcome and never replays the
mutation. Connection failures before a tool request is submitted record a failed
admission. A received Task ID is retained before the client can observe acceptance.
No provider completion or provider polling logic exists here.

Agent tools enter the same dispatcher with the exact current run fence and human
credentials. They wait for the bounded native invocation to settle its receipt,
which keeps the response run alive through dispatch without waiting for Task
completion. The worker rechecks authority after connection setup. Explicit human
continuations use current human admission independently of the old model run.

Every detail read rechecks current Workspace admission. Native Task reads then
pass the gateway's existing owner, profile and invocation-authority checks.
Stored synchronous results and pending continuation forms require current tool
discovery permission. Receipts are private to their initiator. Chat membership
does not confer Task read, input, cancellation or result authority. Leaving a
conversation preserves personal receipt navigation under current Work Context
authority, without restoring that conversation's history.

Operation summaries include the retained agent ID and display name when a run
requested the work. The same private receipt authority governs attribution. Both
personal and in-chat Activity can identify the agent without fetching chat history
or relying on the latest response window.

## Tasks And Input

Task payloads remain native. A completed Task can carry a tool-domain error, which
the client distinguishes from success. A cancellation acknowledgement changes no
Task status. Status messages are retained; no percentage is synthesized when
the native Task has no numerical progress report. Task expiry and authorization
failure make the detail unavailable rather than converting it to failed work.

Task input responses first read the current native input map. Each answer binds
its request key and the SHA-256 digest of the complete request. Stale forms and
duplicate keys are rejected. Form values are validated against the current schema,
including unknown-field rejection. An external elicitation URL must use HTTPS
without embedded credentials and opens only through an explicit client action.
Unsupported input kinds are displayed without automatic approval or model sampling.

Multi-round tool input retains the exact opaque continuation envelope server-side.
Answering claims one receipt revision before submitting the continuation. A
request-state-only continuation requires an explicit Continue action. The browser
cannot supply or replace request state. Native Task updates retain the Task
runtime's lifetime-unique request keys and response deduplication.

## Subscriptions And Bounds

`personal.rs` serves `/workspace-api/{profile}/events` independently of the open
chat. A process shares two projected LIVE sources across people. Every source
recovery wakes readers after subscription establishment; every emitted inventory
comes from a fresh authorized bounded snapshot. Fifteen-second reconciliation
recovers missed operation or invitation changes. Policy replacement, revocation
or token expiry ends observation. A stream lasts at most 55 seconds.

The personal watch reconciles up to 32 recent Task references through native
`tasks/get`, with four concurrent reads. Unavailable or expired Tasks lose their
observation individually. Only active, readable Tasks enter the exact acknowledged
subscription. Reconnection obtains a new baseline without submitting work. Private
events contain an operation identity, closed state and timestamp; they exclude
Task payloads, input prompts, results and progress messages. The inventory says
when its recent-activity window is limited. Older visible items retain their own
bounded native watch and detail reconciliation.

A tab watches at most 32 owned operation references through one native Task
subscription. Its accepted filter must exactly match the requested Task IDs.
There are four watches per person and 64 per process. Streams close within 55
seconds or at bearer expiry and recheck current Workspace authority every five
seconds. They emit no Task payloads. Native Task notifications prompt an authorized
detail read; a bounded `tasks/get` reconciliation interval remains the correctness
path and honors a longer server-supplied polling interval.

Activity navigation uses stable receipt cursors and 100-item pages. Arguments are
capped at 64 KiB, stored response envelopes at 1 MiB, and the existing browser edge
bounds forwarded JSON at 4 MiB. Tool discovery stops after 16 pages or 512 tools.
Resource links retain canonical URIs; they do not become direct storage URLs.

Required-capability admission opens a native tool-list subscription before reading an
isolated catalog. When a required server is still unavailable, admission waits for its
list-change notification and reads a fresh bounded catalog. This settlement has an
eight-second deadline and never dispatches a tool or model call. Missing notifications,
closed streams and unresolved capabilities fail admission. Optional unavailable servers
cannot block publication of an unrelated capability. Ordinary inventory reads retain
their partial-catalog behavior.

The result projection also preserves inline PNG, JPEG and WebP MCP image blocks.
It admits at most eight images and 1 MiB of encoded image data per result, validates
standard base64, and reports the count of omitted image blocks. Unsupported media
types, empty or malformed data, and excess payload do not hide the remaining result.
Image bytes cross the same current-authority detail read as text; they never enter
shared chat history or the agent's dispatch receipt. A reload reads the existing
native result and cannot repeat capture. The browser decodes the admitted raster
format; the gateway does not render or transcode images.

## Qualification And Delivery

The native integration fixture uses real Streamable HTTP, the shared durable Task
runtime and an isolated SurrealDB instance. It checks restart recovery, one initial
dispatch, schema validation, consumed input, Task subscriptions, completed tool
errors, cancellation acknowledgement and terminal confirmation. A separate browser
fixture checks private activity, form submission, reload and independent agent/Task
controls in headed hardware Chrome. These checks do not establish installed
acceptance. Agent capability adapters have a separate real model-to-native-Task
fixture. Installed public acceptance qualifies native Task completion, cancellation
and recovery, governed Artifact preview, App invocation and inline capture images.
A production Task input round remains a nonblocking installed follow-up under the
user’s September 16 delivery decision: current domains do not issue input requests.
Local native input qualification and installed outcomes remain distinct. Evidence is tracked in
[`WORKSPACE_PLAN.md`](../../../../../../../docs/WORKSPACE_PLAN.md).

## Native App Adapter

`apps.rs` hosts the MCP Apps `2026-01-26` tool and Task adapter over the native
`2026-07-28` client. Current `ui://` resource discovery and current tools establish
admission. The shared `mcp/apps-extension` resolver admits exact linked tools or
explicit imported aliases; a frame cannot supply a global gateway tool name.
Resource discovery is bounded to sixteen pages and 1,024 entries. Dispatch
rechecks App admission before calling the tool.

An App intent records its URI in the private operation journal. Task access joins
that origin with the current person, profile and Work Context before any native
request. App removal or current discovery denial stops access through the App
bridge. Activity remains the human owner's separate recovery surface, subject to
its ordinary current tool and Task authority.

An App receives native result envelopes. Multi-round input replaces upstream
request state with `workspace-operation:{id}:{revision}`. This reference confers no
authority: current ownership, exact App/chat/tool/arguments, phase and revision all
must agree before the shared continuation claim. The native opaque state remains
in the store. Native Task input binds the runtime's lifetime-unique request key and
validates the current form. The adapter never reports cancellation from an ack.

The real HTTP/Task-runtime fixture qualifies origin denial, current App revocation,
restart without replay, input validation, cancellation acknowledgement, terminal
confirmation and fenced multi-round continuation. The browser fixture separately
qualifies the sandbox channel and recovery in Activity; it is not installed proof.

## Request Progress

Each dispatch observes the exact progress token assigned by RMCP. A single early
observation closes the race between notification delivery and request-handle
creation. Finite, nonnegative measurements must increase. Messages are bounded to
2,000 bytes. A coalescing channel writes at most four observations per second;
the active dispatch fence and database deadline guard each transaction. Settlement
and explicit continuation clear this request-scoped observation. Native Tasks keep
their own status and input lifecycle.

The private operation projection carries measured units and an optional total. The
client displays a numeric bar only when the total is positive and contains the
measurement. Otherwise its indicator remains indeterminate. Shared chat receives
only the run phase and operation count. Model reasoning and private tool messages
are never shared through this feedback path.
