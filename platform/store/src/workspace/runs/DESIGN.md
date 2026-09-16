# Workspace Agent Runs

## Standards And Protocols

This private persistence boundary uses SurrealDB `3.2.4` transactions and typed Rust
records. It has no model-provider or MCP transport. Gateway workers own model
streams and capability authority; browser projections must omit execution fences
and internal record identities. Migration `0078` adds tables without rewriting
human messages. Application-contract upgrades are coordinated with the gateway and browser edge.

## Admission And Context

A chat owner admits a server-validated agent definition, including the public
provider/model disclosure and an exact configuration digest. This membership has
its own record and never reuses an autonomous agent's inbox or memory. Adding an
agent does not create a service credential or a Computer automation grant.

A run addresses one admitted agent from one human-authored message. The human must
still be an active contributor. Its UUID derives from the message and agent, and a
unique index enforces the pair. A repeated admission returns that same run, including
its terminal state. Different agents receive independent run records. The chat head
serializes admission with membership changes and enforces four concurrent runs.

Atomic message admission freezes that message's committed sequence for every selected agent. Explicit assignment of an existing own message freezes the current chat sequence. Prompt reads include immutable human
messages and agent results completed before that boundary, with fixed page bounds.
They exclude other chats and output still in progress at admission. A model worker
must apply its own prompt byte/token budget before dispatch.

## Publication And Recovery

Only one worker can move a queued run to running. Claim assigns a fresh execution
fence. Every publication checks that fence, current human/context/chat admission,
active agent membership, configuration digest, deadline and lease. Cumulative text
must retain the already-published prefix. Heartbeats extend a 20-second lease without
creating visible events when content is unchanged. The absolute deadline cannot
exceed three minutes at admission and is also checked independently of the lease.

The initiator or chat owner may cancel. Cancellation clears the fence; a late worker
cannot publish. Existing output remains attributed to the agent. Changes commit with
contentless events in the same transaction. Human message sending does not depend on
any run's state.

An authorized run-window read reconciles expired leases, deadlines and removed
participants before returning the latest 64 runs. It persists an interrupted state
and a closed reason, preserving partial output. Recovery never dispatches a provider
request. Returning after a service replacement cannot restart execution; an explicit
new human message creates new work. Workers must independently stop provider streams
on cancellation or authority loss. No MCP Task state is fabricated by this store.

## Security Boundary And Qualification

`WorkspaceAuthority` and agent-admission values are trusted Rust inputs. Browser
requests cannot set model credentials, provider URLs, context digests or execution
fences. Gateway admission additionally checks OAuth/session-family revocation before
model/tool dispatch and output publication; these tables do not replace that check.
Private capability-derived output requires the separate governed-publication gate.

Real database fixtures exercise two humans and two agents, racing worker claims,
immutable context, concurrent capacity, cumulative output ordering, independent
cancellation, lost workers, removed people/agents and request replay. They simulate
no production model and make no claim about deployed agent execution.
