# Deployment Verification

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.ai/module-installation-evidence/v1` | Typed setup, installed DB/Job outcomes, runtime image observations, redacted failure excerpts and UID-owned cleanup |
| Kubernetes `networking.k8s.io/v1` NetworkPolicy | Producer-only deny-all ingress/egress; positive and rejected native health probes qualify enforcement |
| OCI distribution digest references | Maintained OCI parsing admits untagged SHA-256 gateway, manager and kernel images; the gateway image supplies the composition binding |
| Kubernetes admissionregistration `v1` | Actual chart ValidatingAdmissionPolicy and Binding objects restrict managed workload identities and configuration; UID-owned fixture cleanup |
| SurrealDB 3.3.0 LIVE and Store authoring APIs | Privileged fixture publication/provision, ordered runtime lease observations and explicit owned LIVE query cancellation |
| `veoveo.ai/module-selection/v1` and `veoveo.ai/module-plan/v1` | Checked camelCase JSON; optional selection, decimal-string generation, nonsecret credential revision and composition-generated lane/runtime bindings |
| Flux source and Kustomization APIs `v1`, HelmRelease API `v2` | exact Git artifact and applied revision, observed generation, readiness, terminal Helm failure, and release inventory |
| Kubernetes apps `v1` | Deployment rollout status and Available condition through kubectl |
| Kubernetes batch `v1` | initialization Job names bound to their complete rendered specs, including immutable Pod templates |
| Kubernetes JSON watch events | initial resource observation followed by bounded watch output |
| `veoveo.ai/gitops-convergence-evidence/v3` | repository-owned JSON evidence with reconciliation mode, wall-clock start and observation times, elapsed phases, and terminal outcome |
| Deployment profile and lock `v7` | shared disposable-profile execution from the smoke owner; schema belongs to `deploy/contract` |
| Flux CLI 2.9.5 and Helm 4.3.0 | native OCI configuration and chart publication for isolated controller verification |
| `veoveo.ai/flux-cancellation-evidence/v1` | repository-owned controller state, source digest, latency bound, failure, and fixture cleanup evidence |
| `veoveo.ai/component-installation/v2` | successful selected CLI receipt defined by the deployment contract |
| `veoveo.ai/component-scope-evidence/v1` | independent Git/OCI fixture inputs, selected installation duration, applied/reused units, native API request metadata, runtime snapshots, overlap rejection, and cleanup |
| kubectl/client-go v1.37.0/v0.37.0 local proxy logs | internal test observer of completed HTTP method and URI at verbosity 6; canary writes and ordered barriers verify the observer before accepting scope evidence |

## Module Render Qualification

`helm-config` renders genuine gateway-generated fixture plans. Its Job assertions
compare the emitted lane set with the typed plan, preserve no-op identity across
chart and release metadata changes, and check generation/credential rotation. It
inspects every rendered database client pod template and confines migration Secret
references to preparation/lane Jobs and database root provisioning. The manager's
native recovery decisions qualify stale-workload retirement and drain fencing.
These source and render checks do not establish OCI command availability or installed
fresh/upgrade completion; those require the locked image and an isolated installation.

## Responsibility

This focused Rust harness owns Helm configuration assertions, deployment-profile
dispatch, and exact-revision GitOps observation. Installation mutation remains with
Helm and the installation's GitOps controllers.

`src/helm_config.rs` exercises generic platform contracts with the anonymous
`testing/fixtures/platform-selection` and `testing/fixtures/fork-installation` inputs.
`src/helm_config/bioma.rs` owns the reference installation's values, public hostname,
control-plane bundle, cluster, and GitOps assertions. Both modules run in `helm-config`.
Configuration rendering establishes chart behavior; installed smoke uses the
[installation target](../../deploy/contract/DESIGN.md#installation-target) to select
its running release explicitly.

`src/helm_config/object_store.rs` checks the bundled RustFS worker count and storage
readiness probe. It rejects single-worker, fractional and string configurations
through the chart schema. Runtime S3 behavior belongs to the Artifact service's
installed multipart harness.

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
[transport logger](https://github.com/kubernetes/client-go/blob/v0.37.0/transport/round_trippers.go)
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

## Installed Module Lifecycle

`module-installation-verify` uses the supplied digest-only gateway OCI reference,
derives its composition binding and generates plans with that actual image. The
fixture also takes digest-only manager and kernel images. It requires a reachable
registry, an enforced NetworkPolicy CNI, local-path storage and permission to create
two isolated namespaces and their resources, including cluster-scoped admission
policies. Local OpenSSL generates disposable signing keys. Helm renders against
the selected Kubernetes server version observed during setup.
Positive and denied probes use the chart's pinned SurrealDB 3.3.0 CLI. A deadline,
scheduling failure or unavailable canary cannot establish policy enforcement.

The harness applies actual chart database, ServiceAccount, plan ConfigMap and
preparation/lane/publication Jobs. It checks fresh generation 1, unchanged object
UIDs and persisted publication replay, generation 2 account rotation with stale
preparation/migration/publication rejection, preserved disabled Time history and
later Media enablement in generation 3. Current-publication replay after stale Jobs
checks the persisted revision, hash and counts. Selected lanes install their complete
current schemas; these checks exercise real owner bodies and migration histories.
The same chart starts a gateway and lifecycle manager. The fixture uses privileged
Agent repository APIs over the shared Store connection to create, publish and provision
one managed agent. Setup
reads and verifies the Work Context created by production control-plane publication;
it does not create or overwrite installation policy. A native database test runs
production publication before the complete fixture authoring/provision sequence. This setup
does not qualify gateway authoring authorization. A real kernel authenticates
through the gateway's OAuth and managed registration routes. No GPU or provider
workload is selected.
The authenticated profile exposes no MCP servers and grants no server-scoped
actions. Its discovery catalog is empty; the kernel has no subscriptions or
pending tasks that require domain calls. Native setup checks keep capability
admission strict and verify both generated signing keys through gateway APIs.

All three installation generations select the Agents schema lane while Time is
disabled in generation 2 and re-enabled with Media in generation 3. Native checks
generate these selections from owner schemas and render the actual fixture values;
the installed gateway producer supplies the complete composition catalog.

Generation 1 requires persisted managed readiness correlated with the ready Pod UID
and two advancing lease expiries under one owner and fence. Healthy DB and watch
samples can arrive separately; missing runtime acknowledgment or a pending Pod
observation waits within the 180-second deadline. Duplicate or contradictory
identities and watch errors fail qualification. The fixture writes a
content witness to the retained PVC. Generation 2 rotates the actual database
account and both owned runtime Secrets, verifies fresh connection rejection with
the old credential, and observes foreground workload retirement. Pod and Deployment
watches begin with raw single-resource API lists whose resourceVersion survives
an empty inventory. Raw API watches carry that revision and the same selector,
with a 550-second server timeout inside the 600-second observer budget. A
replacement cannot overlap its old object in either ordered stream. A watch error, unknown identity or disconnected
observer fails qualification.

The runtime's ordered LIVE stream must show old-owner release or expiry before a
new owner acquires a newer fence. These observations establish workload and lease
non-overlap within their respective providers. Independent watch streams and
database/Kubernetes clocks do not establish a cross-provider total order. The
manager's native recovery tests separately qualify its rule that replacement waits
for both Pod drain and released/expired lease, including unknown observations.
The installed gate requires the observed replacement to use generation 2's
credential revision and renew twice. Registration, client identity, managed
generation, signing Secret UID/public JWK and PVC UID/content must match generation
1. Replaying the installation keeps the replacement Pod, Deployment and lease fence
for more than two renewal intervals.

The approved HTTPS model uses the reserved `.invalid` domain and a disposable
fixture Secret. Chart policy denies external model egress. The manifest has no
startup prompt or subscriptions and delays heartbeat for one hour; setup produces
no actionable wakes, Task results or input requests. From before launch through
terminal archive and namespace cleanup, the fixture requires no persisted episodes
and an unchanged next-episode sequence of 1 for every kernel runtime. The kernel's
native persistence-before-dispatch test qualifies the implication that these
observations prove zero model dispatch. Controls alone do not establish that claim.

Every account and namespace belongs to the fixture. Secret JSON files have mode
0600. Errors retain capped pod state/log excerpts with current, root and prior
fixture passwords redacted before namespace deletion. JSON evidence includes setup
failure, observed runtime image IDs, each installed outcome and cleanup failure.
Namespace and cluster policy deletion send their created UIDs as Kubernetes
DeleteOptions preconditions. A failed final observation is recorded while cleanup
still attempts every owned resource. The database stays available for the final
episode read after kernel workloads exit. Fixture database futures have a 30-second
transport budget. Each LIVE stream keeps its Tokio runtime alive and is destroyed
inside that runtime after an explicit KILL attempt, including failed setup and
teardown. Setup diagnostics preserve the primary error when LIVE cleanup also
fails. Watches reject gaps, and background observers own their process
groups. Readiness, recovery and terminal archive each have a 180-second observation
budget; individual native commands retain their own shorter deadlines.
Commands have deadlines, file-backed output capped at 2 MiB and owned process-group
cleanup on timeout; no blind mutation retries are added. Command failures identify
the phase, program and exit status. The fixture-owned Helm render
exposes stderr through the same secret replacement and 16 KiB cap as pod diagnostics.
The old-runtime credential probe first runs a credential-free `curl` readiness
init in the same Pod, using the same pinned gateway image and database Service.
Readiness has a 60-second retry budget with 5-second transfers inside the probe's
120-second Job deadline. The main validation command runs once with the old
password Secret; only its authentication rejection proves the negative case.
The harness verifies successful init termination before reading the main logs.
Failed init diagnostics identify the readiness phase and exit status.
Cleanup errors preserve the deletion or observation stage and source error chain,
with secret replacement and a 16 KiB aggregate limit. Namespace deletion keeps
its 60-second Kubernetes wait and 70-second subprocess deadline.
The harness preserves the first unexpected probe's secret-redacted output before
inventory and pod logs share the 16 KiB diagnostic limit. An unavailable inventory
cannot suppress that probe diagnostic. Native publication tests establish the
runtime validation path with valid credentials and its authentication rejection
with a wrong password; unrelated connection or command errors fail the probe.
Failure diagnostics describe the UID-owned agent namespace before the installation
namespace, summarize Deployment and Pod inventories before collecting logs, and
apply the output cap after replacing every fixture-owned secret. Observer exits
identify the Pod watch, Deployment watch or Store port-forward and report exit
status without command arguments or child stderr.
Other command output and argv stay private.

The harness pins `oci-spec` 0.10.0 with only its distribution feature for maintained
OCI reference admission. Its [upstream stable release](https://github.com/youki-dev/oci-spec-rs/releases/tag/v0.10.0)
was checked before adoption. `nix` reuses qualified 0.31.3 for safe process-group
signals; neither dependency adds a registry transport client.

```sh
cargo xtask smoke module-installation-verify \
  --context "$FIXTURE_CONTEXT" \
  --gateway-image "$PINNED_GATEWAY_IMAGE" \
  --manager-image "$PINNED_MANAGER_IMAGE" \
  --kernel-image "$PINNED_KERNEL_IMAGE" \
  --evidence-output "$FIXTURE_EVIDENCE"
```
