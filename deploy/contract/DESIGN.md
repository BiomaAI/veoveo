# Deployment Contract Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/deployment/v7` | installation-repository profile with exact platform targets, independently versioned workload and extension sources, split Helm values ownership, explicit host-push and cluster-pull registry endpoints, and a managed GPU allocator closure |
| `veoveo.io/deployment-lock/v7` | immutable installation revision, registry endpoints and transport, source-role, OCI image, chart, platform resolution, and GPU allocator artifacts |
| `veoveo.io/local-registry/v1` | repository-owned loopback registry declaration |
| `veoveo.io/image-release-evidence/v3` | one publication snapshot, typed registry endpoints, per-image build revision, runnable manifest digest, and attested publication index digest shared by publication and compatibility generation |
| `veoveo.io/gateway-activation/v1` | SHA-256 over a domain prefix and the sorted, length-prefixed UTF-8 ConfigMap data keys and values; covers the complete public gateway bundle |
| `veoveo.io/component-mutation-plan/v2` | internal preflight evidence for exact atomic targets and installation snapshots; it records allowed actions and does not attest to executed writes |
| `veoveo.io/component-installation/v2` | successful disposable installation receipt: mutation plan, applied or reused units, unselected object and Helm observations, and exact released cluster-coordination identity |
| `veoveo.io/installed-deployment-unit/v1` | typed local provenance and observed object fingerprints for verified installation reuse; contains no object bodies or Secret values |
| `veoveo.io/atomic-deployment-unit/v2` | repository-owned SHA-256 identity over typed source, installation snapshot, target, input closure, and sorted rendered object digests |
| `veoveo.io/atomic-deployment-content/v2` | repository-owned SHA-256 identity of the same deployable contents with source and installation revisions and extension release provenance excluded; used only after exact lock validation |
| `veoveo.io/source-chart-content/v1` | SHA-256 over sorted chart-relative file paths, Git executable modes, and exact file bytes in a verified source checkout; commit metadata and archive export attributes do not enter this identity |
| `veoveo.io/extension-release/v1` and Semantic Versioning 2.0.0 | component references retain the extension ID, exact release version, and manifest digest using the shared extension contract's validated types |
| Docker Buildx Bake | one exact multi-target platform build plus source-owned workload and extension groups |
| Kubernetes/K3s v1.36.2 and Helm v4.2.3 | qualified DRA destination and ordered release inputs; process execution remains outside this crate |
| Kubernetes core `v1`, apps `v1`, and batch `v1` | Secret references in Pods and pod templates, including environment variables, image pulls, and volume projections |
| Kubernetes Ingress `networking.k8s.io/v1` | TLS Secret references |
| Kubernetes Gateway API `gateway.networking.k8s.io/v1` | listener certificate references to core Secrets; other certificate kinds remain outside this profile |
| Kubernetes Dynamic Resource Allocation `resource.k8s.io/v1` | persistent `ResourceClaim` allocation, named requests, per-container claims, and distinct-device constraints |
| NVIDIA DRA Driver for GPUs Helm chart `0.5.0` and `resource.nvidia.com/v1beta1` | digest-locked standalone GPU allocation, full-GPU and MIG DeviceClasses, CDI preparation, and measured time-slicing configuration; GPU allocation and `TimeSlicingSettings` remain upstream technology-preview features |

## Responsibility

This crate owns the typed multi-source deployment profile, immutable deployment lock,
local registry declaration, controlled path resolution, platform component graph, and
pure validation used by operational tooling. It does not execute Git, Docker, Buildx,
k3d, Kubernetes, or Helm commands.

The sibling `../runtime` crate owns shared execution for the release publisher and the
disposable profile installer. It consumes this crate's contracts and digest encodings.

## Atomic Ownership Planner

`src/components/` implements the pure preflight boundary for `DEPLOY-SCOPE-023`.
The disposable profile compiler and installer consume deployment v7 with mandatory
component ownership and compiled inventories. Installation requires explicit full or
component selection and expands declared dependencies before source resolution. The
runtime uses typed local receipts to verify reuse and feeds checked installed observations
into the planner before executing the expanded selection.
The internal planner is not an alternative installer or an enterprise mutation owner.

