{{- define "veoveo-extension.recordingForwarder" -}}
- name: recording-forwarder
  {{- if .restartableInit }}
  restartPolicy: Always
  {{- end }}
  image: {{ include "veoveo-extension.image" (dict
      "image" .image
      "registry" .registry
      "production" .production
    ) }}
  imagePullPolicy: {{ default "IfNotPresent" .imagePullPolicy }}
  args:
    - --gateway-url
    - {{ required "recordingForwarder requires gatewayUrl" .gatewayUrl | quote }}
    - --gateway-transport-url
    - {{ required "recordingForwarder requires gatewayTransportUrl" .gatewayTransportUrl | quote }}
    - --protected-resource
    - {{ required "recordingForwarder requires protectedResource" .protectedResource | quote }}
    - --client-id
    - {{ required "recordingForwarder requires clientId" .clientId | quote }}
    - --key-id
    - {{ required "recordingForwarder requires keyId" .keyId | quote }}
    - --signing-algorithm
    - {{ default "EdDSA" .signingAlgorithm | quote }}
    - --private-key-pem-file
    - /run/secrets/recording-producer/private-key.pem
    - --queue-dir
    - /var/lib/veoveo-recording-forwarder
    - --maximum-queue-bytes
    - {{ required "recordingForwarder requires maximumQueueBytes" .maximumQueueBytes | quote }}
    - --batch-message-limit
    - {{ required "recordingForwarder requires batchMessageLimit" .batchMessageLimit | quote }}
    - --grpc-memory-limit-bytes
    - {{ required "recordingForwarder requires grpcMemoryLimitBytes" .grpcMemoryLimitBytes | quote }}
    {{- if .finishSupersededRecordings }}
    - --finish-superseded-recordings
    {{- end }}
  startupProbe:
    exec:
      command: [nc, -z, 127.0.0.1, "9876"]
    periodSeconds: 2
    failureThreshold: 150
  env:
    - name: RUST_LOG
      value: info
  securityContext:
    {{- include "veoveo-extension.containerSecurityContext" . | nindent 4 }}
  resources:
    {{- toYaml (required "recordingForwarder requires resources" .resources) | nindent 4 }}
  volumeMounts:
    - name: recording-forwarder-queue
      mountPath: /var/lib/veoveo-recording-forwarder
    - name: recording-producer-key
      mountPath: /run/secrets/recording-producer
      readOnly: true
{{- end -}}
