{{- define "veoveo.selectedComponents" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service" "recording-data-plane" "simulation-runtime-support" "agent-runtime-support" "console" "telemetry" "ingress") -}}
{{- else if eq .Values.installationPreset "extension-foundation" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service" "recording-data-plane") -}}
{{- else -}}
{{- toYaml .Values.components -}}
{{- end -}}
{{- end -}}

{{- define "veoveo.selectedMcpServers" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "artifact" "media" "timeseries" "optimization" "frames" "map" "time" "view" "datasheet" "duckdb" "chart" "rerun" "recording" "stream" "reason") -}}
{{- else if eq .Values.installationPreset "extension-foundation" -}}
{{- toYaml (list "artifact" "frames" "recording") -}}
{{- else -}}
{{- toYaml .Values.mcpServers -}}
{{- end -}}
{{- end -}}

{{- define "veoveo.componentEnabled" -}}
{{- $root := index . 0 -}}
{{- $component := index . 1 -}}
{{- $selected := include "veoveo.selectedComponents" $root | fromYamlArray -}}
{{- if has $component $selected -}}true{{- end -}}
{{- end -}}

{{- define "veoveo.mcpServerEnabled" -}}
{{- $root := index . 0 -}}
{{- $server := index . 1 -}}
{{- $selected := include "veoveo.selectedMcpServers" $root | fromYamlArray -}}
{{- if has $server $selected -}}true{{- end -}}
{{- end -}}

{{- define "veoveo.validateSelection" -}}
{{- $components := include "veoveo.selectedComponents" . | fromYamlArray -}}
{{- $servers := include "veoveo.selectedMcpServers" . | fromYamlArray -}}
{{- if and (ne .Values.installationPreset "custom") (or .Values.components .Values.mcpServers) -}}
{{- fail "components and mcpServers are valid only with installationPreset=custom" -}}
{{- end -}}
{{- if and $servers (not (has "gateway" $components)) -}}
{{- fail "selected MCP servers require component gateway" -}}
{{- end -}}
{{- if and (has "artifact-service" $components) (not (has "platform-store" $components)) -}}
{{- fail "component artifact-service requires component platform-store" -}}
{{- end -}}
{{- if and (has "artifact-service" $components) (not (has "object-store" $components)) -}}
{{- fail "component artifact-service requires component object-store" -}}
{{- end -}}
{{- if and (has "recording-data-plane" $components) (not (has "platform-store" $components)) -}}
{{- fail "component recording-data-plane requires component platform-store" -}}
{{- end -}}
{{- if and (has "recording-data-plane" $components) (not (has "artifact-service" $components)) -}}
{{- fail "component recording-data-plane requires component artifact-service" -}}
{{- end -}}
{{- if and (has "agent-runtime-support" $components) (not (has "gateway" $components)) -}}
{{- fail "component agent-runtime-support requires component gateway" -}}
{{- end -}}
{{- if and (has "agent-runtime-support" $components) (not (has "platform-store" $components)) -}}
{{- fail "component agent-runtime-support requires component platform-store" -}}
{{- end -}}
{{- $artifactServers := list "artifact" "media" "timeseries" "optimization" "frames" "map" "datasheet" "duckdb" "recording" "stream" "reason" -}}
{{- range $server := $servers -}}
{{- if and (has $server $artifactServers) (not (has "artifact-service" $components)) -}}
{{- fail (printf "mcpServer %s requires component artifact-service" $server) -}}
{{- end -}}
{{- end -}}
{{- if and (has "reason" $servers) (not (has "recording" $servers)) -}}
{{- fail "mcpServer reason requires mcpServer recording" -}}
{{- end -}}
{{- if and (has "recording" $servers) (not (has "recording-data-plane" $components)) -}}
{{- fail "mcpServer recording requires component recording-data-plane" -}}
{{- end -}}
{{- if and .Values.global.gpuPlacement.enabled (not (regexMatch "^sha256:[a-f0-9]{64}$" .Values.global.gpuPlacement.evidenceDigest)) -}}
{{- fail "global.gpuPlacement.evidenceDigest must be a sha256 digest when DRA placement is enabled" -}}
{{- end -}}
{{- end -}}
