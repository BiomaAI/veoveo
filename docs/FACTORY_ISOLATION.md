# Veoveo Software Factory: OpenShell Isolation Plan

> Status: exploratory and non-normative. This document describes the proposed
> product, user experience, security boundary, typed contracts, implementation
> shape, and adoption path for a continuously operating Veoveo software factory.
> It does not approve an OpenShell dependency, a factory service, an unattended
> merge or deployment policy, or a change to the current engineering boundary.
> The normative architecture remains in
> [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md),
> [`TECH_DESIGN.md`](TECH_DESIGN.md),
> [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md), and the owning component
> designs.

## Purpose

Veoveo needs a software factory that can accept a reviewed product specification,
author a candidate change with any admitted coding-agent harness, verify that change
independently, build immutable artifacts, and promote those artifacts through the
installation owner's ordinary GitOps process. The factory may operate continuously.
No coding harness receives the authority held by the factory control plane, source
publisher, artifact signer, or deployment worker.

The product is not a long-lived coding agent with broad engineering credentials. It is
a continuously available control plane that creates short-lived, bounded jobs. Each
author job starts from an identified source revision in a disposable OpenShell sandbox
and ends with candidate data. A fresh verifier decides whether that data satisfies the
reviewed specification. Typed brokers perform every effect beyond the sandbox.

This plan resolves the container-socket question from the earlier exploration.
Compilation and tests may execute inside disposable workers, while OCI publication,
signing, source promotion, and deployment are broker operations outside the author
sandbox. Docker or Podman sockets, kubeconfig, registry credentials, signing keys, and
Git push credentials never enter an author job.

## Standards And Protocols

