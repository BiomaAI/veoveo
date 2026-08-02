# GPU placement

Deployment profiles can own a restart-stable physical-GPU topology through Kubernetes
Dynamic Resource Allocation. The regular `profile-up` workflow installs the qualified
allocator, creates one durable claim, deploys its consumers, and proves each
in-container physical UUID. Direct Helm installation is not part of this workflow.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Kubernetes Dynamic Resource Allocation | `resource.k8s.io/v1` on the qualified Kubernetes/K3s v1.36.2 runtime |
| NVIDIA DRA Driver for GPUs | Standalone Helm chart `dra-driver-nvidia-gpu` v0.4.1; GPU allocation and configurable time slicing are accepted as upstream technology-preview features |
| NVIDIA resource configuration | `resource.nvidia.com/v1beta1` `GpuConfig` with exact `TimeSlicing` intervals |
| Container Device Interface | GPU injection through a CDI-enabled container runtime |
| OCI Distribution and SHA-256 | Exact chart manifest, chart archive, driver image index, and platform image manifests |
| Helm | v4.2.3; atomic managed dependency installation and upgrade |

## Qualified closure

The deployment v5 contract accepts one allocator closure. These coordinates are
validated in Rust rather than copied from arbitrary profile input.

| Artifact | Exact identity |
|---|---|
| Helm chart | `oci://registry.k8s.io/dra-driver-nvidia/charts/dra-driver-nvidia-gpu:0.4.1` |
| Chart OCI manifest | `sha256:7a00373fdef1025f27ebb1d353719446bbbe6ec4697e9a503c5ffd7e4f1525dd` |
| Downloaded chart archive | `sha256:c1c316f6bdcfe5fed3ff649cff1b43be50d27d0cb1aaf9d29e7bdca1eaa331ce` |
| Driver image index | `registry.k8s.io/dra-driver-nvidia/dra-driver-nvidia-gpu:v0.4.1@sha256:eefe67396dedea4df74f68a94d5883f33204888b83979babd42b91501a2de1d8` |
| Linux AMD64 image | `sha256:ad86983849542f6ef22f02e963ecbf545706e037455e0c265889ace137863556` |
| Linux ARM64 image | `sha256:b51290bbc1ee6745adf8ffff040d2b917d3e07dbd5cd36fd444b0e371ccc9166` |

The GPU-only installation keeps a host-installed driver mounted at `/`. It does not
install or upgrade the driver, Container Toolkit, container runtime, or Kubernetes.
The qualified field tuple is Kubernetes/K3s v1.36.2, NVIDIA driver 610.43.02,
Container Toolkit package 1.19.1-1, and a CDI-enabled runtime. `profile-up` requires the
exact Kubernetes and driver versions. The repository-managed K3s node image pins the
Toolkit package; claim preparation and the final workload UUID gate prove CDI injection.
ComputeDomains are disabled and impose no IMEX or NVLink prerequisite.

## Profile contract

`platform.gpuScheduling` describes groups, not vendor scheduler objects. Workloads in a
same-device group consume the same named request from one persistent ResourceClaim.
Each different-device constraint compiles to `distinctAttribute:
gpu.nvidia.com/uuid`. A measured time-slicing group adds a bounded `GpuConfig`; it does
not multiply or invent physical devices.

The allocator section supplies:

- a stable claim, release, and namespace;
- every chart and image digest from the qualified closure;
- a nonempty selector for nodes the installation permits the profile to manage;
- `nvidiaDriverRoot: /` for the host-installed driver;
- one explicit conflicting-device-plugin removal authorization;
- `maturityAcceptance: technology-preview`; and
- an atomic Helm timeout from 60 through 1,800 seconds.

Use `require-absent` when selected nodes do not run the NVIDIA device plugin.
Use `delete-daemon-set` for an installation-owned raw DaemonSet. Use
`uninstall-helm-release` for an installation-owned Helm release and state its expected
chart version. The installer refuses an undeclared device-plugin pod. The device plugin
cannot remain on a node where DRA owns `nvidia.com/gpu`.

Deployment v5 is a hard cut. A deployment v4 profile must add the complete
`gpuScheduling.allocator.installation` object, change both schema identifiers to v5,
and regenerate the deployment lock. No implicit allocator or mutable chart default is
retained.

