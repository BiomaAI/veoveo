# View MCP Design

This document is the canonical design and operational contract for the
`view-mcp` crate.

`view-mcp` captures reproducible points of view over governed static scene
compositions. Each composition binds one configured georeferenced 3D Tiles
layer to exact Map, Frames, Artifact, Recording, or simulation-owned inputs and
bounded declarative overlays. The service runs Bevy without a window, keeps
bounded tile and GPU residency across captures, and returns images with exact
composition provenance.

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
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Camera, immutable scene composition, overlay geometry, capture policy, layer, tile, frame, and structured-result contracts. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; frame capture uses durable creation, progress, cancellation, terminal `tasks/get` payloads, and `subscriptions/listen`. |
| [MCP Apps SEP-1865](../../mcp/apps-extension/DESIGN.md) | `ext-apps` version `2026-01-26`; `ui://view/preview.html` drives canonical resources, direct view tools, and task-based capture. |
| OGC 3D Tiles 1.0 and 1.1 | Explicit tile trees, external tilesets, bounding boxes/spheres/regions, transforms, geometric error, and `REPLACE`/`ADD` refinement. Implicit tiling and legacy payloads are rejected. |
| glTF/GLB 2.0 | Meshes, standard materials and textures, node transforms, and GLB binary content. |
| Bevy `0.19.1`, WGPU/HAL `29.0.4`, Vulkan | Offscreen NVIDIA rendering and GPU RGB packing; Linux opaque-FD external buffer memory with explicit queue-family release. |
| CUDA toolkit `13.3.1`, CUDART `13.3.29`, nvJPEG `13.2.1.68` | CUDA device UUID matching, imported Vulkan device buffers and explicit `NVJPEG_ENC_BACKEND_GPU` encoding. The runtime image selects CUDA `13.3.1`; this uses the GPU CUDA encoder, not Thor's fixed-function encoder. |
| PNG and JPEG | `FrameEncoding` selects the image format; frame metadata admits only `image/png` and `image/jpeg`. |
| Draco glTF geometry compression | Native decode of Draco-compressed GLB geometry. Preview resources preserve the original compressed bytes. |
| WGS 84 and ECEF | Exact geodetic camera definitions and `f64` planetary transforms resolved into a local east-up-north rendering frame. |
| HTTPS | External tilesets and content follow configured credential, host, redirect, deadline, and byte policies. API keys never enter MCP requests or resource identities. |
| [Veoveo shared artifact plane](../../platform/artifacts/service) | Exact governed overlay geometry and oriented GLB mesh inputs are resolved under the caller's forwarded gateway authority. |
| [Veoveo Frames contract](../frames-mcp/DESIGN.md) | A composition with local metre coordinates binds one exact world revision, frame URI, transform, and Frames operation input. |
| PNG and JPEG | Bounded captured-frame encodings returned as MCP image content and governed frame resources. |
| RFC 3986 and RFC 6570 | The [foundational URI profile](../../platform/types/DESIGN.md) supplies concrete component parsing, encoding and template expansion. View admits its own route shapes and typed parameters. |
| Map, Frames, Recording and Artifact resource contracts | Governed scene references contain owner-defined URI types. Recording uses `recording://recordings/{id}` with RFC 9562 UUIDv7; Map product IDs require RFC-variant UUIDv5/v7 and source features name their release. |

Controlled View JSON uses camelCase fields and rejects unknown fields. Camera,
overlay and source variants keep snake_case values. MCP tool names, completion
arguments, URI template variables and scene query parameter names follow their
own declared profiles. Configured layers use the same owned field spelling;
external OGC, glTF and Google tiles payloads keep their provider vocabulary.

The composition algorithm schema declares its current literal with the foundational
`format_tag` scalar naming role. Numeric `schemaVersion` is a version number.

Scene compositions require `schemaVersion: 2` and the identity algorithm
`veoveo.ai/view-scene-composition/v2`. Admission checks the version before state
or rendering effects. The request digest hashes compact Serde JSON bytes of the
typed request after ordering governed inputs by `inputId`. The authority digest
hashes compact Serde JSON bytes of the typed authority. Stable
UUIDv5 identities include the current algorithm and request and authority digests.
The authority wrapper uses `principalId`; its embedded foundational invocation
claims keep their declared native spelling. Writers and readers require one
coordinated drain and upgrade. Stored compositions and capture snapshots with
unsupported versions require regeneration from admitted inputs; View supplies
no historical reader or destructive conversion. Recovery preserves the original
rows and Artifact bytes until the current replacement is qualified.