| Standard, protocol, or repository contract | Factory boundary |
|---|---|
| [NVIDIA OpenShell v0.0.111](https://github.com/NVIDIA/OpenShell/releases/tag/v0.0.111) | Exact trial baseline published August 21, 2026. OpenShell remains an alpha product despite the stable tag; this document records a candidate, not an approved dependency. Adoption pins every binary and image by digest. |
| [OpenShell sandbox policy](https://docs.nvidia.com/openshell/latest/sandboxes/policies.html) | Landlock filesystem confinement, seccomp process restriction, network and inference policy, exact inspected endpoint rules, and MCP-aware request matching. The factory uses `landlock.compatibility: hard_requirement`, explicit `enforcement: enforce`, and fail-closed validation. |
| [OpenShell compute drivers](https://docs.nvidia.com/openshell/latest/reference/sandbox-compute-drivers.html) | Docker, rootless Podman, MicroVM, and Kubernetes are available driver families. The first trial uses rootless Podman on a disposable Linux VM. Higher-risk production work moves to the MicroVM driver. |
| [OpenShell workspace access](https://docs.nvidia.com/openshell/latest/sandboxes/manage-workspaces.html) | The runner receives Workspace User authority. A separate administrator owns policies, providers, profiles, and membership. Local gateways without configured OIDC roles do not satisfy this plan. |
| [OpenShell OCSF export](https://docs.nvidia.com/openshell/latest/observability/ocsf-json-export.html) and OCSF 1.8.0 | Complete JSONL security events leave the sandbox through a trusted collector and become factory evidence. The agent does not author or select the exported event set. |
| [Landlock](https://docs.kernel.org/userspace-api/landlock.html) and Linux seccomp | Kernel-enforced filesystem and syscall restrictions. Missing Landlock support, an inaccessible declared path, or an unproved effective policy blocks the run. |
| [Model Context Protocol](../mcp/contract/DESIGN.md) | Veoveo hosted-server contract revision 3 over MCP `2026-07-28`. The factory profile uses `server/discover` rather than `initialize`, because the hosted Veoveo profile deliberately excludes Initialize. |
| JSON-RPC 2.0 and JSON Schema 2020-12 | OpenShell inspects admitted MCP and JSON-RPC methods. Planned factory records use closed, versioned schemas at every controlled boundary. |
| Git object identity and SHA-256 | Source bases, candidate trees, patches, policies, images, evidence, evaluations, and promotion inputs carry immutable identities. |
| OCI Image and Distribution Specifications | Factory and harness images are prebuilt and selected by digest. Veoveo runtime images retain their existing runnable-manifest and publication-index identities. |
| SPDX SBOM and SLSA provenance | Qualified Veoveo publications retain the attestations required by [`IMAGE_BUILDS.md`](IMAGE_BUILDS.md). A candidate or staging build is not release evidence. |
| [`veoveo.io/local-test-report/v2`](CONTINUOUS_INTEGRATION.md) | The committed local report remains an engineering status note. The factory never treats it as independent verification, release provenance, or a security boundary. |
| [`veoveo.io/deployment-lock/v6`](ENTERPRISE_DEPLOYMENT.md) | Qualified release closure for installation promotion. Development image locks remain ineligible for production release. |
| Helm, Kubernetes, and Flux 2.9.4 | Installation-owned desired state and reconciliation. The factory does not patch live Kubernetes workloads or become a second reconciliation owner. |
| NVIDIA CUDA, Vulkan, RTX, NVENC, WebGPU, WebGL, and Chrome DevTools Protocol | Hardware-GPU execution and headed-browser proof remain mandatory for visual, simulation, perception, rendering, encode, and visual-verification acceptance. |

The OpenShell tag, documentation, and open issues cited here were current on August 24,
2026. The adoption preflight must verify the latest stable release again and update the
pin, tests, and this document in the same change.

## Product Definition

The Veoveo Software Factory turns a reviewed product intent into a traceable candidate,
verified source revision, immutable release artifact, and optional GitOps promotion.
The developer interacts with one Factory Run. The factory may use several short-lived
jobs and more than one coding harness to complete that run.

The factory promises four properties to a developer:

- The reviewed product specification remains the durable definition of success.
- Progress and failures appear as evidence against that specification.
- No coding harness can publish, sign, deploy, or widen its own authority.
- Every promoted deployment can be traced back through artifact, commit, candidate,
  verifier, policy, source base, and specification identities.

The factory is harness-neutral. Codex, Claude Code, OpenCode, Aider, a repository-owned
agent, or another admitted harness may fill an author slot. Harness selection changes
the implementation engine, not the containment boundary, evidence contract, or
promotion authority.

### Product Roles

| Role | Product responsibility | Authority boundary |
|---|---|---|
| Proposer | Drafts a product specification, supplies context, and requests a delivery target. | May request work. A requested target does not grant promotion authority. |
| Product reviewer | Confirms user outcome, acceptance criteria, non-goals, and material assumptions. | Approves one immutable specification revision. |
| Component owner | Reviews contract, migration, security, or ownership effects for governed code surfaces. | May approve component-scoped changes according to repository policy. |
| Release owner | Selects a verified candidate for environment promotion. | May authorize the Git change for an allowed environment. |
| Factory administrator | Pins OpenShell, images, harnesses, providers, policies, and admission rules. | Cannot act through an author sandbox and remains separate from ordinary runners. |
| Auditor | Reads specifications, evidence, decisions, promotions, denials, and lineage. | Has no authoring or promotion mutation authority. |
| Coding harness | Produces a candidate in one sandbox. | Has no durable identity outside that sandbox and no promotion authority. |

One person may hold several human roles in a small installation, but the service
identities remain separate. A coding harness never inherits the authority of the human
who proposed the work.

### Product Terms

| Term | Meaning |
|---|---|
| Product Specification | A developer-authored statement of the outcome, scope, constraints, evidence, delivery target, and stopping condition. |
| Specification Revision | One immutable, reviewed version of a Product Specification. Amending a run creates another revision. |
| Factory Plan | The preflight result that resolves source base, affected components, risks, jobs, checks, budgets, and promotion gates. |
| Factory Run | The durable lifecycle that carries one accepted specification revision from queue through a terminal state. |
| Job | One bounded execution by an author, verifier, builder, or acceptance worker. |
| Harness | A coding-agent command and adapter admitted into an immutable factory image. |
| Candidate | A collected source-tree change and author claims. It is not a commit, verified result, or release. |
| Verification Result | Independent evidence produced from the recorded base and exact candidate patch in a fresh worker. |
| Promotion | A typed broker action that creates a commit or PR, publishes an artifact, or changes GitOps desired state. |
| Trust root | Policy, evaluator, admission, signing, promotion, identity, or deployment material that a candidate cannot change and approve in the same lineage. |

### Goals

- Keep the software factory available around the clock without preserving agent
  sandboxes around the clock.
- Let developers describe product outcomes without operating an agent harness,
  container runtime, or Kubernetes cluster.
- Admit competing coding harnesses behind one contract and compare them on verified
  outcomes.
- Run untrusted source, dependencies, build scripts, tests, and harness binaries in
  disposable environments without engineering-host credentials.
- Preserve Veoveo's current OCI, deployment-lock, GitOps, GPU, smoke, and test-report
  contracts.
- Make a denied request, failed check, exhausted budget, or missing capability an
  ordinary visible state rather than a reason to widen policy.
- Support skill-first improvement without letting a candidate skill change the policy
  or evaluator that judges it.

### Non-Goals

- Replacing the Veoveo MCP gateway. OpenShell cannot authorize tool arguments,
  resources, leases, budgets, or Work Context effects.
- Giving coding harnesses Docker, Podman, Kubernetes, Git, registry, signing, or GitOps
  credentials.
- Treating model alignment, prompts, harness approvals, or a harness-native sandbox as
  the factory security boundary.
- Making OpenShell part of the fielded Veoveo installation.
- Making the product repository the owner of a customer's cluster or GitOps
  controller.
- Claiming that passing automated tests proves product correctness.
- Automatically merging or deploying production changes during the first adoption
  phases.
- Maintaining compatibility aliases for a future factory contract after its hard cut.

## Developer Experience

The experience resembles proposing a product change and watching independently
collected evidence accumulate. OpenShell, provider routing, harness commands, and
sandbox lifecycle remain implementation details. The developer never approves an
individual shell command.

The Factory Console is separate from the installation Console described in
[`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md). The installation Console
shows the live customer installation. The Factory Console is engineering
infrastructure with its own origin, identity, backend, and audit boundary. It may share
the Veoveo visual system, but it does not share browser sessions, deployment
credentials, or installation administration routes.

`cargo xtask factory ...` is the repository-native command surface for automation and
terminal use. The Factory Console and xtask are clients of the same typed factory API.
Neither client executes the workflow locally.

### Experience Principles

- Ask for product decisions, not execution approvals.
- Present one durable run even when the factory uses several disposable jobs.
- Put acceptance criteria, current risk, and promotion readiness before raw logs.
- Preserve exact evidence behind every concise status.
- Make pause, cancel, amendment, rejection, and rollback ordinary actions.
- Keep policy administration out of the developer run interface.
- Treat a model-generated plan as a proposal until a human reviews the specification
  revision that contains it.

### Entry Points

A developer starts from one of three inputs:

- A new Factory Console proposal.
- A checked-in Product Specification submitted through `cargo xtask factory propose`.
- A trusted issue or incident imported by the factory intake broker.

An issue body, CI log, provider response, screenshot, document, or web page is untrusted
content. The intake broker records its source and includes it as reference material. It
does not convert the author's credentials, issue labels, embedded instructions, or links
into sandbox authority.

### Drafting A Product Specification

The developer begins with a short statement of intent. A drafting assistant may help
turn that statement into the controlled specification, but it cannot submit the result
or approve its own assumptions.

A specification contains these sections:

| Section | Developer question |
|---|---|
| Outcome | What becomes possible for the user? |
| Users and context | Who uses it, and which product or installation context matters? |
| Acceptance | What observable evidence demonstrates the outcome? |
| Scope | Which repositories or components may change? |
| Non-goals | What must remain unchanged? |
| Constraints | Which contracts, performance limits, security rules, and operational requirements apply? |
| Delivery | Should the run stop at a candidate, draft PR, integration revision, staging deployment, or production request? |
| Rollout | Which environment order, soak, rollback trigger, and recovery target apply? |
| Budget | How much elapsed time, model usage, compute, storage, and retry capacity may the factory consume? |
| References | Which issues, designs, traces, images, datasets, or prior decisions should the factory read? |

Acceptance criteria are typed. The initial profile supports:

- source checks with an exact command selected from a trusted check catalog;
- contract or conformance assertions selected by identifier;
- artifact assertions over digest, SBOM, and provenance;
- deployment assertions over an exact Git and deployment-lock revision;
- GPU runtime or headed-browser assertions with hardware proof;
- performance assertions with a named metric, comparison, and bound;
- product-review assertions that require a named human decision.

A free-form sentence may explain a criterion, but it cannot replace the typed evidence
selector when the factory is expected to decide automatically.

### Preflight

Preflight is read-only. It resolves the specification against the selected repository
and source base, then returns a proposed Factory Plan. It identifies:

- the complete base commit;
- likely component and contract owners;
- affected package, image, schema, documentation, Helm, and GitOps surfaces;
- required design-document and code-map updates;
- existing tests and additional independent checks;
- GPU and headed-browser requirements;
- dependency, migration, security, data, and rollout risk;
- protected surfaces that prevent unattended promotion;
- planned job count, budget, and stopping condition;
- requested environment and the human authority required there;
- assumptions that materially affect the product outcome.

The developer reviews a concise interpretation before submitting work. A useful
preflight statement is concrete:

> This change adds recorded-camera playback to the Console. It affects the media MCP
> contract, gateway authorization, Console playback, installation values, and
> hardware-backed browser acceptance. It does not change recording retention or public
> sharing.

An unresolved product choice moves the draft to `needs_decision`. Missing build access,
a denied endpoint, or an unavailable GPU is a factory capability result. It does not
become a developer prompt to expand OpenShell policy.

### Submission

Submission freezes four identities:

- the Product Specification Revision;
- the complete source base SHA;
- the reviewed Factory Plan;
- the policy set and factory image family eligible for the run.

The developer sees a durable record:

```text
VEO-1842  Recorded camera playback
Spec       Revision 3
Base       91f04a2c6e...
Target     Staging, then production request
Risk       Elevated: MCP contract and deployment values
State      Queued
Budget     6 author jobs / 3 verifier cycles / 8 hours
```

The requested target is an upper bound on the journey, not a grant of authority. A
proposer without production rights may still request production. The run stops at
`ready_for_production` until an authorized release owner decides.

### Build Journey

The run timeline presents checkpoints rather than process chatter:

```text
✓ Specification accepted
✓ Repository preflight
● Authoring candidate 2
○ Independent verification
○ Product review
○ Artifact qualification
○ Staging acceptance
○ Production promotion
```

During authoring, the developer sees:

- the current candidate number and harness identity;
- the acceptance criteria being addressed;
- changed components detected by the trusted collector;
- completed and pending checks;
- elapsed, compute, model, and retry budget;
- policy denials and other boundary events;
- the next automatic action;
- any decision that requires human input.

Raw transcripts and OpenShell events remain available through a restricted evidence
view. They are not the default status surface. Transcripts may contain proprietary
source and untrusted text, so their retention and access policy is explicit.

### Verification And Repair

The first verifier always starts from the recorded base in a fresh sandbox. It applies
the exact candidate patch, runs the trusted check plan, and compares the result to the
specification. It cannot reuse the author's writable workspace, mutable cache, harness
history, or credentials.

A failed criterion may create a repair task when the remaining budget and risk policy
allow it. The repair task receives a bounded failure summary and starts another fresh
author sandbox. It never resumes the failed author's environment.

The UI collapses routine attempts:

> Candidate 2 failed media contract conformance. Candidate 3 started from the recorded
> base with the verifier's failure evidence attached.

Repeated failure, a changed risk class, a stale base, or an exhausted budget moves the
run to `needs_decision` or a terminal state. The factory does not loop indefinitely.

### Candidate Review

A verified candidate produces one review package bound to the candidate digest:

- product-level change summary;
- each acceptance criterion and its evidence;
- source diff and proposed commit structure;
- contract, schema, dependency, migration, and deployment effects;
- test, conformance, security, and performance results;
- screenshots, video, or visual metrics when required;
- hardware adapter, driver, rendering, and encode/decode identity for GPU evidence;
- known limitations and follow-up proposals;
- exact source, harness, OpenShell, policy, factory image, verifier, and catalog
  identities;
- reasons that prevent automatic promotion.

The author summary is labeled as an untrusted claim. The factory derives changed paths,
patch identity, check results, and policy evidence independently.

### Promotion And Deployment

The product review action selects the next allowed stage:

| Stage | Developer-visible result |
|---|---|
| Candidate | Downloadable patch and evidence package; no source mutation. |
| Draft PR | Broker-created branch and draft PR against the expected base. |
| Integration | Verified commit on a resettable integration branch after merge-queue revalidation. |
| Staging | Qualified or explicitly development-only artifact selected by an installation-owned GitOps commit. |
| Production request | Complete decision package awaiting release-owner approval. |
| Production | Exact approved digest reconciled by the installation controller and observed through acceptance. |

The production decision package shows the exact evidence that matters:

```text
Ready for production

Acceptance        12/12 passed
Security          No blocking findings
Contract changes  Component owner approved
GPU acceptance    Passed on the recorded NVIDIA adapter
Staging soak      Complete and healthy
Release artifact  registry.example/veoveo@sha256:...
Rollback target   registry.example/veoveo@sha256:...
```

Approval causes the promotion broker to create the reviewed GitOps desired-state
change. A coding harness never receives kubeconfig and never invokes `kubectl`, Helm,
Flux, a registry, or a signer.

### Observation And Closure

Deployment enters a bounded observation window defined by the specification. The run
shows controller convergence, workload readiness, domain acceptance, product-specific
signals, and any pre-authorized rollback. Kubernetes and Flux observation use their
typed watch-based harnesses. Provider jobs remain webhook-only; the factory does not
poll a provider for missing completion.

Terminal states are:

- `completed`;
- `completed_with_followups`;
- `rejected`;
- `cancelled`;
- `budget_exhausted`;
- `verification_failed`;
- `promotion_failed`;
- `rolled_back`;
- `operational_failure`.

The final record preserves a navigable lineage from specification to deployment. A
follow-up is a new proposal. It does not silently keep the completed run alive.

### Steering And Intervention

The developer has five meaningful controls:

| Control | Effect |
|---|---|
| Amend specification | Creates a new immutable revision, reruns preflight, and invalidates evidence affected by the change. |
| Answer decision | Resolves one recorded product question without expanding factory authority. |
| Pause | Stops new jobs and requests termination of the active job. Resumption creates a fresh sandbox. |
| Cancel | Terminates active work, prevents promotion, and retains evidence. |
| Extend budget | Adds a reviewed bounded allowance; it does not remove any resource or policy limit. |

Policy edits, provider changes, harness admission, and signing or deployment grants do
not appear on the run screen. They belong to the factory administrator's separate
workflow.

### Notifications

The factory notifies developers only at decision boundaries:

- specification review requested;
- material ambiguity found;
- risk class increased;
- budget nearly exhausted;
- candidate ready for product review;
- component or release approval required;
- staging ready or failed;
- production ready, completed, or rolled back.

Per-command progress remains on the run page. This avoids turning continuous operation
into notification noise.

## Security Model

The security design assumes that the model, harness, repository, task content,
dependencies, compiler plugins, build scripts, tests, MCP results, and generated output
may be malicious. A safe factory must still prevent them from acquiring promotion or
installation authority.

### Security Objective

A factory containment breach occurs when untrusted job content causes a read, write,
disclosure, execution, network request, source mutation, artifact publication,
signature, policy change, or deployment beyond the reviewed job envelope. A denied
attempt is evidence of enforcement, not a breach.

Correctness failure is separate. A candidate may pass available checks and still be
wrong. Product review, independent evaluation, staged rollout, observation, and
rollback address that risk.

### Invariants

1. A coding harness receives no credential that can push source, publish an artifact,
   sign an object, change policy, or access a target cluster.
2. A candidate cannot change the evaluator, policy, factory image, admission rule, or
   promotion threshold used for its own decision.
3. Every job starts from immutable inputs and ends before another job consumes its
   output.
4. Every cross-boundary effect uses a typed broker that validates exact identities and
   rejects ambient authority.
5. Every promoted object retains source, specification, candidate, verification,
   policy, and build lineage.
6. Missing policy, evidence, identity, webhook, GPU proof, or expected base state fails
   closed.
7. The runner can stop cognition without depending on harness cooperation.
8. The factory can remain available while one harness, provider, repository, or
   environment is quarantined.

### Trusted And Untrusted Components

| Component | Factory treatment |
|---|---|
| Product Specification draft and references | Untrusted until one revision is reviewed. |
| Coding model and harness | Untrusted. May execute arbitrary code and misreport results. |
| Candidate repository tree | Untrusted, including tests and build configuration. |
| OpenShell policy and gateway | Trusted containment layer for the selected runtime tuple. Compromise is bounded further by the dedicated disposable host. |
| Factory supervisor and durable ledger | Trusted control plane. Never built or loaded from candidate source during the candidate lineage. |
| Independent verifier image and held-out evaluators | Trusted decision inputs. Candidate code still executes as untrusted workload inside them. |
| Source, artifact, signing, and GitOps brokers | Trusted, separate, typed effect boundaries. |
| Target installation | Separately trusted according to [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md) and enterprise ownership. |

OpenShell itself is alpha and cannot be the only protection for an engineering host.
The first trial runs on a dedicated disposable Linux VM with no route to an engineer's
primary machine or privileged enterprise networks. A compromised OpenShell gateway,
container runtime, or shared kernel is therefore contained by the VM and its external
firewall. MicroVM jobs add a stronger guest boundary for higher-risk work.

### Two Gateway Boundaries

| Boundary | Responsibility |
|---|---|
| OpenShell gateway | Sandbox lifecycle, filesystem and process confinement, endpoint and method-level egress, credential isolation, inference routing, workspace RBAC, and OCSF events. |
| Veoveo MCP gateway | Actor identity, hosted profile, Work Context, action and resource authorization, tool arguments, data labels, budgets, leases, artifact authority, and audit. |

OpenShell can inspect MCP methods and `tools/call` tool names. It cannot currently
authorize arbitrary tool arguments or inspect server responses. An OpenShell rule is an
outer capability filter. The Veoveo gateway remains the authority for every admitted
call.

### Protected Changes

A candidate may propose a protected change, but it cannot receive unattended promotion.
Protected changes include:

- OpenShell policies, images, providers, gateway configuration, and factory deployment;
- factory supervisor, verifier, check catalog, promotion broker, schemas, and admission
  code;
- `AGENTS.md`, harness settings, skills, plugins, prompts, and model-routing policy;
- held-out evaluations, conformance rules, test-report implementation, and smoke
  orchestration;
- CI workflows, repository permissions, branch protection, and source-broker code;
- dependencies, lockfiles, build scripts, proc macros, Dockerfiles, and package-source
  configuration;
- authentication, authorization, internal trust, signing, secret, and audit code;
- Helm, Kubernetes RBAC, NetworkPolicy, deployment locks, GitOps, and rollback logic;
- database migrations and destructive data operations;
- MCP contract, gateway policy, and cross-component protocol changes;
- GPU acceptance, browser preflight, simulation, rendering, encode, perception, and
  visual-evidence code.

This classification is semantic rather than path-only. A source file can create a
build-time effect outside its apparent component. The preflight combines path rules,
Cargo and image graphs, generated inputs, ownership metadata, and content inspection.

## Reference Architecture

```text
Developer or trusted task source
              |
              v
Factory Console / cargo xtask factory
              |
              v
+-------------------------------+
| Trusted factory control plane |
| - specification revisions     |
| - planning and risk           |
| - queue, leases, budgets       |
| - policy and harness selector |
+---------------+---------------+
                | AuthorJob
                v
+-------------------------------+
| OpenShell author sandbox      |
| - disposable source copy      |
| - one admitted harness        |
| - no push/deploy identity     |
| - restricted MCP and egress   |
+---------------+---------------+
                | candidate data only
                v
+-------------------------------+
| Trusted candidate collector   |
| - computes tree and patch     |
| - hashes logs and OCSF events |
| - destroys author sandbox     |
+---------------+---------------+
                | CandidateBundle
                v
+-------------------------------+
| Fresh verifier sandbox        |
| - recorded base + exact patch |
| - independent check catalog   |
| - no author cache or identity |
+---------------+---------------+
                | signed result
                v
+-------------------------------+
| Typed promotion brokers       |
| - source commit and PR        |
| - isolated OCI builder        |
| - attestation and signing     |
| - GitOps desired-state change |
+---------------+---------------+
                | exact digests
                v
+-------------------------------+
| Deployment acceptance worker |
| - controlled target identity  |
| - GPU and browser proof       |
| - observation and rollback    |
+-------------------------------+
```

### Component Responsibilities

| Component | Inputs | Outputs | Explicit exclusions |
|---|---|---|---|
| Intake broker | Authenticated proposal and referenced sources | Draft Product Specification | No sandbox or source mutation |
| Planner | Reviewed draft and repository snapshot | Factory Plan and risk decision | No promotion decision |
| Supervisor | Accepted plan, policy catalog, harness catalog | Bounded jobs and durable run state | No candidate code execution in its own process |
| OpenShell runner | Immutable job definition | Sandbox lifecycle and raw execution status | No policy administration |
| Harness adapter | Job and workspace | Harness process and optional progress projection | No authoritative diff or success decision |
| Candidate collector | Base manifest and terminated workspace | Canonical patch, tree identity, and evidence hashes | No trust in sandbox Git metadata or author report |
| Verifier | Base, exact patch, trusted check plan | Signed Verification Result | No source push or deployment |
| Source broker | Verified promotion request | Commit, branch, or PR | No arbitrary Git command from a harness |
| Build service | Verified committed source and build plan | Quarantined OCI outputs and build evidence | No signing or deployment key |
| Signer and registry publisher | Qualified digest and attestations | Signed published artifact | No source compilation |
| GitOps broker | Approved release and expected installation base | Desired-state commit or PR | No direct cluster mutation |
| Acceptance worker | Approved deployment revision and scoped target identity | Deployment and product evidence | No source authoring or policy change |

The publisher is not another general coding shell. Each broker accepts a closed request,
validates exact inputs, performs one owned action, and returns a typed result.

## OpenShell Execution Plane

### Initial Runtime Choice

The first trial uses:

- a dedicated disposable Linux VM;
- rootless Podman as the OpenShell compute driver;
- a prebuilt factory image pinned by digest;
- no host bind mounts;
- no container-control socket inside the sandbox;
- an external default-deny firewall around the VM;
- a separately administered OpenShell workspace;
- one sandbox per job, created with automatic deletion.

Rootless Podman reduces operational lift for the trial. It does not eliminate shared
kernel risk, which is why the VM boundary remains required. Higher-risk jobs, including
self-modifying factory work and adversarial dependency evaluation, move to the MicroVM
driver once the author and verifier contracts are stable.

Kubernetes adoption is deferred. The current combined topology grants the agent
container `SYS_ADMIN`, `NET_ADMIN`, `SYS_PTRACE`, `SYSLOG`, and related capabilities so
the full supervisor can operate. Sidecar topology removes those capabilities from the
agent container and has been validated with Kata Containers, but it relaxes parts of
the combined enforcement path. gVisor is currently unsupported for that topology.
Those tradeoffs require a separate recorded decision before Kubernetes evidence counts.

### Immutable Factory Images

Every factory image records:

- parent image digest;
- OpenShell component versions and digests;
- harness binary and plugin versions;
- Rust, Python, Node, GPU, browser, and other toolchain identities;
- installed package set and SBOM;
- default user, working directory, and filesystem layout;
- supported job roles;
- build provenance and signature.

The agent cannot self-update its harness, install a persistent plugin, or modify the
image. Toolchains are baked into the image. Additional dependencies arrive only through
exact approved mirrors and remain inside the disposable job.

OpenShell still has an open issue for
[gateway-owned Dockerfile builds](https://github.com/NVIDIA/OpenShell/issues/2779).
The trial therefore prebuilds and publishes every factory image. Runtime image builds
from an author-controlled Dockerfile are not admitted.

### Filesystem Contract

The proposed sandbox layout is:

```text
/factory/task/       read-only task and specification inputs
/factory/reference/  read-only approved reference material
/workspace/          writable disposable source tree
/factory/output/     writable untrusted author notes and requested artifacts
/tmp/                bounded per-job scratch
```

No real engineering worktree, home directory, SSH directory, container configuration,
cloud configuration, kubeconfig, secret store, Git credential store, browser profile,
or host package cache is mounted.

The source broker materializes the selected Git tree into the job. A local disposable
Git database may be present for harness compatibility, but its remotes and credentials
are absent. The candidate collector does not trust that database. After the author
process stops, it compares the workspace against a trusted base manifest and computes
file contents, modes, symlinks, deletions, and patch identity outside the sandbox.

Shared Cargo, npm, uv, compiler, and model caches are read-only and content-addressed.
Any writable cache is job-local and destroyed with the sandbox. Writable host caches
are prohibited because candidate build code can poison later jobs.

The policy uses the exact literal:

```yaml
landlock:
  compatibility: hard_requirement
```

OpenShell issue
[#2356](https://github.com/NVIDIA/OpenShell/issues/2356) reports that invalid
compatibility strings can silently fall back to `best_effort`. The factory preflight
parses the policy against its own closed enum, checks the exact literal, inspects the
effective policy, and requires startup evidence that Landlock applied. A failure blocks
the job.

### Process And Resource Contract

The sandbox runs as a dedicated non-root identity with no privilege escalation. The
supervisor enforces:

- wall-clock deadline;
- CPU and memory limits;
- process and file-descriptor limits;
- writable-byte and inode quotas;
- maximum output and transcript size;
- network byte and request budgets;
- model token and cost budgets;
- child-process termination and bounded cleanup time.

The harness process is the canonical main process. Cancellation sends a bounded graceful
termination, then forcibly ends the sandbox. A harness that daemonizes, ignores
termination, or leaves descendant processes fails admission.

### Network Contract

Network policy denies by default. The author profile admits only endpoints required by
the selected job:

- OpenShell-managed inference routing;
- exact read-only dependency mirrors;
- the dedicated Veoveo factory MCP gateway when the task needs it;
- a narrow read-only research proxy in a separately classified research job.

The author cannot reach Git hosting, OCI registries, customer installations, the
Kubernetes API, container daemons, cloud metadata, arbitrary public internet hosts, or
private enterprise ranges. A task that needs current dependency research uses a
separate read-only researcher job. That worker verifies authoritative upstream releases
and returns a signed fact record to the author; it carries no source or publication
credential.

Every inspected REST, WebSocket, MCP, or JSON-RPC endpoint explicitly sets
`enforcement: enforce`. OpenShell's friendly default is audit behavior for inspected
traffic, which is not acceptable factory enforcement. `tls: skip`, uninspected
credentialed traffic, broad binary globs, and unrestricted TCP endpoints are prohibited.

The OpenShell gateway uses `policy_validation_failure_mode = "fail_closed"`. A policy
validation failure quarantines network access and invalidates the job. The factory does
not retain the last valid policy for availability.

### MCP Policy

The outer OpenShell profile keeps:

```yaml
mcp:
  allow_all_known_mcp_methods: false
```

It admits only exact methods required by the role, including a subset of:

- `server/discover`;
- `tools/list`;
- selected `tools/call` tool names;
- `resources/list` when the workflow needs catalog discovery;
- `resources/read` for selected factory resources;
- future `skills/list` and `skills/get` extension methods after their Veoveo contract
  exists.

It does not add OpenShell's example `initialize` or `notifications/initialized`, because
the hosted Veoveo profile excludes Initialize. Unknown methods, unlisted tools, wildcard
tool grants, and broad `tools/call` rules are denied.

OpenShell does not currently match arbitrary MCP tool arguments or server responses.
The Veoveo gateway evaluates the authenticated actor, profile, Work Context, action,
target, arguments, resource URI, budget, and lease after OpenShell admits the method and
tool name. Tool results return as untrusted content. No response can widen either
gateway.

### Identity And Credentials

The external runner authenticates to OpenShell as a Workspace User. It may create and
use sandboxes in the factory workspace, but it cannot change policies, providers,
profiles, membership, or global configuration. A separate Platform or Workspace Admin
preloads reviewed resources.

The sandbox contains no OpenShell administrative credential or CLI configuration.
Inference uses OpenShell routing, which keeps the provider API key at the gateway. A
provider host is never separately added to ordinary network policy. Package mirrors and
read-only research endpoints use gateway-managed credentials only when the exact
endpoint can be inspected and bound.

Git push tokens, registry credentials, signing keys, kubeconfig, service-account tokens,
GitOps deploy keys, cloud credentials, and customer secrets are absent. Short-lived
credentials do not make an author operation safe when the credential still grants an
out-of-scope effect.

### Policy Immutability During A Run

Filesystem and process policy lock at sandbox creation. OpenShell network policy can be
hot-reloaded, but unattended factory runs do not use Policy Advisor, interactive
approval, or agent-authored policy proposals. The supervisor records the starting
policy digest and generation. Any widening or unexpected generation change invalidates
the job. An emergency administrative change may tighten or quarantine a sandbox, after
which the job ends and is not promoted.

The runner cannot select an arbitrary policy supplied by the specification or harness.
The trusted plan maps one admitted role and risk class to one reviewed policy digest.

### Telemetry And Security Evidence

All OpenShell binaries used by the factory are built with telemetry support compiled
out through `--no-default-features`. The gateway also sets
`OPENSHELL_TELEMETRY_ENABLED=false` as defense in depth. The trial proves absence of
OpenShell telemetry traffic rather than relying on configuration inspection alone.

OCSF 1.8.0 JSON events are exported through a trusted collector outside the sandbox.
The collector closes and hashes the event stream after termination. The Candidate
Bundle records that digest and the durable ledger retains the immutable export. An
author-created log file cannot substitute for the collector's record.

The evidence includes denied operations. It also records missing events, collector
failure, truncation, clock discontinuity, and policy-generation change as verification
failures.

## Harness-Neutral Authoring

### Harness Descriptor

Each admitted harness has an immutable descriptor:

```text
HarnessDescriptor {
  id
  version
  image_digest
  executable
  argument_template
  working_directory
  model_route_class
  required_environment_names
  required_filesystem_features
  required_network_capabilities
  progress_adapter
  termination_contract
  maximum_supported_job_schema
}
```

The descriptor may translate a Product Specification into a harness-native prompt or
configuration. It cannot change job scope, add endpoints, inject credentials, or decide
success. Optional progress parsing improves the UI but is never evidence.

### Harness Admission

A harness version passes admission only when it proves:

- non-interactive startup and completion;
- operation without host bind mounts or runtime sockets;
- compatibility with OpenShell inference routing;
- correct behavior when network and MCP operations are denied;
- termination when the supervisor cancels the job;
- no required self-update or mutable global plugin directory;
- bounded logs and output;
- no dependency on a persistent writable home or cache;
- correct source editing inside `/workspace` only;
- clean sandbox destruction after success, failure, timeout, and forced termination.

Admission also runs adversarial probes for policy change, credential discovery, host
filesystem access, socket access, direct provider access, process escape, and output
spoofing. A harness may provide its own sandbox and approval system as defense in depth.
Factory acceptance assumes both can be bypassed.

### Harness Selection

The scheduler selects a harness from the admitted catalog according to language,
component, task class, required context, historical verified success, latency, cost,
and available provider capacity. The selection and reason appear in the run evidence.

A developer may request a harness or a comparative run when policy permits. A request
does not admit an unregistered version. Comparative runs receive independent sandboxes
and produce separate candidates against the same specification and base.

## Independent Verification

The verifier starts from the recorded base and exact collected patch. It does not trust
the author's repository metadata, claimed changed paths, test output, exit status
interpretation, or committed `testing/local-test-report.json`.

Verification has three layers:

1. Structural verification checks patch application, path and mode rules, generated
   files, contract documents, code-map obligations, dependency changes, and protected
   surfaces.
2. Source verification runs focused compiler, unit, schema, lint, conformance, and
   repository enforcement checks in a secret-free sandbox.
3. Product verification runs deployment, GPU, browser, performance, or held-out
   evaluations selected by the accepted specification.

The trusted check catalog lives outside the candidate lineage. A candidate may update
repository tests as part of the product change, but it cannot remove the external check
selection or change held-out assertions. When a candidate changes `tools/xtask`, smoke
code, conformance code, or the test-report recorder, the risk classifier requires
independent base-owned checks and protected review.

Cargo build scripts, proc macros, npm lifecycle scripts, Python build backends,
Dockerfile stages, tests, and generated tools are arbitrary candidate code. Verifier
workers therefore have the same absence of secrets and default-deny network posture as
author workers. A passing check is evidence of behavior, not evidence that running the
check was harmless.

A Verification Result is signed by the verifier service identity and includes the
verifier image, policy, check-catalog revision, base, patch, timestamps, resource use,
raw evidence digests, and verdict. Missing required evidence yields `incomplete`, never
`passed`.

## Source Promotion

The source broker accepts only a closed Promotion Request containing:

- candidate digest;
- expected base SHA and target branch;
- accepted specification and plan digests;
- passing Verification Result identities;
- required human decisions;
- proposed commit messages;
- protected-surface classification;
- maximum allowed repository operation.

The broker independently applies the patch to a clean repository, repeats structural
validation, verifies compare-and-swap against the expected base, creates the commit,
and optionally opens a PR. It rejects force pushes, history rewriting, unexpected base
movement, extra files, unsigned inputs, and a target beyond the request.

Base movement invalidates the verification result for promotion. The merge queue may
rebase or combine candidates only by producing a new source identity and rerunning the
required verification. Component leases reduce conflicts but do not make stale evidence
current.

The initial factory stops at broker-created draft PRs. Later phases may admit a
resettable integration branch for low-risk changes. Protected main and production
promotion remain explicit decisions until separate evidence approves another policy.

## Artifact Build And Publication

The build service receives one verified committed source revision and a trusted build
plan. It uses the existing Cargo, Bake, BuildKit, image-planning, and evidence contracts
from [`IMAGE_BUILDS.md`](IMAGE_BUILDS.md). The author sandbox never receives the managed
builder socket.

The build worker executes candidate build code without signing keys or target-cluster
identity. It emits images and attestations into quarantine storage. A separate
qualification service checks:

- exact source revision and clean publication state;
- affected image closure;
- runnable digest consistency between accepted staging and qualification;
- SPDX SBOM presence;
- maximum-mode SLSA provenance presence;
- required GPU certification;
- complete deployment-lock closure.

Only then may the signer sign the digest and the registry publisher make it available
at the approved repository. The builder cannot sign or publish by itself. The signer
does not compile source.

Development staging evidence remains `releaseEligible: false`. It may support a preview
or disposable integration environment but cannot cross into production qualification.

## GitOps And Deployment Acceptance

The GitOps broker works against the installation owner's configuration repository. It
accepts an approved release manifest, exact artifact digests, an expected Git base, and
an environment-specific change template. It creates a normal desired-state commit or
PR. It does not run a coding harness and does not patch Kubernetes resources directly.

The installation controller remains the reconciliation owner. Promotion between
environments is an explicit Git change as required by
[`ENTERPRISE_DEPLOYMENT.md`](ENTERPRISE_DEPLOYMENT.md). Rollback restores the previous
known-good manifests and digests.

The acceptance worker receives a short-lived identity scoped to one target and one
accepted check plan. It verifies controller health, exact source and applied revision,
Helm inventory, rollout, readiness, ingress, identity, MCP discovery, hosted-server
health, storage, and required domain behavior. It uses the existing typed Rust smoke
harness. It does not own service lifecycle beyond the operations already admitted by
that harness.

Visual, simulation, perception, rendering, video encode, and visual-verification checks
run only with accessible hardware GPUs. Headed browser acceptance proves hardware-backed
WebGPU or WebGL before navigating. SwiftShader, llvmpipe, software adapters, and
software-rasterizer warnings fail the job. Browser-side H.264 software decode retains
the one existing exception and must be labeled according to Media Capabilities.

Provider job completion remains webhook-only. A missing webhook creates an operational
failure. Neither a coding harness nor an acceptance worker polls the provider or adds a
fallback status path.

## Typed Factory Contracts

The initial design introduces closed JSON Schema 2020-12 records. Names are provisional
until an owning component design approves them.

| Planned schema | Responsibility |
|---|---|
| `veoveo.io/factory-product-spec/v1` | Developer intent, acceptance, scope, constraints, delivery, rollout, and budget. |
| `veoveo.io/factory-plan/v1` | Resolved base, components, risks, jobs, checks, policies, harness eligibility, and gates. |
| `veoveo.io/factory-job/v1` | One immutable role-specific execution request. |
| `veoveo.io/factory-candidate/v1` | Trusted patch identity and collection metadata plus explicitly untrusted author claims. |
| `veoveo.io/factory-verification/v1` | Independent check results, evidence digests, verifier identity, and verdict. |
| `veoveo.io/factory-promotion/v1` | Exact requested source, artifact, or GitOps broker operation and prerequisites. |
| `veoveo.io/factory-run-evidence/v1` | Complete run lineage, resource use, policy events, decisions, and terminal state. |
| `veoveo.io/factory-harness/v1` | Immutable harness descriptor and admission result. |

### Product Specification Shape

```text
FactoryProductSpec {
  schema
  specification_id
  revision
  title
  proposer
  repository
  requested_base
  outcome
  users_and_context
  acceptance_criteria[]
  scope
  non_goals[]
  constraints[]
  references[]
  delivery_target
  rollout_policy
  budget
  stopping_condition
}
```

The `users_and_context` section may reference a Veoveo tenant or Work Context needed for
staging acceptance. That reference does not grant build or deployment authority and is
not caller-supplied Work Context authorization. The acceptance worker receives its own
resolved target identity after promotion approval.

### Candidate Shape

```text
FactoryCandidate {
  schema
  run_id
  job_id
  specification_digest
  plan_digest
  base_sha
  base_tree_digest
  candidate_tree_digest
  patch_digest
  patch_artifact
  changed_paths[]
  author_claims
  harness
  factory_image_digest
  openshell_version
  openshell_policy_digest
  mcp_catalog_identity
  ocsf_export_digest
  transcript_digest
  resource_usage
  collection_status
}
```

`author_claims` is the only field supplied by the harness. The collector derives every
identity and status field.

### Verification Shape

```text
FactoryVerification {
  schema
  verification_id
  candidate_digest
  base_sha
  verifier_image_digest
  verifier_policy_digest
  check_catalog_revision
  checks[]
  protected_change_findings[]
  evidence_digests[]
  resource_usage
  verdict
  signer_identity
  signature
}
```

The verdict enum is `passed`, `failed`, or `incomplete`. There is no warning value that
promotion may reinterpret as passed.

## Run State Machine

```text
draft
  -> needs_specification_review
  -> preflighting
  -> needs_decision | queued
  -> preparing_author
  -> authoring
  -> collecting_candidate
  -> verifying
  -> repairing -> preparing_author
  -> needs_product_review
  -> promoting_source
  -> building_artifacts
  -> qualifying_artifacts
  -> needs_staging_approval
  -> deploying_staging
  -> accepting_staging
  -> observing_staging
  -> ready_for_production
  -> deploying_production
  -> accepting_production
  -> observing_production
  -> completed
```

Pause is an overlay on non-terminal states. Cancellation, rejection, budget exhaustion,
verification failure, promotion failure, rollback, and operational failure are explicit
terminal transitions. A state transition records the actor, previous state, next state,
reason, source event, and timestamp.

The durable ledger belongs outside OpenShell and outside the target installation. It
stores immutable specification revisions, run transitions, decisions, leases, and
evidence references. Blob evidence resides in factory-owned content-addressed storage.
The storage implementation remains an adoption decision; it must support transactional
compare-and-swap, append-only event history, retention, and backup without reusing
`platform/store` as an accidental cross-boundary dependency.

## Scheduling, Budgets, And Concurrency

The scheduler accepts only reviewed specification revisions and trusted automatic task
sources. A self-generated improvement idea enters the proposal queue and cannot start
authoring until its specification is reviewed.

Every run and job has enforced limits for:

- elapsed time;
- model input, output, and total calls;
- provider spend;
- CPU, memory, process count, and storage;
- network requests and bytes by endpoint class;
- patch bytes, file count, and changed component count;
- author attempts and verifier cycles;
- build, GPU, browser, and environment occupancy;
- consecutive identical failure classes.

Patch and component limits are operational guards, not security boundaries. A small
patch may still be high risk.

The scheduler holds component and branch leases. A lease prevents two runs from assuming
exclusive ownership of one moving base. It does not authorize source mutation. A source
broker still performs compare-and-swap at promotion time.

When no task is ready, the factory remains healthy and idle. Continuous operation does
not require inventing work. A reviewed backlog, a trusted failing check, an approved
dependency advisory, or a human proposal supplies the next objective.

## Failure Semantics

| Condition | Required result |
|---|---|
| Write outside workspace | OpenShell denial, recorded event, failed job; no policy widening. |
| Unlisted egress, MCP method, or tool | OpenShell denial and recorded event. |
| Forbidden MCP arguments | Veoveo gateway denial and audit event. |
| Policy validation failure | Network quarantine, invalid job, fresh sandbox required. |
| Missing Landlock proof | Sandbox startup failure. |
| Harness timeout or ignored cancellation | Forced sandbox termination and harness admission finding. |
| Candidate collector mismatch | `collection_status: failed`; candidate cannot enter verification. |
| Verification failure | Bounded repair job or terminal `verification_failed`. |
| Missing required evidence | `incomplete`; never promotable. |
| Base branch moved | Promotion rejection and fresh plan or verification. |
| Signer or registry unavailable | Promotion waits or fails; unsigned artifact is not substituted. |
| Missing provider webhook | Operational failure; no status polling. |
| GPU or hardware browser unavailable | Acceptance failure; no software-renderer evidence. |
| Deployment acceptance failure | Stop promotion and apply only the pre-authorized rollback policy. |
| OCSF export or trusted ledger unavailable | Job cannot produce promotable evidence. |

## Self-Improving Skills And Harnesses

Skills, prompts, plugins, harness descriptors, check selection, and model routing are
versioned factory inputs. An author may propose changes to them as candidate source,
but the active run continues with the original immutable versions.

The improvement loop is:

1. Select a baseline skill or harness and a reviewed optimization objective.
2. Author a candidate in the ordinary restricted sandbox.
3. Keep policy, held-out tasks, evaluator, scoring threshold, and promotion rules
   outside the author workspace.
4. Evaluate the candidate in fresh sandboxes across held-out tasks.
5. Compare correctness, regressions, cost, latency, and containment evidence.
6. Promote an immutable candidate digest only after protected review.
7. Canary the new version on eligible low-risk jobs.
8. Retain the prior digest and route back to it when the canary violates a gate.

The evaluator returns bounded results to the author. Held-out task contents and answer
keys do not enter author prompts or transcripts. An evaluation corpus that becomes
exposed is rotated before it provides future promotion evidence.

## Proposed Repository Placement

No implementation boundary is approved yet. If the first trial proceeds, the expected
shape is:

```text
tools/factory/
  DESIGN.md                 normative component design after approval
  AGENTS.md                 component-specific contribution rules
  contract/                 typed records and schema generation
  supervisor/               queue, state machine, budgets, and leases
  openshell/                driver adapter and policy materialization
  harness/                  harness descriptors and admission
  collector/                trusted source-tree and evidence collection
  verifier/                 check planning and signed verdicts
  brokers/                  source, build, registry, signer, and GitOps clients
  policies/                 complete reviewed role policies

tools/xtask/src/factory/     thin repository-native client and trial commands
testing/factory-conformance/ Rust containment and contract acceptance
apps/factory-console/        separate engineering UI and BFF
deploy/factory/              factory-only installation material
```

The exact split must keep binary entrypoints thin and keep source promotion, artifact
publication, and deployment credentials in distinct services. A single `factory.rs`
that mixes API routes, persistence, OpenShell lifecycle, policy, brokers, and tests
would violate the repository's module boundary.

`cargo xtask factory` owns repository-specific coordination and local trial entrypoints.
One-step OpenShell, Cargo, Helm, Git, and Kubernetes commands remain native where they
do not encode Veoveo policy. Factory smoke and containment acceptance are implemented
in Rust.

The Factory Console talks to a factory BFF. It never connects directly to OpenShell,
the durable ledger, a source forge, a signer, or a cluster. The BFF projects typed
states and evidence according to the user's factory role.

## Implementation Sequence

### Phase 0: Contract And Threat Model

- Approve the Product Specification, Plan, Job, Candidate, Verification, Promotion,
  Harness, and Run Evidence schemas.
- Define the trusted computing base and protected-change classifier.
- Define runner, administrator, verifier, broker, and auditor identities.
- Build the Rust factory-conformance harness before admitting a coding harness.
- Pin the trial OpenShell source, binaries, images, and build recipe.

Exit requires schema round trips, state-machine tests, negative authorization tests,
and a reviewed threat model. No coding job runs yet.

### Phase 1: Author-Only Spike

The spike runs on one disposable Linux VM with rootless Podman. It accepts one reviewed
task, creates one author sandbox, collects a patch, exports evidence, and destroys the
sandbox. There is no source broker, artifact publisher, or deployment integration.

The spike proves:

- writes outside the disposable workspace are denied;
- host secrets, credential stores, and runtime sockets are absent;
- unlisted egress is denied;
- a forbidden MCP method and tool are denied by OpenShell;
- a permitted tool with forbidden arguments is denied by the Veoveo gateway;
- the runner and harness cannot update policy or providers;
- invalid or unavailable `hard_requirement` enforcement blocks startup;
- telemetry traffic is absent;
- OCSF evidence is exported and hashed outside the sandbox;
- cancellation terminates the complete process tree;
- the collected patch reproduces from the recorded base in a fresh workspace;
- sandbox destruction removes filesystem state, dynamic approvals, credentials, and
  writable caches.

### Phase 2: Fresh Verification And Draft PRs

- Add the trusted check catalog and verifier worker.
- Produce signed Verification Results.
- Add bounded repair cycles using new author sandboxes.
- Add the source broker with expected-base compare-and-swap.
- Stop every successful run at a broker-created draft PR.
- Operate continuously against a reviewed task queue.

Exit requires repeated clean-room reproduction, no cross-job cache mutation, correct
stale-base rejection, and human review of an initial evidence sample.

### Phase 3: Integration And Artifact Qualification

- Add a resettable integration branch and merge queue.
- Run affected-image planning through trusted coordination.
- Connect the isolated build service, quarantine store, qualification service, signer,
  and registry publisher.
- Prove SBOM, provenance, runnable-digest equality, and deployment-lock closure.
- Keep production promotion disabled.

Exit requires reproducible artifact identity and proof that no build worker can sign or
publish independently.

### Phase 4: Staging Deployment

- Add the GitOps broker for one disposable staging installation.
- Add typed Flux convergence, deployment, domain, GPU, and browser acceptance.
- Exercise rollback to the previous known-good digest.
- Add Factory Console review and staging evidence views.

Exit requires exact Git-to-artifact-to-deployment lineage, hardware-GPU proof, bounded
observation, and recovery from interrupted reconciliation.

### Phase 5: Controlled Production Requests

- Produce production decision packages.
- Require explicit release-owner approval.
- Use separate production broker and acceptance identities.
- Enforce environment-specific soak, maintenance, and rollback policy.
- Export audit and evidence to installation-owned retention.

This phase does not imply unattended production promotion. Any later relaxation needs
its own architecture decision and operating evidence.

### Phase 6: Evaluated Self-Improvement

- Admit skill and harness candidates as protected artifacts.
- Add held-out evaluation, canary selection, regression budgets, and rollback.
- Compare admitted harnesses by verified outcome rather than self-reported completion.
- Keep evaluator and policy changes in a separate approval lineage.

## Trial Acceptance Record

Every trial record includes:

- OpenShell version, binary digests, source commit, and factory image digest;
- compute driver, host kernel, Landlock ABI, container runtime, and VM identity;
- policy source and effective digest, generation, and validation mode;
- runner and administrator role evidence;
- Product Specification, Factory Plan, base SHA, and candidate digest;
- harness, model route, skill, plugin, and evaluator identities;
- MCP catalog identity and exact OpenShell method/tool grants;
- verifier image, check-catalog revision, results, and signature;
- CPU, memory, storage, process, network, token, cost, and elapsed usage;
- complete OCSF export hash and trusted transcript hash;
- sandbox destruction result;
- every denial, exception, incomplete field, and human decision.

A successful functional test with missing containment evidence does not pass the trial.

## Open Questions

- Which durable database and content-addressed evidence store should the factory own
  without coupling it to a target Veoveo installation?
- Which organization identity and authorization profile should govern the Factory
  Console, component owners, and release owners?
- Which repository metadata provides canonical component ownership for preflight and
  protected-change review?
- Which trusted sources may enqueue work without a human drafting step?
- Which read-only dependency and research mirrors satisfy current-release verification
  without giving author jobs general internet access?
- Which checks remain base-owned, which are candidate-owned, and which held-out suites
  are maintained outside the product repository?
- What retention and access policy applies to proprietary source transcripts and OCSF
  evidence?
- When does the MicroVM driver become mandatory rather than risk-selected?
- Which preview and staging installations may the factory create, and who owns their
  lifecycle and cost?
- Which low-risk change classes, if any, may later merge automatically to an integration
  branch or protected main?
- How does the factory coordinate independent extension repositories without making the
  Veoveo repository their build owner?

These decisions belong in the owning component design before implementation becomes
normative.

## Adoption Path

A passing Phase 1 trial permits a reviewed architecture change. That change:

1. Records the OpenShell selection and factory trust boundary in
   [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md).
2. Adds the owning component `DESIGN.md` and `AGENTS.md` beside implementation.
3. Adds complete pinned OpenShell policies, schemas, and Rust conformance tests.
4. Adds the operative root `AGENTS.md` instruction that factory coding jobs run through
   the admitted boundary.
5. Updates [`CODEMAP.md`](CODEMAP.md), the repository README, build evidence, and
   deployment documentation.
6. Removes exploratory statements that conflict with the adopted hard cut.

Until that change lands, engineering sessions continue under the current repository
rules. This plan authorizes no unattended agent, source push, image publication,
signature, cluster access, or deployment.
