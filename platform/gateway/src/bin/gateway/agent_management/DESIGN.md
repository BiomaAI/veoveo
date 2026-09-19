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
The pending browser integration must use each application's own cookie, CSRF and
profile boundary; Console credentials must never satisfy a Workspace request.

## Modules

`models.rs` owns installation model admission. `authority.rs` binds policy to current
Work Context state. `projection.rs` explicitly maps private store records into public
DTOs. Mutation, validation and event handlers remain separate from existing durable
agent messaging routes. Routine definition edits use the durable store and do not reload
gateway configuration or start model execution.

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
