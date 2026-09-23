# Fork Development Plan

## Standards And Protocols

Git commits identify customized source and reviewed upstream merges. OCI digests
identify deployed images and charts. Helm and Kubernetes own workload packaging;
Flux reconciles the Bioma reference installation. MCP 2026-07-28, MCP Apps, Tasks,
subscriptions and the existing HTTP APIs keep their current contracts. SurrealDB
3.2.4 migration history records immutable SQL checksums.

## Accepted Decision

Developers customize Veoveo by forking this repository and integrating upstream
changes through reviewed merges. A fork owns its builds, tests, deployment
qualification and downstream data migrations. Veoveo retires independently
distributed extension packages and the extension compatibility matrix. Installation
configuration may live in a separate repository. Bioma is a reference configuration.

External MCP connections continue through gateway registration and policy. Source
contributions grant no runtime authority. Workloads keep separate deployment units
where security, GPU execution, scaling or release isolation requires them. Component
selection and immutable image reuse remain part of the build and deploy system.

## Implementation

- [x] Move reusable artifact and simulation types out of the extension contract.
- [x] Convert the extension Helm library into internal shared chart helpers without
  changing resource names, selectors, ownership or security settings.
- [x] Remove extension compatibility/release manifests, their publication commands,
  standalone gateway composition, and extension-only schemas and fixtures.
- [x] Simplify source publication around the fork checkout; keep installation
  configuration and retained per-image build revisions explicit.
- [x] Make in-repository Python development and image builds the documented path.
- [x] Add downstream migration identities to the existing store runner, preserving
  upstream history and rejecting drift, missing dependencies and ambiguous ordering.
- [x] Qualify downstream customization followed by an upstream merge with native
  tooling and isolated fixtures.
- [x] Replace extension integration documentation and update component designs,
  the code map, architecture decisions and contributor instructions.
- [ ] Record affected checks, build affected artifacts, deploy through Bioma GitOps
  and verify installed user journeys and workload health.

## Transition

The coordinated Bioma upgrade is a hard cut. Retained image/chart digests provide
deployment recovery. Internal release metadata changes do not authorize deleting
user data, regenerating agent identities or replacing retained volumes. Inventory
installed charts and in-repository consumers before removing their inputs. Old
development profiles and locks that encode extension releases require regeneration;
old schema versions must fail with an actionable diagnostic.

The upstream migration sequence and its applied checksums stay unchanged. Downstream
history is recorded separately. A fork must adapt conflicting schema changes before
merging upstream; separate migration identities cannot prove SQL compatibility.

## Acceptance

A developer can add a capability to a fork, test and deploy it, and integrate a later
upstream change without producing an extension-specific release artifact. Protocol,
authorization and GPU tests qualify the behavior they own. An unrelated component
deployment leaves pilot and simulator workloads untouched. Implementation and
deployment measurements will be recorded as each stage completes.

## Implementation Measurements

The initial removal moves artifact identity types to `deploy/contract` and simulation
qualification to `platform/runtimes/simulation/contract`. Application charts bundle
`deploy/helm/common`. A comparison against the prior source commit rendered 90 platform
objects and 10 UAV objects with identical resource bodies, including pod templates and
selectors. The extension release and compatibility publishers, gateway composer and
private-index fixture are removed. The Python template now owns its only image build.

Native Python checks completed in 1.22 s for the protocol fixture and 3.46 s for the
template. Deployment/runtime checks passed; eight tests require an explicit live-cluster
profile and remain separate integration acceptance. Two release-tool tests had stale
workload counts and omitted the managed-agent image inside the manager configuration;
the checks now account for that deployment model.

Disk inspection found 281 GiB free before testing and roughly 276 GiB afterward.
Docker's age-filtered dangling-image prune reclaimed no bytes. Active build caches and
running workloads were preserved. Python commands use `--directory`: `--project` alone
leaves pytest in the repository root and caused an avoidable broad collection during
this work. Stable commands are recorded only after implementation stops changing their
inputs to avoid stale receipts and repeated recorder overhead.

The full affected source suite passed 429 tests, with nine explicitly ignored
integration checks. Repository Python enforcement passed 120 tests against the local
SDK. Helm configuration smoke validates current working-tree files; immutable source
publication remains a separate check. This removes temporary Git clones from that
pre-commit smoke and avoids inspecting the previous commit's fixture paths.


SurrealDB 3.2.4 qualification now covers a fork migration followed by an upstream
advance, equal numeric versions in both histories, concurrent runners, rollback of
failed fork SQL, and drift rejection before pending upstream changes. The production
upgrade applies only migration 92 and preserves every earlier history row and checksum.
Disposable containers own their databases and cleanup; production data is untouched
by these tests. A missing downstream history table is accepted only before its
introducing upstream migration. After that point, absence fails validation.


The all-target workspace check found an existing missing `Utc` import in the speech
browser harness when reused by flight smoke. The shared module now qualifies that
reference directly. Release preflight passed with 262 GiB free, a 40 GiB build-growth
budget and a 183 GiB reserve. Kubernetes reports no disk pressure. The shared Bake and
lockfile changes conservatively select every image; deployment selection will retain
unchanged GPU payloads after reviewing their actual input diff.
