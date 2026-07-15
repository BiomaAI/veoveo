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
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end }}

{{- define "veoveo.selectorLabels" -}}
app.kubernetes.io/name: {{ include "veoveo.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "veoveo.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "veoveo.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- required "serviceAccount.name is required when serviceAccount.create=false" .Values.serviceAccount.name -}}
{{- end -}}
{{- end }}

{{- define "veoveo.image" -}}
{{- $root := index . 0 -}}
{{- $image := index . 1 -}}
{{- $registry := trimSuffix "/" $root.Values.global.imageRegistry -}}
{{- $repository := $image.repository -}}
{{- if $registry -}}{{- $repository = printf "%s/%s" $registry $repository -}}{{- end -}}
{{- if $image.digest -}}
{{- printf "%s@%s" $repository $image.digest -}}
{{- else -}}
{{- printf "%s:%s" $repository $image.tag -}}
{{- end -}}
{{- end }}

{{- define "veoveo.podAnnotations" -}}
{{- with .Values.global.podAnnotations }}{{ toYaml . }}{{ end }}
{{- if .Values.global.serviceMesh.enabled }}
{{- with .Values.global.serviceMesh.podAnnotations }}{{ toYaml . }}{{ end }}
{{- end }}
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
