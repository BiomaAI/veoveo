# Extension Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| JSON Schema 2020-12 | generated schemas for every exported controlled document |
| Semantic Versioning 2.0.0 | extension, compatibility-release, and artifact versions |
| OCI Distribution Specification | private artifact coordinates and immutable distribution |
| SHA-256 | lowercase digest identity in `sha256:<64 hexadecimal digits>` form |
| `veoveo.io/compatibility-manifest/v1` | Veoveo-supported release tuple |
| `veoveo.io/extension-release/v1` | independently owned extension release |

## Responsibility

This crate owns types, validation, and JSON Schema generation for artifacts crossing an
external repository boundary. It does not own gateway fragments, installation
bindings, or deployment profiles. Those types remain in `mcp/contract` and
`deploy/contract`.

The crate performs no network, registry, Git, Helm, or Kubernetes operations.
Operational commands consume these types and keep process execution in `xtask`.

## Invariants

- Every production artifact has an immutable coordinate and SHA-256 digest.
- Every release version parses as Semantic Versioning 2.0.0.
- Source revisions are full lowercase SHA-1 or SHA-256 object identifiers.
- Compatibility contract kinds are unique.
- SDK language and artifact pairs are unique.
- The standalone conformance and gateway-composer distributions are immutable.
- Runtime profiles identify one immutable base image and conformance result.
- Extension releases contain a chart, gateway fragment, and conformance evidence.
- Installation authorization and Secret values never enter an extension release.
