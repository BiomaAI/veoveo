# Self-Improving Harness: Exploration

> Status: exploratory and non-normative. This document records a direction for
> investigation. It does not approve a component, dependency, storage schema, public
> protocol, runtime behavior, or rollout. The normative architecture remains in
> [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md),
> [`TECH_DESIGN.md`](TECH_DESIGN.md), and the owning component designs.

## Purpose

Veoveo profiles assemble MCP servers into a coherent surface, while gateway policy and
invocation authority determine what an authenticated caller can actually use. An agent
must then learn how to select and compose those capabilities across resources, prompts,
tools, tasks, subscriptions, notifications, and artifacts.

This exploration asks whether Veoveo can improve that behavior from evaluated
interaction evidence without changing model weights or weakening gateway authority. It
also asks whether action-to-observation evidence can teach the harness how the profile
behaves, not merely which action previously received a successful final score.

Model-weight changes belong to the separate
[`HARNESS_MEDIATED_MODEL_POST_TRAINING.md`](HARNESS_MEDIATED_MODEL_POST_TRAINING.md)
exploration. The two directions may share governed trajectory and evaluation evidence,
but this document keeps the model frozen and changes only external strategy or dynamics
records.

The working term is **self-improving harness**. Names such as *adaptive profile*,
*profile strategy*, *profile dynamics*, and the component paths discussed below are
provisional.

## Sources of the Idea