The content digest hashes compact Serde JSON bytes of a private borrowed content
struct. Its fields serialize in this order: `schemaVersion`, `compositionId`,
`compositionUri`, `revision`, `baseLayer`, `mapReleases`, `localFrame`, `styleId`,
`governedInputs`, `overlays`, `algorithmRevision`, `requestDigestSha256`,
`authority`. An absent `localFrame` is omitted; empty content collections are
included. Only `createdAt` and `compositionDigestSha256` are excluded from the
record. Exhaustive wire-record destructuring requires each added field to have
an explicit inclusion or exclusion decision.

Nested values serialize through their owner types and declared Serde field order.
Ordered sets use their admitted ordering, overlays keep request order, and finite
floats use their typed JSON serialization without conversion through a JSON value.
The preimage contains no dynamic JSON objects. Enabling `serde_json/preserve_order`
therefore leaves request, authority, content and UUID identities unchanged. The
record owner freezes one complete governed-marker preimage and its hashes across
both feature profiles; clock exclusion, input normalization, authority binding,
digest rejection and serialized float round trips have separate controls. These
fixtures qualify contract serialization and admission; installed Task recovery
and hardware rendering require their own acceptance.

Frame and preview records, capture snapshots, layer configuration and overlay
geometry JSON are unversioned owned formats. Their current writers and readers
use the same closed DTOs. Overlay Artifact JSON uses
`application/vnd.veoveo.view-overlay-geometry+json`; updated owned geometry JSON
produces its own current byte digest. Tile source digests, binary GLB, Draco,
PNG and JPEG bytes, native SQL columns, Task ownership claims and credential
references keep their declared formats. Prepared overlay caches include current
composition, style and camera values and cannot share entries with a different
composition digest. The shared readiness probe returns plain text. Production
startup and encoder logs preserve the renderer's backend and device vocabulary.

## Library Features

The `contract` feature exposes camera and capture types, composition validation,
resource addresses, `ViewScope` and `ViewTaskKind`. Pure WGS 84 camera resolution
uses the existing `glam` dependency in this profile. It depends on foundational types
and the owning Map, Frames, Recording and Artifact contracts. `SceneCompositionAuthority` keeps the complete foundational
`InvocationAuthority`; the authenticated adapter supplies it from the verified caller.
Its serialization participates in the stable composition digest.

Consumers select `default-features = false, features = ["contract"]`. This library
profile requires no MCP transport, database client, async runtime or renderer.
`runtime` adds scene resolution, state, tile access and the GPU renderer.
`mcp` adds the authenticated server and its Task adapter; the binary requires that feature.
The default is `mcp`, including the mandatory GPU runtime.

`ViewScope` owns the read, write and capture spellings. Ordinary requests and capture
Tasks use the same typed permission guard in `server/auth.rs`. The guard requires the
individual capability in the authenticated grant set. Unrelated grants remain valid
and supply no View permission. Discovery uses the same check when exposing the preview App.

## Resource Addresses And Discovery

`ViewResource` owns the `view://` routes and the preview App address. Individual
`ViewUri`, `CompositionUri`, `FrameUri`, `LayerUri` and `TileUri` values require their
domain IDs. Their `ResourceAddress` declarations drive component parsing and building
with the existing inline string schemas. Tile keys use an owner codec. Public records
carry these types and serialize them as URI strings.
`TileKey` stays typed through the in-process registry and byte lookup. Documents use
the closed `ViewDocument` vocabulary. An address establishes neither ownership nor a grant.

`ViewSceneUri::new` checks positive viewport dimensions and finite screen error in
the supported range before constructing an address. Runtime reads also apply the
installation's capture limits. The shared builder handles all component encoding.
Parsing rejects credentials, fragments, unsupported or repeated query parameters,
wrong parents and noncanonical spellings. Scene query parameters use the order and
number spelling produced by the builder, which matches the discovery template.

`server/setup.rs` implements `McpServerContract` and checks documents, descriptors,
scope membership and all seven templates before Store access or renderer startup.
Discovery serves fixed collection roots, documents and the permitted preview App;
resource reads enumerate caller-owned state. Mutations invalidate resource contents.
Subscriptions accept the mutable composition, view and frame collections, individual
views and explicit Task handles. The gateway registrations declare this discovery profile.

## Boundary

Map owns geographic source truth, immutable releases, derived geometry, and
the `map://` identities supplied to a composition. Frames owns local coordinate
authority. Artifact owns governed large bytes. View validates and snapshots
those exact inputs, then owns declarative visual styling, visibility, camera
state, 3D Tiles traversal, GPU rendering, and captured-frame provenance.

View does not derive routes, reinterpret geographic features, ingest
physics-rate poses, or execute caller code. It renders the geometry declared by
the governed input and preserves the upstream identity and digest.

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
  |-- immutable owner and Work Context scoped compositions
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

