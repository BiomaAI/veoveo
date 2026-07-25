# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v1` | repository-owned local development profile |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| Docker Buildx Bake | image-group names selected by a deployment profile |
| Kubernetes and Helm | typed destination and ordered release inputs; process execution remains outside this crate |

## Responsibility

This crate owns the typed deployment profile, local registry declaration, controlled
path resolution, and pure repository validation used by operational tooling. It does not
execute Git, Docker, Buildx, k3d, Kubernetes, or Helm commands.

The profile is an internal repository adapter. Enterprise installation and external
extension contracts remain owned by the deployment and extension designs.
