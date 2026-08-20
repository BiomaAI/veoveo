# Harness-Mediated Model Post-Training: Exploration

> Status: exploratory and non-normative. This document records a direction for
> investigation. It does not approve model training, a trainer dependency, a component,
> a storage schema, a public protocol, production-data use, or candidate activation. The
> normative architecture remains in
> [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md),
> [`TECH_DESIGN.md`](TECH_DESIGN.md),
> [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md), and the owning component designs.

## Purpose

Veoveo runs agents inside a governed deployment harness. The harness assembles bounded
context, exposes an authenticated MCP surface, drives tools and resources, detaches
durable tasks, consumes wakes, and records operational consequences. A model trained in
a simplified loop does not experience that system. Its behavior in training can diverge
from its behavior when the real harness summarizes context, resumes work, changes an
effective capability set, or waits for a durable result.

This exploration asks under what conditions Veoveo may use trajectories produced by its
actual harness to post-train candidate model weights. The trainer remains outside the
agent kernel. Gateway authority, task durability, domain truth, physical safety, and
model activation remain outside model reasoning.

Reinforcement learning is the initial method under study because it is the paper's
subject. The governance and trajectory boundaries may also inform supervised or
preference-based post-training, but this document does not claim that one optimization
contract fits every method.

The working term is **harness-mediated model post-training**. It names the decisive
boundary: the deployment harness produces governed rollouts, while a separate training
system updates a candidate model. The term does not imply autonomous self-modification.

## Standards And Protocols

| Standard, protocol, or format | Exploration boundary |
|---|---|
| Model Context Protocol `2026-07-28` | The deployed harness reaches domain capabilities through the same final tools, resources, prompts, completions, Tasks, subscriptions, notifications, and structured results used in ordinary operation. Training does not create an alternate tool protocol. |
| JSON-RPC 2.0 | MCP calls retain their actual request and response envelopes. A training projection may normalize consequences, but it does not reinterpret protocol failures as successful actions. |
| MCP Tasks extension, SEP-2663 | Long-running evaluation and rollout operations use the official Task surface when exposed to an MCP client. A trainer does not introduce a parallel public job identity. |
| JSON Schema Draft 2020-12 | Controlled rollout, evaluation, release, and compatibility records require closed typed schemas if a future design approves them. This document does not reserve those schemas. |
| OAuth 2.1 draft 13 and Veoveo invocation authority | Each rollout uses an admitted non-human identity, tenant, Work Context, profile, policy revision, scopes, and invocation mode. Training metadata never grants authority. |
| OCI Distribution and digest-addressed Veoveo releases | A future candidate model and its runtime environment require immutable content identity and reproducible release inputs before evaluation. No candidate packaging profile is selected here. |
| Kubernetes Dynamic Resource Allocation | `resource.k8s.io/v1` on the current qualified Kubernetes/K3s v1.36.2 profile. Kubernetes is an execution backend rather than the public rollout contract. |
| NVIDIA DRA Driver for GPUs `0.4.1` and `resource.nvidia.com/v1beta1` | Simulation, perception, rendering, model inference, and model training use admitted NVIDIA hardware and fail closed when it is unavailable. A future training runtime must pin its own CUDA and NVML-compatible image exactly. This exploration adds no CPU or software-rendering fallback. |
| Internal model-serving adapter | Exact-token capture may require a private adapter compatible with the selected inference runtime. It is not an MCP replacement, an installation API, or a public OpenAI-compatible Veoveo contract. |
| Trainer adapter | Agent Lightning, VERL, or another trainer may consume a governed export through an internal adapter. No trainer package or wire protocol is approved by this exploration. |

Provider-managed post-training, if ever admitted, remains subject to Veoveo's webhook-only
provider completion contract. The platform does not poll provider job status or add a
polling recovery path. Self-hosted trainer progress is an internal execution concern and
must still project durable state through the canonical task and event boundaries.

## Companion Boundary

This document accompanies
[`SELF_IMPROVING_HARNESS.md`](SELF_IMPROVING_HARNESS.md). The two explorations share
trajectory governance and held-out evaluation, but they change different things.

