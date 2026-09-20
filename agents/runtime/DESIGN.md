# Durable Agent Runtime

## Standards And Protocols

The runtime uses the pinned SurrealDB 3.2.4 WebSocket SDK and repository-owned
schema migrations. Typed records, transaction fences, outbox events and managed
generations are internal Veoveo contracts. Native MCP `2026-07-28` Tasks keep
their canonical gateway identity and retention pin; this crate owns delivery to
the agent, not the MCP transport or provider completion protocol.

## Episode Ownership

One renewable scheduler lease admits episodes for an agent. Admission updates
the agent and creates its episode in one transaction. A second episode cannot
overlap a running episode. Lease recovery marks interrupted episodes crashed and
retains actionable wakes for their next owner.

A managed process supplies its admitted instance and generation. The runtime
requires that binding whenever a managed record owns the same tenant and agent
key. Admission writes the managed record in the episode transaction, making a
concurrent pause, update or disable conflict with admission. The episode retains
its exact definition revision, generation and dispatch epoch.

Pause closes admission while the bounded active episode drains. A paused kernel
may retain its lease to observe accepted Tasks; observation creates no model
call. Switching workload generations still requires the old lease to end.

Stop advances the dispatch epoch, marks the active episode stopped and
acknowledges its claimed wakes atomically. A late completion cannot overwrite
that terminal result or replay the request. Accepted domain operations retain
their own cancellation contracts. Task results consumed by the stopped episode
release their existing retention pins in the same transaction.

## Readiness And Recovery

The kernel records readiness only after connecting and installing its tools.
Readiness binds the managed generation and Kubernetes Pod UID to the current
scheduler lease. Acquiring a lease clears prior readiness. The lifecycle manager
must also check current Kubernetes readiness before reporting convergence.

An idle heartbeat is acknowledged without an episode. Task results and input
answers persist independently of process lifetime. The integration suite starts
an isolated database with the repository's exact image digest and database-scoped
runtime credentials; missing test environment variables cannot silently skip it.
