# View MCP Design

This document is the canonical design and operational contract for the
`view-mcp` crate.

`view-mcp` captures reproducible points of view over georeferenced 3D Tiles.
It runs Bevy without a window, keeps bounded tile and GPU residency across
captures, and returns images that dashboards can display. Several callers can
own independent views while the service shares immutable source content.

## Status

Implemented in this workspace.

The canonical service identity is:

```text
crate       veoveo-view-mcp
folder      servers/view-mcp
slug        view
URI scheme  view
MCP         /view/mcp
health      /view/healthz
readiness   /view/readyz
```

Gateway-mounted tools use names such as `view__capture_frame`. Resource
identities keep the `view://` scheme.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with view tools, task-only capture, resources and templates, completions, subscriptions, notifications, image content, and structured results. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Camera, scene, capture policy, layer, tile, frame, and structured-result contracts. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; frame capture uses durable creation, progress, cancellation, results, and subscriptions. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; `ui://view/preview.html` drives canonical resources, direct view tools, and task-based capture. |
| OGC 3D Tiles 1.0 and 1.1 | Explicit tile trees, external tilesets, bounding boxes/spheres/regions, transforms, geometric error, and `REPLACE`/`ADD` refinement. Implicit tiling and legacy payloads are rejected. |
| glTF/GLB 2.0 | Meshes, standard materials and textures, node transforms, and GLB binary content. |
| Draco glTF geometry compression | Native decode of Draco-compressed GLB geometry. Preview resources preserve the original compressed bytes. |
| WGS 84 and ECEF | Exact geodetic camera definitions and `f64` planetary transforms resolved into a local east-up-north rendering frame. |
| HTTPS | External tilesets and content follow configured credential, host, redirect, deadline, and byte policies. API keys never enter MCP requests or resource identities. |
| PNG and JPEG | Bounded captured-frame encodings returned as MCP image content and governed frame resources. |

## Boundary

Map owns geographic source truth, releases, coverage, licensing metadata, and
the `map://` identities that select a scene. View owns camera state, 3D Tiles
traversal, source sessions, request scheduling, decoded residency, Bevy assets,
offscreen rendering, and captured frames.

View does not add routing, location search, overlays, feature identities, or
interaction semantics to Map. It consumes a configured scene layer and emits a
visual result.

```text
caller
  |
  | MCP
  v
gateway
  |
  | signed internal identity
  v
view-mcp
  |-- owner-scoped logical views
  |-- shared 3D Tiles source runtimes
  |-- raw and decoded byte-budgeted caches
  |-- Bevy 0.19 offscreen Vulkan renderer
  `-- bounded frame resources
```

## Initial Scene Sources

One view selects one complete scene layer. A layer is a hierarchical 3D Tiles
dataset, not an object category. Google Photorealistic 3D Tiles therefore
supplies every terrain surface, building, monument, and other textured mesh in
the provider's available coverage through one layer.

The initial source kinds are:

- Google Photorealistic 3D Tiles through its keyed live-session protocol;
- an HTTPS `tileset.json` with relative content;
- a mounted local `tileset.json` for deterministic tests and private data.

The public view contract refers to a configured layer id. It never accepts an
API key. Redirects, credentials, local roots, request caps, and cache behavior
belong to the server-side layer catalog.

The first implementation supports explicit 3D Tiles trees, external tilesets,
GLB content, standard glTF materials and textures, and native Draco geometry.
It rejects implicit tiling and legacy `b3dm`, `i3dm`, and `pnts` content with a
typed unsupported-content failure.

## Camera Contract

An exact geodetic pose is the canonical camera state. Target-based rigs are
input conveniences that resolve to the same pose before selection or capture.

```text
CameraDefinition
  pose
    WGS84 position + heading/pitch/roll + vertical FOV
  look_at
    WGS84 eye + WGS84 target + vertical FOV
  orbit_target
    WGS84 target + distance + azimuth + elevation + vertical FOV
