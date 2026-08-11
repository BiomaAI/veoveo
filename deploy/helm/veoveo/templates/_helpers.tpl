{{- define "veoveo.name" -}}
veoveo
{{- end }}

{{- define "veoveo.fullname" -}}
{{- printf "%s-veoveo" .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- end }}

{{- define "veoveo.labels" -}}
app.kubernetes.io/name: {{ include "veoveo.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: veoveo
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
veoveo.ai/installation: {{ required "global.installationId is required" .Values.global.installationId | quote }}
{{- end }}

{{- define "veoveo.selectorLabels" -}}
app.kubernetes.io/name: {{ include "veoveo.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
veoveo.ai/installation: {{ required "global.installationId is required" .Values.global.installationId | quote }}
{{- end }}

{{- define "veoveo.installationSelectorLabels" -}}
veoveo.ai/installation: {{ required "global.installationId is required" .Values.global.installationId | quote }}
{{- end }}

{{- define "veoveo.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "veoveo.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- required "serviceAccount.name is required when serviceAccount.create=false" .Values.serviceAccount.name -}}
{{- end -}}
{{- end }}

{{- define "veoveo.image" -}}
{{- $image := index . 1 -}}
{{- if $image.digest -}}
{{- printf "%s@%s" $image.repository $image.digest -}}
{{- else -}}
{{- printf "%s:%s" $image.repository $image.tag -}}
{{- end -}}
{{- end }}

{{- define "veoveo.ownedImage" -}}
{{- $root := index . 0 -}}
{{- $image := index . 1 -}}
{{- $registry := trimSuffix "/" $root.Values.global.veoveoRegistry -}}
{{- $repository := $image.repository -}}
{{- if $registry -}}{{- $repository = printf "%s/%s" $registry $repository -}}{{- end -}}
{{- $lockedDigest := get $root.Values.global.imageDigests $image.repository | default "" -}}
{{- $digest := $image.digest | default $lockedDigest -}}
{{- if and $root.Values.global.production (not $digest) -}}
{{- fail (printf "global.production requires an immutable digest for %s" $repository) -}}
{{- end -}}
{{- if $digest -}}
{{- printf "%s@%s" $repository $digest -}}
{{- else -}}
{{- $tag := default $image.tag $root.Values.global.veoveoTag -}}
{{- printf "%s:%s" $repository $tag -}}
{{- end -}}
{{- end }}

{{- define "veoveo.podAnnotations" -}}
{{- with .Values.global.podAnnotations }}{{ toYaml . }}{{ end }}
{{- if .Values.global.serviceMesh.enabled }}
{{- with .Values.global.serviceMesh.podAnnotations }}{{ toYaml . }}{{ end }}
{{- end }}
{{- end }}

{{- define "veoveo.gpuRequest" -}}
{{- $root := index . 0 -}}
{{- $workload := index . 1 -}}
{{- if $root.Values.global.gpuPlacement.enabled -}}
{{- required (printf "global.gpuPlacement.workloadRequests[%s] is required" $workload) (get $root.Values.global.gpuPlacement.workloadRequests $workload) -}}
{{- end -}}
{{- end }}

{{- define "veoveo.gpuRuntimeClass" -}}
{{- $root := index . 0 -}}
{{- $fallback := index . 1 -}}
{{- if $root.Values.global.gpuPlacement.enabled -}}
{{- required "global.gpuPlacement.runtimeClassName is required" $root.Values.global.gpuPlacement.runtimeClassName -}}
{{- else -}}
{{- $fallback -}}
{{- end -}}
{{- end }}

{{- define "veoveo.gpuReplicas" -}}
{{- $root := index . 0 -}}
{{- $workload := index . 1 -}}
{{- if $root.Values.global.gpuPlacement.enabled -}}
{{- required (printf "global.gpuPlacement.workloadReplicas[%s] is required" $workload) (get $root.Values.global.gpuPlacement.workloadReplicas $workload) -}}
{{- else -}}
1
{{- end -}}
{{- end }}

{{- define "veoveo.gpuPodAnnotations" -}}
{{- $root := index . 0 -}}
{{- $workload := index . 1 -}}
{{- if $root.Values.global.gpuPlacement.enabled }}
veoveo.ai/gpu-placement-evidence: {{ required "global.gpuPlacement.evidenceDigest is required" $root.Values.global.gpuPlacement.evidenceDigest | quote }}
veoveo.ai/gpu-placement-request: {{ include "veoveo.gpuRequest" (list $root $workload) | quote }}
{{- end }}
{{- end }}

{{- define "veoveo.gpuPodClaim" -}}
{{- $root := index . 0 -}}
{{- $workload := index . 1 -}}
{{- if $root.Values.global.gpuPlacement.enabled }}
resourceClaims:
  - name: veoveo-gpu
    resourceClaimName: {{ required "global.gpuPlacement.claimName is required" $root.Values.global.gpuPlacement.claimName | quote }}
{{- end }}
{{- end }}

{{- define "veoveo.gpuResources" -}}
{{- $root := index . 0 -}}
{{- $workload := index . 1 -}}
{{- $resources := index . 2 -}}
{{- if $root.Values.global.gpuPlacement.enabled -}}
requests:
  {{- omit $resources.requests "nvidia.com/gpu" | toYaml | nindent 2 }}
limits:
  {{- omit $resources.limits "nvidia.com/gpu" | toYaml | nindent 2 }}
claims:
  - name: veoveo-gpu
    request: {{ include "veoveo.gpuRequest" (list $root $workload) | quote }}
{{- else -}}
{{- toYaml $resources -}}
{{- end -}}
{{- end }}

{{- define "veoveo.surrealEnv" -}}
- name: VEOVEO_SURREAL_ENDPOINT
  value: ws://surrealdb:8000
- name: VEOVEO_SURREAL_NAMESPACE
  value: {{ .Values.surrealdb.namespace | quote }}
- name: VEOVEO_SURREAL_DATABASE
  value: {{ .Values.surrealdb.database | quote }}
- name: VEOVEO_SURREAL_USERNAME
  valueFrom:
    secretKeyRef:
      name: {{ .Values.surrealdb.runtimeExistingSecret }}
      key: username
- name: VEOVEO_SURREAL_PASSWORD
  valueFrom:
    secretKeyRef:
      name: {{ .Values.surrealdb.runtimeExistingSecret }}
      key: password
- name: VEOVEO_SURREAL_AUTH_LEVEL
  value: database
{{- end }}

{{- define "veoveo.commonEnv" -}}
{{ include "veoveo.surrealEnv" . }}
- name: VEOVEO_INTERNAL_TRUST_JWKS
  valueFrom:
    secretKeyRef:
      name: {{ .Values.global.existingSecret }}
      key: internal-trust-jwks
{{- if .Values.telemetry.enabled }}
- name: OTEL_EXPORTER_OTLP_ENDPOINT
  value: http://otel-collector:4318
{{- end }}
- name: VEOVEO_CONNECTIVITY_MODE
  value: {{ ternary "offline" "connected" .Values.global.offline | quote }}
{{- end }}

{{- define "veoveo.streamRecordingForwarder" -}}
{{- $root := . -}}
{{- $config := $root.Values.stream.recordingOutput.forwarder -}}
{{- $image := $config.image -}}
{{- $lockedDigest := get $root.Values.global.imageDigests $image.repository | default "" -}}
{{- include "veoveo-extension.recordingForwarder" (dict
    "image" (dict
      "repository" $image.repository
      "tag" (default $image.tag $root.Values.global.veoveoTag)
      "digest" (default $lockedDigest $image.digest)
    )
    "registry" $root.Values.global.veoveoRegistry
    "production" $root.Values.global.production
    "imagePullPolicy" $root.Values.global.imagePullPolicy
    "gatewayUrl" (printf "%s/" (trimSuffix "/" $root.Values.global.publicBaseUrl))
    "gatewayTransportUrl" (printf "%s/" (trimSuffix "/" $config.gatewayTransportUrl))
    "protectedResource" (printf "%s/ingest/recordings" (trimSuffix "/" $root.Values.global.publicBaseUrl))
    "clientId" $config.clientId
    "keyId" $config.keyId
    "signingAlgorithm" $config.signingAlgorithm
    "maximumQueueBytes" (printf "%.0f" $config.maximumQueueBytes)
    "batchMessageLimit" $config.batchMessageLimit
    "grpcMemoryLimitBytes" (printf "%.0f" $config.grpcMemoryLimitBytes)
    "resources" $config.resources
  ) -}}
{{- end }}

{{- define "veoveo.reasonRuntimeData" -}}
catalog.json: |
  {
    "models": [
      {
        "id": {{ .Values.reason.model.id | toJson }},
        "title": {{ .Values.reason.model.title | toJson }},
        "description": {{ .Values.reason.model.description | toJson }},
        "format": "local_checkpoint",
        "model_path": {{ .Values.reason.model.path | toJson }},
        {{- if .Values.reason.model.digest }}
        "model_digest": {{ .Values.reason.model.digest | toJson }},
        {{- end }}
        "engine": {
          "kind": "vllm",
          "gpu_memory_utilization": {{ .Values.reason.engine.gpuMemoryUtilization }},
          "max_model_len": {{ .Values.reason.engine.maxModelLen }}
        }
      }
    ],
    "pipelines": [
      {
        "id": "video-reasoning",
        "title": "Video reasoning",
        "description": "Describe segments, detect events, and answer questions over recording video.",
        "operation": "video_reasoning",
        "model_id": {{ .Values.reason.model.id | toJson }},
        "prompt_template_path": "/etc/veoveo/reason/prompt-template.txt",
        "prompt_revision": "v1",
        "observation": {
          "width": {{ .Values.reason.observation.width }},
          "height": {{ .Values.reason.observation.height }},
          "maximum_frames": {{ .Values.reason.observation.maximumFrames }}
        }
      }
    ]
  }
prompt-template.txt: |
  You observe frames sampled from one governed video recording. Answer only
  from what the frames show. When detecting events, report each event with
  its inclusive source-timeline range, a short label, and one factual
  description. When grounding detections are provided, cite their track
  identities instead of inventing object references.
{{- end }}

{{- define "veoveo.containerSecurityContext" -}}
allowPrivilegeEscalation: false
capabilities:
  drop: ["ALL"]
readOnlyRootFilesystem: true
runAsNonRoot: true
runAsUser: 10001
seccompProfile:
  type: RuntimeDefault
{{- end }}

{{- define "veoveo.podSecurityContext" -}}
fsGroup: 10001
fsGroupChangePolicy: OnRootMismatch
runAsNonRoot: true
seccompProfile:
  type: RuntimeDefault
{{- end }}