The repository-managed K3s node image no longer embeds a device-plugin manifest. A
local cluster created from the earlier node image must be deleted, the current pinned
node image rebuilt, and the cluster recreated before its first DRA `profile-up`.
Stopping and starting that older cluster is insufficient because its static manifest
remains on the node. A field cluster whose device plugin is an ordinary DaemonSet or
Helm release uses the explicit `conflictingDevicePluginRemoval` authorization instead.

## Managed lifecycle

`profile-cluster-up` starts the cluster and waits for a Ready node. It does not wait for
the legacy extended resource when DRA is selected. `profile-up` then performs this
ordered transaction:

1. Validate the profile, lock, Kubernetes API version, and selected Ready nodes.
2. Pull the exact chart and check its registry-reported manifest digest and local
   archive digest before changing GPU ownership.
3. If the declared conflicting allocator exists, scale declared GPU Deployments to zero and
   perform only the selected removal action.
4. Mark eligible nodes with the chart selector and reject remaining device-plugin pods.
5. Run an atomic Helm upgrade with chart-owned RBAC. The values fix
   `resource.k8s.io/v1`, enable GPU resources and `TimeSlicingSettings`, disable
   ComputeDomains, and pin the driver image index.
6. Verify the Helm chart, rendered image, DeviceClass, Ready kubelet plugins,
   ResourceSlice node coverage, driver floor, physical UUID uniqueness, and device
   count.
7. Create or reuse the persistent ResourceClaim, run the application Helm releases,
   wait for their one-replica Deployments, and inspect `nvidia-smi` in every declared
   container.

The UUID gate proves equality inside each same-device group and inequality across every
different-device constraint. It also rejects zero devices, multiple visible devices,
an unallocated request, a missing replica, and placement drift. This proof is repeated
after every `profile-up`, including after a Helm upgrade or cluster restart.

The GPU-only chart profile has no controller Deployment. NVIDIA's controller belongs
to ComputeDomains, which this contract disables. The GPU kubelet-plugin DaemonSet
publishes ResourceSlices and prepares CDI devices; Kubernetes' scheduler allocates the
claim. `profile-up` verifies one Ready plugin pod on every selected node.

## Validation

Run the ordinary profile workflow with the installation-owned profile and lock:

```sh
cargo xtask smoke profile-validate --profile deploy/deployment.json
cargo xtask smoke profile-cluster-up --profile deploy/deployment.json
cargo xtask smoke profile-up \
  --profile deploy/deployment.json \
  --lock deploy/deployment.lock.json
kubectl --context <context> get deviceclass gpu.nvidia.com
kubectl --context <context> get resourceclaim -n <namespace> <claim> -o json
```

Successful `profile-up` prints the ResourceSlice physical UUID inventory and the one
UUID observed in every declared workload container. CUDA, Vulkan, RTX, and NVENC remain
application readiness concerns and must still pass on hardware; DRA supplies the
physical allocation and CDI device injection.

## Upgrade, rollback, and recovery

An allocator upgrade starts as a contract change. Verify the latest stable upstream
release, replace all chart and image identities together, update schema and fixtures,
run contract and smoke tests, and publish one new deployment lock. `profile-up` then
uses Helm's atomic upgrade and repeats the complete ResourceSlice and UUID gates. It
never changes the host driver as a side effect.

Application rollback keeps the qualified allocator and claim in place. Reconcile the
previous digest-locked application sources through a deployment v5 lock, then run
`profile-up`; the same requests retain their physical assignments. Do not recreate the
claim merely to roll back application images.

Allocator rollback uses the same profile workflow. Check out the previous platform
revision that qualifies the previous allocator, select its matching deployment v5
profile and lock, and run `profile-up`. That revision owns its chart and image pins;
Helm performs the atomic transition and the earlier profile repeats its ResourceSlice
and UUID checks. The current validator never silently accepts an older chart. A return
to device-plugin allocation is not a supported rollback path for a DRA profile.

For ordinary loss of a kubelet-plugin pod, DeviceClass, or ResourceSlice, rerun
`profile-up`. Its verified chart installation is idempotent. If the claim is
allocated but a node no longer advertises its recorded UUID, keep consumers stopped
and repair the node, driver, Toolkit, CDI runtime, or hardware. Claim deletion is a
last-resort topology replacement and requires all consumers to be down because it may
select different physical devices.