The composition contract refers to a configured layer id. It never accepts an
API key. Redirects, credentials, local roots, request caps, and cache behavior
belong to the server-side layer catalog.

`tools/list` refines the generated `create_scene_composition.baseLayer`
schema with the exact identifiers from the active catalog. A single configured
layer is also the schema default. Labels and source kinds remain descriptive
catalog fields and are never accepted as identifiers. An unknown identifier
fails with the complete credential-free identifier set and directs the caller
to `view://layers`, which lets a model correct one malformed call without
guessing names.

The first implementation supports explicit 3D Tiles trees, external tilesets,
GLB content, standard glTF materials and textures, and native Draco geometry.
It rejects implicit tiling and legacy `b3dm`, `i3dm`, and `pnts` content with a
typed unsupported-content failure.

## Static Scene Composition

`create_scene_composition` is the only path from a base layer to a view. A
base-only request is valid and produces an immutable composition. The stable
composition identity is UUIDv5-derived from the canonical request digest,
algorithm revision, and complete invocation authority. Repeating the same
request under the same Work Context authority returns the same record.

A composition records its schema version, one base layer, immutable Map
release identities, a style identity, ordered overlays, exact governed inputs,
authority, Work Context, algorithm revision, request digest, and composition
digest. Every governed input carries an exact resource URI, SHA-256 digest,
license, attribution, and an exact media type when bytes are resolved.

`composition/references.rs` defines the scene's admitted resource variants. Each
variant contains the owning contract's URI type; the public JSON value is its URI
string. Parsing delegates to those owners. Map release inputs use `MapReleaseUri`
directly. Source feature inputs must name a release declared by the composition,
and Map-presented Artifact references require a declared Map release. A typed URI
establishes resource shape; it grants no access and proves no claimed digest or license.
Artifact byte resolution applies the forwarded caller's policy before rendering.

Supported overlay geometry is marker, polyline, explicitly triangulated
polygon, oriented GLB mesh instance, and bounded label. Positions are WGS 84
or local metres. Local positions require one Frames binding that names the
world revision, exact frame, affine ECEF transform, and a governed
`frames://operation/...` input. View does not triangulate geographic polygons
or infer a coordinate transform.

Styles contain finite bounded colors and physical dimensions. Visibility is
explicit and can apply minimum or maximum camera distance. An overlay can
carry one timestamp or a validity interval. `capture_frame.sceneTime`
determines the visible set.

Inline geometry is limited to 4,096 points and 256 KiB for the complete
overlay declaration. Larger geometry uses the
`application/vnd.veoveo.view-overlay-geometry+json` artifact profile. Each
artifact is limited to 16 MiB, while retained artifact bytes for one
composition are limited to 64 MiB. The server resolves artifact bytes with the
forwarded caller token, validates their declared media type and SHA-256 digest,
and snapshots them into the recoverable capture task. No composition accepts
executable content, credentials, arbitrary URLs, or an ungoverned mesh.

## Record Admission

`SceneComposition::new` validates the request, sorts governed inputs by identity, and
computes its request digest, authority-bound identity and composition digest. Private
fields prevent mutation after construction. Decoding reconstructs the same record and
rejects disagreeing URIs, revisions, algorithm versions, digests or contents. The JSON
parser uses float round trips because coordinates participate in content identity.
Creation time is excluded from that identity.

`ViewRecord::new` derives the composition parent and resolves the supplied camera rig.
Its addresses derive from the stored IDs. Camera replacement validates the entire
replacement and checked revision increment before mutation. Creation and update times
follow composition creation and camera revision order. Decoding checks both addresses,
a positive revision, timestamps and the resolved pose. Explicit poses agree exactly;
look-at and orbit resolution allow `1e-7` degrees and `0.1` millimetres of height
variation across math implementations, then preserve the saved pose for replay.

`ViewCaptureSnapshot` freezes a view with its resolved composition. Admission verifies
the composition parent, layer and digest. Its immutable `ResolvedSceneComposition`
checks overlay metadata and geometry against the declared inline input or artifact
bytes at construction and decoding. Ordinary captures reuse that checked value.
Artifact admission checks declared media type, digest and the per-artifact and
aggregate byte limits. Unreferenced bytes
are rejected. Resolved local geometry requires a Frames binding, including geometry
loaded from an artifact. The Frames owner parses revision and frame relationships.

