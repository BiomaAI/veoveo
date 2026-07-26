# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v2` | named-source deployment profile with independent Git revisions |
| `veoveo.io/deployment-lock/v2` | immutable source, OCI image, chart, and platform resolution |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| Docker Buildx Bake | image-group names selected by a deployment profile |
| Kubernetes and Helm | typed destination and ordered release inputs; process execution remains outside this crate |

## Responsibility

This crate owns the typed multi-source deployment profile, immutable deployment lock,
local registry declaration, controlled path resolution, platform component graph, and
pure validation used by operational tooling. It does not execute Git, Docker, Buildx,
k3d, Kubernetes, or Helm commands.

Each named source owns its repository, independently resolved revision, Bake phases,
and Helm releases. The lock records the resolved repository and revision with image
manifest digests and chart-content digests. Local development may use source charts;
production composition replaces source coordinates with digest-addressed private OCI
chart coordinates.

The platform resolver expands `full`, `extension-foundation`, or a typed custom
selection. Gateway composition requirements fail closed against that graph. Artifact,
Frames, Map, Media, Recording, and RRD requirements select their actual hosted server
and infrastructure dependencies; portable composition tools do not link those server
implementations.
