# Local k3d development

The local environment is a real Kubernetes installation. k3d owns the cluster,
Helm owns workload releases, and kubectl provides inspection and process control.
Each simulator keeps its values and gateway profile beside its own source. The
SUMO development cluster contains no simulator workload until that profile is
installed.

The Bioma enterprise reference uses a second cluster and explicit Kubernetes context.
It installs its GitOps controller as a separate local platform fixture, then reconciles
OCI charts the same way a fielded cluster does. See
[`examples/bioma/README.md`](../../../examples/bioma/README.md).

One standalone OCI Distribution registry serves every local cluster on loopback
port 5001. All Veoveo images use full Git revisions as tags. The registry
deduplicates blobs, so a push moves only missing layers, and k3d nodes pull from it
exactly as connected enterprise clusters pull from theirs.

The development ingress has one origin: `http://localhost:8780`. It uses plain HTTP
because the cluster is local and disposable. Fielded profiles use HTTPS only and
terminate TLS at their Kubernetes Ingress.

## Tool versions

[`versions.env`](versions.env) records the current stable release of every tool
and GPU runtime component used by this profile. Install those exact releases from
their upstream projects and place `k3d`, `kubectl`, and `helm` on `PATH`.

```bash
source deploy/local/k3d/versions.env

install -d ~/.local/bin
curl -fsSLo ~/.local/bin/k3d \
  "https://github.com/k3d-io/k3d/releases/download/$K3D_VERSION/k3d-linux-amd64"
curl -fsSLo ~/.local/bin/kubectl \
  "https://dl.k8s.io/release/$KUBECTL_VERSION/bin/linux/amd64/kubectl"
curl -fsSLo /tmp/helm.tar.gz \
  "https://get.helm.sh/helm-$HELM_VERSION-linux-amd64.tar.gz"
tar -xzf /tmp/helm.tar.gz -C /tmp
install /tmp/linux-amd64/helm ~/.local/bin/helm
chmod 0755 ~/.local/bin/k3d ~/.local/bin/kubectl

k3d version
kubectl version --client
helm version
```

Check the published SHA-256 files before installing downloaded binaries. The
repository dependency policy requires an upstream release check whenever one of
these versions is changed. `registry.json` pins the OCI Distribution image used
by local deployment profiles.

## GPU cluster

The node image combines K3s with the NVIDIA Container Toolkit and a CDI-enabled
containerd runtime. It does not embed an allocator. Each deployment profile either uses
managed DRA or bootstraps the NVIDIA device plugin, and the two allocators never run
on the same node. GPU workloads do not have a CPU fallback.
The image restores Ubuntu's GNU tar after the K3s filesystem copy because `dpkg-deb`
requires it for package maintenance on a retained node.
The reference profile publishes six time-sliced device-plugin allocations because
the UAV simulator, View, Stream, Reason, the cuOpt executor, and the
Rerun viewer MCP run at the same time. Each workload still requests one ordinary
`nvidia.com/gpu` resource. Time-slicing provides schedulability, not memory or
fault isolation. Profiles that need restart-stable physical pairing use the managed
DRA contract in [GPU placement](../../../docs/GPU_PLACEMENT.md).

```bash
nvidia-smi
source deploy/local/k3d/versions.env
docker build \
  --build-arg K3S_VERSION="$K3S_VERSION" \
  --build-arg CUDA_VERSION="$CUDA_VERSION" \
  --build-arg NVIDIA_CONTAINER_TOOLKIT_VERSION="$NVIDIA_CONTAINER_TOOLKIT_VERSION" \
  --tag "$VEOVEO_K3D_NODE_IMAGE" \
  deploy/local/k3d/node
cargo xtask smoke profile-cluster-up \
  --profile showcase/sumo/deploy/deployment.json

kubectl --context k3d-veoveo-sumo get node -o 'custom-columns=NAME:.metadata.name,GPU:.status.allocatable.nvidia\.com/gpu'
kubectl --context k3d-veoveo-sumo delete job veoveo-gpu-probe --ignore-not-found
kubectl --context k3d-veoveo-sumo apply -f deploy/local/k3d/gpu-probe.yaml
kubectl --context k3d-veoveo-sumo wait --for=condition=complete job/veoveo-gpu-probe --timeout=5m
kubectl --context k3d-veoveo-sumo logs job/veoveo-gpu-probe
```

