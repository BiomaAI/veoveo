# Deployment Runtime Design

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/deployment/v7` and `veoveo.io/deployment-lock/v7` | Disposable installation profiles and immutable artifacts defined by `../contract/DESIGN.md` |
| `veoveo.io/source-chart-content/v1` | Shared content identity for source charts in verified immutable checkouts |
| `veoveo.io/gateway-activation/v1` | Complete public ConfigMap bundle identity from the deployment contract |
| `veoveo.io/component-publication/v1` | Internal xtask receipt for exact component lock composition; records chart revisions, configuration refresh, image evidence, and retained owners, with no cluster execution claim |
| `veoveo.io/installed-deployment-unit/v1` | Local installation provenance, exact Helm revision and manifest identity, and observed object fingerprints used to verify reuse |
| `veoveo.io/component-installation/v1` | Successful selected installation plan, actual unit outcomes, and unselected object and Helm observations; no API audit or cross-host fencing claim |
| Git | Immutable source checkouts, origin verification, and tracked installation input checks |
| Docker Buildx Bake | Read-only expansion of platform targets and source-owned workload groups during profile validation; locked installation consumes the published artifact closure |
| Helm v4.2.4 | Complete release rendering, source values before installation values, digest-locked images, and atomic release operations |
| Helm, Flux, and Argo CD ownership metadata | Exact Helm release annotations and managed-by label, with selected Flux and Argo ownership markers checked for imperative conflicts; absence of a marker grants no authority |
| Kubernetes/K3s v1.36.2 | Explicit contexts, namespace and object operations, Deployment readiness, Secret presence, GPU resource discovery, and server dry-run (`dryRun=All`) for normalized installation baselines |
| Kubernetes DRA `resource.k8s.io/v1` | Persistent ResourceClaims, named requests, and distinct-device constraints |
| NVIDIA DRA chart `0.5.0` and `resource.nvidia.com/v1beta1` | Pinned standalone allocator, verified chart and image artifacts, CDI preparation, and declared sharing configuration; hardware qualification is pending and upstream technology-preview features remain bounded by the deployment contract |
| k3d | Repository-managed disposable cluster and registry lifecycle through native commands |

## Responsibility

`veoveo-deploy-runtime` is Veoveo's shared operational deployment library. The focused
deployment smoke binary calls it for profile validation and lifecycle commands. The
general smoke suite also reaches this library through its Helm configuration scenario.
The release publisher calls its source-chart lock constructor. These consumers therefore
use the same chart inventory and content checks.

The library owns source checkouts and tool execution. Pure schemas, ownership types,
digest encodings, and mutation planning remain in `veoveo-deploy-contract`. The runtime
has no command-line parser. `cargo xtask` coordinates the native commands and dispatches
the Rust scenario harness, which retains scenario assertions and evidence.

Veoveo is the product being built and deployed. An installation supplies configuration;
Bioma is one reference configuration. This library does not create an imperative owner
for an enterprise installation governed by GitOps.

## Modules

| Module | Responsibility |
|---|---|
| `profile.rs` | Ordered profile validation, install, uninstall, and GPU verification |
| `sources.rs` | Installation input checks, immutable source checkouts, and Git identity |
| `snapshot.rs` | Exact Git blob and executable-mode verification of deployment inputs, independent of index hints and clean filters |
| `charts.rs` | Shared source-chart locks, ordered Helm values, rendering, and release commands |
| `compile.rs` and `compile/objects.rs` | Complete prepared component objects, non-secret inputs, and offline scope declarations |
| `compile/inputs.rs` | Publication and locked-installation snapshots keyed by component source name, repository, and revision |
| `compile/configuration.rs` | Exact installation profile snapshots, retained values files, and shared temporary checkouts for older configuration commits |
| `compile/execution.rs` | Gateway activation, GPU settings, claim manifests, and owned rollout targets prepared from each component's immutable configuration |
| `compile/images.rs` | Per-release image selection, exact build provenance, and rendered image closure checks |
| `publication.rs` | Chart, configuration, and qualified image updates for exact requested components, retaining other owners' inputs and inventories |
| `discovery.rs` | Destination API scope verification for every locked owner and proposed CRD |
| `helm_bundle.rs` | Temporary charts containing the complete prepared render consumed by Helm |
| `helm_state.rs` | Exact Helm metadata and the deployed or successful revisions an upgrade or rollback may use |
| `helm_state/snapshot.rs` | Stored manifest content and successful hook execution from Helm status |
| `installed.rs` and `installed/` | Local receipt storage, actual object fingerprints, and verified reuse for the expanded component selection |
| `installed/unselected.rs` | Read-only before/after snapshots of unselected permitted objects and Helm release metadata |
| `installed/normalize.rs` | Server dry-run projection of installed intent when Kubernetes serialization differs from the prepared object |
| `installed/planning.rs` | Checked installed observations, pure mutation decisions, and execution of the prepared atomic-unit plan |
| `ownership.rs` | Read-only historical inventory and live ownership checks before installation writes |
| `images.rs` | Source-owned Bake selection and locked image inventories |
| `configuration.rs` | Rendered Secret-reference closure, bounded Secret observations, public ConfigMaps, and gateway activation |
| `cluster.rs` | Local registry and k3d lifecycle, node bootstrap, and cluster readiness |
| `gpu.rs` and `gpu/` | Qualified allocator orchestration, persistent claims, admission, and workload placement |
| `gpu/migration.rs` | Preflight inventory of device-plugin retirement and workload quiesce effects, selected-owner checks, and version-bound execution |
| `gpu/workloads/quiescence.rs` | Deployment child UID tracking and a bounded wait for every owned Pod to disappear before device-plugin retirement |
| `process.rs` | Native command invocation helpers and explicit JSON object application |
| `profile/execution.rs` | Selected compiled operational inputs and rejection of incompatible retained GPU policies before writes |
| `profile/operations.rs` | Actual applied/reused unit outcomes and complete correspondence with the mutation plan |

The runtime retains the Secret gate before profile installation writes and mandatory GPU
qualification for selected GPU workloads. `profile_up` accepts `ComponentSelection::All`
or `Exact` and returns an `InstallationReceipt`. The CLI requires `--all-components` or
repeated `--component <id>`, plus a create-only `--receipt-output` path.

Dependency expansion precedes source resolution, rendering, installed-state planning,
and Secret preflight. The complete locked catalog remains authoritative for ownership,
including absent reserved identities and unselected releases. Namespace creation, node
bootstrap, public resources, gateway activation, allocator installation, persistent
claims, and source releases execute only when their atomic units enter the expanded
selection. Source releases follow component dependency order, then declaration order
within each component. Readiness waits use the compiled owner's Deployment identities.

GPU consumers require declared dependencies on both the allocator and claim owners;
a selected claim requires its allocator owner. Allocator-only selection performs its
admission checks without executing consumers. Selected consumer verification checks
hardware UUIDs for those consumers, while the persistent claim retains its full immutable
specification and allocation constraints. Standalone `profile-gpu-verify` checks every
configured consumer. An unrelated CPU component needs Ready nodes and does not wait for
the obsolete extended-resource interface on a DRA cluster.

The successful receipt records each planned atomic target exactly once as applied or
reused. Unselected observations include complete-object hashes and versions, explicit
absence, and Helm metadata; they contain no object bodies or Secret bytes. A receipt
is published only after all selected readiness checks and final observations succeed.
Snapshots can reveal outside activity, but they cannot establish zero API writes alone.
The separate native scope fixture supplies request metadata and runtime observations.
Cluster creation remains a separate lifecycle command; cross-host fencing and general
raw-object adoption remain outside this implementation.

## Immutable Render Inputs

Loading a profile checks source declarations and installation files without opening
source worktrees. The expanded component selection determines the exact source and
release footprint. The resolver clones only those sources and checks out their locked
commits. Preparation verifies selected charts and values against the component's
configuration snapshot. Unselected repositories
may be unavailable. A selected working checkout may have missing chart files because
its committed Git objects supply the installation snapshot.

Installation resolves each component's chart from its recorded source name, repository,
and revision. Components at the same revision share a checkout; components at different
revisions receive distinct checkouts even when they share a repository. The top-level
source revision records publication resolution and does not override retained component
revisions. Preparation verifies the actual checkout commit before rendering. Release
execution follows expanded dependency order and retains each owner's declaration order,
independent of checkout ordering.

Each component's `configuration` records its installation document and commit.
Preparation shares one checkout per configuration source revision, restores the recorded
document, and verifies its file inputs against Git. Source chart declarations and
installation values come from that document. Its installation name, namespace, context,
and registry must match the current destination. Replaced or deleted current values files
do not change a retained component. Temporary checkouts live through compilation and
are removed afterward.

Compilation also retains the operational inputs needed after rendering. Gateway Secret
requirements and the public activation bundle come from the gateway unit's configuration
snapshot. The allocator and claim carry their recorded GPU settings. Source units that
contain a declared GPU Deployment carry the same topology for placement verification.
The installer rejects disagreement between selected consumers and the allocator before
writing resources; changing that shared policy requires updating the affected component
configurations together. Gateway and claim manifests must equal the prepared objects.
Only the component owning a configured Deployment carries its explicit rollout wait.
Every configured wait must identify a Deployment in the complete locked inventory;
an unknown target fails preflight instead of disappearing during selection.
These inputs contain public configuration and Secret names and keys, never Secret values.

The installer consumes these compiled inputs directly. It does not reopen the current
gateway document or substitute the current global GPU policy after rendering retained
components. A native Git/Helm regression replaces and deletes a gateway configuration
file, changes required Secret keys and rollout waits, refreshes only the extension's
configuration, and verifies retained gateway requirements and owner-specific waits.
This test performs no Kubernetes mutation or GPU execution.

The complete locked artifact and ownership catalogs still validate before resolution.
Installation checks platform image completeness, registry ownership, image provenance,
and exact rendered image references against that qualified closure. It does not run
Docker Bake. Profile validation and publication retain the source build checks.

The chart lock constructor verifies the complete chart directory and each source-owned
values file against the checkout's Git tree. Installation preflight uses the same
verifier for every declared profile input. It hashes actual files with Git filters
disabled and compares their executable modes with the committed entries. An
`assume-unchanged` or `skip-worktree` index flag cannot hide local edits from this check.
Ignored files inside a chart fail the complete inventory comparison because Helm can
read them. Unrelated edits and build outputs outside the declared inputs do not
participate.

Input paths use literal Git pathspecs. Traversal must stay in the owning repository,
and every visited path segment must be free of symlinks. The caller retains the
immutable checkout through rendering; verification does not lock a user-editable
filesystem against concurrent writers. This check changes no image or chart digest
encoding and performs no Kubernetes operation.

## Component Execution Migration

The v7 migration is in progress. Publication prepares a complete ownership catalog.
Installation compares that catalog with the lock before reading required Secrets or
writing resources. Kubernetes discovery verifies the declared scope of every locked
object, including reserved identities. A proposed CRD can introduce a resource kind;
its declaration cannot override the scope of an existing API.

Each locked image records the immutable commit that produced it. The component's
chart snapshot may advance while that image remains unchanged. Compilation and
development image locks preserve the image's original source revision and publication
digest. Deployable-content hashes stay stable when only source provenance advances.
Installation builds each Helm release's image values from that atomic unit's locked
inputs. A newer image in another unit cannot replace its retained version. The renderer
rejects missing images, changed digests, and source images outside the unit's closure.
One release selects one build provenance per image repository; two versions in that
scalar values entry are ambiguous even if their runnable digests match. Source checkouts
carry no image selection.

Fresh publication still derives candidates from its qualified source artifacts, then
records only the images consumed by each render. When candidates remain unused,
publication renders again with the exact selection that installation will receive.
That render must consume precisely the same images; template-dependent image expansion
fails qualification. Platform and Veoveo-source values use
their source's candidates; extension values can consume platform-owned images as well.
Component publication accepts exact component IDs and a complete base lock through
`cargo xtask release components`. At least one input must be supplied: an exact chart
source commit, a configuration refresh, or qualified image evidence. Each omitted input
retains its recorded identity. Configuration refresh selects the current immutable
installation snapshot only for requested components. Chart updates replace only their
release locks, including when another component retains an older chart from the same
repository. Image selection replaces only explicitly supplied targets in the recorded
consumer closure. A chart or configuration update performs no image build.

The runtime restores and validates the base installation commit before composing new
inputs. Destination, ownership topology, and shared platform selection must stay fixed;
changing those boundaries requires a complete publication. It renders only requested
components and retains every other component, including dependencies, verbatim. Final
catalog validation rejects overlap and inconsistent artifact bindings. Installation
still requires the expanded dependency set.

The command verifies each supplied OCI publication index and runnable manifest through
the existing image qualification reader. Its create-only output includes a
`veoveo.io/component-publication/v1` receipt with base and output lock digests,
requested IDs, dependency IDs, retained IDs, chart source revisions, configuration
refresh intent, and image evidence digests. The image-operation
recorder captures command timing. Source association follows the declared evidence input;
this does not add cryptographic publisher authentication. The receipt establishes lock
composition, not live zero-write evidence. Development-lock promotion remains migration
work. Native Git and Helm regressions cover chart-only, configuration-only, and combined
updates in both directions between independent platform and extension repositories,
with the unrequested repository absent. Retained configurations remain reproducible
after current values files are replaced. These fixtures use synthetic image identities.
The reserved source name `installation` identifies installation-owned inputs; a
platform, workload, or extension source cannot use that name.

Raw resource application consumes the prepared objects. Helm receives a temporary
chart whose only template emits the prepared manifests through `Files.Get`. The
original source templates run during compilation, which makes their result independent
of Helm's later release counter and destination capabilities. Literal Go-template text
inside configuration stays literal. The temporary chart retains the source chart name,
version, and application version. Its installed values are the compiled manifests;
the deployment lock records the original chart and values inputs.

Hooks receive explicit Helm release-name, release-namespace, and managed-by metadata
before object hashing. A hook that declares another owner fails compilation. Helm
executes hooks separately from its normal manifest metadata visitor, which makes these
fields necessary for later ownership checks. Earlier locks containing hooks require
regeneration for this render change.

CRDs in a compiled Helm unit receive `helm.sh/resource-policy: keep` before object
hashing. Helm manages their declared contents, and uninstall retains them. Namespace
creation belongs to the explicit installation unit. Helm operations wait for Jobs and
use Helm 4's `--rollback-on-failure` operation.

Before namespace, bootstrap, allocator, configuration, or source-release writes,
installation reads the exact stored Helm manifests and hooks. It checks the current
revision and, when they differ, the deployed revision and most recent successful
rollback candidate. This follows the pinned
[Helm upgrade implementation](https://github.com/helm/helm/blob/v4.2.4/pkg/action/upgrade.go).
Historical namespaced objects can retire within the owner's declared namespaces;
cluster objects retain their explicit permission requirement. An object reserved by
another component or assigned to another atomic target cannot transfer through an
upgrade or rollback.

Live reads group exact object names by resource kind and namespace. Existing Helm
objects must identify the expected release and namespace. Raw application rejects
Helm-owned objects, and recognized Flux or Argo ownership markers reject imperative
application. Unserved kinds introduced by a prepared CRD have no live objects yet.
The preflight rechecks release metadata after reading objects and rejects a concurrent
release transition. These reads do not establish execution fencing or installed-content
equality. General raw-resource adoption still requires its migration boundary.

The ignored Rust live test in `ownership/live_tests.rs` requires an explicit
`VEOVEO_OWNERSHIP_TEST_CONTEXT`. It installs two ConfigMap-only releases in a unique
namespace, verifies ordinary preflight, and rejects historical manifest transfer,
historical hook transfer, and raw application over a Helm-owned object. It compares
ConfigMap identities and versions and both Helm revisions after rejection, then removes
and verifies removal of its namespace. These resources exercise ownership without
claiming GPU workload or selected-deployment acceptance.

Device-plugin migration resolves its removal and quiesce inventory before the first
profile write. Every configured workload must belong to a selected locked component.
Retiring objects cannot overlap any current component, and a retiring Helm release
cannot be a current atomic target. Helm removal reads the exact stored manifest and
hooks, verifies live owner metadata, and rejects deletion hooks or removal of a
Namespace, Node, or CRD whose effects exceed that inventory.

The executor consumes the prepared target set. It rechecks object UIDs and resource
versions, observed absences, and the Helm revision before quiescing. Each scale operation
carries its observed resource-version precondition. After rollout reports completion,
the executor waits for every owned Pod to disappear. It tracks all ReplicaSet revisions
by owner UID and retains observed child UIDs across deletion or ownership changes.
Terminating Pods still block retirement, as Kubernetes can exclude them from
[replica counts](https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#terminating-pods).
The wait rejects a replaced Deployment or a changed desired replica count and times
out after five minutes. Retirement observations are checked again after quiescing.
These checks detect drift; they do not fence another host between
the final observation and deletion. The isolated Rust regression uses zero-replica
Deployments and a ConfigMap-only retiring release. It verifies rejection of an
unselected workload and changed retirement state, successful checked removal, preserved
unselected object contents, and namespace cleanup. It executes no GPU workload.
A separate native regression starts a digest-pinned CPU sleep Pod, verifies its removal
before retirement, and restores a ready replica through the planned Helm operation.
This establishes lifecycle behavior only; it does not qualify GPU execution.

Installation still processes the full profile. Exact component selection,
execution fencing, raw-resource adoption, complete mutation receipts,
and live zero-write acceptance remain unfinished. The NVIDIA DRA 0.5.0
artifact and render checks pass; hardware qualification of that release is pending.

## Verified Installation Reuse

The installer prepares a mutation plan for the expanded selection before its first write. It feeds
checked Helm history, live object inventories, and reusable receipt provenance to the
pure planner. A missing or drifted baseline becomes `RequiresApply`; it does not assert
that desired inputs were previously installed. Planned GPU quiesce invalidates reuse
for the affected workload units, which ensures their releases restore desired replicas.
Namespace, bootstrap, allocator, claim creation, public configuration, gateway
activation, and source releases execute through their exact planned atomic unit.
Existing claims retain their separate immutable-spec and UID verification.

Execution checks each unit's owner, input digests, and object inventory against the
plan. An `Unchanged` decision must pass live reuse verification again; drift rejects
that decision and requires replanning. It cannot silently authorize a write. An apply
decision permits the existing installer to verify reuse or perform its recorded
operation. The internal plan records permitted effects, not actual mutation receipts.
The native ConfigMap regression covers absent units, reuse, a single-release update,
missing receipts, and rejection of changed inputs or live drift after planning.

The existing `profile-up` command records each successful Helm or prepared manifest-set
operation in the installation repository's Git common directory under
`veoveo-deployment/<cluster-uid-hash>/`. The `kube-system` Namespace UID binds the
destination; recreating a cluster creates a different receipt directory. A nonblocking
file lock covers preflight and execution for worktrees sharing this directory. It does
not fence another host or an independent installation repository.

Before installation writes, the runtime decodes all relevant receipts and checks their
schema, target, owner, provenance, and object inventory. A missing receipt takes the
ordinary installation path. A malformed receipt fails preflight. Each receipt contains
typed provenance and object hashes, with no manifest bodies or Secret values. It is a
local optimization record and grants no Kubernetes ownership authority.

Reuse requires the desired content digest to match the recorded installation. Helm
must still report the same deployed revision and canonical stored manifest inventory,
including hooks. Every existing object must retain its UID and actual content hash.
The hash includes server-added fields and status; only `resourceVersion` and
`managedFields` are excluded. Status changes can conservatively trigger a normal apply.
Live ownership is checked again when verifying reuse, and Helm metadata is reread after
object observation. Concurrent writers remain outside this local lock's boundary.

After an apply succeeds, baseline capture compares every declared field with the actual
API object. Added API defaults are accepted. When that comparison fails, the runtime
requests a [server dry-run](https://kubernetes.io/docs/reference/using-api/api-concepts/#dry-run)
of the prepared object. Helm projection uses its server-side field manager and owner
metadata; raw manifests use the installer's client-side apply behavior. The projected
object's full hash must equal the observed object's hash. This admits canonical resource
quantities and omitted empty fields without guessing Kubernetes normalization rules.
Projection cannot persist changes or force field conflicts. A rejected request fails
baseline capture; changed projected content declines caching. Unchanged reuse remains
read-only and does not request projection.

Helm's stored manifest must match the compiled inventory. An operation invalidates its previous receipt before invoking
its mutation, which prevents a failed or interrupted apply from reusing obsolete evidence.
Receipt publication uses an atomic local rename. Reuse preserves the source and
configuration provenance of the operation that actually installed the unit.

A `batch/v1` Job observed Complete without Failed and with a declared TTL may later be
absent. A Job removed before completion was observed cannot establish that evidence.
An absent hook requires Helm's stored `Succeeded` phase and `hook-succeeded` deletion
policy, following its [hook execution format](https://github.com/helm/helm/blob/v4.2.4/pkg/release/v1/hook.go).
Other missing objects decline reuse. This avoids rerunning completed initialization
solely because an expected cleanup removed its object.

The reuse gate covers node bootstrap, namespaces, public resources, gateway activation,
the GPU allocator Helm release, and source Helm releases. Allocator admission and GPU
readiness checks still execute. Persistent ResourceClaims retain their existing exact
specification and identity verification, which already avoids an apply when unchanged.
Cluster creation and device-plugin migration remain separate lifecycle boundaries.

The explicit-context Rust test in `installed/tests.rs` creates isolated ConfigMap
releases and a raw ConfigMap. It verifies unchanged object versions and Helm revisions,
retained installed provenance, an update confined to one release, successful hook
cleanup, live drift and replacement detection, and invalidation after failure. A
foreign field-manager conflict remains an error through Helm's normal protection.
An additional zero-replica Deployment fixture verifies quantity normalization and
omitted empty Pod fields. It projects a changed memory request and verifies that neither
the live resource version nor the Helm revision changes. Its synthetic image is never
executed. Each fixture verifies its namespace removal. Unit tests cover TTL completion, malformed
receipts, cluster isolation, and the local lock. These checks do not qualify a GPU
workload. Selected CLI acceptance is described in the
[deployment verification design](../../testing/deployment-smoke/DESIGN.md#component-selection).

## Dependencies

The extraction reuses the existing resolved dependency graph. Direct upstream dependencies
are pinned exactly in the workspace: `anyhow 1.0.104`, `hex 0.4.3`, `jsonwebtoken 11.0.0`,
`reqwest 0.13.4`, `serde 1.0.229`, `serde_json 1.0.151`, `serde_yaml_ng 0.10.0`, `sha2 0.11.0`,
`tempfile 3.27.0`, and `url 2.5.8`. Each was verified as its latest non-yanked stable release
from its authoritative `https://crates.io/api/v1/crates/{name}` metadata on September 7,
2026. No upstream package version changed during extraction.

The deployment contract owns the managed GPU component pins. Unit tests and rendered
configuration checks do not establish a new hardware acceptance result.
