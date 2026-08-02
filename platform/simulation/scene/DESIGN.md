# Simulation View Scene Contract

This crate owns the provider-neutral scene declaration shared by a simulation
producer and Simulation View. A producer declares governed visual assets, stable
entity-to-prototype bindings, the immutable Frames revision, and the camera kinds
that its scene admits. Simulation View validates and renders that declaration.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/simulation-view-scene/v1` | one canonical JSON scene body and SHA-256 digest |
| JSON Schema 2020-12 | generated schemas for the complete typed declaration |
| OpenUSD | governed `.usd` and `.usdz` environment and prototype assets |
| glTF 2.0 | governed `.gltf` and `.glb` prototype assets |
| WGS 84 and local tangent frames | immutable Frames revision with one local simulation frame |
| OGC 3D Tiles | installation-selected streamed-world layer identity; tile transport is outside MCP |
| `veoveo.io/simulation-view-layer-catalog/v1` | closed installation catalog for sources, host admission, budgets, attribution, and exact Frames/WGS 84 binding |
| SHA-256 | scene-body and visual-asset content identities |
| `artifact://` | governed visual-asset occurrence identity |

The scene declaration is a repository-owned producer-to-renderer contract. It is
not an MCP tool contract. Hosted servers decide how a caller prepares or reads a
declaration.

## Boundary

This crate contains scene, visual-asset, transform, lighting, quality, and
streamed-world catalog types.
It contains no renderer sessions, cameras, capacity, leases, WebRTC, MCP
transport, HTTP client, deployment, or simulator implementation.

A producer owns its visual assets and entity bindings. It cannot select a
renderer implementation or create an operator camera through this contract.
Simulation View owns artifact materialization, pose admission, rendering,
streaming, and lifecycle.

A scene may select one installation layer by stable ID. The declaration never
contains a provider URL or credential. The catalog binds that ID to a provider
adapter, Secret environment name, admitted source and redirect hosts, budgets,
required attribution, and the exact Frames revision, local ENU frame, and WGS
84 origin. Simulation View rejects an unknown ID or a revision mismatch before
sending the scene to the renderer.

## Governed Lighting

`SceneLighting` declares one physical directional illuminant. The renderer
authors one normalized OpenUSD distant light with exposure `0`. It maps
`intensityLux` directly to the light intensity without a divisor, and it
enables OpenUSD color-temperature processing before applying
`colorTemperatureKelvin`. The fixed sun angular diameter is `0.53` degrees.
The renderer does not derive dome radiance from illuminance because the two
quantities do not share a valid conversion.

The accepted color-temperature interval is `1000..=10000` kelvin, matching
the selected OpenUSD light profile. Scene admission rejects values outside
that interval. A renderer that cannot apply intensity, normalization,
exposure, or color temperature rejects the scene rather than silently
dropping a field.

## Canonical Identity

`SceneDeclaration::from_body` serializes the typed body with `serde_json` and
computes its SHA-256 identity. Validation recomputes that digest and rejects any
declaration whose body, Frames revision, assets, prototype bindings, or admitted
camera kinds are inconsistent.
