# Local deployment profiles

Typed deployment profiles are a repository-development convenience for disposable
showcases. They select Docker Bake groups, a local k3d cluster, development Secrets,
and local Helm charts. Enterprise installations use the OCI and GitOps contract in
[Enterprise deployment](ENTERPRISE_DEPLOYMENT.md).

The current complete profile is the SUMO development environment:

| Concern | Canonical owner |
|---|---|
| Image definitions and reusable groups | docker-bake.hcl |
| Local image destination | Profile-selected registry host and port with revision-addressed image tags |
| Platform workload graph | deploy/helm/veoveo |
| Showcase workload graph | Its adjacent Helm chart |
| Development composition | A `veoveo.io/deployment/v3` installation-repository JSON profile |
| Local registry lifecycle | deploy/local/k3d/registry.json |

## Workflow

Commit source before publishing. The publisher resolves the requested revision to a
full commit SHA and moves a locked persistent publication worktree to that commit.
Unchanged source paths retain their metadata, while local edits cannot change bytes
published under another revision.

~~~bash
PROFILE=showcase/sumo/deploy/deployment.json
LOCK=output/deployments/sumo/deployment.lock.json
REVISION=$(git rev-parse HEAD)

cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output "$LOCK"
cargo xtask smoke profile-up --profile "$PROFILE" --lock "$LOCK"
~~~

BuildKit pushes images directly to the profile-selected OCI registry. It does not load
release images into the host Docker image store. The publisher configures the managed
builder from `registry.address` and `registry.transport`; an insecure development
registry may use any declared host port. A transport change preserves the builder state
and its cache.

The exact typed platform closure runs as one multi-target Bake invocation. Bake retains
the shared dependency graph and Cargo family consolidation inside that invocation.
Workload and extension sources publish their repository-owned groups as separate
phases.

Each compatible Rust family compiles its selected binaries in one Cargo invocation.
Target caches derive from source identity, builder family, platform, and profile.
Builders with a different operating-system ABI or native SDK use separate identities.

## Contract

Installation-owned paths resolve relative to the profile. Source-owned chart and values
paths resolve inside that source's exact checkout. The fields are:

| Field | Meaning |
|---|---|
| schemaVersion | `veoveo.io/deployment/v3` |
| name | Stable local environment identity |
| registry.address | OCI host and port |
| registry.transport | `tls` or explicitly admitted `insecure-http` |
| registry.localConfig | Shared k3d registry definition |
| sources | Named repositories with `platform`, `workload`, or `extension` ownership and independent revisions |
| sources[].imageGroups | Ordered source-owned phases for workload and extension sources; prohibited on the platform source |
| sources[].releases[].sourceValues | Helm values resolved from the exact source checkout |
| sources[].releases[].installationValues | Later Helm overrides resolved from the installation repository |
| kubernetes.context | Explicit kubectl and Helm context |
| kubernetes.localCluster | k3d configuration and node bootstrap manifests |
| namespace | Namespace for local resources |
| resources.manifests | Kubernetes resources applied before Helm |
| resources.configMaps | File-backed development ConfigMaps |
| resources.secrets | Environment-backed development Secrets |
| platform | Typed installation preset or exact components, MCP servers, and artifact audiences |
| gatewayRequirements | Composer outputs that the selected runtime must satisfy |
| waitForDeployments | Extra rollout gates |

`cargo xtask release images --profile` accepts a profile inside another Git repository.
`--profile-revision` names that installation repository's exact commit. The command
requires the checked-out profile and every referenced installation input to match that
commit, then resolves each source revision independently.

The publisher derives only the required platform targets, rejects missing or
unnecessary platform images and duplicate repository/tag references, and executes the
platform set once. Workload and extension groups remain source-owned. It writes one
`veoveo.io/deployment-lock/v3` document with the installation revision, registry
transport, source repositories and revisions, image manifest digests, chart-content
digests, and expanded platform graph.

`cargo xtask smoke profile-up` requires that lock. It verifies the installation
revision and referenced profile files, checks out each source at the recorded revision,
and verifies its origin, exact Bake repositories, and source-chart archive digest. Helm
receives source values followed by installation-owned overrides and the source-owned
digest map in production mode. Installation never re-resolves `HEAD`, a branch, or
another mutable source expression.

The `extension-foundation` preset selects the gateway, platform store, object store,
artifact service, Artifact MCP, Frames MCP, and Recording MCP/hub. A custom selection
can add Map and Media. If a gateway fragment requires either capability and the
corresponding server is absent, profile validation fails before Helm runs.
`optimization` requires Optimization MCP and the cuOpt GPU executor. `rrd`
requires the Recording MCP and hub because that runtime owns governed RRD playback,
and adds `recording-forwarder` to the required image closure for producer-side
transport. Profile validation and publication reject a platform selection that omits a
required target or introduces an unnecessary one.

Secret values pass to Kubernetes over stdin. The JSON file contains environment
variable names, not bytes. This mechanism is confined to local development; enterprise
Secrets are projected by the owner's secret-management platform.

## Registry and GPU

One standalone OCI Distribution registry may serve several local k3d clusters. Its host
port comes from `registry.address` and the matching local registry declaration; it is
not a Veoveo constant. Nodes pull missing layers into their containerd store through
the registry. Deleting a cluster leaves the shared registry volume intact.

A profile applies the NVIDIA device-plugin bootstrap and waits for allocatable
nvidia.com/gpu capacity. The local workflow fails before application installation
when the GPU contract is unavailable.

Local k3d schedulers do not share GPU allocation state. A single-GPU
workstation therefore runs one GPU-bearing profile cluster at a time. Stop a
completed profile before starting GPU acceptance in another profile; do not
remove required workloads from either profile to make them overlap.

Use the profile lifecycle commands for the SUMO environment:

~~~bash
cargo xtask smoke profile-cluster-stop --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask smoke profile-down --profile "$PROFILE"
cargo xtask smoke profile-cluster-delete --profile "$PROFILE"
~~~

A new local showcase may add an image group and adjacent Helm chart, then select those
surfaces through a `workload` source. A fielded installation may keep deployment v3
selection in its private configuration repository, but it consumes published OCI
artifacts rather than building from a checkout inside the cluster.
