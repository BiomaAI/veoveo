{{/*
Veoveo private extension chart API: veoveo.io/extension-helm-library/v1.
All helpers take an explicit dictionary so the library does not prescribe a consumer
chart's values shape.
*/}}

{{- define "veoveo-extension.requireInstallation" -}}
{{- $installation := required "Veoveo extension helpers require installation" .installation -}}
{{- if not (regexMatch "^[a-z0-9]([-a-z0-9]*[a-z0-9])?$" $installation) -}}
{{- fail "installation must be a lowercase Kubernetes label value" -}}
{{- end -}}
{{- $installation -}}
{{- end -}}

{{- define "veoveo-extension.requireComponent" -}}
{{- $component := required "Veoveo extension helpers require component" .component -}}
{{- if not (regexMatch "^[a-z0-9]([-a-z0-9.]*[a-z0-9])?$" $component) -}}
{{- fail "component must be a lowercase Kubernetes label value" -}}
{{- end -}}
{{- $component -}}
{{- end -}}

{{- define "veoveo-extension.installationSelector" -}}
veoveo.ai/installation: {{ include "veoveo-extension.requireInstallation" . | quote }}
{{- end -}}

{{- define "veoveo-extension.componentSelector" -}}
{{ include "veoveo-extension.installationSelector" . }}
app.kubernetes.io/component: {{ include "veoveo-extension.requireComponent" . | quote }}
{{- end -}}

{{- define "veoveo-extension.selectorLabels" -}}
app.kubernetes.io/name: {{ required "Veoveo extension helpers require name" .name | quote }}
app.kubernetes.io/instance: {{ required "Veoveo extension helpers require releaseName" .releaseName | quote }}
{{ include "veoveo-extension.componentSelector" . }}
{{- end -}}

