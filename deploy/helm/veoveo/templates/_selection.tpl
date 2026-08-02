{{- define "veoveo.selectedComponents" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service" "recording-data-plane" "gpu-renderer" "simulation-runtime-support" "console" "telemetry" "ingress") -}}
{{- else if eq .Values.installationPreset "extension-foundation" -}}
{{- toYaml (list "gateway" "platform-store" "object-store" "artifact-service" "recording-data-plane") -}}
{{- else -}}
{{- toYaml .Values.components -}}
{{- end -}}
{{- end -}}

{{- define "veoveo.selectedMcpServers" -}}
{{- if eq .Values.installationPreset "full" -}}
{{- toYaml (list "artifact" "media" "timeseries" "optimization" "frames" "map" "time" "view" "datasheet" "duckdb" "chart" "rerun" "recording" "stream" "reason" "simulation-view") -}}
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
{{- if and (has "gpu-renderer" $components) (not (has "simulation-runtime-support" $components)) -}}
{{- fail "component gpu-renderer requires component simulation-runtime-support" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (not (has "simulation-view" $servers)) -}}
{{- fail "component gpu-renderer requires mcpServer simulation-view" -}}
{{- end -}}
{{- if and (has "simulation-view" $servers) (not (has "gpu-renderer" $components)) -}}
{{- fail "mcpServer simulation-view requires component gpu-renderer" -}}
{{- end -}}
{{- if and (has "simulation-view" $servers) (not (has "frames" $servers)) -}}
{{- fail "mcpServer simulation-view requires mcpServer frames" -}}
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
{{- if and (has "gpu-renderer" $components) (not .Values.global.gpuPlacement.enabled) (ne (get .Values.simulationView.rendererResources.requests "nvidia.com/gpu" | toString) "1") -}}
{{- fail "simulationView.rendererResources.requests must select exactly one nvidia.com/gpu" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (not .Values.global.gpuPlacement.enabled) (ne (get .Values.simulationView.rendererResources.limits "nvidia.com/gpu" | toString) "1") -}}
{{- fail "simulationView.rendererResources.limits must select exactly one nvidia.com/gpu" -}}
{{- end -}}
{{- if and .Values.global.gpuPlacement.enabled (not (regexMatch "^sha256:[a-f0-9]{64}$" .Values.global.gpuPlacement.evidenceDigest)) -}}
{{- fail "global.gpuPlacement.evidenceDigest must be a sha256 digest when DRA placement is enabled" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (gt (int .Values.simulationView.capacity.maximumStreamedCameras) (int .Values.simulationView.capacity.maximumRenderedCameras)) -}}
{{- fail "simulationView maximumStreamedCameras cannot exceed maximumRenderedCameras" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (gt (int .Values.simulationView.capacity.maximumRenderedCameras) (int .Values.simulationView.capacity.maximumLogicalCameras)) -}}
{{- fail "simulationView maximumRenderedCameras cannot exceed maximumLogicalCameras" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (gt (int .Values.simulationView.capacity.maximumStreamedCameras) (int .Values.simulationView.capacity.maximumNvencSessions)) -}}
{{- fail "simulationView maximumStreamedCameras cannot exceed maximumNvencSessions" -}}
{{- end -}}
{{- $lastSlot := sub (int .Values.simulationView.capacity.maximumRenderedCameras) 1 -}}
{{- if and (has "gpu-renderer" $components) (gt (add (int .Values.simulationView.ports.signalingBase) $lastSlot) 65535) -}}
{{- fail "simulationView signaling port range exceeds 65535" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (gt (add (int .Values.simulationView.ports.mediaBase) $lastSlot) 65535) -}}
{{- fail "simulationView media port range exceeds 65535" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (eq .Values.simulationView.media.exposure "NodePort") (gt (add (int .Values.simulationView.media.nodePortBase) $lastSlot) 32767) -}}
{{- fail "simulationView media NodePort range exceeds 32767" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (eq .Values.simulationView.signaling.exposure "Ingress") (or (not (has "ingress" $components)) (not .Values.ingress.enabled)) -}}
{{- fail "simulationView signaling exposure Ingress requires the enabled ingress component" -}}
{{- end -}}
{{- if and (has "gpu-renderer" $components) (lt (int .Values.simulationView.reconciliation.retryMaximumSeconds) (int .Values.simulationView.reconciliation.intervalSeconds)) -}}
{{- fail "simulationView reconciliation retryMaximumSeconds must be greater than or equal to intervalSeconds" -}}
{{- end -}}
{{- end -}}
