# Deployment Verification

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Flux source and Kustomization APIs `v1`, HelmRelease API `v2` | exact Git artifact and applied revision, observed generation, readiness, terminal Helm failure, and release inventory |
| Kubernetes apps `v1` | Deployment rollout status and Available condition through kubectl |
| Kubernetes batch `v1` | initialization Job names bound to their complete rendered specs, including immutable Pod templates |
| Kubernetes JSON watch events | initial resource observation followed by bounded watch output |
| `veoveo.io/gitops-convergence-evidence/v3` | repository-owned JSON evidence with reconciliation mode, wall-clock start and observation times, elapsed phases, and terminal outcome |
| Deployment profile and lock `v6` | shared disposable-profile execution from the smoke owner; schema belongs to `deploy/contract` |

## Responsibility

This focused Rust harness owns Helm configuration assertions, deployment-profile
dispatch, and exact-revision GitOps observation. Installation mutation remains with
Helm and the installation's GitOps controllers.

`gitops-converge` defaults to `--reconciliation observe`. It reads Flux resources,
watches their status, and observes Deployment rollout and availability. It issues no
reconciliation annotations. `--reconciliation request` explicitly annotates the named
Git source, root Kustomization, and each HelmRelease before observing their phase.
These requests can accelerate convergence and therefore cannot prove passive latency.
[Flux describes explicit Helm reconciliation](https://fluxcd.io/flux/cmd/flux_reconcile_helmrelease/).

Both modes require a full locally resolvable Git commit, generation-current source
and root readiness at that revision, nonempty Helm inventories, and readiness of every
named Deployment. Evidence identifies the chosen mode and remains create-only.
A failing phase retains its diagnostic in the failed record.

Elapsed time begins when typed verification starts, before local revision checks.
Cargo prerequisite builds and xtask dispatch precede this clock. It measures the
observer's duration, not Git
commit-to-ready latency when the observer starts after publication or when the expected
revision is already active. Start passive observation before publishing the prepared
commit and retain the publication timestamp when measuring that interval. The root
phase checks root state and terminal Helm failures at intervals of at most two seconds;
this contributes observation delay. Source and Helm readiness use resource watches.

The verifier does not prove a particular chart digest, values digest, image digest, or
unchanged Pod identity merely from Ready status. Activation acceptance must compare
those installation inputs and Pod identities separately.

## Acceptance

`src/helm_config/jobs.rs` renders the actual platform chart with changed Helm revision
contexts and chart metadata. It requires unchanged initialization Job names and specs,
then changes individual runtime inputs and checks the exact affected Job identities.
The cases also cover long release names, explicit gateway bundle revisions, and an
unrelated Console image change. This is rendered configuration acceptance; it does not
claim that a live Job ran or that a workload used a hardware GPU.

`tests/gitops_convergence.rs` executes the real CLI against a Rust kubectl fixture.
It records every command and rejects mutation in observation mode. Cases cover default
observation, explicit observation across a source watch, requested reconciliation,
wrong source revision, stale source generation, and terminal Helm failure. Successful
cases require all five phases and both selected Deployment readiness checks.
The fixture establishes command and evidence behavior; it is not live Flux latency
or GPU workload evidence.
