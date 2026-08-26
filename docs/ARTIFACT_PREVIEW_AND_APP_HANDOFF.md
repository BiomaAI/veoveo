# Artifact Preview And App Handoff Investigation

## Status

This is an exploratory document based on the repository state on 2026-08-26. It
records current behavior, registration paths, gaps, and design choices. It is not a
normative contract, an implementation plan, or authorization to change the artifact
plane, Console, gateway, MCP Apps host, or extension boundary.

The investigation uses *artifact catalog* for the operator-facing collection in the
Console. The repository also has an Artifact MCP resource catalog and an MCP App
catalog. They have different membership rules and must not be treated as one surface.

## Standards And Protocols

| Standard or protocol | Relevance to this investigation |
|---|---|
| Model Context Protocol `2026-07-28` | Veoveo's pinned protocol for artifact resources, resource links, tools, tasks, discovery, and subscriptions. |
| [MCP Apps SEP-1865 / `ext-apps` `2026-01-26`](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx) | Stable UI-resource, tool-link, sandbox, and host-bridge contract used by Veoveo Apps. It does not define a generic artifact-handler registry or artifact launch payload. |
| JSON Schema Draft 2020-12 | Controlled MCP tool schemas, extension manifests, gateway fragments, and installation bindings. Artifact producer metadata itself remains open JSON. |
| HTTP GET, HEAD, and single byte ranges | Governed artifact preview and download data plane through the installation origin. Object-store URLs are private implementation detail. |
| [RFC 6838 media types](https://www.rfc-editor.org/rfc/rfc6838.html) and registered structured syntax suffixes | Portable starting point for format matching. A `+json` suffix carries more reliable generic meaning than a filename or a substring test. |
| [RFC 8288 Web Linking](https://www.rfc-editor.org/rfc/rfc8288.html) and the [IANA Link Relation registry](https://www.iana.org/assignments/link-relations/) | Existing relation vocabulary includes `preview`, `edit`, `edit-media`, `alternate`, and `describedby`. Those relations name intent but do not provide App discovery, authorization, ranking, or launch semantics. |
| `veoveo.io/gateway-server-fragment/v1` and `veoveo.io/gateway-binding/v1` | Current installation-owned path for adding an external hosted MCP server and its Apps. |

## Executive Finding

Veoveo already has a governed artifact plane and a dynamic MCP App catalog, but the
two catalogs do not meet. Artifact preview dispatch is compiled into the Console and
selects one renderer from a MIME string plus one recording-provenance special case.
MCP Apps are discovered independently from `ui://` resources and linked tools. No
contract says which App can view, import, analyze, or edit a selected artifact.

More artifact occurrences can enter Veoveo through first-party and external domain
servers. More Apps can enter an installation through the external-extension contract.
Neither action makes a new viewer appear on an artifact automatically.

The missing seam contains three separate questions:

1. What does an artifact mean beyond its MIME type and filename?
2. Which caller-visible Apps can act on that meaning, and with which operation?
3. How does the selected App receive governed context or bytes without escaping the
   artifact policy and audit boundary?

Solving only App discovery would leave the launch and data-plane questions open.

## Current Topology

The repository has four catalog-like projections and one hardcoded dispatch table:

| Surface | Authority and membership | Consumer |
|---|---|---|
| Artifact occurrence ledger | SurrealDB occurrence, blob, grant, release, retention, provenance, and open metadata records | Artifact service and Console gateway projection |
| Artifact MCP catalog | Artifact service `list`; only retained artifacts on which the caller has effective read access | MCP clients through `resources/list` and `artifact://index` |
| Console artifact catalog | Newest tenant rows loaded directly from the platform store, with effective access calculated for the active caller | Console table and artifact drawer |
| MCP App catalog | Caller-visible `text/html;profile=mcp-app` resources and their linked tools, federated through the gateway | Console navigation and standalone App host |
| Console preview dispatch | A TypeScript MIME-family test plus recording provenance | Artifact drawer |

```text
domain server ──put/capability──> Artifact service ──> occurrence + private blob
                                              ├────> Artifact MCP resources
                                              └────> Console artifact snapshot

hosted MCP server ──ui:// resource + linked tools──> gateway ──> App catalog

Console artifact snapshot ──MIME/recording checks──> built-in preview or download

There is no artifact-selection edge from the Console catalog to the App catalog.
```

The separation is deliberate at the storage boundary. Artifact service owns bytes and
access decisions, while domain servers own domain meaning. The weak point is the
presentation projection, which currently drops most of that meaning without replacing
it with a handler relation.

## How Artifacts Enter Veoveo Today

Artifact service creates immutable occurrences. A producer sends bytes and bounded
presentation metadata through `ArtifactPlane::put`, or redeems a task-bound write
capability after asynchronous work. The service assigns the UUIDv7 occurrence, stamps
trusted tenant and ownership authority, stores the bytes, and creates initial grants.
There is no reference-only registration that adds an arbitrary external URL to the
catalog.

The artifact put descriptor carries:

- MIME type and filename;
- classification, data labels, and retention expiry;
- at most 4 KiB of producer-defined JSON metadata.

The MIME check is intentionally shallow today: the value must be trimmed, bounded,
control-free, and contain `/`. Artifact service does not parse the full RFC 6838
grammar or normalize parameters. Any future matching logic cannot assume that stored
values are registered or canonical media types.

Artifact MCP does not expose an upload or create tool. Its six tools read metadata and
manage grants, release state, and share links. An ordinary Console user also has no
generic upload action. New occurrences therefore arrive through a domain workflow:
Media generation, DuckDB export, Frames batch output, Map acquisition or publication,
Optimization output, recording sealing, Stream or Reason analysis, Timeseries
forecasting, or an external server using the shared artifact plane.

An external server can produce artifacts when its release declares the `artifact`
platform capability and its artifact audience is admitted by the installation. The
extension does not grant itself that audience. The installation binding and platform
selection remain authoritative.

This gives three distinct meanings to “users can add more”:

| Actor | Current path |
|---|---|
| Console operator | Invoke a domain tool that produces an artifact. There is no arbitrary file registration surface. |
| Domain developer | Use the shared Artifact client or SDK and return the resulting domain-presented artifact resource link. |
| Installation owner | Install an external hosted MCP server, admit its artifact audience, and expose its tools and resources through a gateway binding. |

## What The Console Actually Catalogs

The Console does not consume the Artifact MCP list. Its gateway snapshot queries the
newest 200 tenant artifact occurrences, then joins at most 200 blobs, grants, and share
links. The browser filters those rows locally by release state and a text search over
ID, filename, owner, and labels.

That has several consequences:

- Search is not a search of the complete artifact plane. It is a search of the current
  snapshot.
- The view has no next page, server-side query, MIME facet, producer facet, domain
  facet, semantic kind, or relationship filter.
- The Console can show a same-tenant artifact for which the caller lacks read access,
  calculate why access is denied, and offer an access request. Artifact MCP discovery
  instead omits artifacts without effective read access.
- Artifact service list filters expired retention. The Console snapshot query itself
  does not apply that filter because it reads occurrence rows directly.
- The generic `metadata` object does not enter `ArtifactSummary`. The sole semantic
  extraction recognizes a `metadata.provenance` object containing `recording_id`, then
  offers the recording workspace.

The Artifact MCP list is broader as a protocol surface but narrow as a query language.
`ListArtifactsRequest` has only `cursor` and `limit`. Resources are returned in pages of
100, newest first. `artifact://index` also returns the first 100. There are no protocol
filters for text, owner, Work Context, media type, label, producer, time, task, or
semantic profile.

## Current Preview Dispatch

The artifact drawer first authorizes the governed preview with a one-byte range
request. It then selects from this fixed table:

| Stored MIME value | Current action | Byte behavior |
|---|---|---|
| exact `application/vnd.rerun.rrd` | Embedded Rerun WebViewer 0.36.0 | Fetches the complete artifact into an `ArrayBuffer`, then sends it to a Rerun channel. |
| `image/*` | Browser image | Original governed URL. |
| `video/*` | Browser video controls | Original governed URL with metadata preload and range support. |
| `audio/*` | Browser audio controls | Original governed URL with metadata preload and range support. |
| exact `application/pdf` | Same-origin iframe | Original governed URL. |
| `text/*`, or any MIME string containing `json`, `xml`, or `yaml` | Escaped text in a `pre` element | First 256 KiB. |
| everything else | No inline renderer | Details and download only. |

The dispatch lowercases the whole MIME string but does not parse its type, subtype,
suffix, or parameters. Exact PDF and RRD matches therefore reject parameterized forms,
while the JSON/XML/YAML checks are substrings rather than registered structured syntax
suffix checks. Filename extensions do not participate.

There is no thumbnail service, representation manifest, alternate preview, conversion
task, handler choice, preferred viewer, or user default. A row action is labeled from
the same MIME table and opens the artifact drawer. It does not open a discovered App.

### Formats Already Produced In The Repository

| Producer | Artifact formats observed | Current presentation |
|---|---|---|
| Recording | `application/vnd.rerun.rrd`; `application/vnd.veoveo.recording-manifest+json` | Rerun or text. Recording provenance also opens the full recording workspace. |
| Stream | `application/vnd.veoveo.stream-results+json`; `application/vnd.rerun.rrd`; `video/mp4` | Text, Rerun, or video. All retain a recording relation. |
| Reason | `application/vnd.veoveo.reason-results+json`; `application/vnd.rerun.rrd`; `video/mp4` | Text, Rerun, or video. All retain a recording relation. |
| Timeseries | `application/vnd.veoveo.rerun-rrd` | No inline renderer. Its MIME value differs from the Console's exact Rerun value even though Timeseries ships a forecast App. |
| DuckDB | `text/csv`; `application/vnd.apache.parquet`; `application/vnd.duckdb` | CSV text; Parquet and DuckDB download only. |
| Frames | `application/json` batch transform | Text preview. |
| Optimization | JSON problem, solution, and report artifacts | Text preview. |
| Map | JSON travel model; GeoJSON Sequence; GeoParquet; GeoPackage; MVT bundle; acquisition products | JSON-like formats render as text. Binary geospatial products download only. |
| Media | Provider-derived image, video, and other admitted MIME types | Browser-native family when recognized, otherwise download. |
| Python extension template | JSON datasheet profile | Text preview. |

The Timeseries mismatch is useful evidence because it shows why MIME dispatch alone is
fragile. The artifact has both a richer semantic profile (`artifact_format:
rerun_rrd`) and a domain App, but neither fact reaches the Console's renderer choice.

## Apps And Viewer Registration Today

Veoveo discovers Apps rather than listing them in Console source. A hosted server:

1. advertises the MCP UI extension;
2. lists a `ui://` resource with `text/html;profile=mcp-app`, title, description, and
   optional data-URI icon;
3. serves the self-contained HTML from `resources/read`;
4. links each App-callable tool to that resource through `_meta.ui.resourceUri` and
   visibility;
5. relies on gateway profile exposure and policy for the caller-visible projection.

The BFF federates those resources, derives App ownership from the projected
`ui://{server}/...` URI, attaches only linked tools from the same server, and gives the
Console a dynamic catalog. No Console code change or manual App card is required.

The current installation example exposes these Apps:

| App | Current purpose | Artifact-specific behavior |
|---|---|---|
| Stream | Live encoded video and pipeline overlays | Operates live sessions, not stored artifact occurrences. |
| Timeseries | Forecast input, preview, and rerun | Does not open a selected forecast artifact from the catalog. |
| Map workspace | Map viewing, authoring, publications, acquisition, and governance | Manually accepts an artifact ID for GeoJSON, GeoJSON Sequence, or GeoPackage inspection/import. This is the clearest existing editor/importer capability. |
| View preview | Interactive 3D scene composition, camera, and capture | Drives View resources and tools; it is not registered as a generic artifact viewer. |
| UAV live cameras | Authoritative live simulator cameras | Operates live views, not stored artifacts. |
| Charts | Interactive Flint chart tool result | Opens from `create_chart_view` structured content, not from an artifact. |

The embedded Rerun artifact renderer is not an MCP App. It is compiled into the
Console. The recording workspace is also a platform-plane view. Those two paths cannot
be extended by installing another App.

### External Installation Path

An installation owner can add a viewer or editor as an external extension today, but
the unit of registration is a hosted MCP server, not an App-only URL or a user-local
preference. The supported path is:

1. package the server, App resource, linked tools, image, chart, conformance result,
   gateway fragment, and extension release;
2. declare `capabilities.apps: true`, `resources: true`, and
   `resource_projection: server_owned` in the fragment;
3. expose the projected `ui://{server}/` resource family and required tools in an
   installation-owned binding;
4. admit required artifact audiences and platform components;
5. compose the control plane and deploy both platform and extension releases.

There is no per-user runtime server registration, App marketplace record, uploaded HTML
viewer, arbitrary external URL registration, or “open with” preference store. The
installation reviews and deploys the code that enters the App sandbox.

The Chart server also exposes a useful registration inconsistency. Its upstream server
advertises `io.modelcontextprotocol/ui`, lists a vendor-owned `ui://flint-chart/...`
resource, and relies on server-owned projection to become `ui://charts/...`. The
example gateway manifest does not set its newer `capabilities.apps` flag even though
the App is exposed by the profile. This does not create artifact handoff, but it shows
that App capability is currently signaled in more than one place.

## Why App Navigation Does Not Carry An Artifact

Console App navigation stores only the selected App resource URI. The route is
`#/apps/{server}/{page}` in the Console or `/apps/{server}/{page}` in the standalone
host. Neither route includes an artifact identity or an operation.

An App can request `ui/open-link`, but Veoveo admits only an exact caller-visible
`ui://` App URI or the two platform targets for Agents and Recordings. The internal
link contains no typed launch context. Arbitrary Console routes and browser-supplied
server aliases fail closed.

The host bridge contains methods that can deliver `ui/notifications/tool-input` and
`ui/notifications/tool-result` to a frame. They support the standard tool-driven App
lifecycle, but no Console artifact flow invokes them. They also do not answer handler
matching, App selection, or governed byte access by themselves.

The Map workspace demonstrates what manual handoff looks like now. A user copies an
artifact UUID into its import form. The App invokes linked Map tools, and the Map
server resolves the artifact through the shared plane. Authorization is correct, but
the catalog has no way to discover or prefill that capability.

## The Governed Data-Plane Constraint

An artifact-aware App cannot safely be treated as ordinary page navigation.

MCP Apps run in an opaque-origin `sandbox="allow-scripts"` iframe. They receive no
Console cookie, gateway bearer, storage authority, or object-store address. Their
ordinary resource bridge is bounded to 2 MiB of JSON-encoded MCP results. Blob resource
contents are base64 inside that bound, which makes the bridge unsuitable for large
video, RRD, Parquet, GeoPackage, database, or 3D assets.

Veoveo's cross-server App dependency contract does permit controlled resource reads,
but each dependency must name a non-root URI family. It can describe
`artifact://metadata/`, for example, but not the root `artifact://{uuid}` content
family. Cross-server subscriptions are not admitted either. Even if the root family
were expressible, the 2 MiB bridge would remain the wrong transfer path for large
bytes.

A viewer's backend can resolve an authorized artifact if its server is installed with
the Artifact capability and audience. It can then expose a bounded domain resource,
invoke a conversion or import tool, or establish a separately governed data-plane
session. No common viewer contract tells it which of those paths to use.

The existing browser preview endpoint already has the desired byte authority: gateway
policy, audit, short-lived internal assertion, Artifact service enforcement,
backpressure, and range semantics on the installation origin. It is Console-session
specific and is not an ambient capability handed into Apps. Raw S3 URLs, provider
URLs, and unscoped bearer URLs are outside the supported design.

## Existing Standards That Help, And Their Limit

MCP Apps supplies the deployable UI, discovery, sandbox, tool linkage, and bridge. Its
stable `2026-01-26` profile links a tool to one UI resource. It does not let an App
declare “I preview these media types,” let an artifact advertise compatible Apps, or
define a host launch message for an already-existing resource. The specification's
future considerations include richer content and preview work, which reinforces that
this is not part of the stable profile Veoveo pins.

RFC 8288 provides suitable relation names. An artifact could be related to a preview,
an editable working surface, an alternate representation, or descriptive metadata
without inventing those English meanings. A link relation does not say whether the
target is an MCP App, which media profiles it accepts, how a host passes the artifact,
whether the caller can use it, or which candidate wins.

RFC 6838 media types remain useful for broad compatibility. Exact types and structured
suffixes can identify a syntactic representation. They cannot distinguish two JSON
artifacts such as a route solution and a recording manifest, nor can they say whether
an operation views, imports, validates, converts, or derives a new artifact.

Installed web-app file handlers are a poor match for the current boundary. They act on
local files through browser and operating-system registration. Veoveo artifacts remain
governed server resources, and Apps deliberately receive neither local-file authority
nor the user's authenticated download capability.

## Registration Models Considered

This section records the design space. It does not select or authorize a model.

| Model | What it would express well | Unresolved cost |
|---|---|---|
| Producer-attached action links | One artifact can name exact `preview`, `edit`, `describedby`, or domain relations at creation. | App URIs become stale when installations change. Producer metadata is open, bounded to 4 KiB, and not installation authority. |
| App-declared handlers | A discovered App can declare accepted media types, semantic profiles, operations, priority hints, and one launch entrypoint. Removal automatically removes the candidate. | Veoveo needs a typed extension to MCP App resource metadata, matching rules, and a governed launch/data path. |
| Installation handler registry | An owner can approve mappings, rank viewers, override defaults, and bind policy without trusting a producer or App to self-authorize. | Adds installation configuration and risks duplicating facts that an App can already declare. |
| Domain-owned artifact relations | A producer can retain a source resource, recording, analysis, publication, or workspace relation and open the domain surface with richer meaning than MIME. | Generic formats and cross-domain tools still need handler discovery. Not every artifact has a durable domain workspace. |
| Built-in Console renderer table | Small, predictable, and already governed by the Console session. | Requires a Console release per format, offers one choice, and cannot use externally installed Apps. |

The evidence favors a hybrid if this area advances: App declarations would describe
capability, installation policy would admit and order it, and artifact metadata would
retain typed domain meaning rather than installed App identities. That observation is
not a schema decision. The launch and byte contracts still need separate design.

## Capability Dimensions A Future Contract Must Settle

| Dimension | Questions exposed by current behavior |
|---|---|
| Operation | Is the action `preview`, `view`, `inspect`, `import`, `analyze`, `convert`, `edit`, or `derive`? |
| Match | Are exact MIME values enough? Are wildcards, `+json` suffixes, filename extensions, domain kinds, schema URIs, and versions admitted? |
| App identity | Is the target one exact projected `ui://` resource, a server-local handler name, or an installation alias? |
| Launch input | Does the host send `artifact_id`, canonical `artifact://` URI, media type, filename, domain relation, requested operation, and return location? |
| Read path | Does the App read bounded MCP content, call its backend, receive a scoped ranged URL, or open a domain-specific streaming session? |
| Output | Does an editor create a new artifact occurrence, a domain revision, or both? Existing artifacts are immutable. |
| Eligibility | Must the candidate be caller-visible, profile-exposed, scope-authorized, label-compatible, and able to read the selected occurrence? |
| Ordering | Who chooses among exact domain viewers, generic format viewers, built-in renderers, and download? Can users retain a preference within installation policy? |
| Lifecycle | What happens when an App is removed, its declaration changes, an artifact expires, access is revoked, or the App's token is replaced? |
| Disclosure | Can the catalog reveal that a handler exists when the caller cannot read the artifact or invoke the App? |

“Edit” needs an explicit answer. The artifact occurrence is immutable, so an editor
cannot save over it. Editing can only produce a new occurrence, create or revise a
domain object, or perform a governed import whose result points back to the source.

## Findings Register

| ID | Finding | Evidence |
|---|---|---|
| F01 | The Console artifact catalog is a newest-200 tenant snapshot, not complete artifact discovery. | [`projection.rs`](../platform/gateway/src/bin/gateway/admin/console/projection.rs) |
| F02 | Console search and release filtering operate only on the browser's snapshot rows. | [`Artifacts.tsx`](../apps/console/web/src/views/Artifacts.tsx) |
| F03 | Artifact MCP discovery has cursor and limit only; its page size is 100. | [`artifact_service.rs`](../mcp/contract/src/artifact_service.rs), [`handler.rs`](../servers/artifact-mcp/src/bin/server/handler.rs) |
| F04 | Console and Artifact MCP catalogs have different membership semantics: tenant occurrence visibility versus effective read visibility. | [`projection.rs`](../platform/gateway/src/bin/gateway/admin/console/projection.rs), [`service.rs`](../platform/artifacts/service/src/service.rs) |
| F05 | Producer metadata is open JSON, while `ArtifactSummary` projects only a recognized recording relation. | [`storage.rs`](../mcp/contract/src/storage.rs), [`projection.rs`](../platform/gateway/src/bin/gateway/admin/console/projection.rs) |
| F06 | Preview selection is a fixed Console MIME test with no App lookup or handler choice. | [`ArtifactPreview.tsx`](../apps/console/web/src/components/ArtifactPreview.tsx), [`artifactPreview.ts`](../apps/console/web/src/artifactPreview.ts) |
| F07 | Timeseries emits `application/vnd.veoveo.rerun-rrd`; the Console Rerun viewer requires exact `application/vnd.rerun.rrd`. | [`forecast.rs`](../servers/timeseries-mcp/src/forecast.rs), [`ArtifactPreview.tsx`](../apps/console/web/src/components/ArtifactPreview.tsx) |
| F08 | App descriptors contain presentation, tools, dependencies, and agent targets, but no accepted artifact formats or actions. | [`apps.rs`](../apps/console/bff/src/apps.rs), [`models.rs`](../mcp/apps-extension/src/models.rs) |
| F09 | Console and standalone App routes identify only an App, with no artifact launch context. | [`App.tsx`](../apps/console/web/src/App.tsx), [`app_host.rs`](../apps/console/bff/src/app_host.rs) |
| F10 | Installation owners can add Apps through external server fragments and bindings without changing Console source. | [`EXTERNAL_EXTENSIONS.md`](EXTERNAL_EXTENSIONS.md), [`anonymous.gateway-fragment.json`](../extensions/examples/anonymous.gateway-fragment.json), [`anonymous.gateway-binding.json`](../extensions/examples/anonymous.gateway-binding.json) |
| F11 | Console users cannot register arbitrary files or viewers. Artifact MCP has no put tool, and App registration is installation-owned. | [`DESIGN.md`](../servers/artifact-mcp/DESIGN.md), [`EXTERNAL_EXTENSIONS.md`](EXTERNAL_EXTENSIONS.md) |
| F12 | The Map workspace can inspect and import a manually entered authorized artifact ID, proving an existing App-side artifact operation without catalog handoff. | [`workspace-app.html`](../servers/map-mcp/assets/workspace-app.html), [`transfers.rs`](../servers/map-mcp/src/contract/transfers.rs) |
| F13 | The App resource bridge is capped at 2 MiB and cross-server dependencies require a non-root URI family, so it is not a generic large-artifact byte path. | [`apps.rs`](../apps/console/bff/src/apps.rs), [`validation.rs`](../mcp/contract/src/gateway/validation.rs) |
| F14 | External producers can use the artifact plane only through installation-admitted platform capability and artifact audience. | [`EXTERNAL_REPOSITORY_INTEGRATION.md`](EXTERNAL_REPOSITORY_INTEGRATION.md), [`composition.rs`](../mcp/contract/src/gateway/composition.rs) |
| F15 | Stored MIME validation is too shallow to serve as the sole trusted handler key. | [`service.rs`](../platform/artifacts/service/src/service.rs) |
| F16 | The embedded Rerun preview downloads the complete RRD before opening it, unlike recording-scoped lazy playback. | [`GovernedRerunArtifactViewer.tsx`](../apps/console/web/src/components/GovernedRerunArtifactViewer.tsx), [`GovernedRerunViewer.tsx`](../apps/console/web/src/components/GovernedRerunViewer.tsx) |
| F17 | MCP Apps `2026-01-26` supplies UI and tool linkage but no generic artifact-handler declaration. | [`MCP Apps design`](../mcp/apps-extension/DESIGN.md), [SEP-1865](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx) |
| F18 | Artifact editing must create a new occurrence or mutate a separate domain object because occurrences are immutable. | [`ARCHITECTURE_DECISIONS.md`](ARCHITECTURE_DECISIONS.md), [`storage.rs`](../mcp/contract/src/storage.rs) |

## Open Questions

- Should catalog membership mean “the caller may know it exists” or “the caller may
  read it now”? The two current catalogs answer differently.
- Which semantic identity should survive outside open producer JSON: artifact kind,
  schema URI, source server, source resource, task relation, or a typed set of links?
- Should a generic viewer match only syntax while a domain App matches a versioned
  semantic profile?
- Can a viewer declare several operations with different scopes and data paths?
- Does App eligibility require a dry authorization check, or should a failed launch
  surface ordinary policy denial?
- How should the host rank a domain App, a generic format App, a built-in renderer,
  and download without allowing an extension to seize every MIME type?
- Can a user preference select among installation-admitted handlers without becoming a
  new authorization source?
- What scoped capability can carry range-readable bytes into an opaque App without
  disclosing a Console cookie, gateway bearer, object-store address, or long-lived
  share link?
- How should an App return a derived artifact and retain `derived-from`, source domain,
  and editing provenance?
- Which thumbnail or lightweight preview representations are worth materializing, and
  who owns their retention and access coupling to the original?

## Evidence Trail

The shortest repository paths for continued investigation are:

- artifact types and service port: [`mcp/contract/src/storage.rs`](../mcp/contract/src/storage.rs),
  [`mcp/contract/src/artifact_service.rs`](../mcp/contract/src/artifact_service.rs);
- artifact enforcement and list behavior:
  [`platform/artifacts/service/src/service.rs`](../platform/artifacts/service/src/service.rs);
- Artifact MCP discovery: [`servers/artifact-mcp/DESIGN.md`](../servers/artifact-mcp/DESIGN.md),
  [`servers/artifact-mcp/src/bin/server/handler.rs`](../servers/artifact-mcp/src/bin/server/handler.rs);
- Console catalog and preview:
  [`platform/gateway/src/bin/gateway/admin/console/projection.rs`](../platform/gateway/src/bin/gateway/admin/console/projection.rs),
  [`apps/console/web/src/components/ArtifactPreview.tsx`](../apps/console/web/src/components/ArtifactPreview.tsx),
  [`apps/console/web/src/drawers/ArtifactDrawer.tsx`](../apps/console/web/src/drawers/ArtifactDrawer.tsx);
- App discovery, host, and navigation:
  [`mcp/apps-extension/DESIGN.md`](../mcp/apps-extension/DESIGN.md),
  [`apps/console/bff/src/apps.rs`](../apps/console/bff/src/apps.rs),
  [`apps/console/web/src/apps/bridge.ts`](../apps/console/web/src/apps/bridge.ts),
  [`apps/console/web/src/App.tsx`](../apps/console/web/src/App.tsx);
- external registration: [`EXTERNAL_EXTENSIONS.md`](EXTERNAL_EXTENSIONS.md) and
  [`EXTERNAL_REPOSITORY_INTEGRATION.md`](EXTERNAL_REPOSITORY_INTEGRATION.md).
