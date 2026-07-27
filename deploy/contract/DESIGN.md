# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v2` | named-source deployment profile with one explicit platform source and independently versioned extension sources |
| `veoveo.io/deployment-lock/v2` | immutable source-role, OCI image, chart, and platform resolution |
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
and Helm releases. Exactly one source has the `platform` role. Every other source has
the `extension` role and can use only the extension chart-values contract. The lock
retains that ownership boundary with the resolved repository and revision, image
manifest digests, and chart-content digests. Local development may use source charts;
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

Simulation View selects Frames MCP, its provider-neutral MCP server, the Artifact
service with the `simulation-view` audience, the canonical runtime support component,
and one renderer GPU. A profile that also places an external simulator on an ordinary
one-GPU node with both workloads marked exclusive fails during pure profile resolution.

The same resolution produces the exact Veoveo-owned OCI image closure. Platform
components contribute their runtime images, each selected MCP server contributes its
image, Recording contributes the hub and MCP images, and an RRD requirement contributes
the producer-side recording forwarder. Only targets from the explicit platform source
can satisfy this closure.

Operational tools resolve every source-qualified target and final repository/tag
reference before publication begins. Pure contract validation rejects a target selected
twice by one source, an OCI reference claimed by two sources, or an omitted platform
target. The immutable lock also rejects repositories and Helm release identities owned
by more than one source. An extension cannot satisfy platform closure by copying a
first-party target name.

The acceptance test creates independent platform, extension, and installation Git
repositories, resolves distinct commits, validates the source-qualified image plan, and
produces one combined lock. It does not introduce an installation coordinator or
prescribe the extension's build system.
