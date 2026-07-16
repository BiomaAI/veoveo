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
  value: {{ .root.Values.world.cachePolicy | quote }}
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
- name: UAV_SIM_RECORDING_PROXY
  value: {{ .root.Values.platform.recordingProxy | quote }}
- name: UAV_SIM_RECORDING_KEY
  valueFrom:
    fieldRef:
      fieldPath: metadata.uid
- name: NVIDIA_DRIVER_CAPABILITIES
  value: all
{{- if .root.Values.session.privacyConsent }}
- name: PRIVACY_CONSENT
  value: "Y"
{{- end }}
{{- end }}