Each component declares its immutable source, role, dependencies, exact Helm release
identities or explicit manifest sets, namespaces, permitted objects, and complete input
closure. Extension components also carry an exact extension-release identity. Source
revisions and artifact digests use the existing extension contract's validated types.
The SHA-256 implementation uses `sha2 =0.11.0`, the latest stable version verified from
the [upstream crate release metadata](https://crates.io/api/v1/crates/sha2) on September 7,
2026. This adds no new algorithm or wire digest format.

The catalog owns every object by API group, kind, namespace, and name. Served API
versions do not create different identities. Reserved identities remain owned when an
object is absent from the current render. The validator rejects overlapping owners and
targets across the complete catalog before selection. One immutable input has one
content digest at each source revision, and every image repository retains one
source-qualified owner even when several components consume it. A source name identifies
one repository throughout the catalog. Components and inputs from that repository can
retain different immutable revisions. Advancing a component does not change the recorded
provenance of an unchanged image built from an earlier commit.

The complete lock binds each component image to its source repository, target, runnable
digest, and per-image `sourceRevision`. Charts bind to the source-owned release and
artifact digest. Values come from the component's source snapshot or the installation
repository. Every Helm unit consumes exactly one chart. Managed allocator inputs match
the installation's pinned chart and image closure.

Every component also records `configuration`: the installation repository, immutable
revision, and repository-relative profile path used for its render. Installation-owned
files must belong to that snapshot. The runtime verifies the document and referenced
files against Git before rendering. A retained component can therefore use an older
configuration even when the current profile has replaced a values file. Configuration
revisions participate in exact provenance; unchanged configuration contents do not force
a mutation. The v7 migration now requires this field and v2 unit digests. Earlier
generated locks require regeneration; no omitted-field default is supported.

The runtime resolves source charts by the component's complete source identity. The
top-level source revision records publication resolution; it cannot replace a retained
component revision during installation.

The artifact catalog retains qualified image versions by source, target, and build
revision. A target keeps one repository across revisions, and that repository keeps one
source-qualified target owner. Repeating a build revision is ambiguous and fails
validation. Each component input binds the exact version, including when two builds have
identical runnable bytes. Runnable and attested publication digests remain distinct.

Profile binding checks every owner's role, dependencies, extension identity, namespaces,
atomic operations, and cluster permissions. Namespaced permissions come from the complete
locked inventory. These checks also cover unselected owners without evaluating their
templates or cloning their repositories. Source origin and actual selected input bytes
remain runtime checks.

Historical Helm manifests and hooks also belong to the mutation boundary, including
the deployed and successful revisions Helm may use during recovery. Their namespaced
objects can retire within the original component's declared namespaces. Historical
cluster objects still require explicit permission. The complete current catalog
rejects a historical object reserved by another owner or assigned to another atomic
target. This permits ordinary Helm deletion of an obsolete object without authorizing
an ownership transfer. The runtime must establish the stored release provenance before
passing historical identities to this pure check.

Dependency closure includes the namespace owner before source releases and namespaced
installation operations. Placement depends on the allocator owner. When node bootstrap
is declared, the allocator also depends on that owner. A component may own its own
prerequisites; otherwise the dependency must be reachable in the declared graph.

Selection names exact component IDs and expands dependencies in deterministic order.
The renderer supplies every expanded atomic target and must not render unselected
targets. Preflight compares each complete rendering with the lock. Its unit digest binds
source identity, extension identity, target, complete inputs, and sorted object digests.
An omitted input or changed object cannot reuse a previous digest. Catalog validation
also recomputes stored unit digests instead of trusting them.

Each unit also records a content digest. This digest retains the component ID and role,
source name and repository, atomic target, extension ID, complete input contents, and
sorted object digests. It omits source revisions and the extension release version and
manifest digest. Any release metadata that affects deployment must enter the actual
input closure or rendered objects. Input projections are sorted after removing revisions,
and identical projections are deduplicated. The full unit digest preserves all exact
provenance for lock validation and receipts. Both digests must match the desired render
before an executor can use the content comparison to skip an upgrade.

Every expanded target needs an explicit current-state observation. The observation
distinguishes absence from verified installed provenance, content digest, and object
inventory. A dependency receives an `unchanged` action when its content digest and
complete object inventory match, including when its desired provenance has advanced.
Matching provenance with a conflicting content digest fails as an inconsistent
observation. Previous Helm objects
remain part of the ownership check because an upgrade may delete them. Ownership cannot
move between atomic targets during an update, including targets within one component.
Such a release split needs a separate explicit migration.
An absence observation covers both the release and its desired objects. A missing Helm
release does not authorize adopting an existing object's ownership.
The plan records retired Helm objects that an upgrade may remove. Raw apply has no
removal verb, so a raw manifest set that retires existing objects requires an explicit
removal migration instead of silently leaving an incomplete desired state.

The renderer and executor retain responsibilities that pure types cannot prove. They
resolve namespace and scope through Kubernetes discovery, hash complete manifest bytes,
verify all actual image and values inputs, and preserve checked inputs through execution.
Hooks, CRDs, generated resources, allocator releases, node bootstrap, and raw installation
objects must enter the same ownership boundary. Secret-reference preflight remains a
separate mandatory gate; components cannot declare Secret objects. The executor must
establish actual current state, prevent concurrent ownership changes, invoke only planned
atomic targets, and record actual writes and Helm revision changes. A stored label alone
does not prove that current objects match a unit digest.

`ComponentMutationPlan` contains source identities, object digests, planned verbs, and
unselected object identities. It contains no manifests or Secret values. Its unselected
inventory describes the ownership boundary; it is not zero-write evidence. Focused tests
prove selection and rejection rules. Independent temporary platform and extension Git
histories exercise revision reuse and committed input reads in the pure planner. Actual
execution receipts and the independent native request observer belong to the runtime
and deployment smoke harness respectively.

`InstallationReceipt` records the successful plan and every executed atomic target once,
with `applied` or `reused` outcomes. Its unselected observations contain UID, resource
version, and a complete-object digest, or explicit absence. Helm observations contain
revision, status, chart, and app version. The receipt contains no object bodies or
Secret values. These observations may change because another controller acts during
installation; they are evidence rather than a cluster-wide transaction or lock.
Receipt equality alone cannot prove that no API write occurred.

Observed units distinguish absence, a verified installed baseline, and `RequiresApply`.
The last state contains checked object inventory without asserting an installed-input
digest. Missing receipts, observed drift, or an earlier planned mutation can prevent
reuse even when an object exists. An empty inventory is valid for an existing Helm
release; an empty raw manifest observation must use absence. `RequiresApply` always
plans the target's apply operation and undergoes the same retirement and ownership
checks as a verified baseline. The observation enum is internal and does not change
the serialized plan schema.

`InstalledUnitReceipt` binds one locked atomic unit and its component declaration to a
cluster UID. Its validator recomputes provenance and content digests and requires exactly
one observation for each rendered object. Helm units require a positive release revision
and canonical manifest digest. A present observation contains the actual object UID and
fingerprint. Only a Job can record completed TTL evidence; completed-hook observations
require Helm revision evidence. The runtime must establish those observations from the
API and Helm's execution record. The schema does not authenticate a local receipt or
prove the absence of concurrent writes. Runtime storage and reuse rules belong in
[`../runtime/DESIGN.md`](../runtime/DESIGN.md).

## Deployment Profile

Both standard presets include the core `computers` MCP server. Custom partial
selections require the gateway and platform store when they include it. Its control
image closure is `computers-mcp`; provider, storage and Computer template images
belong to the configured capacity topology. An explicitly unconfigured core surface
does not select privileged host workloads. The matching chart boundary is documented
in [`../helm/veoveo/DESIGN.md`](../helm/veoveo/DESIGN.md).

The installation repository owns the profile, registry selection, Kubernetes
destination, pre-Helm resources, and `installationValues` files. Each named source owns
its repository, independently resolved revision, source chart, `sourceValues`, and
non-platform Bake groups. Exactly one source has the `platform` role. Separately
selected Veoveo applications use `workload`; independently owned integrations use
`extension`. Their values contracts remain distinct.

Profile loading validates declarations and installation-owned files. It defers source
filesystem checks to immutable snapshot resolution. The selected source footprint
requires exact component IDs and an already expanded dependency set. It keeps whole
catalog validation while limiting source checkouts and chart input inspection to the
selected releases. Publication and installation use this same profile loader.

The installation owner supplies every Secret through its own reconciliation path.
Deployment profiles do not create, patch, replace, copy, or transfer ownership of a
Secret. Raw profile and rendered Helm objects that define a Secret fail before mutation.

`profile-up` renders the expanded selection's locked Helm and raw-manifest closure before its first
Kubernetes or Helm write. The pure contract extracts Secret references from container
environments, image pulls, volumes, Ingress TLS, Gateway listeners, and registered
custom-resource shapes. An unregistered Secret-bearing custom resource makes the
closure unverifiable. Presence checks retain the Secret name, declared key names, and a
bounded terminal status. Values are discarded and never enter evidence or diagnostics.
Missing, forbidden, timed-out, malformed, and transport-failed observations remain
distinct fail-closed results.

An optional gateway activation binds one composed control-plane document, every
file-backed public JWKS or CA bundle it references, and one pre-existing confidential
Secret. Profile validation parses the complete typed document and public files. The
Secret and every required key enter the same rendered closure. After that gate succeeds,
`profile-up` creates one immutable digest-qualified ConfigMap and supplies the exact
activation revision to the platform Helm release. Repeating the command reuses the same
public bundle. A changed document or trust file creates a new bundle before rollout.
The platform chart requires the explicit bundle digest when gateway is selected.
The same value determines gateway rollout and participates in the complete bootstrap
Job spec digest. Helm release counters and cluster reads do not determine those inputs.
`gateway_bundle_digest` owns the encoding for disposable profiles and GitOps
installations. The domain prefix is `veoveo.io/gateway-activation/v1` followed by a zero
byte. Each sorted UTF-8 key and value is preceded by its byte length as an unsigned
64-bit big-endian integer. `gateway.controlPlaneRevision` contains the resulting 64
lowercase hexadecimal characters without the `sha256:` prefix. Exact file bytes,
including final newlines, participate. Bioma acceptance hashes the complete rendered
ConfigMap data with this function and compares its declared revision.

The lock records the exact installation-repository revision, host-push endpoint,
cluster-pull endpoint, and registry transport
alongside source revisions, runnable platform-manifest digests, attested
publication-index digests, and chart-content digests. Helm consumes the runnable
digest. The publication digest retains the exact SBOM and provenance envelope emitted
by one release invocation. Local development may use source charts; production
composition replaces source coordinates with digest-addressed private OCI chart
coordinates.

Source chart digests use `source_chart_content_digest` in both publication and
installation. The resolver first creates the verified immutable source checkout. The
digest then covers every regular file under the declared chart directory, including
files that Git export attributes would omit from an archive. Symlinks and special
files fail validation. Source-relative chart paths cannot traverse outside that tree.
The shared deployment runtime compares the complete chart inventory, actual file
bytes, and executable modes with the Git tree before constructing a lock. It also
checks source-owned values files. Git index hints and clean filters cannot substitute
committed bytes for files Helm will read. The caller preserves the verified checkout
through rendering.

The encoding starts with `veoveo.io/source-chart-content/v1` and a zero byte. Files are
sorted by their UTF-8 chart-relative path. Each record contains the path's unsigned
64-bit big-endian byte length, the path bytes, one executable-mode byte, the file's
unsigned 64-bit big-endian byte length, and its exact bytes. The mode byte is one when
any Unix executable permission is set and zero otherwise. Filesystem timestamps,
owner IDs, and read/write permission bits do not affect the digest. `Chart.yaml` is
required. The digest implementation requires a filesystem that retains executable
modes; it cannot silently invent those modes on another host.

This replaces the previous commit-archive hashing rule. Existing source-chart lock
entries must be regenerated from their recorded source revisions. The installer rejects
archive-derived digests rather than accepting two encodings. The profile and lock field
shapes remain unchanged. OCI chart digests continue to identify their published
artifacts and do not use the source-tree encoding.

Deployment v6 also carries the complete managed GPU allocator closure. The profile and
lock name the standalone NVIDIA chart, its OCI manifest digest, the downloaded archive
digest, the multi-platform driver image index, and each admitted platform manifest.
They select eligible nodes, a host driver root, a bounded Helm timeout, and one typed
removal of a conflicting device plugin. Validation accepts only the qualified
`0.5.0` release. This is a hard cut from deployment v5; an installation replaces
`registry.address` with `registry.pushAddress` and `registry.pullAddress`, then
regenerates its lock.

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
domain MCP server, authoritative simulator, logical cameras, one native GPU product per
streamable camera, shared H.264 fanout, stream endpoint, and GPU request. The
platform supplies only the selected shared services and canonical runtime support. It
does not install a shared simulation renderer, media relay, pose mirror, or live-view
reconciliation controller. A profile whose physical-device groups exceed installation
capacity fails during pure profile resolution.

After rollout, `profile-up` reads the allocated claim and executes `nvidia-smi` inside
each selected GPU container. Full installation and `profile-gpu-verify` cover every
configured consumer. It reports the retained claim UID, allocated devices, and
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
normalized source origin, recomputes every source-chart content digest, and compares the
locked image repositories with the exact Bake selection. Helm applies source values
first and installation values second. Platform and Veoveo-source values contracts
receive only their chart-owning source's digest map. An extension values contract
receives the complete, collision-checked deployment image closure, which lets a separate
release consume a platform-owned support image without copying or republishing it. The
platform chart's closed image schema never receives extension image keys, and the lock
retains one source owner for every repository. The installer does not resolve mutable
source expressions during installation.

The acceptance test creates independent platform, extension, and installation Git
repositories, resolves distinct commits, loads installation-owned Helm values from the
installation repository, validates the source-qualified exact image plan, and produces
one combined lock. It does not introduce an installation coordinator or prescribe the
extension's build system.
