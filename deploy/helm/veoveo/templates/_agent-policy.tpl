{{/* Shared Pod-spec admission; installation-owned CEL, never author input. */}}
{{- define "veoveo.agentPodValidations" -}}
- expression: >-
    variables.owner.matches('^agent-[a-f0-9]{32}$') &&
    variables.meta.annotations['veoveo.ai/managed-generation'].matches('^[1-9][0-9]*$') &&
    variables.meta.labels.all(k, k in ['veoveo.ai/managed-agent', 'pod-template-hash'])
  message: Managed workload identity and generation are required.
- expression: >-
    variables.pod.serviceAccountName == 'veoveo-agent-kernel' &&
    has(variables.pod.automountServiceAccountToken) && !variables.pod.automountServiceAccountToken &&
    (!has(variables.pod.hostNetwork) || !variables.pod.hostNetwork) &&
    (!has(variables.pod.hostPID) || !variables.pod.hostPID) &&
    (!has(variables.pod.hostIPC) || !variables.pod.hostIPC) &&
    (!has(variables.pod.initContainers) || size(variables.pod.initContainers) == 0) &&
    (!has(variables.pod.ephemeralContainers) || size(variables.pod.ephemeralContainers) == 0) &&
    size(variables.pod.containers) == 1 &&
    variables.pod.securityContext.runAsNonRoot &&
    variables.pod.securityContext.runAsUser == 10001 &&
    variables.pod.securityContext.runAsGroup == 10001 &&
    variables.pod.securityContext.fsGroup == 10001 &&
    variables.pod.securityContext.seccompProfile.type == 'RuntimeDefault'
  message: Kernels require an isolated unprivileged service account and security context.
- expression: >-
    variables.kernel.name == 'agent' && !has(variables.kernel.command) &&
    variables.kernel.args == ['run', '--manifest', '/etc/veoveo/agent/manifest.json', '--data-dir', '/var/lib/veoveo/agent'] &&
    (!has(variables.kernel.envFrom) || size(variables.kernel.envFrom) == 0) &&
    !has(variables.kernel.lifecycle) &&
    !variables.kernel.securityContext.allowPrivilegeEscalation &&
    variables.kernel.securityContext.readOnlyRootFilesystem &&
    variables.kernel.securityContext.capabilities.drop == ['ALL'] &&
    (!has(variables.kernel.securityContext.capabilities.add) || size(variables.kernel.securityContext.capabilities.add) == 0) &&
    (!has(variables.kernel.securityContext.privileged) || !variables.kernel.securityContext.privileged) &&
    !has(variables.kernel.securityContext.runAsUser) &&
    !has(variables.kernel.securityContext.runAsGroup) &&
    (!has(variables.kernel.securityContext.runAsNonRoot) || variables.kernel.securityContext.runAsNonRoot) &&
    !has(variables.kernel.securityContext.seccompProfile) &&
    variables.kernel.readinessProbe.exec.command == ['/usr/bin/test', '-f', '/tmp/veoveo-managed-ready'] &&
    !has(variables.kernel.livenessProbe) && !has(variables.kernel.startupProbe)
  message: Only the fixed kernel entrypoint and readiness probe are admitted.
- expression: >-
    size(variables.pod.volumes) == 3 &&
    variables.pod.volumes.exists(v, v.name == 'memory' && has(v.persistentVolumeClaim) && v.persistentVolumeClaim.claimName == variables.owner + '-memory') &&
    variables.pod.volumes.exists(v, v.name == 'tmp' && has(v.emptyDir) && quantity(v.emptyDir.sizeLimit) == quantity('128Mi')) &&
    size(variables.kernel.volumeMounts) == 3 &&
    variables.kernel.volumeMounts.all(v, !has(v.subPath) && !has(v.subPathExpr) && !has(v.mountPropagation)) &&
    variables.kernel.volumeMounts.exists(v, v.name == 'memory' && v.mountPath == '/var/lib/veoveo/agent' && (!has(v.readOnly) || !v.readOnly)) &&
    variables.kernel.volumeMounts.exists(v, v.name == 'tmp' && v.mountPath == '/tmp' && (!has(v.readOnly) || !v.readOnly)) &&
    variables.kernel.volumeMounts.exists(v, v.name == 'config' && v.mountPath == '/etc/veoveo/agent' && has(v.readOnly) && v.readOnly)
  message: Kernels may mount only their retained memory, approved configuration and bounded temporary storage.
