# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v5` | installation-repository profile with exact platform targets, independently versioned workload and extension sources, split Helm values ownership, explicit registry transport, and a managed GPU allocator closure |
| `veoveo.io/deployment-lock/v5` | immutable installation revision, registry transport, source-role, OCI image, chart, platform resolution, and GPU allocator artifacts |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| Docker Buildx Bake | one exact multi-target platform build plus source-owned workload and extension groups |
| Kubernetes/K3s v1.36.2 and Helm v4.2.3 | qualified DRA destination and ordered release inputs; process execution remains outside this crate |
| Kubernetes Dynamic Resource Allocation `resource.k8s.io/v1` | persistent `ResourceClaim` allocation, named requests, per-container claims, and distinct-device constraints |
| NVIDIA DRA Driver for GPUs Helm chart `0.4.1` and `resource.nvidia.com/v1beta1` | digest-locked standalone GPU allocation, full-GPU and MIG DeviceClasses, CDI preparation, and measured time-slicing configuration; GPU allocation and `TimeSlicingSettings` remain upstream technology-preview features |

## Responsibility

This crate owns the typed multi-source deployment profile, immutable deployment lock,
local registry declaration, controlled path resolution, platform component graph, and
pure validation used by operational tooling. It does not execute Git, Docker, Buildx,
k3d, Kubernetes, or Helm commands.

The installation repository owns the profile, registry selection, Kubernetes
destination, pre-Helm resources, and `installationValues` files. Each named source owns
its repository, independently resolved revision, source chart, `sourceValues`, and
non-platform Bake groups. Exactly one source has the `platform` role. Separately
selected Veoveo applications use `workload`; independently owned integrations use
`extension`. Their values contracts remain distinct.

An optional gateway activation binds one composed control-plane document, every
file-backed public JWKS or CA bundle it references, and one pre-existing confidential
Secret. Profile validation parses the complete typed document and public files before
cluster mutation. `profile-up` creates one immutable digest-qualified ConfigMap, proves
that the Secret contains every declared key without copying its values, and supplies the
exact activation revision to the platform Helm release. Repeating the command reuses the
same public bundle. A changed document or trust file creates a new bundle before rollout;
the installation-owned Secret is never rewritten by this path.

The lock records the exact installation-repository revision and registry transport
alongside source revisions, runnable platform-manifest digests, attested
publication-index digests, and chart-content digests. Helm consumes the runnable
digest. The publication digest retains the exact SBOM and provenance envelope emitted
by one release invocation. Local development may use source charts; production
composition replaces source coordinates with digest-addressed private OCI chart
coordinates.

Deployment v5 also carries the complete managed GPU allocator closure. The profile and
lock name the standalone NVIDIA chart, its OCI manifest digest, the downloaded archive
digest, the multi-platform driver image index, and each admitted platform manifest.
They select eligible nodes, a host driver root, a bounded Helm timeout, and one typed
removal of a conflicting device plugin. Validation accepts only the qualified
`0.4.1` release. This is a hard cut from deployment v4; an installation migrates by
adding `gpuScheduling.allocator.installation` and regenerating its lock.

The platform resolver expands `full`, `extension-foundation`, or a typed custom
selection. Gateway composition requirements fail closed against that graph. Artifact,
Frames, Map, Media, Optimization, Recording, and RRD requirements select their actual
hosted server and infrastructure dependencies; portable composition tools do not link
those server implementations. Optimization selects both its MCP control image and the
GPU cuOpt executor.

The component graph distinguishes the recording data plane and canonical
simulation-runtime support from hosted MCP servers and operator surfaces.
External workload identifiers remain source-owned but enter the same immutable
selection and deployment lock. A GPU scheduling profile groups named Deployments and
containers by physical-device identity. Separate constraints state which groups must
use different physical devices. Each workload declares its replica count, while each
group bounds all consumers. The profile also records the installation evidence digest
and the stable DRA claim identity.

