# Workspace Operation Receipts

## Standards And Protocols

SurrealDB `3.2.4` transactions persist private typed records through the existing
store client. MCP `2026-07-28`, SEP-2663 Tasks and SEP-2322 multi-round requests
define the external operation boundary. Persistence retains bounded opaque MCP
envelopes; the gateway's pinned SDK interprets them. This module does not implement
MCP or infer provider status.

## Dispatch And Recovery

A stable operation UUID binds the authenticated initiator, Work Context, chat,
optional agent run, gateway profile, tool and exact JSON arguments. Admission
commits before the gateway calls the tool. One random server-owned fence wins the
right to dispatch. Concurrent submissions return the same receipt, and a later
request cannot claim it again. An agent must have a current running lease and the
matching run fence at admission. Human actions require current chat membership.
The worker checks the exact receipt fence, dispatch deadline, membership and optional
run fence again immediately before its external invocation. This transaction shares
the chat head with membership changes. A stopped run cannot admit another effect.
Eight unexpired dispatches per person and chat are admitted at a time; the gateway
also bounds process-wide work.

An App invocation additionally binds its exact `ui://` origin. That origin is part
of idempotency and cannot be changed by replay. App recovery looks up the recorded
native Task through the current tenant, Work Context, initiator, profile and App
URI. A different view cannot claim a human/model receipt or another view's Task.
The normal private activity list includes App work from this same journal.

The dispatch wait expires after 90 seconds. Reads then present an unconfirmed
outcome, without making a provider query, asserting failure or replaying a mutation.
An authoritative late response can still settle the original fenced receipt.
There is no lease takeover. Restart recovery reads existing receipts and uses the
saved native Task ID, when present.

Settlement may record a Task ID after the initiator loses access, because losing
the accepted identity would orphan work already dispatched. Its fence permits
only the bounded receipt update. It grants no further invocation or read authority.
Every read rechecks the current human and Work Context. Only the initiator can
read a receipt, including within a shared chat. Leaving a chat does not orphan
that person's activity; it still prevents new actions in the chat. Native Task
reads, answers and cancellation require separate current gateway MCP authorization.

## Input And Outcomes

An `input_required` response retains its opaque request state on the server. A
human response claims one exact receipt revision and rotates the dispatch fence.
Competing or stale forms cannot advance it. At most ten continuation rounds are
accepted. An ambiguous continuation stays unconfirmed and is never retried by
this module. A human explicitly continuing an operation does not restart the
model response that originally requested it.

Task acceptance retains the exact opaque gateway Task ID. Task status, progress,
input requests and results remain owned by native MCP. A completed tool response
may contain `isError: true`; receipt completion does not assert domain success.
Definitive MCP errors and ambiguous transport outcomes are distinct states.
Response envelopes are capped at 1 MiB and arguments at 64 KiB. Database records,
continuation envelopes and fences are not browser DTOs.

## Migration And Qualification

Migration `0079` adds one private table and ownership function. It changes no
existing chat or domain data. Old application binaries ignore the table; reverting
the application leaves the receipts available for a later forward rollout.
Database-backed acceptance races independent store clients, restores accepted
Task references, rejects cross-person access, fences stale input revisions and
retains late acceptance across policy revocation. Protocol execution and installed
client acceptance belong to the gateway and Workspace delivery checks.

Additive migration `0080` adds an optional App origin and a bounded lookup index.
Existing receipts retain an absent origin and remain readable by their initiator.
Older binaries ignore the added field; reverting a binary does not delete receipts.
The gateway and App client enforce that persisted origin on native Task access.