```

Heading is clockwise from true north. Pitch is positive above the local
horizon. Roll uses the right-hand rule around the forward axis. Heights are
WGS84 ellipsoidal metres.

Geodetic and ECEF calculations remain `f64`. Each capture establishes a local
east-up-north frame near its camera rig, composes ECEF transforms in `f64`, and
only then casts local transforms to Bevy `f32`:

```text
+X east
+Y up
-Z north
```

Every frame records the resolved exact pose, configured layer identity,
viewport, and achieved detail.

## Views And Concurrency

A view is owner-scoped logical state, not a Bevy window or renderer process.
It survives an MCP transport reconnect until explicit close.
Camera replacement uses an expected revision. A capture snapshots one revision
and is not changed by later camera updates.

Views sharing a configured layer share its root session, flattened tree, raw
bytes, CPU tile content, and Bevy assets. They retain independent cameras,
local origins, and frame results.

Network fetch and CPU decode work run concurrently within a render cut.
Same-layer selection and cache mutation are serialized, which prevents
duplicate source requests. GPU submissions pass through a bounded capture
pool before the single externally driven Bevy renderer.

## MCP Surface

### Tools

| Tool | Invocation | Required scope | Result |
|---|---|---|---|
| `create_view` | direct | `view:write` | owner-scoped view and initial revision |
| `set_camera` | direct | `view:write` | replaced camera and next revision |
| `capture_frame` | task only | `view:capture` | image content and captured-frame metadata |
| `close_view` | direct | `view:write` | closed view identity |

`capture_frame` accepts physical pixel dimensions, a maximum screen-space
error, a deadline, and a typed deadline behavior. Returning the best available
frame reports whether the requested detail was reached. The terminal tool
result carries a bounded MCP image block plus typed structured content.

### Resources

Root resources are:

```text
view://layers
view://views
view://frames
```

Resource templates are:

```text
view://layer/{layer_id}
view://view/{view_id}
view://frame/{frame_id}
view://view/{view_id}/scene{?width_px,height_px,max_screen_error_px}
view://tile/{tile_key}
```

Views and frames are owner scoped. Resource lists are paginated. The server
emits list-change notifications after view creation and close, root-resource
updates after captures, and view-resource updates after camera replacement.

Completion applies to visible view ids, frame ids, and configured layer ids.
No prompt belongs in the initial capture-only domain.

### Preview App

`ui://view/preview.html` is a self-contained MCP App (gated on `view:capture`
like the capture surface it drives) that exercises the real tool lifecycle:
`create_view`, `set_camera` under revision control, task-based
`capture_frame` through the host's task proxy, and `close_view` on teardown.
It never gets parallel convenience tools. The document is composed at serve
time from `assets/preview-app.template.html` plus the vendored three.js/draco
bundle in `assets/vendor/` (rebuilt via `tools/vendor-three/`); guard tests
pin self-containment and the console host's 2 MiB document cap.

