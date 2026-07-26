# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v2` | named-source deployment profile with independent Git revisions |
| `veoveo.io/deployment-lock/v2` | immutable source, OCI image, chart, and platform resolution |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| Docker Buildx Bake | image-group names selected by a deployment profile |
| Kubernetes and Helm | typed destination and ordered release inputs; process execution remains outside this crate |
| Kubernetes NVIDIA device resources | exact exclusive-device accounting plus evidence-gated NVIDIA MIG or time-slicing declarations |

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

The component graph distinguishes the recording data plane, hardware GPU renderer, and
canonical simulation-runtime support from hosted MCP servers and operator surfaces.
External workload identifiers remain source-owned but enter the same immutable
selection and deployment lock. A GPU scheduling profile names every selected GPU
workload, its device count, and its isolation. Exclusive requests consume physical
devices directly. NVIDIA MIG and time-slicing require a digest-addressed measurement
record; configuration alone never makes sharing valid.

Simulation View selects Frames MCP, its provider-neutral MCP server, the canonical
runtime support component, and one renderer GPU. A profile that also places an external
simulator on an ordinary one-GPU node with both workloads marked exclusive fails during
pure profile resolution.
