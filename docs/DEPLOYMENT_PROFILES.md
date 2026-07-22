# Deployment profiles

Veoveo builds software once per Git revision and deploys that revision through typed
profiles. An installation selects image groups, Kubernetes resources, Helm values, and
secret bindings. It does not own another copy of the build pipeline.

This division supports local showcases, enterprise clusters, and public installations
with one release mechanism:

| Concern | Canonical owner |
|---|---|
| Image definitions and reusable groups | `docker-bake.hcl` |
| First-party image destination | `VEOVEO_REGISTRY/veoveo/<image>:<git-sha>` |
| Platform workload graph | `deploy/helm/veoveo` |
| Simulator or domain workload graph | Its adjacent Helm chart |
| Installation composition | A `veoveo.io/deployment/v1` JSON profile |
| Local registry lifecycle | `deploy/local/k3d/registry.json` |
| Offline transfer | The verified offline bundle workflow |

## Release workflow

Commit source before publishing. The publisher resolves the requested revision to a full
40-character commit SHA and builds from a temporary detached worktree. Local edits cannot
change bytes published under another commit's identity.

```bash
PROFILE=showcase/sumo/deploy/deployment.json
REVISION=$(git rev-parse HEAD)

just profile-validate "$PROFILE"
just profile-cluster-up "$PROFILE"
just profile-publish "$PROFILE" "$REVISION"
just profile-up "$PROFILE" "$REVISION"
```

`profile-publish` sends images directly from BuildKit to the selected OCI registry. It
does not load release images into the host Docker image store. Image groups are ordered
publication phases. Targets within one phase build in parallel, while a heavyweight
shared base belongs in an earlier phase than the images that consume it. Registry and
node caches then exchange only missing layers, while identical base layers remain shared
across image names, clusters, and deployment profiles.

`profile-up` applies typed ConfigMaps and environment-backed Secrets before installing
the profile's Helm releases. Every Veoveo-owned image receives the same registry and Git
revision through `global.veoveoRegistry` and `global.veoveoTag`. Upstream images retain
their pinned repositories and versions.

The publisher and installer are separate commands. CI may publish a revision once, after
which any number of installations can run `profile-up` with that SHA. A developer can
publish only the image groups selected by a smaller showcase profile.

## Profile contract

Paths are resolved relative to the profile file. A profile stored outside the repository
is supported when the command runs from the Veoveo worktree. This lets an operator keep
private values and resource composition in a separate configuration repository.

The profile fields are:

| Field | Meaning |
|---|---|
| `schemaVersion` | Must be `veoveo.io/deployment/v1`. |
| `name` | Stable deployment profile identity. |
| `registry.address` | OCI host and port without a URL scheme. |
| `registry.localConfig` | Optional standalone k3d registry definition. Omit it for an enterprise registry. |
| `imageGroups` | Ordered Docker Bake publication phases selected by this deployment. |
| `kubernetes.context` | Explicit kubectl and Helm context. |
| `kubernetes.localCluster` | Optional k3d name, config, and node-bootstrap manifests. Omit it for an existing cluster. |
| `namespace` | Namespace for generated resources and Helm releases. |
| `resources.manifests` | Kubernetes manifests applied in the profile namespace before Helm. |
| `resources.configMaps` | Typed names and file-to-key projections. |
| `resources.secrets` | Secret keys sourced from named environment variables. |
| `releases` | Ordered Helm releases, values files, and bounded timeouts. |
| `waitForDeployments` | Extra deployments whose rollout must complete. |

Secret values are passed to `kubectl apply` over stdin. The profile contains environment
variable names, never secret bytes. `gatewayInternalTrustJwks` entries receive semantic
validation before Kubernetes accepts the Secret.

An existing enterprise cluster needs no k3d configuration:

```json
{
  "schemaVersion": "veoveo.io/deployment/v1",
  "name": "operations-west",
  "registry": {
    "address": "registry.example.com/operations"
  },
  "imageGroups": ["platform-full"],
  "kubernetes": {
    "context": "operations-west"
  },
  "namespace": "veoveo",
  "resources": {
    "manifests": [],
    "configMaps": [],
    "secrets": []
  },
  "releases": [
    {
      "name": "veoveo",
      "chart": "../veoveo/deploy/helm/veoveo",
      "values": ["values.yaml"],
      "createNamespace": true,
      "timeoutSeconds": 900
    }
  ],
  "waitForDeployments": []
}
```

Registry authentication remains an operator concern. Log the publisher into the selected
registry and provide the cluster's image pull Secret through Helm values.

## Shared local registry

All local profiles use one standalone OCI Distribution registry at
`k3d-veoveo-registry.localhost:5001`. The registry is independent of every cluster and
stores blobs in the `veoveo-registry` Docker volume. Deleting one showcase cluster does
not discard image layers needed by another.

Both local k3d configs attach this registry through `registries.use`. Nodes pull immutable
revisions into containerd when Kubernetes schedules a pod. The node keeps its own runtime
copy because containers cannot execute directly from a registry, but layer-aware pulls
replace full-image tar imports.

Local profiles declare the NVIDIA device plugin under
`kubernetes.localCluster.nodeBootstrapManifests`. The runner applies those node-level
prerequisites after cluster startup, then waits for allocatable `nvidia.com/gpu`
capacity. It performs the same check before deployment. A local Veoveo profile cannot
proceed on a cluster that lost its NVIDIA device path. Existing enterprise clusters own
their GPU operator or device-plugin lifecycle and must expose allocatable capacity before
Veoveo is installed.

Use these lifecycle commands for any local profile:

```bash
just profile-cluster-stop "$PROFILE"
just profile-cluster-up "$PROFILE"
just profile-down "$PROFILE"
just profile-cluster-delete "$PROFILE"
```

`profile-down` removes the profile's Helm releases. `profile-cluster-stop` retains cluster
state. `profile-cluster-delete` removes the local cluster and its Kubernetes state while
leaving the shared registry available.

## Adding an installation or showcase

An installation profile chooses an existing platform image group and adds Helm values,
gateway control data, and environment bindings. A new simulator adds its own image group
and adjacent Helm chart, then selects both that group and the appropriate platform group.

Ten enterprise installations can deploy the same published SHA without creating ten
build recipes. Their profiles differ in registry location, Kubernetes context, policy,
capacity, identity, ingress, and enabled add-ons. Build definitions remain common unless
an installation introduces genuinely different software.

Keep air-gapped delivery explicit. The offline bundle carries OCI archives because its
target cannot pull from a registry; connected local and enterprise profiles use the
registry-first path.
