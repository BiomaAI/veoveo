# Shared Helm Helpers

## Standards And Protocols

Helm library charts supply named templates bundled into application chart archives.
Kubernetes application labels, Pod Security Standards and NetworkPolicy define the
rendered resource behavior. GPU helpers support the installation's Kubernetes DRA
claims. OCI digests identify application images. This library is an internal source
module and has no independent public API or OCI release.

## Ownership

Consumers provide the installation ID, component ID and Helm release identity.
Selectors use the consumer release; cross-chart policies use installation and
component labels. The library never selects another release's ownership labels.

Images resolve from an explicit digest, then the installation digest map, then a
source tag for development. Production requires a digest. Application values use
`global.veoveoRegistry`, `global.veoveoTag` and `global.imageDigests` where the release
publisher supplies them. Helper arguments are explicit dictionaries.

Security helpers retain UID 10001, a read-only root filesystem and restricted
container defaults. GPU helpers require the declared allocation and add no software
renderer fallback. Consumers choose resources and permitted network peers.

## Qualification

The chart-library consumer fixture checks selectors, trust input, restricted security
and rejection of mutable production images. The platform and UAV chart renders also
exercise the helpers. Renaming this source library must preserve resource names,
selectors, release ownership and security settings in those consuming charts.
