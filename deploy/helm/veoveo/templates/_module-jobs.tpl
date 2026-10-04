{{- define "veoveo.moduleJob" -}}
{{- $root := index . 0 -}}
{{- $kind := index . 1 -}}
{{- $lane := index . 2 -}}
{{- $spec := include "veoveo.moduleJobSpec" . | fromYaml -}}
{{- $revision := $spec | toJson | sha256sum -}}
{{- $_ := set $spec.template.metadata.annotations "veoveo.ai/module-job-revision" $revision -}}
{{- $name := $kind -}}
{{- if $lane }}{{- $name = printf "module-%s" $lane.module -}}{{ end -}}
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ include "veoveo.revisionedJobName" (list $root $name $revision) }}
  labels:
    {{- include "veoveo.labels" $root | nindent 4 }}
    app.kubernetes.io/component: {{ $kind }}
    {{- if $lane }}
    veoveo.ai/module: {{ $lane.module | quote }}
    {{- end }}
  annotations:
    veoveo.ai/module-job-revision: {{ $revision | quote }}
spec:
  {{- toYaml $spec | nindent 2 }}
{{- end -}}

{{- define "veoveo.moduleJobSpec" -}}
{{- $root := index . 0 -}}
{{- $kind := index . 1 -}}
{{- $lane := index . 2 -}}
{{- $plan := include "veoveo.modulePlan" $root | fromJson -}}
backoffLimit: 0
activeDeadlineSeconds: 1800
ttlSecondsAfterFinished: 3600
template:
  metadata:
    labels:
      {{- include "veoveo.selectorLabels" $root | nindent 6 }}
      app.kubernetes.io/component: {{ $kind }}
      {{- if $lane }}
      veoveo.ai/module: {{ $lane.module | quote }}
      {{- end }}
    annotations:
      {{- include "veoveo.podAnnotations" $root | nindent 6 }}
      {{- include "veoveo.databasePodAnnotations" $root | nindent 6 }}
      veoveo.ai/installation-generation: {{ $plan.generation | quote }}
      checksum/module-plan: {{ $root.Values.moduleInstallation.planJson | sha256sum | quote }}
      {{- if eq $kind "control-plane-publication" }}
      veoveo.ai/control-plane-revision: {{ include "veoveo.controlPlaneRevision" $root | quote }}
      {{- end }}
  spec:
    restartPolicy: Never
    serviceAccountName: {{ include "veoveo.serviceAccountName" $root }}
    automountServiceAccountToken: false
    imagePullSecrets:
      {{- toYaml $root.Values.global.imagePullSecrets | nindent 6 }}
    securityContext:
      {{- include "veoveo.podSecurityContext" $root | nindent 6 }}
    containers:
      - name: {{ $kind }}
        image: {{ include "veoveo.ownedImage" (list $root $root.Values.gateway.image) }}
        imagePullPolicy: {{ $root.Values.global.imagePullPolicy }}
        {{- if $lane }}
        command: {{ list (first $lane.command) | toJson }}
        args: {{ rest $lane.command | toJson }}
        {{- else if eq $kind "installation-prepare" }}
        args: [installation-prepare]
        {{- else }}
        args:
          - control-plane-publish
          - --control-plane
          - /etc/veoveo/gateway/{{ $root.Values.gateway.controlPlaneKey }}
          - --applied-by
          - installation-bootstrap
        {{- end }}
        env:
          {{- include "veoveo.modulePlanEnv" $root | nindent 10 }}
          {{- if eq $kind "control-plane-publication" }}
          {{- include "veoveo.surrealEnv" $root | nindent 10 }}
          {{- else }}
          {{- include "veoveo.migrationEnv" $root | nindent 10 }}
          {{- end }}
          {{- if eq $kind "installation-prepare" }}
          - name: VEOVEO_SURREAL_RUNTIME_PASSWORD
            valueFrom:
              secretKeyRef:
                name: {{ $root.Values.surrealdb.runtimeExistingSecret }}
                key: password
          {{- end }}
        securityContext:
          {{- include "veoveo.containerSecurityContext" $root | nindent 10 }}
        resources:
          {{- toYaml $root.Values.gateway.resources | nindent 10 }}
        volumeMounts:
          - name: module-plan
            mountPath: /etc/veoveo/modules
            readOnly: true
          {{- if eq $kind "control-plane-publication" }}
          - name: control-plane
            mountPath: /etc/veoveo/gateway
            readOnly: true
          {{- end }}
          - name: tmp
            mountPath: /tmp
    volumes:
      - name: module-plan
        configMap:
          name: {{ include "veoveo.modulePlanName" $root }}
      {{- if eq $kind "control-plane-publication" }}
      - name: control-plane
        configMap:
          name: {{ $root.Values.gateway.existingControlPlaneConfigMap }}
      {{- end }}
      - name: tmp
        emptyDir: {}
{{- end -}}
