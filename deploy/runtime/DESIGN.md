# Deployment Runtime Design

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/deployment/v6` and `veoveo.io/deployment-lock/v6` | Disposable installation profiles and immutable artifacts defined by `../contract/DESIGN.md` |
| `veoveo.io/source-chart-content/v1` | Shared content identity for source charts in verified immutable checkouts |
| `veoveo.io/gateway-activation/v1` | Complete public ConfigMap bundle identity from the deployment contract |
| Git | Immutable source checkouts, origin verification, and tracked installation input checks |
| Docker Buildx Bake | Read-only expansion of the exact platform targets and source-owned workload groups |
| Helm v4.2.3 | Complete release rendering, source values before installation values, digest-locked images, and atomic release operations |
| Kubernetes/K3s v1.36.2 | Explicit contexts, namespace and object operations, Deployment readiness, Secret presence, and GPU resource discovery |
| Kubernetes DRA `resource.k8s.io/v1` | Persistent ResourceClaims, named requests, and distinct-device constraints |
| NVIDIA DRA chart `0.4.1` and `resource.nvidia.com/v1beta1` | The existing qualified standalone allocator, chart and image digest checks, CDI preparation, and measured sharing configuration; upstream technology-preview features remain bounded by the deployment contract |
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
| `charts.rs` | Shared source-chart locks, ordered Helm values, rendering, and release commands |
| `images.rs` | Source-owned Bake selection and locked image inventories |
| `configuration.rs` | Rendered Secret-reference closure, bounded Secret observations, public ConfigMaps, and gateway activation |
| `cluster.rs` | Local registry and k3d lifecycle, node bootstrap, and cluster readiness |
| `gpu.rs` and `gpu/` | Qualified allocator orchestration, persistent claims, admission, and workload placement |
| `process.rs` | Native command invocation helpers and explicit JSON object application |

The extraction preserves the current v6 execution behavior, including the Secret gate
before profile installation writes and mandatory GPU qualification. Profile installation
still processes the complete deployment. Component-selected execution remains the
`DEPLOY-SCOPE-023` migration: it must use the contract's complete ownership catalog and
cover every installation mutation before exposing a component selector.

## Dependencies

The extraction reuses the existing resolved dependency graph. Direct upstream dependencies
are pinned exactly in the workspace: `anyhow 1.0.104`, `hex 0.4.3`, `jsonwebtoken 11.0.0`,
`reqwest 0.13.4`, `serde 1.0.229`, `serde_json 1.0.151`, `serde_yaml_ng 0.10.0`, `sha2 0.11.0`,
`tempfile 3.27.0`, and `url 2.5.8`. Each was verified as its latest non-yanked stable release
from its authoritative `https://crates.io/api/v1/crates/{name}` metadata on September 7,
2026. No upstream package version changed during extraction.

The managed GPU component pins and their existing runtime qualification remain in the
deployment contract. Unit tests and rendered configuration checks do not establish a
new hardware acceptance result.
