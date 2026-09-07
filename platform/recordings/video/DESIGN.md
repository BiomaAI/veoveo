# Governed Recorded Video

## Standards And Protocols

| Boundary | Profile |
|---|---|
| Recording resources | Canonical UUIDv7 recording URI and typed reader authority |
| Rerun RRD | Existing repository 0.36.3 VideoStream profile |
| H.264 and MP4 | Bounded Annex B access-unit selection and remux without re-encoding |
| Source identity | Ordered captured layer identities and SHA-256 |

The video materializer depends on the shared Recording reader and RRD libraries.
It validates selectors and limits, obtains the governed snapshot, extracts a bounded
range with decoder-reentrant preroll, and returns the original source identity beside
its encoded clip and MP4 bytes. It owns no Recording Hub or MCP service implementation.

Consumers own hardware decoding and inference. A missing fresh Artifact-read credential
continues to reject materialization before any historical source can be used. This
library does not authorize replay from an old spool path.
