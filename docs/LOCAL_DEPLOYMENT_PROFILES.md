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
| Development composition | A veoveo.io/deployment/v1 JSON profile |
| Local registry lifecycle | deploy/local/k3d/registry.json |

## Workflow

Commit source before publishing. The publisher resolves the requested revision to a
full commit SHA and builds from a detached worktree, which keeps local edits from
changing bytes published under another revision.

~~~bash
PROFILE=showcase/sumo/deploy/deployment.json
REVISION=$(git rev-parse HEAD)

just profile-validate "$PROFILE"
just profile-cluster-up "$PROFILE"
just profile-publish "$PROFILE" "$REVISION"
just profile-up "$PROFILE" "$REVISION"
~~~

BuildKit pushes images directly to the shared local OCI registry. It does not load
release images into the host Docker image store. Ordered image groups publish a
heavyweight shared base before targets that consume it, while independent targets in
one group build concurrently.

Compatible Rust builders share a versioned Cargo target cache. Builders with a
different operating-system ABI or native SDK use a separate cache identity.

## Contract

Paths resolve relative to the profile. The fields are:

| Field | Meaning |
|---|---|
| schemaVersion | veoveo.io/deployment/v1 |
| name | Stable local environment identity |
| registry.address | OCI host and port |
| registry.localConfig | Shared k3d registry definition |
| imageGroups | Ordered Docker Bake publication phases |
| kubernetes.context | Explicit kubectl and Helm context |
| kubernetes.localCluster | k3d configuration and node bootstrap manifests |
| namespace | Namespace for local resources |
| resources.manifests | Kubernetes resources applied before Helm |
| resources.configMaps | File-backed development ConfigMaps |
| resources.secrets | Environment-backed development Secrets |
| releases | Ordered local Helm releases and values |
| waitForDeployments | Extra rollout gates |

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
just profile-cluster-stop "$PROFILE"
just profile-cluster-up "$PROFILE"
just profile-down "$PROFILE"
just profile-cluster-delete "$PROFILE"
~~~

A new local showcase may add an image group and adjacent Helm chart, then select those
surfaces from a profile. A customer installation does not add a profile; it publishes
or selects OCI artifacts and adds desired state to its configuration repository.