The app's in-browser 3D scene reads the parameterized view-scene resource
(owner-scoped, `view:read`). Its typed viewport and screen-space error policy
drives the same frustum selection used for capture, and the transport admits a
complete render cut of up to 256 tiles. The app sends its current capture policy,
which keeps detail inside the camera frustum representative of the requested
frame. The manifest carries the
resolved camera, a local origin with its column-major `local_from_ecef`
frame, aggregated attribution, and per-tile `view://tile/{tile_key}` URIs
with verbatim `ecef_from_content` transforms (glTF Y-up to Z-up baked in;
CESIUM_RTC and node transforms stay inside the GLB and are the consumer's
job, exactly as in the renderer). Tile keys are sha256 tokens over the layer
and credential-free content location, resolved through an in-process
FIFO-bounded registry — like frames, they do not survive process restarts,
and a stale token fails with guidance to re-read the scene. Tile reads serve
raw draco GLB bytes from the source byte cache (refetch on miss under the
source's own credential and host rules) and refuse tiles above 1.5 MB so
base64 blobs stay under the console host's 2 MiB read cap.

## Tile Selection

The renderer flattens explicit tile trees while retaining parent links,
refinement mode, cumulative transforms, bounding volumes, geometric error, and
content identity. Selection uses physical-pixel screen-space error:

```text
SSE = geometric_error * focal_length_in_physical_pixels
      / distance_to_bounding_volume
```

Traversal applies frustum culling, `REPLACE` and `ADD` refinement, coarse
ancestor fallback, and prioritized loading. Its distance floor uses the camera
height above the ellipsoid, which prevents tall coarse globe volumes from
forcing inappropriate street-level refinement. The effective SSE threshold
relaxes beyond the configured 2 km detail falloff, measured from camera height,
and keeps the near target at the requested quality while bounding horizon work.

Frame history protects the render cut in both zoom directions. Load order is
urgent coverage, refinement descent, normal cut content, then ancestor preload;
distance within each tier is weighted toward the camera axis. A capture is
complete when every visible branch has settled at its available provider detail.
A deadline can return the best available covered cut instead.

The implementation does not depend on `bevy_3d_tiles`. Its pure traversal math
and test scenarios are useful behavioral references. Its Bevy 0.18 ECS
scheduler, render-coupled decode types, native worker-per-request model, and
native Draco limitation are not inherited.

## Cache And Residency

Caching is part of correctness and cost control, not a later optimization.
Each configured layer has four reuse levels:

1. provider session and root tileset;
2. raw HTTP response bytes;
3. decoded CPU meshes, materials, and images;
4. Bevy GPU meshes, textures, and materials.

The initial implementation intentionally has no persistent disk cache. Raw
HTTP keys use canonical qualified content identities with credentials removed.
Decoded and GPU keys add the content hash, preventing stale GPU reuse after a
source object changes.

Raw, decoded, and GPU limits are independent byte budgets. An active render
holds its selected content through reference-counted snapshots. Each cache
evicts its least recently used unpinned entries when its own byte budget is
crossed.

HTTP freshness follows `Cache-Control` and `ETag`. Stale entries revalidate;
`no-store` remains transient. The Google adapter keeps its current provider
session and in-process residency, and applies the response directives instead
of inventing an unconditional lifetime.

## Native Decode Boundary

Fetch produces immutable content bytes. CPU decode produces renderer-neutral
meshes, images, materials, node transforms, ECEF offsets, and attribution.
Only the renderer adapter creates Bevy assets.

Native Draco decoding uses the current pure-Rust `draco-core` implementation
through `draco-gltf`. GLB preprocessing preserves planetary translations in
`f64` before glTF's local `f32` transforms are read. The decoder never creates
Bevy `Mesh`, `Image`, or material values.

## Bevy Renderer

The service uses Bevy 0.19 with an exact dependency and a minimal feature set.
It has no Winit plugin, primary window, OS input source, camera controller,
audio stack, UI, or picking backend. An externally driven `App` renders into
`Image` targets and pumps only for asset upload, capture, readback, and cleanup
work.

Production selects the Vulkan backend, high-performance power preference, and
rejects fallback or CPU adapters. Readiness includes the selected adapter name,
backend, and device type. A process without a hardware adapter is not ready.

The NVIDIA image contains the Vulkan loader and runs through NVIDIA Container
Toolkit with `graphics`, `compute`, and `utility` driver capabilities. Helm
requests one `nvidia.com/gpu` resource for each renderer replica.

## Attribution

Each decoded glTF can carry `asset.copyright`. A captured render cut collects,
sorts, and deduplicates the strings from its visible content into an
`AttributionSet`. The frame result returns that set for display beside the
image. Attribution is presentation metadata, not feature analysis.

## Limits And Failure

Configuration bounds active views per owner, total views, captures in flight,
tile load concurrency, response bytes, tree nodes, viewport dimensions, frame
retention, cache bytes, and Google source requests. Limits fail closed with
typed MCP errors.

Closing a view cancels unfinished captures for that view. Renderer startup
fails before readiness when Vulkan selects a CPU, fallback, or non-NVIDIA
adapter in the production profile.

## Deployment And Verification

The Kubernetes Service exposes health and MCP ports only inside the cluster. The
gateway is the normal caller and forwards signed internal identity. The
container runs as the non-root Veoveo user with a read-only root filesystem and
writable `/tmp`.

Rust tests cover camera resolution, ECEF cancellation, tree construction, SSE
selection, weighted eviction, freshness, credential-free cache keys, GLB
preprocessing, and typed unsupported content. The Rust smoke path starts the
renderer inside the NVIDIA container, verifies a non-CPU Vulkan adapter,
captures a deterministic local tileset through source, traversal, decode, and
Bevy through the production MCP task and frame-resource boundaries, then checks
that two owner-scoped views remain revision-isolated. The production image
contains only the View MCP server. `just smoke-view-mcp` builds the image and
dispatches the central Rust smoke harness.

The billed live acceptance scenario captures Google Photorealistic 3D Tiles
from a camera orbiting the Statue of Liberty at 40.6892494, -74.0445004. It
requires the API key in `GOOGLE_MAPS_API_KEY`, passes the variable by name
rather than putting its value in the command line, drives the production MCP
task interface, requires an NVIDIA adapter, and retains only the rendered JPEG:

```sh
just smoke-view-google \
  /tmp/veoveo-view-proof/statue-of-liberty.jpg
```
