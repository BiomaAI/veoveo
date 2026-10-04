# Durable Agent Runtime

## Standards And Protocols

The runtime uses the pinned SurrealDB 3.3.0 WebSocket SDK and repository-owned
schema migrations. Typed records, transaction fences, native changefeeds and managed
generations are internal Veoveo contracts. Native MCP `2026-07-28` Tasks keep
their canonical gateway identity and retention pin; this crate owns delivery to
the agent, not the MCP transport or provider completion protocol.

## Gateway Integration

The optional `gateway` feature owns current managed OAuth registration, installed
template ceilings and authority admission through the gateway's resolver port.
Its [adapter design](src/gateway/DESIGN.md) specifies token issuance, current-request
checks and the remaining shared token vocabulary. Schema-only consumers enable
neither this adapter nor runtime dependencies.

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

## Wake And Authority Observation

`wake_observation` reads the earliest pending availability and claimed lease expiry
for the agent in SQL. Database time converts those deadlines to a delay. The kernel
arms that delay and interrupts it on native LIVE changes; an empty queue has no timer.
A closed local hint channel cannot create a busy loop. Remote writes wake the scheduler
through the same source. Debounced work stays in the durable queue, allowing a priority
wake to interrupt its delay.

The native source persists its versionstamp after the scheduler resumes processing.
Restart and expired history reconcile the current fenced queue. Managed pause removes
the availability timer while authority changes can resume scheduling. Dispatch waits
watch the managed instance, definition, tenant, principal, Work Context and runtime
lease. They recheck dispatch at the known lease expiry and close on source failure.
Input-request waits observe their own table and retain the caller's maximum wait.
Work Context observation needs only an identity and a current authority read, so its
feed does not retain prior row contents.

## Persistence Module Declaration

The independent `schema` feature exports `schema::module_setup(execution)` for the
`agents` optional module. It activates only `veoveo-modules` with default features
disabled; contract and runtime dependencies require their own features. Default
runtime behavior is unchanged. The declaration claims `agent_*`, `managed_agent_*`, and explicit `agent`, `managed_agent`, `wake`; seven existing Agent catalog, admission and result-consumption functions.
It requires Audit, whose transitive requirements include Tasks, Artifacts, Gateway and Identity.

The lane is empty. The composition root supplies the checked execution image and
command; the existing gateway composition image is the initial host candidate.
Its current `installation-bootstrap` command runs the mixed Store catalog, which
continues to own production migration execution. A named-lane command and its Job
require separate implementation and qualification. Future owner migrations and
queries belong together in this owner's crate, with one declaration per object.

The current Store `agent_management/import.surql` transaction reads Workspace runs, updates Workspace participants and chats, and creates Workspace events. The declared target omits a Workspace dependency. Participant import belongs in the Workspace owning API, composed with Agent catalog operations while preserving writer exclusion, transaction checks, rollback and audit. The current `fn::agent_consume_results` also updates kernel Task retention state; its explicit function claim does not approve foreign mutations.