Capture Task decoding requires the request's view ID and revision to match the
snapshot. Before Task creation or lease claiming, the adapter checks principal, tenant,
Work Context and current capture limits. Composition creation and capture can use
different invocation policy revisions within that ownership scope. Direct snapshot
capture also checks ownership before loading a layer or submitting renderer work.
These checks validate content consistency; the authenticated Task runtime supplies
caller authority. Public Task get, update, cancel and subscription methods share
`server/tasks.rs::task_query`, which adds checked Work Context selection to the owner
query. SQL checks context and tenant agreement before decoding Task rows. Subscription
updates and reconnect baselines apply the same selection. Native Store cases exercise
this policy without renderer work; installed qualification is recorded separately.

The query also selects the declared capture operation. `server/tasks/results.rs`
checks each completed snapshot before public Task projection. It admits the saved
request and owner, decodes the frame and image payload, and rebuilds the completion
through the capture constructor and shared result formatter. The stored result must
equal that reconstruction, including parents, scene time, viewport, byte digest and
MIME type. Reads and subscription baselines, updates and reconnects use this check.
Failures report a static validation category without copying retained payloads into
diagnostics. The check verifies payload identity; it does not decode an image or prove
GPU execution.

`CapturedFrame::builder` binds an admitted view, composition, capture policy and scene
time. It checks parent identity and creation order before accepting a render report.
The finished record derives repeated resource identities, provenance, byte length and
output digest. Its private fields prevent replacing metadata independently of bytes.
The builder merges governed-input attribution into the render report and orders the
unique lines. Decode rejects missing required attribution and inconsistent ordering.
`CapturedFrame::from_record` verifies externally obtained bytes against length and digest;
image decoding belongs to the consumer. Frame decoding checks positive revisions and
viewport dimensions, supported encoding, finite achieved detail and governed input
admission. A complete detail report cannot contain pending tiles. Capture timestamps
are checked against the supplied view revision during construction.

`PreviewSceneRecord::new` derives the camera origin and local transform from the view.
Decode checks origin agreement and recomputes the local frame, allowing `1e-12` in the
basis and `0.1 mm` in translation for platform trigonometry differences. Scene tile
records require finite, invertible affine transforms and derive oversize status from
the known byte length. The manifest admits at most 256 tiles. Truncation carries a full
256-tile prefix, and truncated or oversize manifests report partial detail. These
constructors validate metadata without loading tiles or invoking a renderer.

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

A view is logical state scoped by principal, tenant and Work Context. It binds one immutable composition and survives an MCP
transport reconnect until explicit close. Camera replacement uses an expected
revision. A capture snapshots one camera revision, the resolved composition,
exact artifact bytes, and one scene time. Later camera or external artifact
changes cannot alter a queued or recovered capture.

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
| `create_scene_composition` | direct | `view:write` | immutable governed composition |
| `create_view` | direct | `view:write` | owner-scoped view and initial revision |
| `set_camera` | direct | `view:write` | replaced camera and next revision |
| `capture_frame` | task only | `view:capture` | image content and captured-frame metadata |
| `close_view` | direct | `view:write` | closed view identity |

`capture_frame` accepts an explicit scene time, physical pixel dimensions, a
maximum screen-space error, a deadline, and a typed deadline behavior.
Returning the best available frame reports whether the requested detail was
reached. Metadata contains the composition identity and digest, every governed
input, camera revision, capture and scene timestamps, style identity, Frames
revision when present, truncation and detail state, attribution, and output
SHA-256 digest.

### Resources

Domain collection resources are:

```text
view://layers
view://compositions
view://views
view://frames
```

Domain resource templates are:

```text
view://layer/{layer_id}
view://composition/{composition_id}
view://view/{view_id}
view://frame/{frame_id}
view://view/{view_id}/scene{?width_px,height_px,max_screen_error_px}
view://tile/{tile_key}
```

