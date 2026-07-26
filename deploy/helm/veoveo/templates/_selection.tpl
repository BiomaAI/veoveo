{{- define "veoveo.selectedComponents" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service" "console" "telemetry" "ingress") -}}
{{- else if eq .Values.installationPreset "extension-foundation" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service") -}}
{{- else -}}
{{- toYaml .Values.components -}}
{{- end -}}
{{- end -}}

{{- define "veoveo.selectedMcpServers" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "artifact" "media" "timeseries" "optimization" "frames" "map" "time" "view" "datasheet" "duckdb" "chart" "rerun" "recording" "perception") -}}
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
{{- $artifactServers := list "artifact" "media" "timeseries" "optimization" "frames" "map" "datasheet" "duckdb" "recording" "perception" "reason" -}}
{{- range $server := $servers -}}
{{- if and (has $server $artifactServers) (not (has "artifact-service" $components)) -}}
{{- fail (printf "mcpServer %s requires component artifact-service" $server) -}}
{{- end -}}
{{- end -}}
{{- if and (or (has "perception" $servers) (has "reason" $servers)) (not (has "recording" $servers)) -}}
{{- fail "mcpServers perception and reason require mcpServer recording" -}}
{{- end -}}
{{- end -}}
