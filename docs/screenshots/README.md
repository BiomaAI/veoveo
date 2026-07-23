# Screenshot catalog

This directory records the current product gallery and the procedure used to
rebuild it. The catalog declares the publication state shared by the gallery.
Every replacement must have its composition, data, and identity surface
reviewed before it enters the published set.

The [visual gallery](GALLERY.md) presents the current set as a contact sheet.
The canonical inventory is [`catalog.json`](catalog.json). It names each
picture, states what it demonstrates, declares its live prerequisites, and
records whether the recipe invokes a tool or injects deterministic display
data. Generated PNG files live under [`gallery/`](gallery/).

## Console captures

Console recipes attach to an existing Chrome DevTools session. They reuse its
authenticated browser context; credentials and cookies are neither copied to
disk nor printed. A dedicated Chrome profile is the cleanest repeatable setup:

```bash
google-chrome \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9227 \
  --user-data-dir=/tmp/veoveo-screenshot-chrome \
  --window-size=1920,1080 \
  --force-dark-mode \
  --ozone-platform=x11 \
  --use-angle=vulkan \
  --enable-features=Vulkan \
  https://your-installation.example/console/
```

This command must open a visible X11 window. Do not add `--headless`,
SwiftShader, or software-WebGL flags. Complete the normal enterprise login in
that window. Every Console and Rerun recipe performs a fail-closed browser
preflight before navigation and again before capture. The preflight requires a
hardware high-performance WebGPU adapter and a working hardware WebGL context.
A missing or lost context aborts the run; API reachability does not substitute
for visual verification. Install the pinned capture dependency without
downloading another browser, then list or run the recipes:

```bash
cd tools/screenshots
PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci
npm run list
CHROME_CDP_URL=http://127.0.0.1:9227 \
CONSOLE_URL=https://your-installation.example/console/ \
npm run capture -- --ids console-overview,console-mcp-reason
```

For a local k3d installation reached through a public tunnel, a port-forward
keeps API and asset capture independent of tunnel latency while Chrome retains
the public origin and authenticated session:

```bash
kubectl --context <context> -n <namespace> \
  port-forward svc/console-bff 18786:8786

CONSOLE_DIRECT_URL=http://127.0.0.1:18786 \
npm run capture -- --ids console-overview,console-work,console-artifacts
```

The event stream stays on the public origin because Playwright response
fulfilment does not proxy an unbounded stream. Use one Console tab during a
capture run to avoid opening redundant stream connections.

Identity redaction is applied in the page before each Console screenshot.
Principal URIs, email addresses, and bearer-like query values are replaced in
the rendered DOM. The script never changes source records. Publication review
must still inspect every image because free-form artifact bodies can contain
domain data that an automatic rule cannot classify.

The chart recipe makes a real, stateless `create_chart_view` call through the
authenticated Console MCP App bridge. The forecast recipe uses a deterministic
structured-content fixture because a durable forecast run creates governed
work and an artifact. Its catalog entry makes that distinction explicit.

## Prepared MCP App captures

The 3D View images preserve a live state that an operator composes before the
capture. The screenshot tool still owns the browser viewport, identity
redaction, hardware-renderer check, and PNG output. It never fabricates a view
or substitutes fixture data for a scene.

For the Console image, open **Apps → 3D view preview** in the GPU-enabled Chrome
profile. Select the Google Photorealistic 3D Tiles layer, create a governed
view, position the camera, and wait for the visible tile counter to settle.
Capture a rendered frame, confirm that the app reports `frame captured`, then
run:

```bash
cd tools/screenshots
CHROME_CDP_URL=http://127.0.0.1:9227 \
npm run capture -- --ids console-app-view
```

The external-host image uses the same View MCP server and app resource. Connect
the host to `https://your-installation.example/mcp/operator` through its normal
OAuth flow. The resulting session must have `operator:use`, `view:read`,
`view:write`, and `view:capture`. Ask the host:

> Use Veoveo to create and display an interactive 3D view of the Golden Gate
> Bridge using the google-photorealistic scene layer.

Wait for `view__create_view` to complete and for the embedded scene to report
`scene loaded`. Arrange the conversation at a readable width in the same
CDP-enabled Chrome profile, then run:

```bash
cd tools/screenshots
CHROME_CDP_URL=http://127.0.0.1:9227 \
npm run capture -- --ids mcp-app-view-claude
```

Prepared captures find an open tab by the URL substring recorded in the
catalog. Override a match when a host changes its URL:

```bash
SCREENSHOT_CONSOLE_APP_VIEW_URL_PATTERN=/console/#/apps/view/preview.html \
npm run capture -- --ids console-app-view

SCREENSHOT_MCP_APP_VIEW_CLAUDE_URL_PATTERN=claude.ai/ \
npm run capture -- --ids mcp-app-view-claude
```