Compositions, views, and frames are principal and Work Context scoped.
Composition records are immutable. Discovery pages the
[fixed declarations](#resource-addresses-and-discovery). The server invalidates the
corresponding collection resource after composition creation, view creation or
closure, and capture. Camera replacement and closure also invalidate the individual
view resource.

Completion applies to visible view ids, frame ids, and configured layer ids.
No prompt belongs in the initial capture-only domain.

### Preview App

`ui://view/preview.html` is a self-contained MCP App (gated on `view:capture`
like the capture surface it drives) that exercises the real tool lifecycle:
`create_scene_composition`, `create_view`, `set_camera` under revision control, task-based
`capture_frame` through the host's task proxy, and `close_view` on teardown.
It never gets parallel convenience tools. The document is composed at serve
time from `assets/preview-app.template.html` plus the vendored three.js/draco
bundle in `assets/vendor/` (rebuilt via `tools/vendor-three/`); guard tests
pin self-containment and the console host's 2 MiB document cap.

When the host opens the App for a `create_scene_composition` result, the App
hydrates that immutable composition, reports it as ready, and reuses it for the
next `create_view` call while its selected layer remains unchanged. A
`create_view` result hydrates the complete camera view and loads its scene.
Tool failures remain visible inline and never masquerade as an empty scene.

The app's in-browser 3D scene reads the parameterized view-scene resource
(owner-scoped, `view:read`). Its typed viewport and screen-space error policy
drives the same frustum selection used for capture, and the transport admits a
complete render cut of up to 256 tiles. The app sends its current capture policy,
which keeps detail inside the camera frustum representative of the requested
frame. The manifest carries the
resolved camera, a local origin with its column-major `localFromEcef`
frame, aggregated attribution, and per-tile `view://tile/{tile_key}` URIs
with verbatim `ecefFromContent` transforms (glTF Y-up to Z-up baked in;
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

The service uses Bevy `0.19.1` with an exact dependency and a minimal feature set.
It has no Winit plugin, primary window, OS input source, camera controller,
audio stack, UI, or picking backend. An externally driven `App` renders into
`Image` targets. View disables Bevy's pipelined rendering plugin because one
renderer thread owns the render world, Vulkan queue and CUDA context. ECS
systems can still use Bevy's worker pool.

Production requires NVIDIA Vulkan and a CUDA device with the same physical-device
UUID. There is no optional GPU profile. The shared probe returns `ready` only
after renderer startup. Before reporting readiness,
View renders a GPU clear image and completes the same RGB packing, memory import,
JPEG encoding and compressed-bitstream retrieval used by captures. Missing native
libraries, an incompatible version, unavailable GPU backend, mismatched devices
or unsupported external-memory capabilities refuse startup.

Production startup logs identify the NVIDIA Vulkan adapter and device type. GPU
JPEG completion logs identify `nvjpeg_cuda_gpu`, the selected CUDA UUID, dimensions
and completion count. The owning smoke admits these records from its own process,
then checks captured image bytes and subsequent encoder completions. Probe success
alone cannot qualify those captures.

`renderer/gpu_jpeg.rs` owns nvJPEG state, quality and 4:4:4 sampling. Its explicit
`NVJPEG_ENC_BACKEND_GPU` selects CUDA execution on NVIDIA hardware. The Ada profile
does not request the fixed-function Thor encoder. `renderer/packing.wgsl` reads an
unorm view of the sRGB target to preserve its stored encoded channel bytes, removes
alpha and packs RGB on the GPU. JPEG bypasses Bevy's screenshot readback. Only the
compressed bitstream crosses into host memory. The render target stays live until
encoding completes.

`renderer/vulkan_cuda.rs` creates a device-local Vulkan buffer with opaque-FD
export memory. The owner initializes the buffer on the GPU before importing it
into WGPU. After packing, a completed WGPU submission and a native queue-family
release to `EXTERNAL` with a completed Vulkan fence establish CUDA ownership.
CUDA maps the same allocation. Its completion event precedes releasing the mapped
pointer, imported memory, WGPU buffer and backing Vulkan allocation, in that order.
The buffer has one Vulkan writer and one CUDA reader; no GPU pixels are uploaded
from a CPU screenshot.

CUDA context and stream mechanics use maintained `cudarc 0.19.10`. The adapter uses
that release's generated driver result APIs for external memory. The convenience
`driver/safe/external_memory.rs::ExternalMemory::map_range` creates a mapped pointer
before a fallible event creation. View's narrow mapping owner instead establishes
ownership before mapping and frees the pointer before the imported memory on every
admitted release. A pre-submit mapping failure releases only the import. View can
remove this adapter when the maintained wrapper frees mapped pointers on event
creation failure and passes these owner lifetime controls.

The checked-in `renderer/nvjpeg_bindings.rs` is generated by `bindgen 0.73.2` from
NVIDIA nvJPEG `13.2.1.68` header SHA-256
`4a4ffe473dc48d06986993bcf7de14bf31c2c955a9bd823fc3856a8c545094ad`.
The generator uses the complete CUDA `13.3.1` target headers, Rust 2024, Rust
minimum target 1.85 and explicit unsafe blocks. The header inputs include CUDART
`13.3.29-1`, CRT `13.3.73-1`, CCCL `13.3.3.4.1-1`, driver development
`13.3.29-1`, culibos development `13.3.33-1` and nvJPEG development
`13.2.1.68-1` from NVIDIA's signed Ubuntu 24.04 repository. The generator preserves
the header's license notice and suppresses declaration comments and layout tests.
Its allowlist contains only
`nvjpegCreateSimple`, `nvjpegDestroy`, `nvjpegGetProperty`,
`nvjpegEncoderStateCreateWithBackend`, `nvjpegEncoderStateDestroy`,
`nvjpegEncoderParamsCreate`, `nvjpegEncoderParamsDestroy`,
`nvjpegEncoderParamsSetQuality`, `nvjpegEncoderParamsSetSamplingFactors`,
`nvjpegEncodeImage` and `nvjpegEncodeRetrieveBitstream`, plus their referenced types
and version constants. It uses bindgen's all-required dynamic library wrapper
`Nvjpeg` with `libloading 0.9.0`. Normal Cargo builds require neither SDK headers
nor NVCC. Runtime admission reads the generated nvJPEG major, minor and patch
constants through `nvjpegGetProperty`; the image pins the build package. All native
dependencies and renderer modules are gated by `runtime`;
independent `contract` consumers exclude them.

### Completion And Interruption

One 15-second user-space completion deadline covers capture render pumping, native
buffer initialization, packing, ownership release, encoding, retrieval and target
cleanup. Startup completion observations use a 30-second budget. Resource and
shutdown drain observations use five seconds. View's current Pod uses Kubernetes'
30-second termination grace. Shutdown rejects further queued captures, so it waits
for at most the current capture plus drains rather than four FIFO GPU captures.
The service selects the combined Task and View-close cancellation token while
awaiting the renderer. Cancellation closes its queued response before dispatch;
already active GPU work completes before the renderer releases storage. A server
lifetime guard signals queue shutdown when hosted serving returns, independently
of renderer handles retained by Task workers. Installed shutdown still requires
qualification against the hosting lifecycle.

If submitted native work has an uncertain outcome or a completion observation
exceeds its deadline, View exits with status 70 through Linux `_exit`. It skips
Rust and native destructors, produces no core dump and does not reset the GPU.
Fatal device-loss and uncaptured-error callbacks are installed before JPEG
admission. Immediate submission and poll calls catch backend panics with native
owners outside the unwind boundary. The render pump uses the same boundary while
its renderer storage stays outside it; this cannot repair a dependency that has
already freed in-flight storage during its own unwind. Explicit device destruction
is accepted only after the owner observes WGPU and CUDA shutdown drains; native
loss still terminates even after that witness. The service cannot publish Task
success after that exit. Persisted capture
snapshots and unresolved Task outcomes preserve the shared runtime's lease,
process-epoch and output-reuse checks. View establishes shared startup recovery
observation before its baseline and schedules the immediate resumable report.
Tasks held under live leases are revisited automatically on native Task changes
or their earliest retained lease deadline. Every scheduling attempt admits the
stored capture snapshot, validates its owner and current resources, then claims
through the durable lease guard. A claim conflict or active-lease response triggers
one trusted runtime recovery read. Physical deletion or a terminal Task settles the
handoff. A Task reassigned to another server fails before its payload is decoded. An
unfinished Task must preserve its admitted server, operation, recovery profile,
owner and capture input, and only a
live lease held by a different worker proves another replica owns execution.
An unowned, expired or own-worker lease after a failed claim ends serving.
The observer covers only Tasks admitted at startup; normal capture admission owns
new Tasks and active workers.

The shared `TaskRecoveryObserver` owns observation independently of renderer and Task-held
state clones. Hosted exit cancels and drains it within five seconds. A stalled
drain aborts the observer and returns an error. Observer query, admission or
scheduling errors end serving instead of silently abandoning retained Tasks.
Exhausting the startup set is healthy completion. Live leases held by another
worker do not delay readiness. Native controls inject claim error categories
against real Store states without claiming transaction-contention coverage.
Its owning lifetime tests use no renderer and
establish behavioral shutdown and error propagation only.

These user-space observation bounds
do not promise to interrupt a kernel or driver hang.

PNG keeps its explicit public encoding. Its screenshot readback, RGB conversion
and compression are marked `TODO(GPU)` for a device-resident PNG encoder. PNG
cannot establish GPU JPEG acceptance.

The NVIDIA image contains the Vulkan loader and exact CUDA/nvJPEG user-space
libraries. NVIDIA Container Toolkit injects driver capabilities `graphics`,
`compute` and `utility`; Helm requests one `nvidia.com/gpu` per renderer replica.
The pinned CUDA 13.3.1 image requires driver-advertised CUDA capability of at
least 13.3 for the GeForce profile. The NVIDIA container runtime must enforce
`NVIDIA_REQUIRE_CUDA`; disabling that admission check is unsupported. The
driver qualification profile is NVIDIA open driver `610.57.04` with CUDA UMD
`13.3`. View selects cudarc's `cuda-13030` driver bindings. The deterministic owner
GPU control must execute on the installed driver before
image and runtime qualification. Package installation alone supplies no execution
qualification. View uses standard external-memory and nvJPEG APIs without
NVRTC-generated PTX or an older-library fallback.

## Attribution

Each decoded glTF can carry `asset.copyright`. Governed composition inputs
carry required attribution. A captured render cut collects, sorts, and
deduplicates both sources into an `AttributionSet`. The frame result returns
that set for display beside the image.

## Limits And Failure

Configuration bounds compositions per owner and globally, active views,
captures in flight, tile load concurrency, response bytes, tree nodes,
viewport dimensions, overlay counts and geometry, retained artifact bytes,
frame retention, cache bytes, and Google source requests. Limits fail closed
with typed MCP errors.

Closing a view cancels unfinished captures for that view. Renderer startup
fails before readiness when Vulkan selects a CPU, fallback, or non-NVIDIA
adapter in the production profile.

## Deployment And Verification

[Controlled-input fixtures](testdata/controlled-inputs.json) qualify scene positions, overlay geometries and inline or artifact geometry sources through the [hosted admission test](src/mcp/tool_input_tests.rs) and the [independent contract consumer](../../testing/fixtures/server-contract-consumer/DESIGN.md).

The Kubernetes Service exposes health and MCP ports only inside the cluster. The
gateway is the normal caller and forwards signed internal identity. The
container runs as the non-root Veoveo user with a read-only root filesystem and
writable `/tmp`.

Rust tests cover camera resolution, ECEF cancellation, tree construction, SSE
selection, weighted eviction, freshness, credential-free cache keys, GLB
preprocessing, and typed unsupported content. The Rust smoke path starts the
renderer inside the NVIDIA container, verifies a non-CPU Vulkan adapter,
captures both PNG and GPU JPEG from a deterministic local tileset and governed marker, polyline, polygon,
and label overlays through source, traversal, decode, and Bevy through the
production MCP task and frame-resource boundaries. It verifies composition
provenance, output digest, decoded dimensions, frame-resource byte agreement and owner isolation. GPU JPEG completion records must match the admitted device UUID and the local capture dimensions. The owning library tests cover pre-submit map failure, post-submit failure release ordering, canceled queued captures, shutdown queue rejection and destructor-free deadline, backend-panic, device-loss and partial-submission exits. The production image
contains only the View MCP server. Build it with `cargo xtask image build
--target view-mcp`, then dispatch the Rust harness with `cargo xtask smoke
view-mcp`.

The local `view-mcp` smoke also submits eight JPEG Tasks with one capture
in flight. It freezes the owning container after observing a claimed unfinished
Task, rechecks that row, then kills the container. A replacement starts on the
same owned Store before the unchanged 180-second lease expires. The harness
requires no early claim or completion, then verifies GPU JPEG completion,
provenance, digest, frame-resource bytes and owner isolation through the maintained
Task interface. Normal replacement shutdown must exit successfully within
30 seconds and record the GPU UUID and encoder. This lifecycle has a 360-second
deadline inside the smoke scenario's 3,600-second budget. Lifecycle Docker
commands use the maintained asynchronous process owner and the remaining
absolute deadline. Interruption inspection has five seconds. Each final log
collection has five seconds, including after failure. Command timeout or
cancellation kills its owned group immediately and gives reaping at most one
second, preserving unresolved registrations. Caller isolation requires the
declared MCP invalid-parameters response and the domain's unknown-Task or
unknown-resource diagnostic; transport and internal errors fail the probe.
Logs precede cleanup,
including failed observations. This local signed fixture covers the owning
hardware lifecycle; gateway OAuth and installation rollout have separate
acceptance. Executing this scenario is required for GPU runtime qualification.

The billed live acceptance scenario captures Google Photorealistic 3D Tiles
from a camera orbiting the Statue of Liberty at 40.6892494, -74.0445004. It
requires the API key in `GOOGLE_MAPS_API_KEY`, passes the variable by name
rather than putting its value in the command line, drives the production MCP
task interface, requires an NVIDIA adapter, and retains only the rendered JPEG:

```sh
cargo xtask image build --target view-mcp
cargo xtask smoke view-google-live \
  --output /tmp/veoveo-view-proof/statue-of-liberty.jpg
```

## Identity Declaration Mechanics

View and scene input/overlay/style identities use `Id` with their existing owner validators and String schema declarations. Composition stable-key generation and typed resource dispatch keep their separate implementations. An admitted ID establishes neither scene membership nor access.

## Value Admission

Frame, tile, preview and composition records use immutable `Checked` storage with owner geodesy, ordering, identity and digest checks. Composition construction sorts declared inputs before admission; decoding compares canonical facts and rejects unsorted records. ViewRecord keeps its explicit identity projection and camera agreement adapter.

## Native Query Fixture Placement

Store-backed native fixture statements live in `tests/queries/`, grouped by the
calling harness. Colocated fixtures include those files with their existing bindings
and result slots. Complete static statements cover finite SQL grammar choices.

## Task Completion Products

Capture metadata carries `resultUri` equal to its checked `frameUri`. The MCP
result contains short status text, image content and one matching frame resource link.
Task completion admits that envelope before storage; retained reads reconstruct the
same checked capture metadata and content.

## Browser Contract Admission

The App imports its generated `app/generated` types and JSON Schema bundle from
`contract::app_schema`. The bundle selects the same DTOs that tools and resource
readers serialize. Browser adapters validate a complete result before publishing
it to state, then check its requested parent identities. Collection pages keep the
owner's cursor representation and existing stale-response checks. Open provider
maps keep their admitted contents.

The shared MCP Apps browser package bundles the maintained SDK protocol schemas
and CSP-safe CfWorker JSON Schema validator into the App. It has no domain route
registry. This server owns the route and tool selection in `app/contracts.js`.
The package build produces the self-contained HTML asset at its existing path,
with a 2 MiB limit. Schema URLs describe formats and never fetch executable code.
Behavioral contract tests qualify rejection before rendering or decoding; they
make no hardware or visual acceptance claim.

The App typecheck uses maintained `@types/three` `0.186.0` and `@types/draco3d`
`1.4.10`, verified against their authoritative package metadata. The declared
Three surface is limited to constructors and constants consumed from the
qualified vendor; startup checks its required constructors and OrbitControls.
These development declarations do not change the renderer or its GPU qualification.

## NVIDIA Runtime Notice

NOTWITHSTANDING ANY TERMS OR CONDITIONS TO THE CONTRARY IN THE
LICENSE AGREEMENT, NVIDIA MAKES NO REPRESENTATION ABOUT THE
SUITABILITY OF THESE LICENSED DELIVERABLES FOR ANY PURPOSE.  IT IS
PROVIDED "AS IS" WITHOUT EXPRESS OR IMPLIED WARRANTY OF ANY KIND.
NVIDIA DISCLAIMS ALL WARRANTIES WITH REGARD TO THESE LICENSED
DELIVERABLES, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY,
NONINFRINGEMENT, AND FITNESS FOR A PARTICULAR PURPOSE.
NOTWITHSTANDING ANY TERMS OR CONDITIONS TO THE CONTRARY IN THE
LICENSE AGREEMENT, IN NO EVENT SHALL NVIDIA BE LIABLE FOR ANY
SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, OR ANY
DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS
ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
OF THESE LICENSED DELIVERABLES.

U.S. Government End Users.  These Licensed Deliverables are a
"commercial item" as that term is defined at 48 C.F.R. 2.101 (OCT
1995), consisting of "commercial computer software" and "commercial
computer software documentation" as such terms are used in 48
C.F.R. 12.212 (SEPT 1995) and is provided to the U.S. Government
only as a commercial end item.  Consistent with 48 C.F.R.12.212 and
48 C.F.R. 227.7202-1 through 227.7202-4 (JUNE 1995), all
U.S. Government End Users acquire the Licensed Deliverables with
only those rights set forth herein.

Any use of the Licensed Deliverables in individual and commercial
software must include, in the user documentation and internal
comments to the code, the above Disclaimer and U.S. Government End
Users Notice.

## Installed Lifecycle Qualification

The existing [View acceptance owner](../../examples/bioma/acceptance/DESIGN.md#installed-view-lifecycle)
selects an Ops-staged immutable local triangle/catalog and qualified image before
public calls. Independent MCP clients sharing principal, profile, tenant and
WorkContext receive the same completed Task. A comparison WorkContext and different
principal cannot get, cancel or receive notifications for that owner Task. Listen
acknowledgement accepts requested handles; authorized SQL selects the baseline.
The negative listener observes a permitted collection baseline before its declared
one-second exclusion interval.

The installed mode binds the selected Pod through its ReplicaSet to the declared
Deployment, observes successful old-container termination inside configured grace,
and checks replacement readiness separately. It recovers unchanged completed JPEG
bytes through a fresh public client. Its caller collection proves the old
in-process View absent after admitted replacement; failures before replacement
require explicit closure of the admitted or reconciled owned View. A lost Create
reply cannot authorize another Create or make missing cleanup qualified.
This qualifies completed result retention;
interruption of an actively claimed capture and hardware visual acceptance retain
their own qualification requirements. Fixture export and CPU observer tests do not
establish an installed or hardware pass.
