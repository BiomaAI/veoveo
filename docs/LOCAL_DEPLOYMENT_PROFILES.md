# Local deployment profiles

Typed deployment profiles are a repository-development convenience for disposable
showcases. They select Docker Bake groups, a local k3d cluster, development Secrets,
and local Helm charts. Enterprise installations use the OCI and GitOps contract in
[Enterprise deployment](ENTERPRISE_DEPLOYMENT.md).

The current complete profile is the SUMO development environment:

| Concern | Canonical owner |
|---|---|
| Image definitions and reusable groups | docker-bake.hcl |
| Local image destination | k3d-veoveo-registry.localhost:5001/veoveo/image:git-sha |
| Platform workload graph | deploy/helm/veoveo |
| Showcase workload graph | Its adjacent Helm chart |
| Development composition | A `veoveo.io/deployment/v2` named-source JSON profile |
| Local registry lifecycle | deploy/local/k3d/registry.json |

## Workflow

Commit source before publishing. The publisher resolves the requested revision to a
full commit SHA and moves a locked persistent publication worktree to that commit.
Unchanged source paths retain their metadata, while local edits cannot change bytes
published under another revision.

~~~bash
PROFILE=showcase/sumo/deploy/deployment.json
REVISION=$(git rev-parse HEAD)

cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask image builder ensure
cargo xtask release images --profile "$PROFILE" --profile-revision "$REVISION"
cargo xtask smoke profile-up --profile "$PROFILE"
~~~

BuildKit pushes images directly to the shared local OCI registry. It does not load
release images into the host Docker image store. Ordered image groups publish a
heavyweight shared base before targets that consume it, while independent targets in
one group build concurrently.

Each compatible Rust family compiles its selected binaries in one Cargo invocation.
Target caches derive from source identity, builder family, platform, and profile.
Builders with a different operating-system ABI or native SDK use separate identities.

## Contract

Installation-owned paths resolve relative to the profile. Source-owned chart and values
paths resolve inside that source's exact checkout. The fields are:

| Field | Meaning |
|---|---|
| schemaVersion | `veoveo.io/deployment/v2` |
| name | Stable local environment identity |
| registry.address | OCI host and port |
| registry.localConfig | Shared k3d registry definition |
| sources | Named repositories with explicit platform or extension ownership, independent revisions, image groups, and releases |
| kubernetes.context | Explicit kubectl and Helm context |
| kubernetes.localCluster | k3d configuration and node bootstrap manifests |
| namespace | Namespace for local resources |
| resources.manifests | Kubernetes resources applied before Helm |
| resources.configMaps | File-backed development ConfigMaps |
| resources.secrets | Environment-backed development Secrets |
| platform | Typed installation preset or exact components, MCP servers, and artifact audiences |
| gatewayRequirements | Composer outputs that the selected runtime must satisfy |
| waitForDeployments | Extra rollout gates |

`cargo xtask release images --profile` resolves every source revision independently.
It resolves the complete source-qualified image plan before pushing, rejects duplicate
repository/tag references, and accepts platform-image closure only from the single
`platform` source. It then publishes each source's Bake groups under that revision and writes one
`veoveo.io/deployment-lock/v2` document with source repositories, exact revisions,
image manifest digests, chart-content digests, and the expanded platform graph.

The `extension-foundation` preset selects the gateway, platform store, object store,
artifact service, Artifact MCP, Frames MCP, and Recording MCP/hub. A custom selection
can add Map and Media. If a gateway fragment requires either capability and the
corresponding server is absent, profile validation fails before Helm runs. `rrd`
requires the Recording MCP and hub because that runtime owns governed RRD playback,
and adds `recording-forwarder` to the required image closure for producer-side
transport. Profile validation and publication reject Bake groups that omit a required
target.

Secret values pass to Kubernetes over stdin. The JSON file contains environment
variable names, not bytes. This mechanism is confined to local development; enterprise
Secrets are projected by the owner's secret-management platform.

## Registry and GPU

One standalone OCI Distribution registry serves all local k3d clusters at host port
5001. Nodes pull missing layers into their containerd store through the registry.
Deleting a cluster leaves the shared registry volume intact.

A profile applies the NVIDIA device-plugin bootstrap and waits for allocatable
nvidia.com/gpu capacity. The local workflow fails before application installation
when the GPU contract is unavailable.

Use the profile lifecycle commands for the SUMO environment:

~~~bash
cargo xtask smoke profile-cluster-stop --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask smoke profile-down --profile "$PROFILE"
cargo xtask smoke profile-cluster-delete --profile "$PROFILE"
~~~

A new local showcase may add an image group and adjacent Helm chart, then select those
surfaces from a profile. A fielded installation may keep deployment v2 selection in its
private configuration repository, but it consumes published OCI artifacts rather than
building from a checkout inside the cluster.