The tool aborts if the tab lacks hardware WebGPU or WebGL. Publication review
must also verify that the external conversation contains no account identity,
private prompt content, or unrelated history.

## Isaac Sim captures

The Isaac runtime can write a full-resolution showcase camera directly from its
headless RTX viewport. Enable `session.screenshot` in the UAV chart and deploy
the values through the installation owner. This gallery frame uses the
following overrides:

```yaml
session:
  origin:
    latitudeDegrees: 40.7580
    longitudeDegrees: -73.9855
    ellipsoidHeightM: -17.0
  screenshot:
    enabled: true
    outputPath: /tmp/isaac-uav.png
    width: 1920
    height: 1080
    minimumRelativeAltitudeM: 295.0
    settleRenderedFrames: 60
    focalLengthMm: 45.0
    eyeOffsetM:
      x: -2.2
      y: -2.2
      z: 1.2
    targetOffsetM:
      x: 0.0
      y: 0.0
      z: 0.2
```

Prove that the pod reports an active NVIDIA Vulkan device before flight. Call
`uav-sim__arm_vehicle` for `uav-1`, then call
`uav-sim__takeoff_vehicle` with `relative_altitude_m` set to `300`.
The capture waits for the altitude threshold, resident tiles, visible camera
content, and 60 consecutive rendered frames. Copy the completed PNG from the
pod:

```bash
POD=$(kubectl --context <context> -n <namespace> get pods \
  -l app.kubernetes.io/component=uav-sim \
  -o jsonpath='{.items[0].metadata.name}')

kubectl --context <context> -n <namespace> logs "$POD" -c isaac-sim |
  rg 'Graphics API: Vulkan|NVIDIA GeForce|screenshot written'

kubectl --context <context> -n <namespace> cp -c isaac-sim \
  "$POD:/tmp/isaac-uav.png" \
  docs/screenshots/gallery/isaac-uav-new-york.png
```

The runtime restores the canonical sensor camera after the one-shot capture.
Inspect the 1920×1080 output before accepting it into the gallery.

## Rerun captures

The UAV recipe opens a complete governed recording from the authenticated
Console, promotes its embedded Rerun viewer to browser fullscreen, pauses
playback, and seeks to the showcase camera interval. The recording must contain
camera, telemetry, pose, and world data. This path captures stable camera
content without exposing archive segments as manual playback partitions.

The SUMO recipe targets a browser-hosted viewer backed by the simulation pod's
loopback Rerun server. Project that server to the workstation, then open a
separate web viewer:

```bash
PROFILE=showcase/sumo/deploy/deployment.json
REVISION=$(git rev-parse HEAD)
just profile-cluster-up "$PROFILE"
just profile-publish "$PROFILE" "$REVISION"
just profile-up "$PROFILE" "$REVISION"
just showcase-sumo-verify

kubectl --context k3d-veoveo-sumo -n veoveo \
  port-forward deploy/sumo-mcp 9889:9876

set -a
source .env
set +a
RERUN_MAPBOX_ACCESS_TOKEN="$MAPBOX_ACCESS_TOKEN" \
rerun rerun+http://127.0.0.1:9889/proxy \
  --web-viewer --web-viewer-port 9092 --renderer webgpu
```

Open the generated SUMO viewer URL in the CDP-enabled Chrome profile. Arrange
the views, select Dark from Rerun's theme menu, choose the desired timeline
position, and leave the tab open. Rerun persists this explicit theme choice for
the viewer origin. Leave the authenticated Console open for the UAV capture,
then record both browser viewports:

```bash
cd tools/screenshots
CHROME_CDP_URL=http://127.0.0.1:9227 \
npm run capture -- --ids rerun-uav,rerun-sumo
```

The SUMO gallery expects the live showcase recording with `/world/sumo/**`
selected. Record the exact launch and port-forward commands in the catalog
whenever a profile changes; the image is not a substitute for a repeatable
runbook.

Every capture performs a hardware preflight against the browser's
high-performance WebGPU adapter and WebGL renderer. It rejects headless Chrome,
missing graphics contexts, SwiftShader, llvmpipe, software adapters, and any
viewer that exposes Rerun's software rasterizer warning. It also rejects the
standalone SUMO viewer unless Rerun's persisted theme is explicitly Dark.

## Capture policy

- Use 1920×1080 unless a catalog entry names another viewport.
- Keep the Console on Gray Dark and let sandboxed MCP Apps receive the matching
  host theme through the protocol.
- Capture live state only after health and readiness indicators have settled.
- Do not crop away state labels that qualify what the image demonstrates.
- Never publish a capture merely because it passed automatic redaction.
- Replace an image in place. The stable catalog ID is the documentation
  reference; capture dates belong in commit history.