The probe requests one Kubernetes GPU and checks CUDA, the NVIDIA Vulkan ICD, and
the proprietary Vulkan device. A missing device, runtime, driver library, or
graphics capability fails the job.

## Retained cluster upgrades

A k3d cluster with retained local-path volumes must keep its existing node volumes.
Deleting and recreating that cluster removes the installation's local storage. Build
the pinned node image first. Before replacing the running single server's K3s binary,
take a SQLite `.backup` of `/var/lib/rancher/k3s/server/db/state.db` and copy the
server token and old binary outside the node volume. The binary in the built node
image is the source for the in-place replacement. Restart the same Docker node
container, then verify the K3s and kubectl versions, node readiness, advertised GPU
resources, every workload's one ready replica, and the public routes. Keep the
snapshot and old binary until those checks pass. The in-place binary survives a
container restart; a later node replacement must use the newly pinned image and
reattach the retained volumes.

Clusters built from earlier node images, which embedded an allocator, cannot switch to
managed DRA. Rebuild the image and recreate the cluster first. A cluster restart
does not remove a static device-plugin manifest already stored in that node.

## SUMO profile

The SUMO deployment owns these files:

- `showcase/sumo/deploy/deployment.json` composes the image groups and releases.
- `showcase/sumo/deploy/gateway.json` selects the SUMO MCP surface.
- `showcase/sumo/deploy/platform-values.yaml` removes unrelated domain services.
- `showcase/sumo/deploy/helm` defines the simulation and its MCP server.

Validate the profile, create the cluster, and publish one committed revision:

```bash
PROFILE=showcase/sumo/deploy/deployment.json
LOCK=output/deployments/sumo/deployment.lock.json
REVISION=$(git rev-parse HEAD)
cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
kubectl --context k3d-veoveo-sumo apply \
  -f deploy/local/k3d/development-resources.yaml
cargo xtask image builder ensure
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output "$LOCK"
cargo xtask smoke profile-up --profile "$PROFILE" --lock "$LOCK" \
  --all-components --receipt-output output/development/installation.receipt.json
cargo xtask smoke sumo-verify --context k3d-veoveo-sumo
```

BuildKit pushes image layers directly to the registry. The SUMO images share
their pinned upstream runtime and LuST scenario through the layer cache; the
cluster pulls only missing blobs into containerd.

[`development-resources.yaml`](development-resources.yaml) is an
installation-owner fixture with public, fixed development credentials, including the
recording-scoped Redap signing key. Apply it explicitly after the cluster exists and
before `profile-up`. The profile tooling never applies, patches, replaces, or copies a
Secret. This fixture is valid only for this loopback cluster. A shared cluster uses
operator-created Secrets from its own reconciliation path.

`profile-up` renders the expanded selection of locked Helm charts and raw manifests before its first
Kubernetes or Helm write. It computes the complete Secret-reference closure, reads only
the presence and required key names from existing Secrets, and fails closed when a
Secret or key is missing or cannot be verified. The recorded closure never contains Secret
values.

Day-to-day control uses standard Kubernetes commands:

```bash
k3d cluster list
kubectl --context k3d-veoveo-sumo -n veoveo get pods,services
kubectl --context k3d-veoveo-sumo -n veoveo logs -f deployment/sumo-mcp
kubectl --context k3d-veoveo-sumo -n veoveo rollout restart deployment/sumo-mcp
helm --kube-context k3d-veoveo-sumo -n veoveo list
```

## Cleanup

Remove the profile's Helm releases:

```bash
cargo xtask smoke profile-down \
  --profile showcase/sumo/deploy/deployment.json
```

Delete the cluster to remove all local Kubernetes state and persistent volumes:

```bash
cargo xtask smoke profile-cluster-delete \
  --profile showcase/sumo/deploy/deployment.json
```

The standalone registry remains available to other profiles after cluster
deletion. The complete local profile contract is documented in
[`../../../docs/LOCAL_DEPLOYMENT_PROFILES.md`](../../../docs/LOCAL_DEPLOYMENT_PROFILES.md).

## Registry history maintenance

