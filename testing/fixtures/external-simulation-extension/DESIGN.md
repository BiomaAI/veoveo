# Anonymous Simulation Extension Design

This fixture proves that an independently owned Python MCP server can publish
governed visual content and moving poses to Veoveo without implementing the
renderer or importing platform source.

## Standards And Protocols

| Standard or protocol | Supported boundary |
|---|---|
| Model Context Protocol | Sessionful Streamable HTTP with event-stream responses under `/anonymous-simulation/mcp` |
| JSON Schema 2020-12 | Closed, dereferenced tool input and output shapes produced by the selected Python SDK |
| OpenUSD USDA 1.0 | Self-contained, non-executable environment and prototype assets |
| SPIFFE URI | Installation-bound pose-producer identity carried by the mTLS client certificate |
| TLS 1.3 | Private latest-pose stream with mutual authentication |
| `veoveo.io/simulation-view-scene/v2` | Content-addressed scene declaration with governed Artifact URIs |
| `veoveo.io/simulation-view-pose/v1` | Complete, atomic newest-pose snapshots; this is a private adapter protocol, not MCP |
| `veoveo.io/gateway-server-fragment/v1` | Extension-owned gateway contribution |
| `veoveo.io/extension-release/v1` | Immutable independently published release inventory |

## Ownership

The extension publishes the environment, visual prototypes, entity table,
scene intent, and newest complete pose snapshots. It does not load provider
plugins into Veoveo. Its container contains neither Isaac nor a GPU runtime.

The platform's Simulation View service validates and materializes the scene,
admits logical cameras, mirrors poses into Isaac, allocates RTX render
products, encodes H.264 with NVENC, and terminates viewer leases. The generic
live-view MCP App is also a Simulation View artifact.

The installation authorizes the producer ID and exact SPIFFE identity before
the pose endpoint accepts a connection. It also selects the renderer capacity
profile, supplies the NVIDIA RuntimeClass and GPU resource, binds the gateway
fragment, and configures package, image, chart, and certificate credentials.

## MCP Surface

`prepare_scene` publishes fixture-owned USDA bytes through the Artifact data
plane and returns a typed immutable scene declaration that retains canonical
Artifact and Frames resource identifiers. The gateway fragment declares both
cross-server schemes, while its own resources remain under
`anonymous-simulation://`. `start_pose_producer` and `stop_pose_producer`
control only the fixture's private publisher. `get_fixture_state` reports
lifecycle and redacted counters.

The resource surface contains the current producer state, embedded design and
agent documents, and the hosted-server contract declaration. No resource
contains a bearer token, mTLS private key, signaling token, or live-view
access token.

## Data Plane

Each pose snapshot carries the session, epoch, sequence, frame revision,
entity-table revision and digest, simulation timestamp, and all declared
entity poses. A bounded newest-value publisher replaces stale queued
snapshots rather than accumulating physics-rate history.

The assets are small USDA documents with no external references, scripts,
physics schemas, credentials, or arbitrary URLs. The Artifact plane returns
the governed URI; SHA-256 is computed from the exact published bytes.

## Deployment

The Helm workload runs as UID 10001 with a read-only root filesystem and no
service-account token. Network policy permits only DNS, the Artifact Service,
the Simulation View pose endpoint, and gateway ingress to the MCP port.

The chart contains no GPU resource, media Service, signaling path, camera
setting, renderer volume, Isaac container, or live-view App. Those resources
belong to the separately selected Simulation View platform component.
