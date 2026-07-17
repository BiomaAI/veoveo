# Local k3d development

The local environment is a real Kubernetes installation. k3d owns the cluster,
Helm owns workload releases, and kubectl provides inspection and process control.
Each simulator keeps its values and gateway profile beside its own source. The
SUMO development cluster contains no simulator workload until that profile is
installed.

The optional Bioma profile uses a second cluster and explicit Kubernetes context.
See [`examples/bioma/README.md`](../../../examples/bioma/README.md) for the
concurrent Isaac Sim, View, Perception, and public-tunnel proof.

The Bioma cluster creates a pinned OCI Distribution registry on loopback port
5001. UAV images use full Git revisions as tags. The Isaac dependency base
remains stable, while registry blob deduplication moves only the thin runtime
layers changed by a commit. The remaining platform images retain the existing
import path.

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
these versions is changed. `versions.env` also pins the OCI Distribution image
used by the Bioma cluster.

## GPU cluster

The node image combines K3s with the NVIDIA Container Toolkit. The cluster passes
the host GPU through to its server node and installs NVIDIA's Kubernetes device
plugin with `FAIL_ON_INIT_ERROR=true`. GPU workloads do not have a CPU fallback.
The Bioma node profile publishes three time-sliced allocations from that device
because Isaac Sim, View, and Perception run at the same time. Each workload still
requests one ordinary `nvidia.com/gpu` resource. Time-slicing provides
schedulability, not memory or fault isolation; a fielded cluster may instead
provide three physical GPUs or an operator-selected partitioning policy.

```bash
nvidia-smi
just k3d-node-build
just sumo-k3d-create

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

- `showcase/sumo/deploy/gateway.json` selects the SUMO MCP surface.
- `showcase/sumo/deploy/platform-values.yaml` removes unrelated domain services.
- `showcase/sumo/deploy/helm` defines the simulation and its MCP server.

Build and import the profile images after creating the cluster:

```bash
just showcase-sumo-build
just showcase-sumo-import
```

The two SUMO images share a large upstream runtime and LuST scenario. Their direct
containerd stream avoids a temporary multi-gigabyte k3d archive. The remaining
images use `k3d image import`.

Apply the disposable local credentials and the profile-owned gateway data, then
install the platform and simulation releases:

```bash
just showcase-sumo-resources
just showcase-sumo-platform-up
just showcase-sumo-up
just showcase-sumo-verify
```

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

## Rerun

The local values expose Recording Hub through `127.0.0.1:9877`. Put the
canonical token in the repository root `.env` file:

```dotenv
MAPBOX_ACCESS_TOKEN=pk.example
```

The Justfile loads `.env` and maps that value to Rerun's required variable:

```bash
just showcase-sumo-view
```

Rerun connects to `rerun+http://127.0.0.1:9877/proxy`. Add
`/world/sumo/vehicles` to a Map view for the Mapbox projection. The local
Cartesian road geometry at `/world/sumo/network` belongs in a 3D view.

## Cleanup

Remove one simulator without disturbing the platform:

```bash
just showcase-sumo-down
```

Delete the cluster to remove all local Kubernetes state and persistent volumes:

```bash
just sumo-k3d-delete
```
