# Local k3d development

The local environment is a real Kubernetes installation. k3d owns the cluster,
Helm owns workload releases, and kubectl provides inspection and process control.
Each simulator keeps its values and gateway profile beside its own source. The
SUMO development cluster contains no simulator workload until that profile is
installed.

The optional Bioma profile uses a second cluster and explicit Kubernetes context.
See [`examples/bioma/README.md`](../../../examples/bioma/README.md) for the
concurrent Isaac Sim, View, Perception, and public-tunnel proof.

One standalone OCI Distribution registry serves every local cluster on loopback
port 5001. All Veoveo images use full Git revisions as tags. Registry blob
deduplication moves only missing layers, and k3d nodes pull those layers through
the same registry contract used by connected enterprise clusters.

The development ingress has one canonical origin: `http://localhost:8780`.
Loopback HTTP is deliberate for the disposable local cluster. Fielded profiles
remain HTTPS-only and terminate TLS at their Kubernetes Ingress.

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

The node image combines K3s with the NVIDIA Container Toolkit. The cluster passes
the host GPU through to its server node and installs NVIDIA's Kubernetes device
plugin with `FAIL_ON_INIT_ERROR=true`. GPU workloads do not have a CPU fallback.
The node profile publishes four time-sliced allocations from that device because
Isaac Sim, View, Perception, and Reason run at the same time. Each workload still
requests one ordinary `nvidia.com/gpu` resource. Time-slicing provides
schedulability, not memory or fault isolation; a fielded cluster may instead
provide physical GPUs or an operator-selected partitioning policy.

```bash
nvidia-smi
just k3d-node-build
just profile-cluster-up showcase/sumo/deploy/deployment.json

kubectl --context k3d-veoveo-sumo get node -o 'custom-columns=NAME:.metadata.name,GPU:.status.allocatable.nvidia\.com/gpu'
kubectl --context k3d-veoveo-sumo delete job veoveo-gpu-probe --ignore-not-found
kubectl --context k3d-veoveo-sumo apply -f deploy/local/k3d/gpu-probe.yaml
kubectl --context k3d-veoveo-sumo wait --for=condition=complete job/veoveo-gpu-probe --timeout=5m
kubectl --context k3d-veoveo-sumo logs job/veoveo-gpu-probe
```

The probe requests one Kubernetes GPU and checks CUDA, the NVIDIA Vulkan ICD, and
the proprietary Vulkan device. A missing device, runtime, driver library, or
graphics capability fails the job.

## SUMO profile

The SUMO deployment owns these files:

- `showcase/sumo/deploy/deployment.json` composes the image groups and releases.
- `showcase/sumo/deploy/gateway.json` selects the SUMO MCP surface.
- `showcase/sumo/deploy/platform-values.yaml` removes unrelated domain services.
- `showcase/sumo/deploy/helm` defines the simulation and its MCP server.

Validate the profile, create the cluster, and publish one committed revision:

```bash
PROFILE=showcase/sumo/deploy/deployment.json
REVISION=$(git rev-parse HEAD)
just profile-validate "$PROFILE"
just profile-cluster-up "$PROFILE"
just profile-publish "$PROFILE" "$REVISION"
just profile-up "$PROFILE" "$REVISION"
just showcase-sumo-verify
```

BuildKit pushes image layers directly to the registry. The SUMO images share
their pinned upstream runtime and LuST scenario through the layer cache; the
cluster pulls only missing blobs into containerd.

[`development-resources.yaml`](development-resources.yaml) contains public,
fixed development credentials. It is valid only for this loopback cluster. A
shared cluster must use operator-created Secrets instead.

Useful control commands remain standard Kubernetes operations:

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
just profile-down showcase/sumo/deploy/deployment.json
```

Delete the cluster to remove all local Kubernetes state and persistent volumes:

```bash
just profile-cluster-delete showcase/sumo/deploy/deployment.json
```

The standalone registry remains available to other profiles after cluster
deletion. The complete profile contract is documented in
[`../../../docs/DEPLOYMENT_PROFILES.md`](../../../docs/DEPLOYMENT_PROFILES.md).
