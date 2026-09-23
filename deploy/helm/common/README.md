# Shared Helm Helpers

Application charts use this internal library through a local `file://` dependency.
Run `helm dependency update <chart>` after changing it and commit the consumer's
lock and bundled chart. The application chart is the published deployment artifact.

The exported named templates are:

| Template | Purpose |
|---|---|
| `veoveo-common.labels` | complete resource labels |
| `veoveo-common.selectorLabels` | release-local pod selector labels |
| `veoveo-common.installationSelector` | cross-chart installation selector |
| `veoveo-common.componentSelector` | cross-chart installation and component selector |
| `veoveo-common.image` | registry, source-tag, lock-digest resolution, and production digest enforcement |
| `veoveo-common.podSecurityContext` | restricted pod security defaults |
| `veoveo-common.containerSecurityContext` | restricted container security defaults |
| `veoveo-common.gpuPodClaim` | installation DRA ResourceClaim binding for a declared GPU workload |
| `veoveo-common.gpuResources` | per-container DRA request with legacy extended-resource removal |
| `veoveo-common.gpuReplicas` | installation-declared replica count for a GPU workload |
| `veoveo-common.platformEnv` | typed platform-store and trust environment |
| `veoveo-common.httpProbes` | startup, readiness, and liveness probes |
| `veoveo-common.bootstrapVolumeMount` | platform bootstrap mount |
| `veoveo-common.bootstrapVolume` | platform bootstrap ConfigMap volume |
| `veoveo-common.recordingForwarder` | recording producer sidecar; `finishSupersededRecordings` selects a single-recording application slot |
| `veoveo-common.networkPolicy` | default-deny, DNS, gateway, platform, and declared egress policy |

Each template accepts a dictionary. A missing required key fails rendering with a
direct error. The templates ignore any other values, which belong to the consumer chart.

The owning [design](DESIGN.md) describes image resolution and security invariants.
