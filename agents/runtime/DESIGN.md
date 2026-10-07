# Durable Agent Runtime

## Standards And Protocols

The runtime uses the pinned SurrealDB 3.3.0 WebSocket SDK and repository-owned
schema migrations. Typed records, transaction fences, native changefeeds and managed
generations are internal Veoveo contracts. Native MCP `2026-07-28` Tasks keep
their canonical gateway identity and retention pin; this crate owns delivery to
the agent, not the MCP transport or provider completion protocol.

Public operator messages, decisions, wake receipts and conversation projections use closed camelCase members. Input decision tags and conversation vocabularies use snake_case. Private Surreal records and AgentContent digest inputs keep their native field names. The public authoring adapter converts those records; the content digest hashes the internal typed serialization, rather than the public DTO. Runtime template and ModelConnection revisions bind their current public serialized forms.

## Contract Policy Declaration

The lightweight `contract` feature owns fourteen closed Agent actions and their
catalog descriptors. Agent handlers resolve typed keys from the admitted registry
before authorization or audit. Read access covers conversations, definitions and
instruction content; all other Agent actions declare write access. The owner keeps
the existing administrative-operation audit mapping. The same feature owns
[authoring and managed-instance DTOs](src/contract/authoring/DESIGN.md), operator-control
and conversation DTOs, template/model admission and configuration digests.
Contract-only builds enable no MCP, database, async runtime or GPU dependency.

## Gateway Integration

The optional `gateway` feature owns current managed OAuth registration, installed
template ceilings and authority admission through the gateway's resolver port.
Its [adapter design](src/gateway/DESIGN.md) specifies token issuance, current-request
checks and the owner token claim. The separate `catalog` adapter validates a complete
catalog revision and projects installation facts for gateway and Manager consumers.
Schema-only consumers enable
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

## Execution Queries

Runtime execution statements live in `src/queries/runtime`, with episode, managed
readiness and wake-observation queries beside their respective query families.
Operator-control statements live in `src/queries/control`. Rust embeds complete
statements with `include_str!` and binds record identities, content, fences and
timestamps as parameters. Each transaction stays in one query file; its caller
owns response decoding and statement result slots.
Gateway projections and colocated fixtures embed their statements from the matching
families under `src/queries/`. Native integration fixtures use `tests/queries/`.
Fixture mutations select complete statements and keep their values bound.

Both human-authorized and trusted managed repository queries admit stored field
names against the typed native encoding before checking executable digests and
copied projections. This catches undeclared root and nested fields that the SDK's
native derive otherwise ignores. Typed template parameter maps preserve their
arbitrary keys. Chat execution admits the schema's four inactive managed fields
only when their values are `NONE`; `NULL` does not represent an absent field.
Native identities, timestamps and integer values use the driver's typed decoding.
Registration and controller reconciliation validate the revision's content,
digest, execution, model, tools and template revision before consumers receive it.
SQL authorization and claim fencing run before this repository decoding profile.
The profile governs repository results, rather than direct use of SDK derives.

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

Definitions and published revisions copy typed execution, model and tool fields in
same-transaction writes. Chat revision admission selects the declared execution field
and requires its chat kind alongside tenant, audience, publication and lifecycle checks.
Managed template revision is a declared revision field;
parameters remain an opaque whole value for update comparisons. Repository result
types validate these fields against admitted content and its digest. Catalog wrappers
check their selected revision before exposing metadata. Managed identity, resources
and public keys declare their known children, reject missing values and unknown keys,
and enforce the client/workload uniqueness indexes.

## Persistence Module Declaration

The independent `schema` feature exports `schema::module_setup(execution)` for the
`agents` optional module. It activates `veoveo-modules` with default features disabled and the foundational
vocabulary, Serde and schema dependencies used by owner table declarations. MCP,
Store and asynchronous runtime dependencies require their own features. Default
runtime behavior is unchanged. The declaration claims `agent_*`, `managed_agent_*`, and explicit `agent`, `managed_agent`, `wake`; seven existing Agent catalog, admission and result-consumption functions.
It requires Audit, whose transitive requirements include Tasks, Artifacts, Gateway and Identity.