[Microsoft SkillOpt](https://github.com/microsoft/SkillOpt) treats a natural-language
skill as external trainable state for a frozen agent. Its reflective loop turns scored
trajectories into bounded edits and admits a candidate through held-out validation. The
method is described in the
[SkillOpt paper](https://arxiv.org/abs/2605.23904).

SkillOpt is a reference for optimization discipline. This exploration does not propose
adopting its package, local-file state, plugins, MCP wrappers, Markdown format, or product
boundary.

[Agentic World Models](https://cameronrwolfe.substack.com/p/agentic-world-models)
surveys work that treats each action and resulting environment observation as a dense
learning signal. The primary sources considered here are:

- [ECHO: Terminal Agents Learn World Models for Free](https://arxiv.org/abs/2605.24517)
- [Policy and World Modeling Co-Training for Language Agents](https://arxiv.org/abs/2606.02388)
- [Qwen-AgentWorld: Language World Models for General Agents](https://arxiv.org/abs/2606.24597)

These papers concern model-weight training and learned simulators. Veoveo may be able to
apply their supervision and evaluation lessons at the harness level before considering
weight training. The companion post-training exploration uses
[Agent Lightning v1.0](https://arxiv.org/html/2608.17528v1) as its primary reference for
the later model-weight path.

[NVIDIA AVO](https://arxiv.org/abs/2603.24517) demonstrates a seven-day autonomous
optimization carried by one mechanism: a fitness gate over a single lineage, where a
candidate is committed only when correctness passes and a measured improvement clears
the current best. AVO is a reference for gate discipline, not for architecture. Its
agent and its acceptance gate share one trust domain, and nothing in its system
improves except the artifact under optimization. The measured-acceptance shape below
keeps the gate and separates the trust domains.

## Current Veoveo Boundary

A [`GatewayProfile`](../mcp/contract/src/gateway/server_config.rs) declares a maximum
surface. It selects servers and their exposed tools, resources, prompts, completions, and
task support. It also names a policy version, authorization server, required scopes, and
accepted authentication modes.

That declaration is not the caller's effective authority. Gateway discovery and every
subsequent operation are filtered or authorized using the authenticated principal,
scopes, Work Context, data labels, policy, and client capabilities. Object-level access
may remain unknown until a particular request is evaluated.

The current agent kernel records useful evidence. Its
[`RecorderHook`](../agents/kernel/src/recorder.rs) logs tool calls, rendered results,
task transitions, and model usage to the local RRD memory plane. These capped text records
support inspection, but they are not a canonical training contract. Gateway audit is
also not a training corpus. It intentionally excludes prompts, artifact bytes, provider
payloads, tokens, and other sensitive material.

## Working Conceptual Model

The profile definition, authenticated capability set, environment dynamics, and action
strategy answer different questions:

| Concept | Question |
|---|---|
| Profile contract | What capability surface may this profile expose? |
| Effective capability set | What may this authenticated invocation attempt now? |
| Profile dynamics | What observation or state effect should follow an action? |
| Profile strategy | Which available action or workflow should pursue the objective? |

One possible flow is:

```text
profile contract + invocation authority
                |
                v
      effective capability resolver
                |
                +------------------------------+
                v                              v
      strategy compiler                 dynamics context
                |                              |
                +--------------+---------------+
                               v
                         agent episode
                               |
                               v
                    typed transition evidence
                               |
                               v
                  evaluation and candidate updates
```

The gateway remains authoritative throughout this flow. Learned text, strategy clauses,
predictions, and simulations never grant access.

## Auth-Aware Strategy

Optimizing only against a profile's maximum surface would teach privileged workflows to
callers who cannot execute them. A candidate strategy should instead be trained over a
distribution of representative effective capability sets and compiled for the current
invocation.

A provisional strategy release could contain:

```text
ProfileStrategyRelease
  profile_contract_digest
  target model and execution harness
  supported authority classes
  strategy clauses and workflow recipes
  evaluation dataset and evidence identities
  activation state
```

Each clause would carry typed applicability rather than relying on prose alone:

```text
StrategyClause
  objective class
  required MCP capabilities
  preferred operation or workflow
  fallback relationships
  bounded natural-language guidance
  evidence and confidence
```

The compiler would omit a clause whose requirements are absent from the effective
capability set. It could select an applicable fallback when one has been evaluated.
Actual resource and object authorization would still occur at call time.

Training should normally use stable authority classes rather than individual users. A
profile that serves materially different purposes may require multiple strategy variants
or separate profiles. Personal preferences, if explored later, would remain distinct
from authorization.

## Profile Dynamics

Final workflow success is sparse evidence. Each MCP transition also shows how the
environment reacted to an action, including transitions from failed workflows. A
provisional evidence record could capture:

```text
McpTransition
  profile contract and effective capability digests
  Work Context policy revision
  prior state summary
  typed MCP action
  authorization outcome
  normalized observation
  resource, artifact, and task identities produced
  durable state effects
  latency and usage
  final workflow outcome
```

Veoveo should initially model normalized consequences instead of reproducing exact output
tokens. Useful observation classes include schema rejection, authorization denial,
immediate success, durable task creation, notification delivery, artifact production,
state mutation, pagination, partial completion, and infrastructure failure. Runtime IDs,
timestamps, secrets, and user content are poor prediction targets.

A dynamics release might combine validated contract knowledge with empirical transition
evidence. It could help the evaluator find contradictions such as an expected task result
arriving synchronously, a supposedly read-only operation changing state, or a strategy
that expects an artifact where the server returns only a preview.

World-model accuracy would remain an auxiliary signal. A strategy must not receive credit
for selecting predictable no-op operations, repetitive documentation reads, or other
behavior that fails the objective.

## Candidate Learning Loop

One possible offline cycle is:

1. Snapshot a candidate profile contract and its relevant policy definitions.
2. Select representative evaluation authorities and isolated Work Contexts.
3. Discover each effective MCP surface through the real gateway.
4. Run end-to-end workflows against governed fixtures and simulators.
5. Capture typed action, observation, state-effect, usage, and outcome evidence.
6. Identify failures, inefficient paths, and surprising action consequences.
7. Propose bounded typed changes to strategy or dynamics records.
8. Reject references that do not validate against the profile contract.
9. Evaluate the candidate on held-out workflows and authority combinations.
10. Stage an immutable candidate with a human-readable comparison report.

This loop does not imply automatic activation. If activation becomes a product feature,
it should be an explicit, authorized, audited control-plane mutation with rollback to an
earlier immutable release.

## Evaluation Signals

An evaluation would need hard assertions and softer optimization signals. Possible hard
checks include workflow correctness, allowed state transitions, artifact provenance,
scope compliance, task lifecycle behavior, and the absence of forbidden calls. Softer
signals may cover latency, provider cost, tool-call count, token use, information gain,
and the quality of partial results.

Dynamics prediction should help select informative evidence. Hosted models may not expose
action entropy, so the harness could use disagreement among rollouts, prediction error,
novel capability combinations, rare outcomes, evaluator uncertainty, and unexpected
state changes as practical proxies.

Not every observation deserves equal weight. Repetitive error loops, malformed calls,
duplicate retries, empty observations, and unrelated infrastructure outages can overwhelm
the useful signal. Current schema failures and policy denials may still be important
evaluation evidence, but they should be classified separately from valid environment
dynamics.

Held-out evaluation should vary both workflows and authority combinations. A candidate
that improves a privileged path while regressing read-only or agent access should not
become the general profile strategy.

## Measured Acceptance: The Scorer Primitive

Every loop in this document ends at the same requirement: a change is admitted
only when a measurement says the candidate beats the incumbent. The
measurement is always domain specific. The shape around it is not. This
section records a shared shape for that machinery.

Four records define the shape:

- **Scorer.** A registered, versioned definition of one measurement: the
  subject URI kinds it accepts, the owning domain that executes it, the
  procedure reference, units, direction of improvement, required repetitions,
  and the runtime-tuple fields that make two of its scores comparable.
- **Score.** One execution result, stored as an ordinary artifact. Provenance
  names the scorer version, the exact subject URIs, and the runtime tuple.
  A value without uncertainty is not a Score.
- **Baseline.** A governed pointer to the currently accepted candidate and
  its Score, one per lineage. The lineage itself is the artifact provenance
  chain that already exists.
- **Gate.** A typed decision comparing a candidate Score against a Baseline
  Score. A Gate admits only when the improvement clears the scorer's declared
  margin under a matching runtime tuple, and it refuses cross-tuple
  comparisons rather than guessing.

Placement follows the platform's existing pattern: the four records are a
contract profile, tentatively `veoveo.io/score/v1`, that domain servers
implement. Measurement execution never centralizes. Mission efficiency is a
query over recordings and episodes, forecast error belongs to Timeseries
against actuals, solver quality is Optimization's existing independent
verification, and simulator fidelity is a divergence measure over paired
simulated and real recordings.

Two rules keep the loop honest, and both are enforced below the agent:

1. Commit-class actions gate on decisions, not on claims. Releasing an
   improved artifact, publishing a changed tool description, or promoting a
   configuration is admitted by profile policy only when the action
   references an admitting Gate. The proposing agent requests evaluation;
   it never computes its own acceptance.
2. The ruler changes under different authority. Scorer definitions are
   governed records whose mutation requires a scope and Work Context
   membership the proposing agent's profile does not hold. An agent that
   improves tools cannot touch the measurement it is judged by.

The primitive generalizes past profile strategies. Each surface the platform
could improve agentically pairs with a measurement the evidence plane
already carries or can cheaply produce:

| Improvable surface | Candidate scorer | Gate condition |
|---|---|---|
| Tool descriptions and prompts | Task success and action efficiency over a replayed mission set | Efficiency improves beyond margin with no success regression |
| New tools and hosted servers | Contract conformance plus acceptance scenarios plus observed use | Conformance passes and one accepted mission required the capability |
| Analytical memory and queries | Grounded-answer rate and bounded read cost from Reason evidence | Groundedness improves at equal or lower cost |
| Optimization and timeseries configuration | Verified objective values and forecast error against actuals | Objective or error improves on the archived history |
| Simulation runtime | Simulated-versus-real divergence over paired recordings | Divergence shrinks while acceptance scenarios still pass |

The same shape serves three consumers without modification. Candidate-model
admission in
[`HARNESS_MEDIATED_MODEL_POST_TRAINING.md`](HARNESS_MEDIATED_MODEL_POST_TRAINING.md)
is a Gate over model candidates. Mission action efficiency in the style of
interactive-benchmark action-efficiency metrics is a Scorer over episodes
and outcomes. And an installation's operational key ratio, the number its
operators are evaluated on, is a customer-registered Scorer over the same
evidence plane, which turns "cost per outcome is a query" from positioning
into a record with provenance.

A first experiment needs no new server: define two scorers as governed
records, mission action efficiency over an existing UAV acceptance recording
and forecast error over an archived timeseries, execute both through the
owning domains, and store the Scores as artifacts. The registry, baselines,
and policy-bound gating become worth building only after those two scores
prove comparable, repeatable, and cheap.

## Possible Server Shape

If the experiment holds, one hosted server could own the loop's bookkeeping
without owning its trust. A `score` server with the `score://` scheme would
carry canonical identities for scorers, lineages, baselines, gates, and
evaluation runs, and would use the full protocol surface the contract
already prescribes:

| Surface | Responsibility |
|---|---|
| Tools | `register_scorer` under `score:define`, `open_lineage`, task-augmented `request_evaluation`, `submit_measurement`, `evaluate_gate`, and `promote_baseline` under `score:promote` |
| Resources | scorer catalog and templates, lineage histories, baselines, gate decisions, run states, with completions over scorer identities |
| Prompts | the scripted evaluation and self-improvement cycles, from naming the surface and scorer through committing only with an admitting Gate reference |
| Durable tasks | evaluation runs with repetitions and progress, terminating in the Gate or Score URI as `result_uri` |
| Subscriptions | baseline-change notifications, so an improvement agent's manifest-declared subscription wakes it when its baseline moves |
| MCP Apps | a scoreboard over lineage history with uncertainty bands, gate log, and evidence links, plus a scorer-registration app |

Guidance flows through the protocol rather than a scaffold: prompts carry
the procedure, the evaluation task walks the agent stepwise through
`input_required` handoffs, resources make the baseline discoverable instead
of remembered, typed gate refusals teach through diagnostics, and baseline
wakes give agents initiative.

Two scorer procedure kinds keep measurement independent. Query scorers
execute inside the server over the evidence plane on the platform's
sandboxed DuckDB runtime. Domain scorers are measured by the owning server,
and the score server accepts the measurement only when the Score artifact's
provenance shows the owning server's principal produced it under the
declared procedure. The proposing agent cannot hand-author its own score,
and that property is checkable with existing artifact provenance.

The server would be a bookkeeper and a judge, never a measurer and never
the police. Final authority stays in the gateway: the improving agent's
profile holds evaluation access but neither `score:promote` nor the
commit-class scopes on the target surface, and promotion belongs to a
separate principal whose only admitted path references an admitting Gate.
Gateway policy that checks Gate references on arbitrary commit-class
actions across servers would be a later control-plane feature, recorded
here as the deeper version rather than assumed.

## Simulation

A profile simulator could eventually reduce the cost of real rollouts and generate
controlled pagination, delays, cancellations, authorization denials, partial failures,
and notification behavior. Veoveo already owns several protocol state machines that can
be simulated deterministically.

Domain results are harder to simulate honestly. Early experiments should combine exact
protocol fixtures with real isolated MCP servers. A learned simulator would require
calibration against real gateway transitions and explicit confidence limits. It would
never become a source of authorization, durable domain truth, or provider completion.

## Data Governance

Learning evidence would inherit Veoveo's tenant, Work Context, ownership, classification,
label, retention, and artifact rules. A safe design would require:

- tenant-local datasets and releases;
- an explicit training projection separate from operational audit;
- typed summaries that omit secrets and unstable identifiers;
- governed artifacts for raw evidence that must be retained;
- explicit consent before production episodes contribute learning evidence;
- provenance from every learned clause back to evaluation evidence;
- no cross-tenant learning unless a future product and governance decision authorizes a
  deliberately public dataset.

Provider prompts used by an optimizer or judge would be a separate outbound data boundary.
Redaction cannot substitute for an authorization and classification decision.

## Possible Placement

No component boundary is selected. A future design could examine:

- shared typed contracts under `mcp/contract`;
- effective-capability resolution and strategy compilation inside a focused platform
  runtime;
- an offline profile-learning worker using the shared durable task runtime;
- immutable records and activation pointers in `platform/store`;
- evidence and reports in the artifact plane;
- gateway delivery through initialization instructions or another protocol-native
  surface;
- agent episode pinning of strategy, dynamics, profile, policy, and capability versions.

The optimizer should not automatically appear as an ordinary capability inside the
profile it is evaluating. That arrangement would create a recursive self-modification
surface. Administrative resources or tasks, if needed, should remain in a governed
control-plane boundary.

Names such as `platform/profile-runtime` and `platform/profile-learning` are examples for
discussion only. This document does not reserve them.

## Suggested Experiments

The smallest useful experiments would avoid model-weight training and production
self-modification. The companion post-training exploration defines the evidence gates
that would have to open before a model-weight experiment:

1. Define a typed evaluation trajectory for isolated agent episodes.
2. Build a scenario suite spanning one profile and several authority classes.
3. Compare a manually authored strategy with a bounded candidate produced from failed
   trajectories.
4. Add normalized next-observation prediction and use surprise to select new scenarios.
5. Test a deterministic protocol simulator against real gateway traces.

Each experiment should be disposable until it demonstrates reproducible held-out gains
without policy, safety, or data-governance regressions.

## Open Questions

- Is the durable artifact a profile strategy, a strategy per authority class, or a
  composable set of clauses shared by several profiles?
- Which facts belong in the effective capability context, and which would disclose too
  much policy or identity information?
- What canonical digest detects a meaningful profile contract change without invalidating
  a strategy for unrelated control-plane edits?
- How should an MCP client receive and identify the compiled strategy?
- Should a strategy be tied to one model, a model family, or an execution harness?
- Which observations can be normalized without losing the causal information required
  for learning?
- Can domain state effects be derived from existing outbox and recording evidence, or do
  evaluation servers need explicit effect reports?
- What mixture of deterministic checks and model judges provides an acceptable activation
  gate?
- When is a typed empirical dynamics model sufficient, and what evidence would justify a
  learned simulator or model-weight training?
- Which control-plane actor may start evaluation, inspect evidence, stage a candidate, and
  activate or roll it back?
- How should strategy and dynamics versions interact with a gateway control-plane revision
  that is staged but not yet active?

These questions must be resolved in a component design before implementation enters the
normative architecture.
