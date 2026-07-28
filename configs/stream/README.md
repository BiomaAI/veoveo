# Stream Runtime Configuration

Copy `catalog.example.json` to the mounted Stream catalog path. Put each
referenced TensorRT engine and model-specific `gst-nvinfer` configuration at
the exact absolute path declared by the catalog.

The public MCP surface exposes pipeline and model identities, never these
paths or native launch strings. Treat every GStreamer graph as deployment
code. A perception graph names the source, muxer, inference, results, optional
tracker, and live encoded-output elements that the runner controls. A
pass-through live graph names only its source and encoded output.

Live RTP/H.264 graphs declare dimensions, frame rate, expected bit rate, a
dynamic RTP payload type, a 90 kHz clock, and an RFC 6381 AVC codec. The
encoded-output branch must retain Annex B access-unit alignment because the
Stream MCP App decodes that same bitstream without another encode.

A live graph may also declare `recording_output`. Its proxy must be a
pod-loopback Rerun endpoint backed by the standard Recording forwarder. The
application ID must be admitted by the installation's recording producer
policy. Stream forwards the existing H.264 units through a bounded worker and
reports route failure without delaying live processing.

Recording-replay graphs are optional. Omit them from deployments that do not
select Recording Hub. Tracking requires a DeepStream 9.1 low-level tracker
YAML whose dimensions are positive multiples of 32.

The server validates the catalog, model engines, inference configurations,
tracker files, named elements, launch bounds, and live port uniqueness at
startup. Missing inputs are deployment failures.
