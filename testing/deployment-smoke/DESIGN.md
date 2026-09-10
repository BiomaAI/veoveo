# Deployment Verification

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Flux source and Kustomization APIs `v1`, HelmRelease API `v2` | exact Git artifact and applied revision, observed generation, readiness, terminal Helm failure, and release inventory |
| Kubernetes apps `v1` | Deployment rollout status and Available condition through kubectl |
| Kubernetes batch `v1` | initialization Job names bound to their complete rendered specs, including immutable Pod templates |
| Kubernetes JSON watch events | initial resource observation followed by bounded watch output |
| `veoveo.io/gitops-convergence-evidence/v3` | repository-owned JSON evidence with reconciliation mode, wall-clock start and observation times, elapsed phases, and terminal outcome |
| Deployment profile and lock `v7` | shared disposable-profile execution from the smoke owner; schema belongs to `deploy/contract` |
| Flux CLI 2.9.5 and Helm 4.2.4 | native OCI configuration and chart publication for isolated controller verification |
| `veoveo.io/flux-cancellation-evidence/v1` | repository-owned controller state, source digest, latency bound, failure, and fixture cleanup evidence |
| `veoveo.io/component-installation/v2` | successful selected CLI receipt defined by the deployment contract |
| `veoveo.io/component-scope-evidence/v1` | independent Git/OCI fixture inputs, selected installation duration, applied/reused units, native API request metadata, runtime snapshots, overlap rejection, and cleanup |
| kubectl/client-go v1.36.2/v0.36.2 local proxy logs | internal test observer of completed HTTP method and URI at verbosity 6; canary writes and ordered barriers verify the observer before accepting scope evidence |

## Responsibility

This focused Rust harness owns Helm configuration assertions, deployment-profile
dispatch, and exact-revision GitOps observation. Installation mutation remains with
Helm and the installation's GitOps controllers.

The `computers_helm` integration target renders the two standard presets and the
configured Computers boundary with real Helm. It checks no-op configuration/Pod
stability, unprivileged control, explicit configuration/trust references and rejected
missing store dependencies. It creates no cluster resources or provider capacity.

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

`gitops-cancel-verify` exercises live Kustomize and Helm health-check cancellation.
The Rust harness publishes one immutable witness chart and three distinct OCI
configuration artifacts. A newly created namespace contains every fixture resource.
Flux owns the witness Deployment; the harness only changes its source digest.
The witness runs `sleep` and an explicit readiness exit code in the supplied image.
It performs no rendering, simulation, or perception work.

The scenario establishes a healthy release, submits an unhealthy update, and requires
generation-current evidence that both controllers are checking that revision and its
updated Pod remains unready for at least five seconds before submitting the fix.
Both controller timeouts are five minutes. The fixed source, root,
Helm release, and Deployment must converge within 90 seconds. Evidence records source
digests for the chart and every source, observed generations, the intermediate
health-check state, fix latency, and
cleanup errors. The root is deleted before the namespace so Flux can uninstall its
release. Failure remains a failed result even when cleanup succeeds.

Use an already available digest-pinned image with `/bin/sh` and `sleep`:

```bash
cargo xtask smoke gitops-cancel-verify \
  --context <context> \
  --push-registry <host-registry> \
  --pull-registry <cluster-registry> \
  --registry-transport <tls-or-insecure-http> \
  --image <repository@sha256:digest> \
  --evidence-output output/development/flux-cancellation.json
```

This proves cancellation across real OCI source changes and Helm upgrades. Git server
delivery latency and GPU workload acceptance remain separate measurements. The
installed Flux reference uses the latest stable 2.9.5 patch, verified against the
[upstream release](https://github.com/fluxcd/flux2/releases/tag/v2.9.5).

## Component Selection

`component-scope-verify` creates independent platform, extension, and installation Git
repositories. The existing managed builder publishes digest-pinned witness images to
the supplied local HTTP registry. Each source owns one Helm release with a Ready sleep
Pod; both depend on the namespace owner. The harness runs the real `profile-up` CLI for
initial installation, a platform-only image update, and an extension-only image update.
During each update the unselected source repository is unavailable. The changed image
must replace the selected Pod, while the namespace dependency must reuse its receipt.

The child installer uses a loopback kubeconfig connected to the native kubectl proxy.
The proxy alone uses the original credentials. At verbosity 6, client-go's
[transport logger](https://github.com/kubernetes/client-go/blob/v0.36.2/transport/round_trippers.go)
records completed request method, URL, status, and timing; headers begin at verbosity 7.
The harness retains method and URI only, with no request bodies, response bodies,
headers, or credentials in evidence. Create/patch/delete canaries and ordered GET
barriers verify parsing and drain the observer. Rejected proxy requests fail the run.
This log format is an internal adapter boundary, not a stable Kubernetes audit API.

Each selected update must issue API writes, confined to its Deployment and Helm storage.
The unselected Deployment UID, resource version, complete-object hash, Pod UID, image
IDs, restart counts, Helm revision, and every Helm storage Secret's UID and resource
version must remain identical. POSTs to the Secret collection do not expose names in
request metadata; unchanged complete unselected storage metadata and rejection of any
named unselected write complement the observation. This is evidence for the exact
fixture, not server-side auditing or cross-host mutation fencing. An intentionally
overlapping ownership declaration must fail with zero observed writes.

The scenario removes its namespace and verifies absence on success or failure. It
retains the independent Git histories, locks, receipts, and OCI references for review.
`installationElapsedMs` measures the child installation process including preflight and
readiness; it excludes image publication, Cargo compilation, and observer snapshots.
The fixture's extension-manifest identity is synthetic. This test proves operational
scope and image rollout, not extension admission or GPU execution.

```sh
cargo xtask smoke component-scope-verify \
  --repository <veoveo-checkout> --context <context> \
  --push-registry <host-local-http-registry> \
  --pull-registry <cluster-local-http-registry> \
  --base-image <repository@sha256:digest> \
  --evidence-output <new-evidence.json>
```
