# Workspace Client

## Standards And Protocols

Workspace consumes the Veoveo same-origin HTTP JSON API at `/workspace/api` and
Server-Sent Events for chat changes. OAuth authorization code with PKCE runs through
the existing Rust browser edge at `/workspace/auth/`, using Workspace's own
OAuth client and encrypted cookie. Generated JSON Schema and TypeScript come from
`mcp/contract/src/workspace.rs`. The browser never implements MCP Tasks transport;
the Rust platform owns that boundary. Embedded capability views retain MCP Apps
`2026-01-26` and Veoveo's declared App extensions.

React, TypeScript and Vite retain the repository's qualified frontend pins.
assistant-ui `0.15.20` is the first presentation candidate, verified against the
[upstream package registry](https://www.npmjs.com/package/@assistant-ui/react).
It must pass the multi-participant qualification before being accepted as the chat
renderer. Its runtime cannot own chat authorization, persistence or agent execution.
The adapter qualification passes with two human and two agent authors, concurrent
agent statuses, subsequent human messages and replay. Playwright `1.63.0`, verified
from the upstream registry, runs the client-owned browser acceptance fixture.

## Product And State

`usePersonalEvents.ts` owns one personal feed across chat, Apps and Computer
navigation. Invitations refresh when committed changes arrive. My activity shows
input attention and new completed, failed or cancelled work without opening its
chat. Replayed completion baselines do not create new alerts. The client retains
only typed observation metadata in memory and resets it with person or Work Context.

Inventory reads reconcile after connection loss and visibility recovery. A healthy
feed replaces the ten-second inventory poll. Visible Task details retain their
native reconciliation interval, and only items outside the personal watch acquire
another subscription. Repeated Task timestamps coalesce without another detail
read. When the recent window is full, Activity explains how to follow older work.

The application serves `/workspace/` independently of the administrative Console.
Its source and asset bundle are separate; both applications share the existing Rust
browser-edge deployment. The complete accepted product and delivery gates are in
[`WORKSPACE_PLAN.md`](../../docs/WORKSPACE_PLAN.md).

Chat IDs select authoritative server state. Messages retain their author and stable
ID, and replay follows committed sequence order. The client may retain unsent text
in memory while a request is pending. It must not persist message bodies, tokens or
capabilities to local storage implicitly. An ambiguous send retains its request ID
for reconciliation; it never manufactures a fresh ID and repeats execution.

The assistant-ui adapter preserves human and agent metadata with
`joinStrategy: "none"`. Runs and their controls belong to individual messages;
streaming must not disable the whole room's composer. Typed run feedback shows
preparation, response generation and actual capability invocation before final
text. A receipt count points to the initiating person's private Activity without
publishing private capability names or results. Terminal state overrides active
feedback; no progress percentage is invented. Veoveo owns participation,
invitation, attachment access and Task presentation. The qualification fixture has
two humans and two agents, concurrent outputs and a browser reconnect.

Computers, Apps, uploads and activity use existing governed platform operations.
Heavy viewers load on demand. Reuse of a Console component requires an audit of its
authentication and inventory assumptions; Workspace does not grant administrative
access merely to render a reused view.

## Delivery Status

The interactive client implements human chats, latest-first history loading,
invitations, participant controls, ownership transfer and archiving. It renders
plain text without executing HTML. A contentless SSE wake requests incremental
authorized state. A failed ambiguous send retains its original ID and text for
retry; refreshing restores committed messages without repeating a mutation.

An expired chat watch refreshes authorized state and allows native EventSource
reconnection at the server's retry interval. Fresh HTTP admission checks access
again. A temporary authority-read failure can recover without reloading the page;
a denied snapshot clears the retained conversation and composer. Reconnecting
never submits a message, response run or tool operation. The browser fixture
requires a later message to arrive on the renewed stream and separately checks
that revoked access removes the transcript.

The gateway and browser edge serve the live stream and assets. A local headed
RTX 4090 WebGL browser fixture exercises two humans, an interrupted response,
idempotent retry, owner controls, reload and the mobile layout. Its screenshots
are local fixture evidence. Agent admission and explicit recipient selection now
render separate response streams and stop controls; a two-human/two-agent fixture
keeps the composer usable and preserves run identities after reload. The Rust
runner has a separate HTTP-model fixture. Task activity now has private in-chat and
personal views, current status, input forms, cancellation acknowledgement, result
text and canonical resource references. Native subscriptions wake authorized reads;
visible active tasks reconcile at no less than five seconds or the server's longer
poll interval. Completed tasks stop that interval. Unknown progress is indeterminate.
Reload restores receipts and Task references without submitting work. The gateway
connects admitted agent tools to those same receipts. Activity identifies agent
requests for the initiating person and links back to the originating chat. The
agent name comes from the authorized receipt and survives personal Activity reload
and runs falling outside the recent conversation window. Agent
admission discloses the configured capability scope before sharing history. The gateway
has native protocol/runtime acceptance. Installed qualification covers real model
responses, native Task completion and cancellation, Apps, governed uploads and
retained Computer terminals. Public acceptance and the nonblocking distinct-person and
production Task-input follow-ups are recorded in the Workspace plan under the
user’s September 16 delivery decision.

Each native Task input request has its own form and decision. An unsupported form
can be declined while other requests remain answerable. Unchanged requests keep
their in-memory drafts when a sibling is answered; a changed request digest resets
only that request. Submissions and cancellation keep controls disabled until the
current-state read settles. A lost reply triggers a read without repeating the
decision. Synchronous multi-round tool continuations retain one complete response
batch because they consume one protected request state. The headed client fixture
qualifies both paths, including stale-request rejection and an interrupted decline
reply. These fixtures do not establish an installed domain's input behavior.

Task result Artifact references have explicit preview and download actions. The
client recognizes the canonical `artifact://{id}` and domain `scheme://artifact/{id}`
forms with UUIDv7 identities, then uses a fixed same-origin route. It never navigates
to an arbitrary tool-supplied URL. A fresh governed HEAD request supplies MIME type,
length and current access before a raster-image preview, bounded to 20 MiB. Other
types remain downloadable. Revoked or unavailable reads explain that a chat link
does not confer file access. The preview GET rechecks authority; neither a Task
receipt nor the prior HEAD response grants continuing access.

Activity displays inline MCP PNG, JPEG and WebP result images from the authorized
detail response. The gateway bounds their count and encoded size and reports
omitted images explicitly. Images use data URLs with the closed raster MIME type;
tool-supplied network URLs and executable image documents are not used. Reload
restores the original Task image without another tool call. A denied detail refresh
removes cached result content from the rendered card. A browser decode failure is
visible alongside the surviving result metadata.

Uploads reuse the Console-owned queue, hashing worker and accessible panel. Each
browser entrypoint selects its application once through `browserApp.ts`; shared
HTTP, CSRF and login helpers then use that explicit root. A missing selection or
attempt to change applications fails. Workspace never initializes a Console session.
Its upload metadata has a separate storage scope and retains only descriptors and
receipt identities, with the same current-authority recovery as Console. The queue
outlives panel closure and chat navigation. It disposes on identity/context removal.
Uploaded files remain governed resources; the upload panel does not publish them
into shared chat history.

Shared component source stays with its existing owner under `apps/console/web/src`.
Vite resolves shared package imports to Workspace's exact installed dependencies,
and TypeScript uses the same resolution. React and Query have one instance per
application. Docker copies the shared source into the independent Workspace asset
stage; source sharing adds no Rust dependency. The dedicated client evidence catalog
declares both source trees, independently of the Rust Workspace checks.

Computers reuses the native collection, access controls, file transfers and hardware
terminal through Workspace's fixed browser routes. Host props supply the profile and
known principal names; rendering never fetches administrative inventory. The native
browser terminal is available here. Stock CLI pairing retains its Console entry until
that separate public client is qualified for Workspace. Scope keys include the
application, tenant, principal and Work Context. Navigating away disconnects the
terminal and preserves the Computer's running processes and retained files.

The terminal pins `@xterm/xterm` 6.0.0, `@xterm/addon-fit` 0.11.0 and
`@xterm/addon-webgl` 0.19.0, verified as the latest stable releases at their upstream
npm registry on September 15, 2026. These are the same qualified native terminal
dependencies used by Console. The terminal bundle loads only in the Computers view.

## Development And Acceptance

Run `npm ci` and `npm run dev` in this directory. Vite uses port 4174 and proxies
the browser edge on 8786. The edge accepts `VEOVEO_WORKSPACE_ASSET_DIR` for its
built entry directory; the production image uses `/app/workspace`. `npm run build`
builds only the client. Docker assembles Console and Workspace assets in separate
frontend stages while reusing the compiled Rust browser edge.

`npm test` runs generated-contract and assistant-ui adapter tests. `npm run
test:browser` owns a temporary Vite listener, isolated browser context and HTTP
fixtures. It attaches to headed Chrome at `VEOVEO_BROWSER_CDP` (default localhost
9222), probes both graphics APIs, and requires hardware before interacting or
capturing. Its context and listener close on success or failure. No local fixture
counts as production model execution.

## Embedded Apps And Durable Tasks

The lazy Apps view uses the current Workspace MCP catalog and the shared opaque
`allow-scripts` iframe host. Closing a view preserves an unsent chat draft. Catalog
updates with unchanged App descriptors do not reconnect a frame or discard its
resource subscriptions. App output remains private to the initiating person.

The host fixes the App URI and chat on every tool call. The Rust gateway resolves
local tool aliases against current tool links and declared imports. An App cannot
select a profile or claim a Task started by another App. Native Task identities are
stored in the existing operation journal before the bridge returns acceptance.
`tasks/get`, `tasks/update` and `tasks/cancel` use that persisted origin binding.
Activity recovers the same receipt after the iframe closes or the edge restarts.
The initial bridge observes its dispatch receipt; it never repeats `tools/call`
when a response is lost. An unconfirmed outcome points the user to Activity.

Multi-round App calls receive a host-owned receipt/revision reference. The opaque
upstream continuation remains in Rust. The host checks the original App, chat,
arguments and current revision before an explicit continuation. Forms use the
same current-schema validation as Activity. A cancellation acknowledgement does
not establish a terminal Task outcome.

`appProtocol.ts` validates the supported MCP result envelopes, preserving native
extension fields. Generated Rust schemas own the surrounding HTTP DTOs; their
native result field delegates to this pinned protocol adapter. The exact
`@modelcontextprotocol/client` 2.0.0 development pin supplies shared bridge types,
verified against the upstream npm registry on September 15, 2026. No MCP bearer
transport or token is exposed to the browser. The host supports native Task
get/update/cancel, reactive resource subscriptions, ordinary tool/resource calls,
inline views and confirmed HTTPS links. Console-specific agent-message and
Recording projection extensions are not advertised by Workspace.

## Response Participation

Chat details let the owner choose explicit requests, a default assistant or bounded
automatic responses. The composer shows which agents will respond. A leading
`@Name` address selects an active chat agent, including names with spaces. Addresses
inside quoted text, code or an ordinary message body do not request work. Duplicate
display names require the explicit agent selection. Agent identities, rather than
parsed server-side text, cross the HTTP boundary.

One send carries the message and its explicit destinations. The store commits all
response intents with the human message. A lost response retains the complete request
for an exact retry. The browser no longer saves text and then starts runs in separate
requests. A capacity failure appears on the affected response, while people can keep
writing. Recovery fetches committed state without starting another response. If a selected
agent leaves the chat, an explicit action clears unavailable selections. A pending
ambiguous send retains its original destinations until its outcome is confirmed.

Every model response retains its own ID and cancellation control. Its MCP Tasks keep
their independent native IDs in private Activity, including input rounds and terminal
outcomes. Participation settings grant no additional capability authority and do not
publish private Task results into shared history.

## Replies

Each human message and terminal agent response offers a Reply action. The composer
shows the selected author and bounded excerpt, with an explicit removal control.
Replying to an active agent selects that agent as the explicit recipient; the owner’s
participation policy still resolves the final response set. Running responses cannot
be quoted until their text settles. Humans can continue sending other messages.

The pending request freezes the typed reply target alongside text and recipients.
An interrupted confirmation disables changes to that intent until it is reconciled.
The timeline displays the server-captured quote after reload, including when the
original is outside the loaded page. Agent responses also identify their triggering
human message when it is loaded. Human and response IDs have separate presentation
namespaces, so a repeated UUID cannot merge their authorship. Quotes render plain text
and do not carry private operation results or Artifact capabilities.

## Chat Attachments

The composer publishes typed Artifact references as part of the immutable human
message. A person chooses a completed upload or pastes a canonical resource link,
then reviews the name that will appear in the chat. Up to eight distinct references
fit in one message, including a message without text. Selection alone sends nothing.
The original list stays frozen while an uncertain message acknowledgement is resolved.

Names and identities become shared chat content only through this explicit send.
The message contains no signed URL, private Task payload, fetched filename, MIME
claim or grant. A reference may name an unavailable file, just as a person can paste
an opaque identity into text. Admission does not claim that it exists or that every
member can read it. Labels are human-authored untrusted text and render escaped.

Opening a reference uses the existing Workspace Artifact routes with the current
reader's credentials. Preview rechecks access with HEAD and GET. Denied or missing
files have an explicit unavailable state; readable non-image files offer download.
There is no background fetch of every attachment in a conversation. Chat membership
and agent participation never grant Artifact reads. Private Task results stay in
Activity unless a human deliberately publishes a reference; their contents remain
private. Agents receive the shared references and labels without file contents.

The browser fixture exercises selection from an upload receipt, invalid URL
rejection, two attachments without text, acknowledgement loss with exact retry,
owner preview, another member's denied preview, escaped labels and reload without
another send or upload. Real database tests own immutable list admission and
membership revocation. Installed acceptance is recorded in the Workspace plan.
