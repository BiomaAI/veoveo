# Agent Management Gateway

Status: authoring/publication handlers implemented with isolated database acceptance. The [delivery plan](../../../../../../docs/AGENT_MANAGEMENT_PLAN.md)
tracks client, runtime and installation work separately.

## Standards And Protocols

The service exposes the typed Veoveo HTTP JSON authoring contract in
`mcp/contract/src/agent_management`. Existing OAuth session/JWT verification admits
each request. Model connections use the repository-qualified Rig adapter; MCP
`2026-07-28` discovery validates tools under the caller's current gateway profile.
The platform-store catalog owns atomic mutation replay and executable revisions.

## Admission

Dedicated gateway actions separate definition metadata, private content, creation,
editing, publication, use, disable/enable, archive and ownership transfer. Profile,
tenant, scope and role checks use the canonical policy evaluator. The handler then
resolves current Work Context membership and passes its digest to the store. Human
and service callers follow the same action policy; authoring never grants automated
runtime authority.

`VEOVEO_AGENT_MODELS` declares approved model connections, their admitted contexts,
required scopes and execution ceilings. Public model choices omit endpoints and Secret
references. The connection digest binds executable provider/model configuration, while
Secret value rotation remains independent. Definition publication validates that exact
model revision and the caller's current MCP tool discovery.

Every publication audience context receives its own current membership check. Context
custodians/owners with the requested action may manage definitions owned by another
principal. Ordinary authors may manage only their own definitions in their home context.
The shared browser editor uses each application's own cookie, CSRF and profile
boundary. Console credentials cannot satisfy a Workspace request.

## Modules

`models.rs` owns installation model admission. `authority.rs` binds policy to current
Work Context state. `projection.rs` explicitly maps private store records into public
DTOs. Mutation, validation and event handlers remain separate from existing durable
agent messaging routes. Routine definition edits use the durable store and do not reload
gateway configuration or start model execution. `instances/` owns managed admission, operation reads and
public instance projections. Its templates derive every infrastructure and credential
reference from reviewed configuration. The gateway has no Kubernetes write permission.

## Observation And Retry

Publication receipts bind the public mutation, not server-derived context digests.
A retry still needs current identity, home-context access, action policy and audience
admission. It returns the original response without repeating publication. Current
revision checks protect new mutations. History pages use stable timestamp/digest order.

One gateway outbox subscription wakes bounded browser streams. A contentless SSE
sequence invalidates the local catalog; reconnect and periodic database reconciliation
recover missed hints. These checks never invoke a model. Events include former audience
contexts so removing publication access invalidates their catalogs too.

Enable revalidates the retained published model and capability selection. Model
connection revisions bind destinations, model names, Secret references and ceilings;
renaming a connection or changing its visibility does not rewrite executable identity.
Current model visibility is still checked independently.

## Installation Import

`gateway agent-catalog-import` is an explicit root-authorized migration command. It
requires `--control-plane`, `--source`, `--owner` and `--recovery`; approved connections
use the same `VEOVEO_AGENT_MODELS` document as gateway startup. The owner must already
exist as an enabled principal in the source tenant. This command publishes only the
installation-reviewed seed. Every subsequent execution applies normal model, human
and capability authority. Ordinary authoring uses the authenticated management API.

Drain or cancel current chat runs, stop every gateway writer, then install the schema
and updated control-plane bundle before import. The command refuses active runs and
conflicting existing definitions. Existing prompts are never overwritten. A private
mode-0600 recovery file is synced before binding conversion. Only listed source digests
can become the reviewed immutable revision; unknown historical configuration fails the
import. The transaction preserves IDs, presentation, membership and history and emits
chat events. Retries use the same recovery file and verify its source fingerprint.

Before reopening the gateway, `--restore` applies the inverse conversion from that
file. A real database test round-trips the recovery document, replays both directions
and rejects concurrent edits. Retained catalog records are not deleted on restoration.
After management mutations begin, use forward repair; deploying the former environment
catalog over newly authored records is unsupported. Keep recovery files outside Git.
This procedure adds no scheduled backup or runtime compatibility path.

## Managed Intent

`/agent-instances` admits creation and generation-preconditioned lifecycle changes.
`/agent-operations/{id}` exposes the corresponding authorized operation. HTTP 202 means
that the durable intent and capacity reservation committed; the lifecycle manager
reports resource progress separately. Provisioning replay reconstructs the original
resource plan, so configuration removal cannot orphan an uncertain create response.

Current template and model permission are required for deployment, resume, retry and
revision adoption. Owners retain stop, pause and archive controls when installation
configuration is removed. Public projection batches definition/revision metadata and
never returns authored instructions or deployment credentials. Shared event heads
include the current owner's instance changes and context-managed instances.
