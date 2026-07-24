{{- define "uav-sim.labels" -}}
app.kubernetes.io/name: veoveo
app.kubernetes.io/instance: {{ .Values.platform.instanceLabel }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end }}

{{- define "uav-sim.selectorLabels" -}}
app.kubernetes.io/name: veoveo
app.kubernetes.io/instance: {{ .Values.platform.instanceLabel }}
app.kubernetes.io/component: uav-sim
{{- end }}

{{- define "uav-sim.image" -}}
{{- $root := index . 0 -}}
{{- $image := index . 1 -}}
{{- $registry := trimSuffix "/" $root.Values.global.veoveoRegistry -}}
{{- $repository := $image.repository -}}
{{- if $registry -}}{{- $repository = printf "%s/%s" $registry $repository -}}{{- end -}}
{{- $lockedDigest := get $root.Values.global.imageDigests $image.repository | default "" -}}
{{- $digest := $image.digest | default $lockedDigest -}}
{{- if and $root.Values.global.production (not $digest) -}}
{{- fail (printf "global.production requires an immutable digest for %s" $repository) -}}
{{- end -}}
{{- if $digest -}}
{{- printf "%s@%s" $repository $digest -}}
{{- else -}}
{{- $tag := default $image.tag $root.Values.global.veoveoTag -}}
{{- printf "%s:%s" $repository $tag -}}
{{- end -}}
{{- end }}

{{- define "uav-sim.podSecurityContext" -}}
runAsNonRoot: true
runAsUser: 10001
runAsGroup: 10001
fsGroup: 10001
fsGroupChangePolicy: OnRootMismatch
seccompProfile:
  type: RuntimeDefault
{{- end }}

{{- define "uav-sim.containerSecurityContext" -}}
allowPrivilegeEscalation: false
capabilities:
  drop: ["ALL"]
runAsNonRoot: true
runAsUser: 10001
runAsGroup: 10001
seccompProfile:
  type: RuntimeDefault
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
- name: XDG_CACHE_HOME
  value: {{ printf "/var/lib/veoveo/runtime-cache/%s" .root.Values.cache.version | quote }}
- name: UAV_SIM_SESSION_ID
  value: {{ .sessionId | quote }}
- name: UAV_SIM_VEHICLE_COUNT
  value: {{ .root.Values.session.vehicleCount | quote }}
- name: UAV_SIM_PHYSICS_HZ
  value: {{ .root.Values.session.physicsHz | quote }}
- name: UAV_SIM_RENDERING_HZ
  value: {{ .root.Values.session.renderingHz | quote }}
- name: UAV_SIM_TILE_READY_FRAMES
  value: {{ .root.Values.session.tileReadyFrames | quote }}
- name: UAV_SIM_CAMERA_WIDTH
  value: {{ .root.Values.session.camera.width | quote }}
- name: UAV_SIM_CAMERA_HEIGHT
  value: {{ .root.Values.session.camera.height | quote }}
