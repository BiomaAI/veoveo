{{- define "veoveo.streamRuntimeData" -}}
catalog.json: |
  {
    "models": [
      {
        "id": "trafficcamnet",
        "title": "TrafficCamNet",
        "description": "NVIDIA sample traffic-camera object detector compiled locally for this GPU.",
        "format": "tensor_rt_engine",
        "modelPath": "/models/trafficcamnet.engine"
      }
    ],
    "pipelines": [
      {
        "id": "traffic-object-detection",
        "title": "Traffic object detection",
        "description": "Detect vehicles, people, road signs, and two-wheelers in live RTP/H.264 or governed recording video.",
        "profile": {
          "kind": "perception",
          "operation": "object_detection",
          "modelId": "trafficcamnet",
          "inferenceConfigPath": "/etc/veoveo/stream/inference.txt"
        }
        {{- if include "veoveo.mcpServerEnabled" (list . "recording") }},
        "recordingReplay": {
          "launch": "filesrc name=source ! qtdemux name=demux demux.video_0 ! queue ! h264parse ! nvv4l2decoder ! mux.sink_0 nvstreammux name=mux batch-size=1 width=640 height=480 live-source=false batched-push-timeout=40000 ! nvinfer name=infer ! fakesink name=results sync=false async=false",
          "sourceElement": "source",
          "streamMuxerElement": "mux",
          "inferenceElement": "infer",
          "resultsElement": "results"
        }
        {{- end }},
        "live": {
          "inputWidth": {{ .Values.stream.liveInput.width }},
          "inputHeight": {{ .Values.stream.liveInput.height }},
          "codec": "avc1.42e01f",
          "frameRate": {{ .Values.stream.liveInput.frameRate }},
          "expectedBitrateBps": {{ .Values.stream.liveInput.expectedBitrateBps }},
          "ingress": {
            "advertisedHost": "stream-mcp",
            "port": 9000,
            "payloadType": 96,
            "clockRate": 90000
          },
          "graph": {
            "launch": "udpsrc name=source port=9000 caps=\"application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000\" ! rtpjitterbuffer mode=none latency=50 drop-on-latency=true max-dropout-time=1000 max-misorder-time=100 faststart-min-packets=2 ! rtph264depay ! h264parse config-interval=-1 ! video/x-h264,stream-format=byte-stream,alignment=au ! tee name=encoded encoded. ! queue leaky=downstream max-size-buffers=8 ! identity name=encoded-output ! fakesink sync=false async=false encoded. ! queue ! nvv4l2decoder ! mux.sink_0 nvstreammux name=mux batch-size=1 width={{ .Values.stream.liveInput.width }} height={{ .Values.stream.liveInput.height }} live-source=true batched-push-timeout=40000 ! nvinfer name=infer ! fakesink name=results sync=false async=false",
            "sourceElement": "source",
            "streamMuxerElement": "mux",
            "inferenceElement": "infer",
            "resultsElement": "results",
            "encodedOutputElement": "encoded-output"
          }
          {{- if .Values.stream.recordingOutput.enabled }},
          "recordingOutput": {
            "proxyUrl": {{ .Values.stream.recordingOutput.proxyUrl | quote }},
            "applicationId": {{ .Values.stream.recordingOutput.applicationId | quote }},
            "entityPath": {{ .Values.stream.recordingOutput.entityPath | quote }},
            "timeline": {{ .Values.stream.recordingOutput.timeline | quote }},
            "queueCapacity": {{ .Values.stream.recordingOutput.queueCapacity }}
          }
          {{- end }}
        }
      }
    ]
  }
inference.txt: |
  [property]
  gpu-id=0
  net-scale-factor=0.00392156862745098
  model-engine-file=/models/trafficcamnet.engine
  labelfile-path=/opt/nvidia/deepstream/deepstream-9.1/samples/models/Primary_Detector/labels.txt
  batch-size=1
  process-mode=1
  model-color-format=0
  network-mode=2
  num-detected-classes=4
  interval=0
  gie-unique-id=1
  cluster-mode=2

  [class-attrs-all]
  topk=20
  nms-iou-threshold=0.5
  pre-cluster-threshold=0.2

  [class-attrs-0]
  topk=20
  nms-iou-threshold=0.5
  pre-cluster-threshold=0.4
{{- end -}}
