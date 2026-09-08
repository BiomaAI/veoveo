# Deployment Runtime Design

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/deployment/v7` and `veoveo.io/deployment-lock/v7` | Disposable installation profiles and immutable artifacts defined by `../contract/DESIGN.md` |
| `veoveo.io/source-chart-content/v1` | Shared content identity for source charts in verified immutable checkouts |
| `veoveo.io/gateway-activation/v1` | Complete public ConfigMap bundle identity from the deployment contract |
| Git | Immutable source checkouts, origin verification, and tracked installation input checks |
| Docker Buildx Bake | Read-only expansion of platform targets and source-owned workload groups during profile validation; locked installation consumes the published artifact closure |
| Helm v4.2.3 | Complete release rendering, source values before installation values, digest-locked images, and atomic release operations |
| Kubernetes/K3s v1.36.2 | Explicit contexts, namespace and object operations, Deployment readiness, Secret presence, and GPU resource discovery |
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
| `discovery.rs` | Destination API scope verification for every locked owner and proposed CRD |
| `helm_bundle.rs` | Temporary charts containing the complete prepared render consumed by Helm |
| `images.rs` | Source-owned Bake selection and locked image inventories |
| `configuration.rs` | Rendered Secret-reference closure, bounded Secret observations, public ConfigMaps, and gateway activation |
| `cluster.rs` | Local registry and k3d lifecycle, node bootstrap, and cluster readiness |
| `gpu.rs` and `gpu/` | Qualified allocator orchestration, persistent claims, admission, and workload placement |
| `process.rs` | Native command invocation helpers and explicit JSON object application |

The runtime retains the Secret gate before profile installation writes and mandatory GPU
qualification. Profile installation still processes the complete deployment.
Component-selected execution remains the
`DEPLOY-SCOPE-023` migration: it must use the contract's complete ownership catalog and
cover every installation mutation before exposing a component selector.

## Immutable Render Inputs

Loading a profile checks source declarations and installation files without opening
source worktrees. The expanded component selection determines the exact source and
release footprint. The resolver clones only those sources, checks out their locked
commits, and verifies only their selected charts and values. Unselected repositories
may be unavailable. A selected working checkout may have missing chart files because
its committed Git objects supply the installation snapshot.

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
The reserved source name `installation` identifies installation-owned inputs; a
platform, workload, or extension source cannot use that name.

Raw resource application consumes the prepared objects. Helm receives a temporary
chart whose only template emits the prepared manifests through `Files.Get`. The
original source templates run during compilation, which makes their result independent
of Helm's later release counter and destination capabilities. Literal Go-template text
inside configuration stays literal. The temporary chart retains the source chart name,
version, and application version. Its installed values are the compiled manifests;
the deployment lock records the original chart and values inputs.

CRDs in a compiled Helm unit receive `helm.sh/resource-policy: keep` before object
hashing. Helm manages their declared contents, and uninstall retains them. Namespace
creation belongs to the explicit installation unit. Helm operations wait for Jobs and
use atomic rollback.

Installation still processes the full profile. Exact component selection, installed
state receipts, historical object ownership, conflicting-device-plugin transition
inventories, and live zero-write acceptance remain unfinished. The NVIDIA DRA 0.5.0
artifact and render checks pass; hardware qualification of that release is pending.

## Dependencies

The extraction reuses the existing resolved dependency graph. Direct upstream dependencies
are pinned exactly in the workspace: `anyhow 1.0.104`, `hex 0.4.3`, `jsonwebtoken 11.0.0`,
`reqwest 0.13.4`, `serde 1.0.229`, `serde_json 1.0.151`, `serde_yaml_ng 0.10.0`, `sha2 0.11.0`,
`tempfile 3.27.0`, and `url 2.5.8`. Each was verified as its latest non-yanked stable release
from its authoritative `https://crates.io/api/v1/crates/{name}` metadata on September 7,
2026. No upstream package version changed during extraction.

The deployment contract owns the managed GPU component pins. Unit tests and rendered
configuration checks do not establish a new hardware acceptance result.