- expression: >-
    variables.kernel.env.all(e,
      has(e.valueFrom) ?
        (has(e.valueFrom.fieldRef) ? e.name == 'VEOVEO_AGENT_POD_UID' && e.valueFrom.fieldRef.fieldPath == 'metadata.uid' :
          has(e.valueFrom.secretKeyRef) && e.name in ['VEOVEO_MANAGED_PRIVATE_KEY', 'VEOVEO_MANAGED_MODEL_KEY', 'VEOVEO_SURREAL_USERNAME', 'VEOVEO_SURREAL_PASSWORD']) :
        e.name in {{ concat (list "VEOVEO_AGENT_TENANT" "VEOVEO_AGENT_ID" "VEOVEO_AGENT_NAME" "VEOVEO_AGENT_PROFILE" "VEOVEO_AGENT_WORK_CONTEXT" "VEOVEO_AGENT_CLIENT_ID" "VEOVEO_AGENT_KEY_ID" "VEOVEO_GATEWAY_URL" "VEOVEO_GATEWAY_TRANSPORT_URL" "VEOVEO_GATEWAY_AUDIENCE" "VEOVEO_GATEWAY_RESOURCE" "VEOVEO_AGENT_MODEL_URL" "VEOVEO_AGENT_MODEL_ID" "VEOVEO_MANAGED_GENERATION" "VEOVEO_MANAGED_MODEL" "VEOVEO_SURREAL_ENDPOINT" "VEOVEO_SURREAL_NAMESPACE" "VEOVEO_SURREAL_DATABASE" "VEOVEO_SURREAL_AUTH_LEVEL" "RUST_LOG") .parameterNames | uniq | toJson }}
    ) &&
    variables.kernel.env.exists(e, e.name == 'VEOVEO_MANAGED_PRIVATE_KEY' && e.valueFrom.secretKeyRef.name == variables.owner + '-key' && e.valueFrom.secretKeyRef.key == 'private-key-der-b64') &&
    variables.kernel.env.exists(e, e.name == 'VEOVEO_GATEWAY_URL' && e.value == {{ printf "%s/" (trimSuffix "/" .root.Values.global.publicBaseUrl) | toJson }}) &&
    variables.kernel.env.exists(e, e.name == 'VEOVEO_GATEWAY_TRANSPORT_URL' && e.value == {{ printf "http://mcp-gateway.%s.svc:8788/" .root.Release.Namespace | toJson }}) &&
    variables.kernel.env.exists(e, e.name == 'VEOVEO_SURREAL_ENDPOINT' && e.value == {{ printf "ws://surrealdb.%s.svc:8000" .root.Release.Namespace | toJson }})
  message: Kernel environment and credential destinations must follow the installed contract.
- expression: >-
    {{- range $index, $template := .root.Values.gateway.agents.templates }}
    {{ if $index }} || {{ end }}(
      variables.kernel.image == {{ $template.workload.image | toJson }} &&
      variables.kernel.resources.requests == variables.kernel.resources.limits &&
      size(variables.kernel.resources.limits) == 2 &&
      quantity(variables.kernel.resources.limits['cpu']) == quantity({{ printf "%vm" $template.workload.cpu_millis | toJson }}) &&
      quantity(variables.kernel.resources.limits['memory']) == quantity({{ printf "%vMi" $template.workload.memory_mib | toJson }}) &&
      variables.pod.volumes.exists(v, v.name == 'config' && has(v.configMap) && v.configMap.name == {{ $template.workload.config_map | toJson }}) &&
      variables.kernel.env.exists(e, e.name == 'VEOVEO_SURREAL_USERNAME' && e.valueFrom.secretKeyRef.name == {{ $template.workload.database_secret | toJson }} && e.valueFrom.secretKeyRef.key == 'username') &&
      variables.kernel.env.exists(e, e.name == 'VEOVEO_SURREAL_PASSWORD' && e.valueFrom.secretKeyRef.name == {{ $template.workload.database_secret | toJson }} && e.valueFrom.secretKeyRef.key == 'password') &&
      ({{- $firstModel := true }}{{ range $model := $.root.Values.gateway.agents.models }}{{ if has $model.id $template.models }}
      {{ if not $firstModel }} || {{ end }}{{ $firstModel = false }}(
        variables.kernel.env.exists(e, e.name == 'VEOVEO_AGENT_MODEL_URL' && e.value == {{ $model.base_url | toJson }}) &&
        variables.kernel.env.exists(e, e.name == 'VEOVEO_AGENT_MODEL_ID' && e.value == {{ $model.model | toJson }})
      ){{ end }}{{ end }}{{ if $firstModel }}false{{ end }}) &&
      variables.kernel.env.exists(e, e.name == 'VEOVEO_MANAGED_MODEL_KEY' && (
      {{- range $i, $secret := $template.workload.model_secrets }}
      {{ if $i }} || {{ end }}(e.valueFrom.secretKeyRef.name == {{ $secret.secret | toJson }} && e.valueFrom.secretKeyRef.key == {{ $secret.key | toJson }})
      {{- end }}))
    )
    {{- end }}
  message: Image, resources, template and Secret references must match one approved runtime template.
{{- end }}