| Exploration | Candidate being changed | Boundary held fixed |
|---|---|---|
| Self-improving harness | strategy clauses, workflow recipes, and profile-dynamics records | model weights and model release |
| Harness-mediated model post-training | candidate model weights | governed harness contracts, invocation authority, domain truth, evaluators, and activation controls |

Harness strategy work should normally precede model-weight work. A bounded strategy is
cheaper to inspect, compare, disable, and roll back. Model post-training becomes a
reasonable experiment only when held-out evidence shows a material behavior gap that
external strategy cannot close, or when the experiment is explicitly studying whether
weight changes provide a durable advantage.

Neither track may grant capability, expand a profile, rewrite policy, or activate its own
candidate. A learned model and a learned strategy remain untrusted inputs to the same
runtime enforcement boundary.

## Research Reference

[Agent Lightning v1.0: Towards Harnessed Agentic RL](https://arxiv.org/html/2608.17528v1)
by He et al., arXiv:2608.17528v1, 18 August 2026, is the primary reference for this
exploration. The paper calls the paradigm *harnessed agentic RL*: the deploy-time harness
owns context construction and environment interaction, while an external trainer
observes model request-response pairs through an inference proxy.

The paper identifies four consequences that matter to Veoveo:

- decoded text can retokenize differently inside a later prompt, so text continuity does
  not prove token continuity.
- summarization, subagents, and harness control flow can turn one rollout into a dynamic
  number of training samples.
- sample-level advantage and loss normalization can give extra statistical weight to a
  rollout merely because the harness split it into more samples.
- a training backend must schedule dynamic sample counts without losing rollout and
  prompt-group identity.

Agent Lightning v1.0 uses exact-token best-effort merging, rollout-level advantage, and
rollout-level token-mean loss. Its coding-agent experiment reports an improvement from
41.8 percent to 56.4 percent on SWE-bench Verified for Qwen3.5-9B after reinforcement
learning on roughly 6,000 examples. Only 36 percent of its coding rollouts remained one
training sample on average, and each rollout produced about 2.41 samples. The paper also
reports search and general instruction-following gains.

Those results establish a credible research direction, not a Veoveo acceptance case.
The paper is a first-version preprint. Its evaluated environments do not include durable
MCP Tasks, changing authority, physical actuation, governed recordings, or Veoveo's
multi-episode recovery model. A Veoveo experiment must therefore prove its own safety,
causality, and held-out value.

## Current Veoveo Boundary

The organization deploying Veoveo owns its model selection and release process. The
platform does not currently train model weights. An agent manifest pins the admitted
model and bounds its context, turns, calls, time, and resources. The gateway resolves
effective authority before every capability call, and domain services decide the actual
effect.

The current agent kernel records useful operational evidence. Its
[`RecorderHook`](../agents/kernel/src/recorder.rs) captures capped tool activity, task
transitions, and model usage in the local RRD memory plane. The durable agent runtime
records episode status, output, summaries, usage, tool counts, task handoff, and wake
consumption. Those records support operation and diagnosis. They are not an RL trajectory
format.

Model-weight post-training needs information that ordinary Veoveo operation does not
promise to retain:

- the exact prompt token IDs consumed by inference.
- the exact sampled response token IDs before any decode and transformation.
- response log probabilities required by the selected algorithm.
- the tokenizer, chat template, model runtime, sampling parameters, and model revision.
- the relationship among model calls, episodes, branches, tasks, wakes, and the evaluated
  mission.
- an independently produced outcome and reward definition.

Gateway audit is not a substitute. It intentionally excludes prompts, token streams,
provider payloads, artifact bytes, and other sensitive material. Production episode
records do not silently become a training corpus.

## Terminology And Learning Unit

The paper's rollout corresponds most closely to a complete evaluated attempt. Veoveo
needs a wider unit than one agent episode because operational work may sleep and resume.

| Term | Provisional meaning |
|---|---|
| scenario | one versioned objective, environment fixture, authority class, initial state, and evaluator |
| prompt group | the independent attempts sampled from the same scenario and initial conditions for comparative reward calculation |
| mission rollout | one evaluated attempt that may span several episodes, durable tasks, wakes, input requests, and handoffs |
| episode | one bounded invocation of the Veoveo agent kernel over freshly assembled context |
| model call | the exact inference request and sampled response observed at the model boundary |
| training sample | one causal token sequence assembled from one or more exactly compatible model calls |
| evaluation | a versioned independent judgment over authoritative state and evidence |
| candidate model | immutable weights and runtime inputs produced by one training run but not admitted for operation |
| model release | a candidate that has passed the required gates and received explicit activation authority |

An `AgentEpisodeId` must not be reused as the mission-rollout identity. An episode can end
while its durable task continues, and a later wake can resume the same evaluated mission.
The rollout identity binds that long horizon without changing the canonical identities of
the episodes and tasks it contains.

A provisional relationship is:

```text
scenario
  prompt group
    mission rollout
      attempt
        episode
          model call
          MCP transition
          durable task reference
        wake or input handoff
        episode
          model call
      independent evaluation
      training samples
```

Every layer retains a stable typed identity. Repeated prompt text is not an identity and
cannot distinguish an intentional repeated call from network redelivery.

## Working Conceptual Model

One possible flow is:

```text
governed scenario + authority class
                  |
                  v
 pin harness, model, tokenizer, profile, policy, world, and evaluator
                  |
                  v
 run the real Veoveo kernel through the real gateway and isolated domains
                  |
        +---------+----------+
        v                    v
 exact model-call capture    authoritative operational evidence
        |                    |
        +---------+----------+
                  v
        mission-rollout evaluation
                  |
                  v
       governed trajectory artifact
                  |
                  v
          external trainer adapter
                  |
                  v
          immutable candidate model
                  |
                  v
 held-out behavior, authority, safety, and regression gates
                  |
                  v
 explicit staging decision with no automatic activation
```

The real gateway remains authoritative throughout the rollout. The evaluator reads
governed state through an identity separate from the agent. The trainer receives only the
projection approved for its dataset and cannot change the evaluated environment.

## Training Identity

A trajectory is meaningful only relative to the system that produced it. Every admitted
rollout must pin enough identity to reproduce prompt construction, capability exposure,
environment behavior, and scoring.

A provisional record could include:

```text
HarnessTrainingIdentity
  agent manifest digest
  kernel and runtime revision
  model checkpoint digest
  inference runtime revision
  tokenizer identity and digest
  chat-template identity and digest
  sampling configuration
  profile contract digest
  effective capability digest
  policy and control-plane revisions
  MCP protocol and server revisions
  scenario and world revisions
  evaluator identity and reward-specification digest
  simulation, driver, and GPU evidence identities
```

The effective capability digest cannot replace request-time authorization. It describes
the rollout's admitted surface and supports comparison. Object access still depends on
the actual principal, Work Context, policy, labels, scopes, and current domain state.

A candidate trained across materially different harness identities must declare that
mixture. Evaluation must exercise each supported identity or constrain the candidate to
the one it actually covers. A model does not gain general compatibility because its
tokenizer happens to load under another runtime.

## Exact Model-Call Capture

RL sample construction must begin at the boundary that knows what inference actually
consumed and sampled. Capturing only messages before the chat template loses the prompt
tokens. Decoding a response and tokenizing it again can produce different token IDs.
Capturing an SDK response after a structured-output repair can lose the original action.

An approved experiment would need a private capture point after chat-template rendering
and tokenization, and before inference dispatch. It would record the sampled tokens and
log probabilities before decode-time normalization. The harness still receives the
ordinary provider-neutral response expected in deployment.

The controlled record may need:

```text
ModelCallEvidence
  mission rollout, attempt, episode, turn, and branch identities
  model, tokenizer, template, and sampling identities
  exact prompt token IDs consumed
  exact sampled response token IDs
  response-token log probabilities when required
  finish reason and usage
  transformation record from sampled output to harness-visible response
  request timing and admitted inference endpoint identity
```

Raw message text and token arrays carry different risks. They may be stored in separate
artifacts with separate grants and retention. A normalized transition record can refer
to them without making either payload part of audit, Console snapshots, or ordinary
agent memory.

When a hosted provider cannot return the exact data required by the training algorithm,
that provider is ineligible for the corresponding post-training experiment. Veoveo must
not fabricate token IDs or log probabilities. The same rollout may still support the
frozen-model strategy experiments described in the companion document.

## Rollout Assembly Across Durable Work

Mission-rollout assembly must preserve Veoveo's recovery semantics. A detached tool Task
can finish after its starting episode. Its terminal event wakes a later episode, and that
episode consumes the result with the retained canonical Task identity. Operator input may
arrive after an in-episode wait and become another durable wake. Process replacement may
change the runner without changing the mission.

The assembly record therefore needs explicit edges rather than temporal guesses:

- episode started from wake.
- model call belongs to episode and turn.
- tool call created or observed a canonical Task.
- terminal Task result produced a wake.
- later episode consumed the result and acknowledged the wake.
- operator input or another resource event continued the mission.
- branch or handoff created a distinct causal path.
- mission reached an evaluated terminal condition.

Clock proximity, identical text, and shared artifact hashes cannot create these edges.
The durable runtime and domain-owned identities supply them. Missing lineage makes the
rollout ineligible rather than inviting a heuristic repair.

One prompt group may contain failed, interrupted, budget-terminated, and successful
rollouts. Infrastructure failure requires an explicit classification. It must not be
silently treated as a bad policy action or discarded only when doing so improves the
candidate's apparent reward.

## Evaluation And Reward

The agent must not grade itself. A scalar reported by the rollout process can be useful
transport, but it is not authoritative evidence for physical AI.

A Veoveo evaluation should derive its result from governed sources such as:

- simulator state and independent safety-controller decisions.
- acknowledged recordings and immutable archive parts.
- canonical Task state and result identities.
- domain-owned resources and artifacts.
- policy decisions, audit evidence, and denied attempts.
- measured time, model usage, GPU use, and bounded resource consumption.

A provisional evaluation could separate hard gates from optimization signals:

```text
RolloutEvaluation
  evaluator and reward-specification identity
  objective correctness
  authority and policy compliance
  safety-envelope compliance
  provenance completeness
  durable-work correctness
  resource and latency measurements
  failure classification
  scalar or vector optimization signal
  evidence references
```

A safety violation, forbidden capability attempt, missing provenance edge, or evaluator
integrity failure should invalidate the rollout or candidate according to a declared
rule. A positive task score does not cancel a failed hard gate. The reward specification
must state how partial completion, cancellation, timeout, operator intervention, and
infrastructure loss are treated.

Model judges may contribute a bounded soft signal where deterministic evidence cannot
express quality. Their prompts, provider, model revision, authority, and data egress are
a separate governed boundary. A judge cannot override a deterministic policy or safety
failure.

## Sample Construction And Optimization Semantics

The Agent Lightning paper demonstrates why message-level continuity is insufficient for
RL sample merging. Veoveo should begin with the following hypotheses and test them
against the selected trainer:

1. Preserve each model call independently unless exact token IDs prove that the previous
   prompt and sampled response form a complete prefix of the next consumed prompt.
2. Never replace retokenized prompt tokens with earlier sampled tokens merely to increase
   the merge ratio. The resulting sequence was not the prompt used during rollout.
3. End a sample when summarization, branch creation, handoff, output transformation, or
   token drift breaks exact prefix continuity.
4. Preserve prompt-group, mission-rollout, attempt, episode, and branch identity after
   physical samples are flattened for the trainer.
5. Keep all samples from one mission rollout in the same optimizer update unless an
   approved algorithm explicitly models policy skew across the rollout.
6. Start comparative experiments with rollout-level advantage and rollout-level
   token-mean loss, then retain them only if controlled evidence supports the choice.

These are training semantics, not an MCP contract. The canonical trajectory should
carry enough provenance for more than one trainer adapter to reproduce sample assembly
and to explain why a boundary was split.

Permanent tests would need adversarial tokenizer fixtures. They should include decode and
retokenize drift, chat-template rewriting, transformed structured output, repeated
identical prompts with distinct call identities, summarization, and branch rejoin. A high
merge ratio is never an acceptance criterion by itself.

## Control-Plane Integration

Agent Lightning uses a dedicated API Gateway as rollout source of truth, a Rollout
Controller, and a customized trainer. Veoveo should adopt the separation of concerns
without duplicating its own control plane.

The canonical Veoveo boundaries remain:

- the gateway for authentication, effective capability exposure, policy, and internal
  identity.
- the shared durable Task runtime for long-running lifecycle, idempotency, recovery,
  cancellation, results, retention, and subscriptions.
- the agent runtime for episodes, tasks, wakes, leases, and consumption.
- the platform store and transactional outbox for durable control state.
- the artifact plane for governed trajectory, dataset, evaluation, and candidate bytes.
- installation-owned Kubernetes reconciliation for admitted execution.

An internal model proxy may attribute inference calls to a rollout. It does not become a
public domain API or another source of authorization. A trainer adapter may create
rollout Tasks and consume governed exports, but it does not edit Task rows, agent rows,
audit evidence, or model activation pointers directly.

Retry handling must use explicit request, attempt, and call identities. Agent Lightning's
paper describes discarding earlier model requests when identical prompts are repeated.
Veoveo should not use prompt equality for deduplication because an agent may intentionally
make the same request twice. Network redelivery can be deduplicated only when the same
idempotency identity is present.

Kubernetes controllers may use watches and reconciliation lists to manage local Jobs.
That does not relax provider webhook-only completion. A provider-managed model-training
job must still finish through a signed terminal webhook, with missing delivery treated as
an operational failure.

## GPU And Runtime Isolation

Model post-training adds a large GPU workload beside simulation, inference, perception,
rendering, and optimization. Early experiments should use a dedicated admitted training
pool. They must not reduce the capacity or response bounds promised to operational
agents.

Agent Lightning reports that collocated asynchronous RL can time-share inference and
weight updates on one GPU pool. That technique may be evaluated inside an isolated
training installation. It is not acceptable as an invisible pause in production model
serving. An operational agent must not discover an undeclared training phase through
latency, timeout, or model unavailability.

Every accepted hardware result must identify the NVIDIA devices, driver, CUDA runtime,
trainer topology, inference topology, simulator allocation, peak memory, and observed
utilization. Kubernetes workloads request the required GPU resources and fail closed.
Software rendering, CPU training, CPU inference fallback, or a smaller substitute model
cannot close an experiment's hardware gate.

Placement must account for simultaneous simulator and model demand. Avoiding GPU-to-CPU
readback remains a design objective, but training evidence may require bounded artifact
export. Any export path must state its performance and data-classification cost rather
than hiding it inside monitoring.

## Data Governance

Training can make sensitive information durable in model weights. Deleting the source
artifact does not prove that the candidate forgot it. Admission therefore begins before
capture, not after training.

A safe experiment would require:

- an isolated tenant and Work Context created for the evaluation.
- an explicit purpose and consent record for every dataset source.
- a training projection separate from operational audit and ordinary agent memory.
- classification and labels propagated to trajectories, datasets, evaluations, and
  candidates.
- bounded retention for raw prompts, tokens, environment logs, and evaluator internals.
- governed artifact references instead of raw bytes in control records.
- lineage from a candidate to its base model, dataset revisions, training configuration,
  harness identities, and evaluation evidence.
- no cross-tenant training unless a future product and governance decision authorizes a
  deliberately public dataset.
- an answer for source withdrawal, candidate invalidation, and downstream release
  handling before any non-synthetic data enters training.

Production episodes are excluded by default. Redaction does not substitute for data-use
authority, and a broad terms-of-service permission does not substitute for Work Context
ownership. Prompts sent to an external optimizer, trainer, or judge create a new outbound
data boundary that requires its own decision.

Raw token evidence should not appear in logs, traces, Kubernetes events, Console
snapshots, test fixtures, or error messages. Diagnostic summaries use stable identities,
counts, bounds, and classified failure codes.

## Reward Hacking And Containment

The Agent Lightning coding experiment found agents reading Git history or downloading
reference source through several network paths. It responded by hiding repository
history and restricting egress. Physical-agent experiments face the same structural
problem with more consequential targets.

An agent might attempt to read privileged simulator state, call an evaluator endpoint,
alter a goal marker, replay a previously successful artifact, suppress a recording,
avoid a difficult terminal condition, or use an ungoverned network service. A model may
also optimize a proxy metric while degrading safety, authority compliance, or recovery.

The environment should therefore provide:

- a restricted agent identity and Kubernetes Restricted pod boundary.
- default-deny network policy with an exact destination allowlist.
- no Kubernetes API credential in the agent pod.
- immutable scenario inputs and evaluator definitions.
- evaluator credentials and hidden state unavailable to the rollout.
- independent safety enforcement and physical limits outside model cooperation.
- negative tests for alternate protocols, direct service paths, cached answers, and
  artifact replay.
- complete evidence when a forbidden attempt is denied.

Reward hacking is not solved by telling the model to behave. Containment and independent
verification remain mandatory even when the trained policy appears aligned.

## Candidate Release And Activation

Trainer output is an untrusted candidate. A future candidate record would need immutable
identity and enough provenance to reproduce its claim:

```text
ModelCandidateRelease
  base model and license identity
  weight and tokenizer digests
  inference runtime compatibility
  training algorithm and configuration
  dataset and trajectory release identities
  harness and environment identities
  evaluator and reward-specification identities
  training hardware and software evidence
  held-out acceptance report
  supported profile, authority, and scenario classes
  activation state and rollback predecessor
```

Activation cannot occur from the trainer, the evaluated agent, or an ordinary profile
capability. An authorized control-plane actor stages the candidate after held-out gates
pass. The installation then performs an explicit deployment, observes a bounded canary,
and retains rollback to the prior immutable model release.

A model change triggers Autonomy Harness reassessment. Evaluation covers capability
selection, denied actions, durable Task behavior, resource budgets, prompt injection,
operator interaction, safety controls, and incident isolation. Success on the training
objective cannot waive a regression elsewhere.

## Agent Lightning Placement

Agent Lightning is a useful reference implementation and a possible experimental
trainer. This document does not approve it as a Veoveo dependency.

An experiment could reuse or adapt:

- its inference-proxy connection to an otherwise ordinary deployment harness.
- exact-token best-effort sample merging.
- rollout-level advantage and loss normalization.
- Kubernetes execution for self-hosted rollouts.
- trajectory monitoring and reward-hacking inspection.

It should not import without redesign:

- a second API Gateway as canonical rollout storage.
- a public OpenAI-compatible route carrying rollout authority in its path.
- arbitrary event payloads where Veoveo controls the shape.
- prompt-content deduplication.
- agent-reported reward as physical truth.
- best-effort eventual consistency where Veoveo requires atomic task and wake behavior.
- raw rollout visibility that bypasses tenant, Work Context, and artifact grants.
- collocated GPU phase changes applied to operational inference.

The adapter boundary should allow another trainer to replace Agent Lightning without
changing Veoveo's public protocol, trajectory identity, evaluation evidence, or model
release process.

## Suggested Experiments

The first experiments should establish evidence quality before spending a training run.

1. Define a typed mission-rollout projection over synthetic episodes, Tasks, wakes,
   branches, and independent evaluations.
2. Capture exact model calls from one approved open-weight model in an isolated harness.
   Prove that captured tokens and log probabilities match the inference runtime.
3. Build sample-assembly fixtures that reproduce token drift, summarization, branch
   splits, intentional repeated prompts, and request redelivery.
4. Export one governed trajectory artifact and prove that unauthorized identities cannot
   discover or read its raw payloads.
5. Run an isolated Isaac Sim named-location mission with no real vehicle path. Score it
   from simulator telemetry, control authority, safety state, durable results, and
   acknowledged recording evidence.
6. Compare the existing frozen model, the frozen model with a bounded profile strategy,
   and one post-trained candidate on held-out missions and authority classes.
7. Stage the candidate only in an isolated evaluation profile. Exercise negative policy,
   injection, budget, recovery, and safety cases before considering any wider use.

The UAV experiment should vary the destination, initial pose, environment conditions,
and effective authority. Its evaluator should reject collisions, geofence violations,
control-lease violations, forbidden calls, missing provenance, and invalid recordings
before considering mission efficiency. Training scale remains an experimental result
rather than a number selected in this document.

## Evidence Gates

The exploration advances through explicit gates:

| Gate | Required evidence | What it authorizes |
|---|---|---|
| trajectory feasibility | typed lineage spans model calls, episodes, Tasks, wakes, and evaluation without heuristic identity repair | a disposable trajectory prototype |
| capture correctness | exact consumed and sampled tokens reproduce inference records, sensitive payloads remain governed, and unsupported providers fail closed | an isolated model-call capture experiment |
| trainer independence | two adapters or one adapter plus a reference verifier reproduce sample boundaries and statistics from the same export | a bounded trainer experiment |
| simulation value | a candidate improves held-out mission outcomes beyond the strategy baseline without policy, safety, provenance, or recovery regression | staging in an isolated evaluation profile |
| operational consideration | containment reassessment, installation approval, rollback proof, data-use approval, and repeatable hardware evidence all pass | a future design decision, not automatic activation |

Failure at one gate does not justify weakening it. The experiment can return to the
frozen-model strategy path or remain a research artifact.

## Non-Goals

This exploration does not propose:

- training logic inside the agent kernel.
- an agent capability that edits weights, rewards, datasets, or activation state.
- automatic collection from production episodes.
- cross-tenant training or a shared customer corpus.
- a second gateway, task identity, artifact identity, or policy system.
- provider status polling.
- CPU or software-rendered acceptance.
- direct training against real physical actuation.
- adoption of Agent Lightning, VERL, GRPO, RLOO, or another trainer or algorithm.
- automatic promotion of a checkpoint because its average reward increased.

## Possible Placement

No component boundary is selected. A future design could examine:

- an opt-in exact-call instrumentation boundary beside the selected model adapter.
- shared typed trajectory identities only where more than one component genuinely owns
  the vocabulary.
- rollout lifecycle projection through the durable Task runtime and agent runtime.
- immutable trajectory, dataset, evaluation, and candidate records in the artifact plane.
- an offline trainer adapter that has no gateway administration or activation authority.
- a governed MCP administration surface and App only if operators need a product-facing
  workflow.

If a hosted MCP server is approved, its design belongs beside its crate and must satisfy
the complete hosted-server contract. This exploratory document does not reserve a server
name, module path, database table, resource URI, schema version, or deployment profile.

## Open Questions

- What event creates and terminates a mission rollout when an objective spans several
  episodes and operator inputs?
- Which exact model runtimes expose trustworthy sampled-token and log-probability data
  without changing deploy-time behavior?
- Can a provider-neutral trajectory represent exact tokenizer-specific evidence without
  pretending that token IDs are portable across models?
- Which raw evidence must be retained to reproduce training, and which can be destroyed
  after a governed derived artifact is verified?
- How should source withdrawal affect datasets, candidates, activated releases, and
  downstream artifacts?
- Which reward components are hard gates, and which may enter a scalar or vector
  optimization signal?
- How are policy denials included as learning evidence without teaching privileged
  workflows to restricted authority classes?
- When is a mission failure attributable to the model, the harness, a domain service,
  infrastructure, or an invalid scenario?
- What evidence proves that a learned strategy has reached its limit before model-weight
  training begins?
- Should a candidate bind to one profile and harness revision, an evaluated compatibility
  family, or a narrower scenario class?
- Can collocated asynchronous training satisfy hardware utilization goals without
  affecting simulator determinism or inference behavior?
- Which actor may approve data capture, start training, inspect raw trajectories, stage a
  candidate, activate it, or invalidate it?
- What independent safety case is required before a post-trained candidate can approach
  any real actuator?

These questions must be resolved in a component design and an approved implementation
plan before model post-training enters Veoveo's normative architecture.