Review the local registry every week and after a large image release. Run a cleanup
when its volume exceeds 100 GiB, or sooner when the host approaches its build-space
reserve. Check the volume and free space with:

```bash
REGISTRY_DATA=$(docker volume inspect --format '{{.Mountpoint}}' veoveo-registry)
sudo du -sh "$REGISTRY_DATA"
df -h "$REGISTRY_DATA"
```

The registry stores immutable
revision tags; publishing a new revision does not remove the old manifest. Garbage
collection alone frees little space while those manifests still refer to their layers.

Before deleting a manifest, make a repository-and-digest keep list from every current
installation image lock, live Pod image reference, qualified release input, and image
needed for the next rollback. In the Bioma reference, include both files under
`examples/bioma/images/`, the selected chart digests under
`examples/bioma/gitops/sources/`, and retained normalized dependency receipts under
`target/veoveo-xtask/normalized/`. Include all consumers of this shared registry,
not only Bioma. When a cluster is stopped, its checked-in lock is the minimum keep
set; compare it with a recent Pod inventory before deleting anything it does not
name. Keep each selected OCI index, its child platform manifests, and its SBOM and
provenance referrers. Protect the current chart and the chart needed for rollback.
If a reference or its ownership is uncertain, retain it until verified.

Inventory repository tags through the OCI Distribution `/v2/_catalog` and
`/v2/<repository>/tags/list` endpoints, following pagination links. Resolve a
candidate tag with `HEAD /v2/<repository>/manifests/<tag>` using an Accept header
for OCI image indexes, OCI image manifests, Docker manifest lists, and Docker v2
manifests. Record the returned `Docker-Content-Digest`, the tag, and the reason the
digest is absent from the keep list. Delete the **digest** through the registry API
only after checking the complete keep list; deleting a tag is not the manifest
deletion contract. For a reviewed candidate:

```bash
REGISTRY=http://127.0.0.1:5001
REPOSITORY=veoveo/example
DIGEST='sha256:REPLACE_WITH_REVIEWED_DIGEST'
curl --fail --silent --show-error --request DELETE \
  "$REGISTRY/v2/$REPOSITORY/manifests/$DIGEST"
```

Pause image and chart publication before garbage collection. Stop the registry,
then run the pinned OCI Distribution image against its existing volume. Copy the
container's effective config first and confirm its filesystem root is
`/var/lib/registry`. Inspect the dry-run list of blobs eligible for deletion against
the protected manifests and their referenced blobs. Run the real sweep only when
that comparison passes:

```bash
REGISTRY_CONTAINER=k3d-veoveo-registry.localhost
REGISTRY_IMAGE=$(docker inspect --format '{{.Config.Image}}' "$REGISTRY_CONTAINER")
docker cp "$REGISTRY_CONTAINER:/etc/distribution/config.yml" ./registry-gc.yml
docker stop "$REGISTRY_CONTAINER"
docker run --rm --network none --volumes-from "$REGISTRY_CONTAINER":ro \
  --volume "$PWD/registry-gc.yml:/etc/distribution/config.yml:ro" \
  --entrypoint /bin/registry "$REGISTRY_IMAGE" \
  garbage-collect --dry-run /etc/distribution/config.yml
docker run --rm --network none --volumes-from "$REGISTRY_CONTAINER" \
  --volume "$PWD/registry-gc.yml:/etc/distribution/config.yml:ro" \
  --entrypoint /bin/registry "$REGISTRY_IMAGE" \
  garbage-collect /etc/distribution/config.yml
```

Start the registry with `docker start "$REGISTRY_CONTAINER"` when the installation is
ready for publication and pulls; leave it stopped when the installation is intentionally
offline. Verify protected manifests
with `HEAD` at their digest, check the registry volume and free space again, and
retain the keep list, deletion journal, and garbage-collection output with the
maintenance record. Do not use `garbage-collect --delete-untagged`: selected child
manifests and attestations may have no tag. This procedure applies to the local
OCI Distribution registry; other registry products need their own retention policy.
The [OCI Distribution garbage-collection guide](https://distribution.github.io/distribution/about/garbage-collection/)
defines the stopped-writer requirement, and the
[Distribution API](https://distribution.github.io/distribution/spec/api/) defines
digest-based manifest deletion.
