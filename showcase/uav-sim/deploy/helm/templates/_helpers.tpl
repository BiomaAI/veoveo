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
- name: UAV_SIM_POSE_PRODUCER_ID
  value: {{ .root.Values.posePublication.producerId | quote }}
- name: UAV_SIM_POSE_PRODUCER_SPIFFE_ID
  value: {{ .root.Values.posePublication.producerSpiffeId | quote }}
- name: UAV_SIM_POSE_EPOCH_ID
  value: {{ .root.Values.posePublication.epochId | quote }}
- name: UAV_SIM_POSE_INGRESS_HOST
  value: {{ .root.Values.posePublication.endpointHost | quote }}
- name: UAV_SIM_POSE_INGRESS_PORT
  value: {{ .root.Values.posePublication.endpointPort | quote }}
- name: UAV_SIM_POSE_SERVER_HOSTNAME
  value: {{ .root.Values.posePublication.serverHostname | quote }}
- name: UAV_SIM_POSE_CA_CERTIFICATE
  value: /run/secrets/simulation-view-pose/ca.crt
- name: UAV_SIM_POSE_CLIENT_CERTIFICATE
  value: /run/secrets/simulation-view-pose/tls.crt
- name: UAV_SIM_POSE_CLIENT_PRIVATE_KEY
  value: /run/secrets/simulation-view-pose/tls.key
- name: UAV_SIM_POSE_ENTITY_TABLE_REVISION
  value: {{ .root.Values.posePublication.entityTableRevision | quote }}
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
    "batchFlushMilliseconds" .root.Values.recordingForwarder.batchFlushMilliseconds
    "grpcMemoryLimitBytes" (printf "%.0f" .root.Values.recordingForwarder.grpcMemoryLimitBytes)
    "resources" .root.Values.recordingForwarder.resources
  ) -}}
{{- end }}
