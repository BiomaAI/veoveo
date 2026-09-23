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

- [ ] Move reusable artifact and simulation types out of the extension contract.
- [ ] Convert the extension Helm library into internal shared chart helpers without
  changing resource names, selectors, ownership or security settings.
- [ ] Remove extension compatibility/release manifests, their publication commands,
  standalone gateway composition, and extension-only schemas and fixtures.
- [ ] Simplify source publication around the fork checkout; keep installation
  configuration and retained per-image build revisions explicit.
- [ ] Make in-repository Python development and image builds the documented path.
- [ ] Add downstream migration identities to the existing store runner, preserving
  upstream history and rejecting drift, missing dependencies and ambiguous ordering.
- [ ] Qualify downstream customization followed by an upstream merge with native
  tooling and isolated fixtures.
- [ ] Replace extension integration documentation and update component designs,
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
