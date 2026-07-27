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
| SHA-256 | scene-body and visual-asset content identities |
| `artifact://` | governed visual-asset occurrence identity |

The scene declaration is a repository-owned producer-to-renderer contract. It is
not an MCP tool contract. Hosted servers decide how a caller prepares or reads a
declaration.

## Boundary

This crate contains scene, visual-asset, transform, lighting, and quality types.
It contains no renderer sessions, cameras, capacity, leases, WebRTC, MCP
transport, HTTP client, deployment, or simulator implementation.

A producer owns its visual assets and entity bindings. It cannot select a
renderer implementation or create an operator camera through this contract.
Simulation View owns artifact materialization, pose admission, rendering,
streaming, and lifecycle.

## Canonical Identity

`SceneDeclaration::from_body` serializes the typed body with `serde_json` and
computes its SHA-256 identity. Validation recomputes that digest and rejects any
declaration whose body, Frames revision, assets, prototype bindings, or admitted
camera kinds are inconsistent.
