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

{{- define "uav-sim.selectorLabels" -}}
{{ include "veoveo-extension.selectorLabels" (dict
    "name" "uav-sim"
    "releaseName" .Release.Name
    "installation" .Values.platform.installationId
    "component" "uav-sim"
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
- name: UAV_SIM_RUNTIME_EVENT_SOCKET
  value: /var/run/veoveo-uav-sim/runtime-events.sock
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
- name: UAV_SIM_LIVE_VIEWER_SLOTS
  value: {{ .root.Values.liveView.viewerSlots | quote }}
- name: UAV_SIM_LIVE_ACTIVATION_TIMEOUT_SECONDS
  value: {{ .root.Values.liveView.activationTimeoutSeconds | quote }}
- name: UAV_SIM_LIVE_SIGNALING_PORT_BASE
  value: {{ .root.Values.liveView.signalingPortBase | quote }}
- name: UAV_SIM_LIVE_MEDIA_PORT_BASE
  value: {{ .root.Values.liveView.mediaPortBase | quote }}
- name: UAV_SIM_LIVE_PUBLIC_MEDIA_IP
  value: {{ .root.Values.liveView.publicMediaHost | quote }}
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
- name: ROS_DISTRO
  value: jazzy
- name: RMW_IMPLEMENTATION
  value: rmw_fastrtps_cpp
- name: LD_LIBRARY_PATH
  value: /isaac-sim/exts/isaacsim.ros2.core/jazzy/lib
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
{{- $slotCount := int .Values.liveView.viewerSlots -}}
{{- if or (lt $slotCount 1) (gt $slotCount 32) -}}
{{- fail "liveView.viewerSlots must be between 1 and 32" -}}
{{- end -}}
{{- $lastSlot := sub $slotCount 1 -}}
{{- if eq (int .Values.liveView.signalingGatePort) (int .Values.service.port) -}}
{{- fail "liveView.signalingGatePort must differ from service.port" -}}
{{- end -}}
{{- if or (lt (int .Values.liveView.signalingGatePort) 1) (gt (int .Values.liveView.signalingGatePort) 65535) -}}
{{- fail "liveView.signalingGatePort must be between 1 and 65535" -}}
{{- end -}}
{{- if gt (add (int .Values.liveView.signalingPortBase) $lastSlot) 65535 -}}
{{- fail "liveView signaling port range exceeds 65535" -}}
{{- end -}}
{{- if gt (add (int .Values.liveView.mediaPortBase) $lastSlot) 65535 -}}
{{- fail "liveView media port range exceeds 65535" -}}
{{- end -}}
{{- with .Values.liveView.mediaService.nodePortBase -}}
{{- if gt (add (int .) $lastSlot) 32767 -}}
{{- fail "liveView media NodePort range exceeds 32767" -}}
{{- end -}}
{{- end -}}
{{- if and (le (int .Values.liveView.mediaPortBase) (add (int .Values.liveView.signalingPortBase) $lastSlot)) (le (int .Values.liveView.signalingPortBase) (add (int .Values.liveView.mediaPortBase) $lastSlot)) -}}
{{- fail "liveView signaling and media port ranges overlap" -}}
{{- end -}}
{{- if not (regexMatch "^(ws|wss)://[^/@[:space:]]+(/[^[:space:]]*)?$" .Values.liveView.publicSignalingUrl) -}}
{{- fail "liveView.publicSignalingUrl must be an absolute credential-free ws or wss URL" -}}
{{- end -}}
{{- if not (or (regexMatch "^([0-9]{1,3}\\.){3}[0-9]{1,3}$" .Values.liveView.publicMediaHost) (regexMatch "^[0-9A-Fa-f:]+$" .Values.liveView.publicMediaHost)) -}}
{{- fail "liveView.publicMediaHost must be a numeric IP address" -}}
{{- end -}}
{{- if and .Values.liveView.signalingIngress.enabled (empty .Values.liveView.signalingIngress.host) -}}
{{- fail "liveView.signalingIngress.host is required when signaling ingress is enabled" -}}
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
    "resources" .root.Values.recordingForwarder.resources
  ) -}}
{{- end }}