{{- define "veoveo-extension.labels" -}}
{{ include "veoveo-extension.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ required "Veoveo extension helpers require managedBy" .managedBy | quote }}
helm.sh/chart: {{ required "Veoveo extension helpers require chart" .chart | quote }}
app.kubernetes.io/part-of: veoveo
{{- end -}}

{{- define "veoveo-extension.image" -}}
{{- $image := required "Veoveo extension image helper requires image" .image -}}
{{- $registry := trimSuffix "/" (default "" .registry) -}}
{{- $repository := required "image.repository is required" $image.repository -}}
{{- if $registry -}}{{- $repository = printf "%s/%s" $registry $repository -}}{{- end -}}
{{- $lockedDigest := get (default dict .imageDigests) $image.repository | default "" -}}
{{- $digest := default $lockedDigest $image.digest -}}
{{- if and (default false .production) (not $digest) -}}
{{- fail (printf "production requires an immutable digest for %s" $repository) -}}
{{- end -}}
{{- if $digest -}}
{{- if not (regexMatch "^sha256:[a-f0-9]{64}$" $digest) -}}
{{- fail (printf "image digest for %s must be sha256:<64 lowercase hexadecimal digits>" $repository) -}}
{{- end -}}
{{- printf "%s@%s" $repository $digest -}}
{{- else -}}
{{- $tag := default $image.tag .sourceTag -}}
{{- printf "%s:%s" $repository (required "image.tag or sourceTag is required outside production" $tag) -}}
{{- end -}}
{{- end -}}

{{- define "veoveo-extension.podSecurityContext" -}}
fsGroup: 10001
fsGroupChangePolicy: OnRootMismatch
runAsNonRoot: true
runAsUser: 10001
runAsGroup: 10001
seccompProfile:
  type: RuntimeDefault
{{- end -}}

{{- define "veoveo-extension.containerSecurityContext" -}}
allowPrivilegeEscalation: false
capabilities:
  drop: ["ALL"]
readOnlyRootFilesystem: true
runAsNonRoot: true
runAsUser: 10001
runAsGroup: 10001
seccompProfile:
  type: RuntimeDefault
{{- end -}}

{{- define "veoveo-extension.gpuRequest" -}}
{{- $placement := required "gpuRequest requires placement" .placement -}}
{{- $workload := required "gpuRequest requires workload" .workload -}}
{{- if $placement.enabled -}}
{{- required (printf "gpu placement has no request for workload %s" $workload) (get $placement.workloadRequests $workload) -}}
{{- end -}}
{{- end -}}

{{- define "veoveo-extension.gpuReplicas" -}}
{{- $placement := required "gpuReplicas requires placement" .placement -}}
{{- $workload := required "gpuReplicas requires workload" .workload -}}
{{- if $placement.enabled -}}
{{- required (printf "gpu placement has no replica count for workload %s" $workload) (get $placement.workloadReplicas $workload) -}}
{{- else -}}1{{- end -}}
{{- end -}}

{{- define "veoveo-extension.gpuPodClaim" -}}
{{- $placement := required "gpuPodClaim requires placement" .placement -}}
{{- if $placement.enabled }}
resourceClaims:
  - name: veoveo-gpu
    resourceClaimName: {{ required "gpu placement claimName is required" $placement.claimName | quote }}
{{- end }}
{{- end -}}

{{- define "veoveo-extension.gpuResources" -}}
{{- $placement := required "gpuResources requires placement" .placement -}}
{{- $resources := required "gpuResources requires resources" .resources -}}
{{- if $placement.enabled -}}
requests:
  {{- omit $resources.requests "nvidia.com/gpu" | toYaml | nindent 2 }}
limits:
  {{- omit $resources.limits "nvidia.com/gpu" | toYaml | nindent 2 }}
claims:
  - name: veoveo-gpu
    request: {{ include "veoveo-extension.gpuRequest" . | quote }}
{{- else -}}
{{- toYaml $resources -}}
{{- end -}}
{{- end -}}

{{- define "veoveo-extension.platformEnv" -}}
- name: PUBLIC_BASE_URL
  value: {{ required "platformEnv requires publicBaseUrl" .publicBaseUrl | quote }}
- name: VEOVEO_SURREAL_ENDPOINT
  value: {{ required "platformEnv requires surrealEndpoint" .surrealEndpoint | quote }}
- name: VEOVEO_SURREAL_NAMESPACE
  value: {{ required "platformEnv requires surrealNamespace" .surrealNamespace | quote }}
- name: VEOVEO_SURREAL_DATABASE
  value: {{ required "platformEnv requires surrealDatabase" .surrealDatabase | quote }}
- name: VEOVEO_SURREAL_AUTH_LEVEL
  value: database
- name: VEOVEO_SURREAL_USERNAME
  valueFrom:
    secretKeyRef:
      name: {{ required "platformEnv requires surrealSecret" .surrealSecret | quote }}
      key: username
- name: VEOVEO_SURREAL_PASSWORD
  valueFrom:
    secretKeyRef:
      name: {{ required "platformEnv requires surrealSecret" .surrealSecret | quote }}
      key: password
- name: VEOVEO_INTERNAL_TRUST_JWKS
  valueFrom:
    secretKeyRef:
      name: {{ required "platformEnv requires installationSecret" .installationSecret | quote }}
      key: internal-trust-jwks
{{- if .otelEndpoint }}
- name: OTEL_EXPORTER_OTLP_ENDPOINT
  value: {{ .otelEndpoint | quote }}
{{- end }}
{{- end -}}

{{- define "veoveo-extension.httpProbes" -}}
startupProbe:
  httpGet:
    path: {{ required "httpProbes requires startupPath" .startupPath | quote }}
    port: {{ default "http" .port | quote }}
    httpHeaders:
      - name: Host
        value: {{ required "httpProbes requires host" .host | quote }}
  failureThreshold: {{ default 60 .startupFailureThreshold }}
  periodSeconds: {{ default 5 .startupPeriodSeconds }}
  timeoutSeconds: {{ default 3 .timeoutSeconds }}
readinessProbe:
  httpGet:
    path: {{ required "httpProbes requires readinessPath" .readinessPath | quote }}
    port: {{ default "http" .port | quote }}
    httpHeaders:
      - name: Host
        value: {{ required "httpProbes requires host" .host | quote }}
  periodSeconds: {{ default 5 .readinessPeriodSeconds }}
  timeoutSeconds: {{ default 3 .timeoutSeconds }}
livenessProbe:
  httpGet:
    path: {{ required "httpProbes requires livenessPath" .livenessPath | quote }}
    port: {{ default "http" .port | quote }}
    httpHeaders:
      - name: Host
        value: {{ required "httpProbes requires host" .host | quote }}
  initialDelaySeconds: {{ default 10 .livenessInitialDelaySeconds }}
  periodSeconds: {{ default 10 .livenessPeriodSeconds }}
  timeoutSeconds: {{ default 3 .timeoutSeconds }}
{{- end -}}

{{- define "veoveo-extension.bootstrapVolumeMount" -}}
- name: bootstrap
  mountPath: /etc/veoveo/bootstrap
  readOnly: true
{{- end -}}

{{- define "veoveo-extension.bootstrapVolume" -}}
- name: bootstrap
  configMap:
    name: {{ required "bootstrapVolume requires configMapName" .configMapName | quote }}
{{- end -}}
