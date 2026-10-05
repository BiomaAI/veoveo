# Output Audience Plan

Status: accepted direction under
[CE-14](CONTRACT_EVOLUTION.md#ce-14-agent-output-reaches-only-its-audience). No phase
has started. Workspace keeps its current tool-result behavior until Phase 4 passes
acceptance.

Baseline: Veoveo main `7379565ec` on 2026-10-04.

This plan lets a Workspace chat agent use tool results, Knowledge and Artifacts inside
a shared chat without showing any member content that member cannot read. Each phase
lands as a hard cut with its owner's tests and design update in the same change.

## Standards And Protocols

| Standard or contract | Plan boundary |
|---|---|
| [Model Context Protocol](../mcp/contract/DESIGN.md) `2026-07-28` | Tool results carry an optional access descriptor in `_meta`. The plan adds no protocol method and changes no wire version |
| `ai.veoveo/result-access` | Repository-owned `_meta` key defined by the hosted-server contract. The gateway is its only consumer, so the key needs no capability negotiation |
| [`ai.veoveo/knowledge-source`](../mcp/knowledge-extension/DESIGN.md) access descriptor | The single access vocabulary: tenant, Work Context, read policy, owner, read grants and data labels |
| [Work Context governance](WORK_CONTEXT_GOVERNANCE.md#output-ownership-and-access) | The existing read decision: same tenant, membership or a live grant, and clearance for every data label |
| SurrealDB 3.3.0 | Knowledge candidate admission for an audience runs in SQL before ranking and limits |
| JSON Schema 2020-12 | Generated schemas for the descriptor field and recorded audiences |

## Baseline

A chat run submits each tool call with the sender's session
(`platform/workspace/src/gateway/runs/tools.rs`). The model receives a receipt, and the
result appears only in the sender's Activity. Knowledge search admits candidates for
one caller through `CandidateScope` in `platform/store/src/knowledge/admission.rs`.
Tool results carry no access description, so the gateway cannot decide who else may
read them. Chats stay safe, and the agent cannot use results, Knowledge answers or
generated images in its reply.

## Model

| Output | Audience |
|---|---|
| Chat reply | current members of the chat |
| Activity record of an operation | the principal that invoked it |
| Memory write | members of the memory's scope |

The sender's authority admits each tool call. Content enters model input only when
every audience member can read it. A run records the access descriptors of what it
read and publishes only to an audience that satisfies all of them. A result without a
descriptor is readable only by its invoker. CE-14 records the reasoning.

## Phase 1: Shared Access Vocabulary

- Move `AccessDescriptor`, `ReadPolicy` and `ReadGrant` from
  `mcp/knowledge-extension/src/models.rs` beside the access decision in
  `mcp/contract/src/access.rs`. The knowledge-source extension imports them, and wire
  forms and schema titles do not change.
- Add an `Audience` value that holds each member's resolved access facts: tenant, Work
  Context memberships, group memberships and clearance. One check admits a descriptor
  when every member passes the existing read decision.
- Represent "invoker only" as an explicit descriptor value. Every result then has a
  descriptor, and the gateway evaluates results through one path.

Acceptance: owner tests cover each read policy for one-member and multi-member
audiences, an expired grant, a member missing one data label, and the invoker-only
value.

## Phase 2: Result Access Field

- Define the optional `_meta` key `ai.veoveo/result-access` in the
  [hosted-server contract](../mcp/contract/DESIGN.md). Its value is the Phase 1
  descriptor. The gateway rejects a descriptor that names another tenant and stores
  the admitted descriptor with the Task.
- A missing field makes the result readable only by its invoker. A server that has
  not adopted the field lists the gap in its Contract Compliance section.
- First-party servers attach descriptors where their data already has them. The time
  server is the reference adopter. `resolve_time` and `convert_time` attach `tenant`,
  and temporal-event results attach `subjects`, matching the descriptors on its
  Knowledge observations.
- Shared conformance checks each adopter: a member outside the declared audience never
  receives the result.

Third-party servers do not attach the field, and their results stay with the invoker.
An installation override needs a concrete requirement and its own decision.

## Phase 3: Audience-Filtered Reads

- Knowledge search accepts an audience. `admission.rs` builds one candidate predicate
  per member, and SQL admits a candidate only when every predicate passes, before
  ranking and limits. Invitation acceptance caps a chat at 256 members
  (`platform/workspace/src/persistence/queries/decide_invitation.surql`), which limits the
  predicate count.
- Gateway resource reads evaluate the returned observation's descriptor against the
  audience.
- Artifact reads evaluate effective access for every member.

Acceptance: a restricted candidate never affects ranking or reaches the result, a
one-member audience returns the same results as the current caller search, and adding
a member removes exactly the candidates that member cannot read.

## Phase 4: Workspace Runs

- Run admission records the audience as the active chat members at the frozen context
  sequence.
- Each run accumulates the descriptors of the content it read: chat history, tool
  results, Knowledge candidates and Artifacts.
- A tool result enters model input when the audience can read it. Other results keep
  today's receipt and stay in the sender's Activity.
- When a Task finishes after its run ends, the result starts a follow-up run in the
  same chat if the recorded audience can read it.
- Publication compares the current members with the recorded descriptors. A failed
  check withholds the reply and tells the sender to ask again. The sender receives no
  notice that lists excluded sources.
- Each published reply stores its access requirements. History reads show a reply
  only to members who satisfy them.
- The worker's fixed instructions drop the receipt-only wording for results that
  return to the model.

Acceptance: fixtures with two people and one agent cover a restricted Knowledge item,
a restricted Artifact, a result without a descriptor, a member joining during a run, a
later member reading history, and a generated image returning to a chat whose members
can all read it.

## Owning Documents On Delivery

| Document | Change |
|---|---|
| [`WORK_CONTEXT_GOVERNANCE.md`](WORK_CONTEXT_GOVERNANCE.md) | output audience section |
| [`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md) | `ai.veoveo/result-access` field and its default |
| [`mcp/knowledge-extension/DESIGN.md`](../mcp/knowledge-extension/DESIGN.md) | descriptor types imported from the shared contract |
| [`platform/store/src/knowledge/DESIGN.md`](../platform/store/src/knowledge/DESIGN.md) | audience candidate admission |
| [`platform/workspace/src/persistence/runs/DESIGN.md`](../platform/workspace/src/persistence/runs/DESIGN.md) | recorded audience, publication check and reply requirements |
| [`apps/workspace/DESIGN.md`](../apps/workspace/DESIGN.md) | results in chat and replies hidden from later members |
| [`servers/time-mcp/DESIGN.md`](../servers/time-mcp/DESIGN.md) | result descriptors |

## Out Of Scope

- Chat-agent memory, deferred in the
  [agent memory plan](AGENT_MEMORY_PLAN.md#chat-agent-memory).
- Managed agents, which act under their own identity.
- Per-user delegation to external systems, which CE-11 leaves to its own decision.
