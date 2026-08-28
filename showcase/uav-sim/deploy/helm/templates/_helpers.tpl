{{- define "uav-sim.labels" -}}
{{ include "veoveo-extension.labels" (dict
    "name" "uav-sim"
    "releaseName" .Release.Name
    "managedBy" .Release.Service
    "chart" (printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_")
    "installation" .Values.platform.installationId
    "component" "uav-sim"
  ) }}
{{- end }}

{{- define "uav-sim.componentLabels" -}}
{{- $root := .root -}}
{{ include "veoveo-extension.labels" (dict
    "name" .name
    "releaseName" $root.Release.Name
    "managedBy" $root.Release.Service
    "chart" (printf "%s-%s" $root.Chart.Name $root.Chart.Version | replace "+" "_")
    "installation" $root.Values.platform.installationId
    "component" .component
  ) }}
{{- end }}

{{- define "uav-sim.selectorLabels" -}}
{{- include "uav-sim.runtimeSelectorLabels" . -}}
{{- end }}

{{- define "uav-sim.runtimeSelectorLabels" -}}
{{ include "veoveo-extension.selectorLabels" (dict
    "name" "uav-sim"
    "releaseName" .Release.Name
    "installation" .Values.platform.installationId
    "component" "uav-sim"
  ) }}
{{- end }}

{{- define "uav-sim.mcpLabels" -}}
{{ include "veoveo-extension.labels" (dict
    "name" "uav-sim-mcp"
    "releaseName" .Release.Name
    "managedBy" .Release.Service
    "chart" (printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_")
    "installation" .Values.platform.installationId
    "component" "uav-sim-mcp"
  ) }}
{{- end }}

{{- define "uav-sim.mcpSelectorLabels" -}}
{{ include "veoveo-extension.selectorLabels" (dict
    "name" "uav-sim-mcp"
    "releaseName" .Release.Name
    "installation" .Values.platform.installationId
    "component" "uav-sim-mcp"
  ) }}
{{- end }}

{{- define "uav-sim.image" -}}
{{- $root := index . 0 -}}
{{- $image := index . 1 -}}
{{- $lockedDigest := get $root.Values.global.imageDigests $image.repository | default "" -}}
{{- $digest := $image.digest | default $lockedDigest -}}
{{- $tag := default $image.tag $root.Values.global.veoveoTag -}}
{{- include "veoveo-extension.image" (dict
    "registry" $root.Values.global.veoveoRegistry
    "production" $root.Values.global.production
    "image" (dict "repository" $image.repository "tag" $tag "digest" $digest)
  ) -}}
{{- end }}

{{- define "uav-sim.podSecurityContext" -}}
{{ include "veoveo-extension.podSecurityContext" . }}
{{- end }}

{{- define "uav-sim.containerSecurityContext" -}}
{{ include "veoveo-extension.containerSecurityContext" . }}
{{- end }}

{{- define "uav-sim.runtimeEnv" -}}
- name: CESIUM_ION_ACCESS_TOKEN
  valueFrom:
    secretKeyRef:
      name: {{ .root.Values.platform.cesiumSecret }}
      key: {{ .root.Values.platform.cesiumTokenKey }}
- name: UAV_SIM_WORLD_SOURCE
  value: {{ .root.Values.world.source | quote }}
- name: UAV_SIM_CESIUM_ION_ASSET_ID
  value: {{ printf "%.0f" .root.Values.world.cesiumIonAssetId | quote }}
- name: UAV_SIM_TILE_CACHE_POLICY
  value: {{ .root.Values.cache.policy | quote }}
- name: UAV_SIM_TILE_MAXIMUM_SCREEN_SPACE_ERROR
  value: {{ .root.Values.world.streaming.maximumScreenSpaceError | quote }}
- name: UAV_SIM_TILE_MAXIMUM_SIMULTANEOUS_LOADS
  value: {{ .root.Values.world.streaming.maximumSimultaneousLoads | quote }}
- name: UAV_SIM_TILE_MAXIMUM_CACHED_BYTES
  value: {{ printf "%.0f" .root.Values.world.streaming.maximumCachedBytes | quote }}
- name: UAV_SIM_TILE_PRELOAD_ANCESTORS
  value: {{ .root.Values.world.streaming.preloadAncestors | quote }}
- name: UAV_SIM_TILE_PRELOAD_SIBLINGS
  value: {{ .root.Values.world.streaming.preloadSiblings | quote }}
- name: UAV_SIM_TILE_FORBID_HOLES
  value: {{ .root.Values.world.streaming.forbidHoles | quote }}
- name: XDG_CACHE_HOME
  value: {{ printf "/var/lib/veoveo/runtime-cache/%s" .root.Values.cache.version | quote }}
- name: UAV_SIM_SESSION_ID
  value: {{ .sessionId | quote }}
- name: UAV_SIM_ADAPTER_BEARER_TOKEN
  valueFrom:
    secretKeyRef:
      name: {{ .root.Values.platform.adapterSecret }}
      key: {{ .root.Values.platform.adapterTokenKey }}
- name: UAV_SIM_VEHICLE_COUNT
  value: {{ .root.Values.session.vehicleCount | quote }}
- name: UAV_SIM_PHYSICS_HZ
  value: {{ .root.Values.session.physicsHz | quote }}
- name: UAV_SIM_RENDERING_HZ
  value: {{ .root.Values.session.renderingHz | quote }}
- name: UAV_SIM_CAMERA_VEHICLE_ID
  value: {{ .root.Values.session.camera.vehicleId | quote }}
- name: UAV_SIM_OPERATOR_CAMERAS_JSON
  value: {{ .root.Values.liveView.cameras | toJson | quote }}
- name: UAV_SIM_OPERATOR_RTSP_PORT_BASE
  value: {{ .root.Values.liveView.operatorRtspPortBase | quote }}
- name: UAV_SIM_TILE_READY_FRAMES
  value: {{ .root.Values.session.tileReadyFrames | quote }}
- name: UAV_SIM_PX4_CONNECT_TIMEOUT_SECONDS
  value: {{ .root.Values.session.px4ConnectTimeoutSeconds | quote }}
- name: UAV_SIM_FLEET_LOOP_RELATIVE_ALTITUDE_M
  value: {{ .root.Values.session.fleetLoop.relativeAltitudeM | quote }}
- name: UAV_SIM_FLEET_LOOP_VERTICAL_SEPARATION_M
  value: {{ .root.Values.session.fleetLoop.verticalSeparationM | quote }}
- name: UAV_SIM_FLEET_LOOP_TAKEOFF_TIMEOUT_SECONDS
  value: {{ .root.Values.session.fleetLoop.takeoffTimeoutSeconds | quote }}
- name: UAV_SIM_FLEET_LOOP_CENTER_EAST_M
  value: {{ .root.Values.session.fleetLoop.centerEastM | quote }}
- name: UAV_SIM_FLEET_LOOP_CENTER_NORTH_M
  value: {{ .root.Values.session.fleetLoop.centerNorthM | quote }}
- name: UAV_SIM_FLEET_LOOP_EAST_RADIUS_M
  value: {{ .root.Values.session.fleetLoop.eastRadiusM | quote }}
- name: UAV_SIM_FLEET_LOOP_NORTH_RADIUS_M
  value: {{ .root.Values.session.fleetLoop.northRadiusM | quote }}
- name: UAV_SIM_FLEET_LOOP_RADIAL_SEPARATION_M
  value: {{ .root.Values.session.fleetLoop.radialSeparationM | quote }}
- name: UAV_SIM_FLEET_LOOP_WAYPOINT_COUNT
  value: {{ .root.Values.session.fleetLoop.waypointCount | quote }}
- name: UAV_SIM_FLEET_LOOP_SPEED_MPS
  value: {{ .root.Values.session.fleetLoop.speedMps | quote }}
- name: UAV_SIM_FLEET_LOOP_HOLD_SECONDS
  value: {{ .root.Values.session.fleetLoop.holdSeconds | quote }}
- name: UAV_SIM_CAMERA_WIDTH
  value: {{ .root.Values.session.camera.width | quote }}
- name: UAV_SIM_CAMERA_HEIGHT
  value: {{ .root.Values.session.camera.height | quote }}
- name: UAV_SIM_CAMERA_FPS
  value: {{ .root.Values.session.camera.fps | quote }}
- name: UAV_SIM_RECORDING_TELEMETRY_HZ
  value: {{ .root.Values.recording.telemetryHz | quote }}
- name: UAV_SIM_RECORDING_QUEUE_CAPACITY
  value: {{ .root.Values.recording.queueCapacity | quote }}
- name: UAV_SIM_RECORDING_MAP_PROVIDER
  value: {{ .root.Values.recording.mapProvider | quote }}
- name: UAV_SIM_RECORDING_MAXIMUM_SEGMENT_BYTES
  value: {{ .root.Values.recording.maximumSegmentBytes | quote }}
- name: UAV_SIM_RECORDING_MAXIMUM_SEGMENT_SECONDS
  value: {{ .root.Values.recording.maximumSegmentSeconds | quote }}
- name: UAV_SIM_CAMERA_FOCAL_LENGTH_MM
  value: {{ .root.Values.session.camera.optics.focalLengthMm | quote }}
- name: UAV_SIM_CAMERA_CLIPPING_NEAR_M
  value: {{ .root.Values.session.camera.optics.clippingRangeM.near | quote }}
- name: UAV_SIM_CAMERA_CLIPPING_FAR_M
  value: {{ .root.Values.session.camera.optics.clippingRangeM.far | quote }}
- name: UAV_SIM_CAMERA_TRANSLATION_X_M
  value: {{ .root.Values.session.camera.mount.translationM.x | quote }}
- name: UAV_SIM_CAMERA_TRANSLATION_Y_M
  value: {{ .root.Values.session.camera.mount.translationM.y | quote }}
- name: UAV_SIM_CAMERA_TRANSLATION_Z_M
  value: {{ .root.Values.session.camera.mount.translationM.z | quote }}
- name: UAV_SIM_CAMERA_ORIENTATION_W
  value: {{ .root.Values.session.camera.mount.orientationWxyz.w | quote }}
- name: UAV_SIM_CAMERA_ORIENTATION_X
  value: {{ .root.Values.session.camera.mount.orientationWxyz.x | quote }}
- name: UAV_SIM_CAMERA_ORIENTATION_Y
  value: {{ .root.Values.session.camera.mount.orientationWxyz.y | quote }}
- name: UAV_SIM_CAMERA_ORIENTATION_Z
  value: {{ .root.Values.session.camera.mount.orientationWxyz.z | quote }}
- name: UAV_SIM_RECORDING_KEY
  valueFrom:
    fieldRef:
      fieldPath: metadata.uid
{{- if .root.Values.streamPublication.enabled }}
- name: UAV_SIM_STREAM_HOST
  value: {{ .root.Values.streamPublication.endpointHost | quote }}
- name: UAV_SIM_STREAM_PORT
  value: {{ .root.Values.streamPublication.endpointPort | quote }}
- name: UAV_SIM_STREAM_PAYLOAD_TYPE
  value: {{ .root.Values.streamPublication.payloadType | quote }}
- name: UAV_SIM_STREAM_SOURCE_VEHICLE_ID
  value: {{ .root.Values.streamPublication.sourceVehicleId | quote }}
- name: UAV_SIM_STREAM_QUEUE_CAPACITY
  value: {{ .root.Values.streamPublication.queueCapacity | quote }}
{{- end }}

- name: NVIDIA_DRIVER_CAPABILITIES
  value: compute,graphics,utility,video
{{- if .root.Values.session.privacyConsent }}
- name: PRIVACY_CONSENT
  value: "Y"
{{- end }}
{{- end }}

{{- define "uav-sim.validateLiveView" -}}
{{- $cameraCount := len .Values.liveView.cameras -}}
{{- if or (lt $cameraCount 1) (gt $cameraCount 32) -}}
{{- fail "liveView.cameras must contain 1-32 logical cameras" -}}
{{- end -}}
{{- if eq (int .Values.liveView.streamGatePort) (int .Values.service.port) -}}
{{- fail "liveView.streamGatePort must differ from service.port" -}}
{{- end -}}
{{- if or (lt (int .Values.liveView.streamGatePort) 1) (gt (int .Values.liveView.streamGatePort) 65535) -}}
{{- fail "liveView.streamGatePort must be between 1 and 65535" -}}
{{- end -}}
{{- $lastRtspPort := add (int .Values.liveView.operatorRtspPortBase) (sub (mul $cameraCount 2) 1) -}}
{{- if or (lt (int .Values.liveView.operatorRtspPortBase) 1) (gt $lastRtspPort 65535) -}}
{{- fail "liveView operator RTSP port range exceeds 65535" -}}
{{- end -}}
{{- if not (regexMatch "^(ws|wss)://[^/@[:space:]]+(/[^[:space:]]*)?$" .Values.liveView.publicStreamUrl) -}}
{{- fail "liveView.publicStreamUrl must be an absolute credential-free ws or wss URL" -}}
{{- end -}}
{{- if and .Values.liveView.streamIngress.enabled (empty .Values.liveView.streamIngress.host) -}}
{{- fail "liveView.streamIngress.host is required when stream ingress is enabled" -}}
{{- end -}}
{{- end -}}

{{- define "uav-sim.recordingForwarder" -}}
{{- $image := .root.Values.images.forwarder -}}
{{- $lockedDigest := get .root.Values.global.imageDigests $image.repository | default "" -}}
{{- include "veoveo-extension.recordingForwarder" (dict
    "image" (dict
      "repository" $image.repository
      "tag" (default $image.tag .root.Values.global.veoveoTag)
      "digest" (default $lockedDigest $image.digest)
    )
    "registry" .root.Values.global.veoveoRegistry
    "production" .root.Values.global.production
    "imagePullPolicy" .root.Values.images.pullPolicy
    "gatewayUrl" (printf "%s/" (trimSuffix "/" .root.Values.platform.publicBaseUrl))
    "gatewayTransportUrl" (printf "%s/" (trimSuffix "/" .root.Values.recordingForwarder.gatewayTransportUrl))
    "protectedResource" (printf "%s/ingest/recordings" (trimSuffix "/" .root.Values.platform.publicBaseUrl))
    "clientId" .root.Values.recordingForwarder.clientId
    "keyId" .root.Values.recordingForwarder.keyId
    "signingAlgorithm" .root.Values.recordingForwarder.signingAlgorithm
    "maximumQueueBytes" (printf "%.0f" .root.Values.recordingForwarder.maximumQueueBytes)
    "batchMessageLimit" .root.Values.recordingForwarder.batchMessageLimit
    "grpcMemoryLimitBytes" (printf "%.0f" .root.Values.recordingForwarder.grpcMemoryLimitBytes)
    "finishSupersededRecordings" true
    "restartableInit" false
    "resources" .root.Values.recordingForwarder.resources
  ) -}}
{{- end }}
