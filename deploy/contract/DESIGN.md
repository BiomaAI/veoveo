# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v4` | installation-repository profile with exact platform targets, independently versioned workload and extension sources, split Helm values ownership, and explicit registry transport |
| `veoveo.io/deployment-lock/v4` | immutable installation revision, registry transport, source-role, OCI image, chart, and platform resolution |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| Docker Buildx Bake | one exact multi-target platform build plus source-owned workload and extension groups |
| Kubernetes and Helm | typed destination and ordered release inputs; process execution remains outside this crate |
| Kubernetes NVIDIA device resources | exact exclusive-device accounting plus evidence-gated NVIDIA MIG or time-slicing declarations |

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

The platform resolver expands `full`, `extension-foundation`, or a typed custom
selection. Gateway composition requirements fail closed against that graph. Artifact,
Frames, Map, Media, Optimization, Recording, and RRD requirements select their actual
hosted server and infrastructure dependencies; portable composition tools do not link
those server implementations. Optimization selects both its MCP control image and the
GPU cuOpt executor.

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