The version-zero lane installs the complete current Agent schema from
`src/schema/migrations/0000_current.surql`. The composition root supplies its checked
execution image and command. Gateway composition prepares the selected installation
and executes this lane through `module-migrate`; serving requires current lane proof.
The declaration itself does not certify an installed migration Job or image.

Workspace owns retained chat-participant conversion in its private
`persistence/agent_import` implementation. Agent scheduling SQL mutates Agent-owned
rows. `fn::agent_consume_results` reads the explicit Gateway retention-route leaf
and calls the Tasks retention-release leaf for a present canonical Task. The caller's
result-consumption transaction includes that release, preserving rollback and pin
checks. Its version-zero migration declares the introducing Identity, Gateway and
Tasks API minima.

## Persistence Observation

`AgentObservationTable` declares the owner's closed observation table names under the
`schema` feature. Runtime consumers convert these declarations into checked
`ObservationTable` descriptors and compose them with kernel tables. The descriptor
admits an identifier; it does not certify installed schema or grant read authority.
These owner tables declare 30-day changefeed retention matching the installed SQL.
LIVE invalidation and changefeed recovery keep their existing reconciliation and
checkpoint behavior. Public DTO contract features do not activate observation sources.

## Retained Execution Envelopes

The owner declares five wake payload variants matching `WakeKind`: terminal Task
identity, resource URI, named timer, operator message and input-request phase.
`persistence/envelopes.rs` owns their JSON and native-driver admission. Scheduling
checks the kind against the payload before enqueue and after claim. Operator
conversation reads require request, actor and Work Context attribution. Kernel
priority and heartbeat decisions match the typed phase and timer vocabulary.
Resource URIs, canonical Task IDs, actor principals and Work Context identities
reuse their admitted shared scalar types while preserving their public strings.
Wake identities, deduplication keys, claim fences and episode lineage live outside
these payloads and participate in the existing transactions.

Agent Tasks retain Rig's v1 descriptor fields `version`, `backendType`,
`executionId` and `payload`. The backend owns the JSON shape of `payload`.
Rig backend execution IDs stay backend-owned strings in the descriptor.
A complete descriptor must name the retained Task. The empty incomplete envelope
keeps its Task identity and retention pin in the surrounding row; the watcher
retries observation without dispatching another mutation. Descriptor completion
updates the same retained row. Unsupported versions fail admission.

Terminal results use an output body with optional `in_run` or `watcher` delivery,
an error with optional delivery, or the retained watcher error wrapper. Output
bodies accept arbitrary JSON. Controlled members reject unknown keys, native
record values and nonfinite JSON numbers. The shared Store JSON codec preserves
unsigned integer precision. Native `NONE` represents absence only for each
variant's declared optional members. The terminal settlement transaction still
creates one wake, and episode consumption releases the retained Task pin.

Managed readiness declares generation and Pod UID. Episode bindings declare
instance, definition revision, generation and dispatch epoch. Their native
adapters reject unknown members, wrong record parents and invalid counters;
episode admission and readiness queries check the current instance relationship.
The stop transaction reads the retained binding to select the running episode.

## Internal Client Authority

The `internal-clients` feature supplies `ManagedInternalClientAuthorityResolver`
with persistence and contract dependencies independently of the full runtime and
Gateway. It checks live managed registration even for an installed static client,
rejecting registration collisions before choosing a branch. Managed admission
checks enabled state, tenant and Work Context, instance generation and dispatch
epoch, profile/issuer/resource/authorization server, session-family exclusion,
scopes, roles and revision tools. The adapter declares ManagedAgent and
AgentDefinition observation tables to trigger current-authority revalidation.

## Lifecycle Contract

The public Agent contract owns `AgentState`. Persistence enables its native SDK
adapter and uses the same declaration for retained rows. Contract-only consumers
receive the lifecycle vocabulary without the database SDK or runtime services.