`profile-up` first verifies Kubernetes, eligible Ready nodes, and the locked allocator
artifacts. It quiesces declared GPU Deployments only when configured removal of a
conflicting device plugin is actually required. The command then labels the selected
nodes, rejects undeclared device-plugin pods, and renders the verified chart archive.
The render must use the exact image, the platform-managed selector, and no required node
affinity. The managed selector is the sole admission predicate, so neither manual
hardware labels nor a separate node-discovery installation is required. The command
then atomically installs the chart-owned GPU kubelet plugin, RBAC, DeviceClasses, and
ResourceSlices. GPU allocation is explicitly enabled, `resource.k8s.io/v1` is fixed,
ComputeDomains are disabled, and the alpha `TimeSlicingSettings` feature gate is
enabled. A host-installed NVIDIA driver uses `nvidiaDriverRoot=/`; the platform never
replaces or upgrades that driver.

After install, `profile-up` reads the exact Helm v4 release row and verifies its
namespace, deployed status, positive revision, chart, and application version. The OCI
manifest, downloaded archive, and rendered image retain their independent digest
checks. It requires `gpu.nvidia.com` with its `nvidia.com/gpu` extended-resource bridge,
one desired, current, Ready, and available kubelet plugin per selected node, complete
ResourceSlice coverage, nonempty product names, unique physical UUIDs, and the declared
device count. The qualified integration baseline is Kubernetes/K3s v1.36.2, NVIDIA
driver 610.43.02, Container Toolkit package 1.19.1-1, and CDI-enabled containerd. The
Kubernetes and driver versions are checked exactly. The NVIDIA device plugin does not
remain on DRA-owned nodes.

The installer then compiles the provider-neutral topology into one
`resource.k8s.io/v1` ResourceClaim before Helm runs. Workloads in one group reference
the same claim request. Different groups are allocated atomically and use a
`distinctAttribute` constraint for shared devices. The claim persists through pod
replacement, node restart, and Helm upgrade. `profile-up` creates it only when absent;
an existing claim must retain its UID and match the canonical spec and evidence digest
exactly. Drift is reported without mutating or replacing the claim. NVIDIA full-device
and MIG DeviceClasses are implementation details selected by the installation. Measured
time slicing adds opaque driver configuration and requires its own evidence digest;
exclusive groups permit one consumer only.

Simulation applications are separate workload or extension sources. Each owns its
domain MCP server, authoritative simulator, cameras, encoded products, and GPU request.
The platform supplies only the selected shared services and canonical runtime support.
A profile whose physical-device groups exceed installation capacity fails during pure
profile resolution.

After rollout, `profile-up` reads the allocated claim and executes `nvidia-smi` inside
every declared GPU container. It reports the retained claim UID, allocated devices, and
the one visible physical UUID for each replica. Each GPU Deployment must retain the
exact replica count declared by the profile. Same-device drift, different-device drift,
a missing replica, or more than one visible device fails the command with the exact
workload and group.

The same resolution produces the exact Veoveo-owned OCI image closure. Platform
components contribute their runtime images, each selected MCP server contributes its
image, Recording contributes the hub and MCP images, and an RRD requirement contributes
the producer-side recording forwarder. Only targets from the explicit platform source
can satisfy this closure.

Operational tools derive the platform source targets from the exact typed selection and
resolve them in one Bake invocation. Platform profiles do not repeat that set through a
named image group. Other sources retain ordered repository-owned groups. Pure contract
validation rejects a target selected twice by one source, an OCI reference claimed by
two sources, an omitted platform target, or an unnecessary platform target. The
immutable lock also rejects repositories and Helm release identities owned by more than
one source. An extension cannot satisfy platform closure by copying a first-party
target name.

Local installation consumes that lock as an explicit input. The installer requires the
checked-out installation repository to match the locked revision and rejects changed or
untracked profile inputs. It checks out each recorded source revision, confirms the
normalized source origin, recomputes every source-chart archive digest, and compares the
locked image repositories with the exact Bake selection. Helm applies source values
first and installation values second, then receives a source-owned image-digest map
with production enforcement enabled. It does not resolve mutable source expressions
during installation.

The acceptance test creates independent platform, extension, and installation Git
repositories, resolves distinct commits, loads installation-owned Helm values from the
installation repository, validates the source-qualified exact image plan, and produces
one combined lock. It does not introduce an installation coordinator or prescribe the
extension's build system.