- name: UAV_SIM_CAMERA_FPS
  value: {{ .root.Values.session.camera.fps | quote }}
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
- name: UAV_SIM_FOLLOW_CAMERA_WIDTH
  value: {{ .root.Values.session.followCamera.width | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_HEIGHT
  value: {{ .root.Values.session.followCamera.height | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_FPS
  value: {{ .root.Values.session.followCamera.fps | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_FOCAL_LENGTH_MM
  value: {{ .root.Values.session.followCamera.focalLengthMm | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_X_M
  value: {{ .root.Values.session.followCamera.eyeOffsetM.x | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_Y_M
  value: {{ .root.Values.session.followCamera.eyeOffsetM.y | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_Z_M
  value: {{ .root.Values.session.followCamera.eyeOffsetM.z | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_X_M
  value: {{ .root.Values.session.followCamera.targetOffsetM.x | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_Y_M
  value: {{ .root.Values.session.followCamera.targetOffsetM.y | quote }}
- name: UAV_SIM_FOLLOW_CAMERA_TARGET_OFFSET_Z_M
  value: {{ .root.Values.session.followCamera.targetOffsetM.z | quote }}
- name: UAV_SIM_LIVE_STREAM_SIGNAL_PORT
  value: {{ .root.Values.liveStream.privateSignalPort | quote }}
- name: UAV_SIM_LIVE_STREAM_PROXY_PORT
  value: {{ .root.Values.liveStream.proxyPort | quote }}
- name: UAV_SIM_LIVE_STREAM_MEDIA_PORT
  value: {{ .root.Values.liveStream.mediaPort | quote }}
- name: UAV_SIM_LIVE_STREAM_PUBLIC_IP
  value: {{ .root.Values.liveStream.publicIp | quote }}
- name: UAV_SIM_LIVE_STREAM_SIGNALING_PATH
  value: {{ .root.Values.liveStream.signalingPath | quote }}
- name: UAV_SIM_LIVE_STREAM_LEASE_TTL_SECONDS
  value: {{ .root.Values.liveStream.leaseTtlSeconds | quote }}
{{- if .root.Values.session.screenshot.enabled }}
- name: UAV_SIM_SCREENSHOT_PATH
  value: {{ .root.Values.session.screenshot.outputPath | quote }}
- name: UAV_SIM_SCREENSHOT_MINIMUM_RELATIVE_ALTITUDE_M
  value: {{ .root.Values.session.screenshot.minimumRelativeAltitudeM | quote }}
- name: UAV_SIM_SCREENSHOT_SETTLE_RENDERED_FRAMES
  value: {{ .root.Values.session.screenshot.settleRenderedFrames | quote }}
{{- end }}
- name: UAV_SIM_RECORDING_KEY
  valueFrom:
    fieldRef:
      fieldPath: metadata.uid
- name: NVIDIA_DRIVER_CAPABILITIES
  value: all
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

{{- define "uav-sim.recordingForwarder" -}}
- name: recording-forwarder
  restartPolicy: Always
  image: {{ include "uav-sim.image" (list .root .root.Values.images.forwarder) }}
  imagePullPolicy: {{ .root.Values.images.pullPolicy }}
  args:
    - --gateway-url
    - {{ printf "%s/" (trimSuffix "/" .root.Values.platform.publicBaseUrl) | quote }}
    - --gateway-transport-url
    - {{ printf "%s/" (trimSuffix "/" .root.Values.recordingForwarder.gatewayTransportUrl) | quote }}
    - --protected-resource
    - {{ printf "%s/ingest/recordings" (trimSuffix "/" .root.Values.platform.publicBaseUrl) | quote }}
    - --client-id
    - {{ .root.Values.recordingForwarder.clientId | quote }}
    - --key-id
    - {{ .root.Values.recordingForwarder.keyId | quote }}
    - --signing-algorithm
    - {{ .root.Values.recordingForwarder.signingAlgorithm | quote }}
    - --private-key-pem-file
    - /run/secrets/recording-producer/private-key.pem
    - --queue-dir
    - /var/lib/veoveo-recording-forwarder
    - --maximum-queue-bytes
    - {{ printf "%.0f" .root.Values.recordingForwarder.maximumQueueBytes | quote }}
    - --batch-message-limit
    - {{ .root.Values.recordingForwarder.batchMessageLimit | quote }}
    - --batch-flush-milliseconds
    - {{ .root.Values.recordingForwarder.batchFlushMilliseconds | quote }}
    - --grpc-memory-limit-bytes
    - {{ printf "%.0f" .root.Values.recordingForwarder.grpcMemoryLimitBytes | quote }}
  env:
    - name: RUST_LOG
      value: info
  startupProbe:
    exec:
      command: [nc, -z, 127.0.0.1, "9876"]
    periodSeconds: 2
    failureThreshold: 150
  securityContext:
    {{- include "uav-sim.containerSecurityContext" .root | nindent 4 }}
    readOnlyRootFilesystem: true
  resources:
    {{- toYaml .root.Values.recordingForwarder.resources | nindent 4 }}
  volumeMounts:
    - name: recording-forwarder-queue
      mountPath: /var/lib/veoveo-recording-forwarder
    - name: recording-producer-key
      mountPath: /run/secrets/recording-producer
      readOnly: true
{{- end }}
