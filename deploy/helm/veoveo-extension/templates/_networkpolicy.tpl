{{- define "veoveo-extension.networkPolicy" -}}
{{- $name := required "networkPolicy requires name" .name -}}
{{- $namespaceSelector := required "networkPolicy requires dnsNamespaceSelector" .dnsNamespaceSelector -}}
{{- $selector := dict "installation" .installation "component" .component -}}
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: {{ printf "%s-default-deny" $name | quote }}
  labels:
    {{- include "veoveo-extension.labels" .labels | nindent 4 }}
spec:
  podSelector:
    matchLabels:
      {{- include "veoveo-extension.componentSelector" $selector | nindent 6 }}
  policyTypes: [Ingress, Egress]
---
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: {{ printf "%s-integration" $name | quote }}
  labels:
    {{- include "veoveo-extension.labels" .labels | nindent 4 }}
spec:
  podSelector:
    matchLabels:
      {{- include "veoveo-extension.componentSelector" $selector | nindent 6 }}
  policyTypes: [Ingress, Egress]
  ingress:
    - from:
        - podSelector:
            matchLabels:
              {{- include "veoveo-extension.componentSelector" (dict "installation" .installation "component" "gateway") | nindent 14 }}
      ports:
        - protocol: TCP
          port: {{ required "networkPolicy requires mcpPort" .mcpPort }}
  egress:
    - to:
        - podSelector:
            matchLabels:
              {{- include "veoveo-extension.installationSelector" (dict "installation" .installation) | nindent 14 }}
    - to:
        - namespaceSelector:
            matchLabels:
              {{- toYaml $namespaceSelector | nindent 14 }}
      ports:
        - protocol: UDP
          port: 53
        - protocol: TCP
          port: 53
    {{- range .egress }}
    - to:
        {{- if .component }}
        - podSelector:
            matchLabels:
              {{- include "veoveo-extension.componentSelector" (dict "installation" $.installation "component" .component) | nindent 14 }}
        {{- else if .cidr }}
        - ipBlock:
            cidr: {{ .cidr | quote }}
        {{- else }}
        {{- fail "networkPolicy egress requires component or cidr" }}
        {{- end }}
      ports:
        - protocol: {{ default "TCP" .protocol }}
          port: {{ required "networkPolicy egress requires port" .port }}
    {{- end }}
{{- end -}}
