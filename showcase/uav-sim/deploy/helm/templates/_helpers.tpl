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
{{- if and $root.Values.global.production (not $image.digest) -}}
{{- fail (printf "global.production requires an immutable digest for %s" $image.repository) -}}
{{- end -}}
{{- if $image.digest -}}
{{- printf "%s@%s" $image.repository $image.digest -}}
{{- else -}}
{{- printf "%s:%s" $image.repository $image.tag -}}
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
- name: UAV_SIM_FRAME_URI
  value: {{ .root.Values.session.frameUri | quote }}
- name: UAV_SIM_ORIGIN_LATITUDE
  value: {{ .root.Values.session.origin.latitudeDegrees | quote }}
- name: UAV_SIM_ORIGIN_LONGITUDE
  value: {{ .root.Values.session.origin.longitudeDegrees | quote }}
- name: UAV_SIM_ORIGIN_ELLIPSOID_HEIGHT_M
  value: {{ .root.Values.session.origin.ellipsoidHeightM | quote }}
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
- name: UAV_SIM_RECORDING_PROXY
  value: {{ .root.Values.platform.recordingProxy | quote }}
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
