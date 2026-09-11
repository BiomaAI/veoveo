# Native Computers Workspace

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo Computers JSON | Generated from `platform/computers/contract`; lifecycle receipts, snapshots and server-issued action flags |
| HTTP, RFC 9110 | Same-origin cookie requests, CSRF mutations, bounded JSON, fixed BFF destinations |
| Server-sent events | CSRF-protected POST subscription; canonical snapshot reread after each collection invalidation |
| WebSocket, RFC 6455 | Same-origin terminal, one-use first-frame ticket, no URL credentials |
| OpenShell CLI `0.0.116` pairing adapter | Explicit code comparison and bounded CORS JSON delivery to the validated IPv4 loopback port; custom profile |
| Browser local-network access permission | Loopback callback may require user consent; the site requests access only to the exact admitted local CLI port |
| Veoveo terminal v2 | Ready, binary output/input, bounded resize, ReplayComplete and increasing service lease sequence |
| xterm.js | `@xterm/xterm` 6.0.0, fit 0.11.0, WebGL 0.19.0; versions verified at the authoritative npm registry on 2026-09-10 |
| WebGL 2 | Required hardware-backed xterm renderer; software adapters and lost graphics contexts fail closed |

The Console opens from the small authenticated bootstrap response. Computers and
permitted Apps are available without administrator inventory. Administrative pages
remain behind the current `canReadInstallation` hint and their existing endpoint
authorization. Each profile/tenant/actor/Work Context receives a separate query client
and controller lifetime. A stale asynchronous result cannot populate a new scope.

The collection shows authoritative phases and action flags. Create uses the admitted
default template. Explicit Stop explains that process memory ends and retained files
remain. Selection persists per scope, with an exact Computer fragment route. Template
selection, provider setup controls and file Tasks remain subsequent work.

`controller.ts` coalesces invalidations during an in-flight read into another read.
The first subscription baseline overlaps the initial HTTP snapshot. Stream loss marks
the view stale and disables lifecycle admission in the UI. Reconnection establishes
a fresh subscription with exponential backoff capped at ten seconds. Authorization
denial requires explicit Refresh. Events fit 4 KiB and an inactive stream expires after
25 seconds. Pagination is bounded to ten pages of the service's 100-row projection.
No provider query or browser status polling is introduced.

Lifecycle request IDs are saved in sessionStorage before dispatch. A lost response
retains the exact ID, action and selected Computer. Reload restores it. Recover status
reads a known operation; an unconfirmed response retries the original idempotent request.
Collection baselines and invalidations refresh nonterminal receipts through the same
read projection, with at most four concurrent reads. Concurrent retries share their
request. Saved data contains no ticket, cookie, token or shell text. Up to 32 receipts
are retained. Storage failure prevents a fresh mutation; clearing ambiguous recovery
state requires explicit review. Late reads cannot restore dismissed receipts or cross
controller scopes.

`AccessPanel.tsx` shows outstanding browser and named CLI grants for the selected Computer. The
existing scope-owned query client isolates results across actors and Work Contexts.
Collection invalidations refresh the inventory; there is no periodic status query.
The UI labels sign-in ownership and configured expiry without asserting transport
liveness. Revoke sends the exact grant ID and keeps the Computer running. A lost
response can safely retry the same reduction. The panel never receives a grant token.

`CliConnect.tsx` presents uncredentialed registration and shell commands using the
public Computer UUID. The dedicated `CliPairingPage.tsx` uses existing SSO, current
Computer action flags, a named grant and explicit confirmation that the displayed
code matches the initiating terminal. A single confirmed gesture first sends a
credential-free OPTIONS request to the stock callback, allowing at most sixty seconds
for local browser consent and reachability. No grant exists during this check. The
stock callback's qualified OPTIONS handler accepts the origin and changes no pairing
state. Only a 204 response permits creating and consuming the one-use challenge
through `pairing.ts`. The credential stays in function-local
memory while a bounded CORS POST delivers it to the exact validated IPv4 loopback
callback within ten seconds. It enters no query cache, browser storage, copied command or diagnostic.
Failed delivery revokes a known grant; lost confirmation or revocation responses
direct the user to review Computer access. No delivery retry can replay a consumed
challenge. Closing the page after success does not log out or stop the Computer.
The browser owns the permission prompt; Veoveo cannot grant itself permission.
The UI explains the local-device request before the action. Chrome's boundary is
documented in its [local network access guidance](https://developer.chrome.com/blog/local-network-access).

The terminal module is a separate lazy production chunk. Explicit Connect requests a
fresh one-use ticket and sends it in the first WebSocket frame. Navigation unmounts
the attachment and never calls Stop. Disconnect stays disconnected until another
explicit Connect. An interrupted attachment reports that its Computer may still run;
the client never replays input. Automatic recovery using an existing grant requires
separate qualification because minting grants on every unknown close would reset
idle and absolute boundaries.

`terminalSession.ts` owns protocol order independently of React and xterm. Output is
capped at 64 KiB per frame, 256 KiB queued and 32 queued chunks. ReplayComplete enables
input only after callbacks drain all historical writes; later live output cannot
starve that boundary. Answerbacks use the same input gate. Pasted UTF-8 and binary
input fit one 64 KiB frame, and pending socket writes stay below 128 KiB. Congestion
closes the attachment and discards unsent input. Resize is bounded to 2–500 columns
and 1–200 rows. Attachment establishment has a ten-second deadline.

Current service deadlines are converted to monotonic browser deadlines with a
one-second allowance. The browser must be within one second of service time for this
profile. Invalid or expired leases close access even while rendering is blocked.
Late callbacks cannot revive a closed attachment. The gateway and BFF independently
enforce their authoritative deadlines; browser enforcement grants no authority.

Terminal output cannot write the clipboard or create OSC hyperlinks. Copy is an
explicit user action. The renderer must expose a recognized hardware WebGL adapter;
graphics loss closes attachment and rendering. History is bounded, with the replay
window limitation visible. Exact server truncation metadata remains a release item.

Node behavior tests qualify protocol ordering, callback drain, bounded buffers,
lease loss, lifecycle recovery and stale epochs. They do not establish interactive
browser behavior, headed GPU presentation, actual SSO or public deployment. Those
remain acceptance gates in `docs/COMPUTERS_PLAN.md`.

`AutomationPanel.tsx` shows named grants, application scope, original bounds and expiry.
Issuance uses current server limits and explicit consent for whole-run Stop on command
interruption. Management hints disable unavailable actions. The form requests only
Execute; broader lifecycle permissions require their separate agent integration.
Existing cached principal names improve selection without requiring administrator
inventory for core Computers. The application client ID remains an explicit scope.

Before sending, the form saves the generated UUIDv7 request and exact grant body in
scope-and-Computer-specific session storage. Retry and reload preserve absolute expiry
and scope. A confirmed result clears that intent; clearing an uncertain request requires
review and does not revoke an accepted grant. The BFF keeps credentials in its session
and applies the existing CSRF boundary. Resource invalidations refresh the grant list
without replacing the focused terminal. This implementation still requires installed
headed hardware browser acceptance.
