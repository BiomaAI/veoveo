# Factory Isolation: Exploration

> Status: exploratory and non-normative. This document records a direction for
> investigation. It does not approve a sandbox runtime dependency, a policy
> file, an `AGENTS.md` instruction, or any change to the operational boundary.
> The normative architecture remains in
> [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md),
> [`TECH_DESIGN.md`](TECH_DESIGN.md),
> [`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md), and the owning component
> designs.

## Purpose

The runtime boundary governs operational agents completely. Their only effect
channel is MCP through the gateway, where identity, profile policy, budgets,
leases, and audit apply below every harness. The factory loop has no such
boundary. Coding agents that extend, deploy, and operate installations run on
engineering hosts holding the operator's credentials, repository write access,
registry push rights, kubeconfig, and unrestricted egress.

NVIDIA's [agent-stack security guidance](https://developer.nvidia.com/blog/where-security-fits-in-an-ai-agent-stack)
directs operators to treat every component above the security boundary as
untrusted. The factory currently sits above every boundary the platform
enforces. This exploration asks how the delivery loop gains a kernel-enforced
boundary of its own without changing the operational one.

## What The Runtime Boundary Already Covers

A production agent cannot execute arbitrary code. The kernel constrains it to
bounded episodes over MCP tools, governed resource reads, and typed memory, so
the operational loop needs no process sandbox. The uncovered surface is the
engineering host, where a coding agent's mistake or a poisoned dependency acts
with the full authority of the signed-in engineer.

## Candidate Runtime

[NVIDIA OpenShell](https://github.com/NVIDIA/OpenShell) is the leading
candidate. The name below records the candidate, not a selection; any runtime
with equivalent kernel-enforced, version-controlled policy satisfies this
exploration.

Facts relevant to adoption:

- Apache 2.0, alpha maturity, active public roadmap.
- Kernel enforcement through Landlock filesystem confinement and seccomp
  syscall restriction.
- Declarative YAML policy across four domains: filesystem, network, process,
  and inference. Network and inference hot-reload; filesystem and process lock
  at sandbox creation.
- Launches existing harnesses directly, for example
  `openshell sandbox create -- claude`.
- A privacy router intercepts model API calls, strips caller credentials, and
  reroutes inference to approved backends, which keeps agent context inside
  the engagement boundary.
- Helm chart for cluster deployment. Linux, Apple Silicon macOS, and WSL2.
- Telemetry defaults on. Every Veoveo profile disables it with
  `OPENSHELL_TELEMETRY_ENABLED=false`.

## Proposed Shape

- Policy YAML is versioned in this repository beside the tooling it governs
  and is reviewed like any other control, matching the reviewed-Git rule for
  installation desired state.
- Filesystem confines the session to the active worktree and the pinned
  toolchain caches.
- Network denies by default and allowlists the forges, the registries, and the
  target installation.
- Inference routes model traffic to backends the engagement approves.
- Process policy blocks privilege escalation and bounds forks.
- Profiles split by task class. A documentation session, a code session, and a
  deployment session carry different authority, mirroring the risk tiers the
  guidance names.

## Open Questions

- Container-control sockets. Deployment work needs docker, k3d, and kubectl.
  Handing the sandbox a container socket restores host control and defeats
  the isolation. Candidate resolutions to trial: a build profile with no
  sockets plus a separate deployment step outside the sandbox, or a broker
  that admits only declared operations.
- Kernel floor. Landlock requires a recent Linux kernel. The build hosts must
  be verified before enforcement claims are made.
- Enforcement parity off Linux. Landlock and seccomp are Linux mechanisms. The
  depth of macOS enforcement must be established before a macOS session
  counts as contained.
- Pre-release dependency. Alpha maturity triggers the Dependency Currency
  rule: pinning a pre-release requires a recorded product reason. A passing
  trial recorded here is that reason.
- Harness reach-back. The harness needs its own provider endpoint. The trial
  decides whether the allowlist or the privacy router carries it.

## Acceptance Criteria

Adoption requires one recorded trial on a Linux build host in which every
criterion holds:

- `cargo xtask enforce rust` completes inside the sandbox.
- `cargo xtask smoke helm-config` completes inside the sandbox, or the
  container-socket question resolves with a documented profile split.
- A policy-violating action, a write outside the worktree and an egress to a
  non-allowlisted host, is denied and logged.
- Telemetry is verified disabled.
- The evidence lands through `cargo xtask test-report` like every other check.

## Non-Goals

- Operational agents. The gateway remains the sole authority boundary for
  agents acting on an installation.
- Cluster hardening. Pod Security Standards and NetworkPolicy remain the
  workload controls; this exploration does not substitute for them.
- Contract changes. The hosted-server contract and the control plane are
  untouched.

## Adoption Path

A passing trial promotes this work out of exploration in one change:
[`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md) records the decision,
`AGENTS.md` gains the operative instruction that factory sessions run inside
the sandbox, the policy file lands under `tools/`, and the README's software
factory section records the fact in one sentence. Until that change, no
normative surface references this work beyond the open-work pointer in
[`AUTONOMY_HARNESS.md`](AUTONOMY_HARNESS.md) and the gap register entry in
[`REGULATED_READINESS.md`](REGULATED_READINESS.md).
