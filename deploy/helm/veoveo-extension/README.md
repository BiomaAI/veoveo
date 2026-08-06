# Veoveo Extension Helm Library

This library chart supplies the stable Kubernetes integration boundary for an
independently released Veoveo extension chart. It is packaged as a versioned private
OCI artifact or included in a verified offline bundle. It is not installed as a Helm
release.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Helm library chart API | Helm v3/v4 `type: library`, API `veoveo.io/extension-helm-library/v1` |
| Kubernetes labels | recommended application labels plus `veoveo.ai/installation`; component identity uses `app.kubernetes.io/component` |
| Kubernetes NetworkPolicy | namespaced ingress and egress selected by the installation label |
| OCI Distribution Specification | authenticated private chart distribution with immutable digest locks |
| Kubernetes Pod Security Standards | restricted-compatible pod and container security contexts |

## Consumer Contract

Declare the exact library version in the extension application's `Chart.yaml`:

```yaml
dependencies:
  - name: veoveo-extension
    version: 0.1.0
    repository: oci://registry.internal.example/charts
```

The installation lock records the resolved package digest. Local development may use
`file://` for a checked-out library, but a production release cannot depend on an
unresolved mutable package.

Every workload supplies an installation identity chosen by the installer and a stable
component identity chosen by the extension. Release-local selectors keep using the
consumer chart's Helm release. Cross-chart policies select
`veoveo.ai/installation`; they never copy another release's
`app.kubernetes.io/instance`.

The exported named templates are:

| Template | Purpose |
|---|---|
| `veoveo-extension.labels` | complete resource labels |
| `veoveo-extension.selectorLabels` | release-local pod selector labels |
| `veoveo-extension.installationSelector` | cross-chart installation selector |
| `veoveo-extension.componentSelector` | cross-chart installation and component selector |
| `veoveo-extension.image` | registry, source-tag, lock-digest resolution, and production digest enforcement |
| `veoveo-extension.podSecurityContext` | restricted pod security defaults |
| `veoveo-extension.containerSecurityContext` | restricted container security defaults |
| `veoveo-extension.gpuPodClaim` | installation DRA ResourceClaim binding for a declared GPU workload |
| `veoveo-extension.gpuResources` | per-container DRA request with legacy extended-resource removal |
| `veoveo-extension.gpuReplicas` | installation-declared replica count for a GPU workload |
| `veoveo-extension.platformEnv` | typed platform-store and trust environment |
| `veoveo-extension.httpProbes` | startup, readiness, and liveness probes |
| `veoveo-extension.bootstrapVolumeMount` | canonical bootstrap mount |
| `veoveo-extension.bootstrapVolume` | canonical bootstrap ConfigMap volume |
| `veoveo-extension.recordingForwarder` | recording producer sidecar; `finishSupersededRecordings` selects a single-recording application slot |
| `veoveo-extension.networkPolicy` | default-deny, DNS, gateway, platform, and declared egress policy |

Each template accepts a dictionary. Required keys fail rendering with a direct error;
unknown values remain owned by the consumer chart.

Deployment v4 injects the installation GPU placement under
`veoveo.gpuPlacement` for extension releases. A GPU extension passes that object and
its canonical workload identifier to the GPU helpers. Non-GPU extensions retain the
disabled object in their closed values schema and render no claim fields.

The image helper accepts installation-owned `registry`, source-owned `sourceTag`, and
an `imageDigests` map keyed by the image's declared repository. A literal image digest
wins over the lock map, and either digest wins over the source tag. Production rejects
an image absent from both digest inputs.
