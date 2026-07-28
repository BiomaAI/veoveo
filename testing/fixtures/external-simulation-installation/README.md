# Anonymous Simulation Installation

This installation fixture composes the Veoveo platform source with an
independently built external simulation source. The platform selection
contains Artifact, Frames, the canonical simulation runtime, and Simulation
View. Only `simulation-view-renderer` receives an NVIDIA GPU.

The installation owns `gateway-binding.json`, the composed `gateway.json`,
composition provenance, OCI credentials, trust roots, the pose-producer
certificate, public media coordinates, GPU placement, and the combined
deployment lock. The extension source owns its server fragment, application
chart, image, and release manifest.

Because this acceptance fixture is checked into the Veoveo test tree, both
local source declarations resolve the surrounding Git repository. The
`external-simulation-extension-fixture` Bake group uses the adjacent repository
adapter Dockerfile. That adapter installs the dependency closure from the
extension's exact lock, then copies the selected SDK and fixture modules from
the same committed source. It neither needs private package credentials nor
changes the external repository's native release path. `smoke
external-simulation-fixture` separately copies the subtree into an isolated
checkout and exercises its native Bake graph, authenticated private package
lock, tests, and package build.

The checked-in cluster profile is loopback-only. Its control tokens and mTLS
private keys are public development credentials, and its NVIDIA device plugin
advertises one exclusive GPU allocation. Four host UDP ports map exactly to
the four declared media slots. Fielded installations replace the PKI, tokens,
origins, media addresses, and registry.

Regenerate the deterministic gateway outputs after changing a fragment,
binding, or base selection:

```sh
jq -f testing/fixtures/external-simulation-installation/gateway-base.jq \
  configs/gateway.local.json \
  > testing/fixtures/external-simulation-installation/gateway-base.json

cargo build -p veoveo-gateway-composer --bin gateway-compose
target/debug/gateway-compose \
  --base testing/fixtures/external-simulation-installation/gateway-base.json \
  --fragment testing/fixtures/external-simulation-extension/gateway-fragment.json \
  --binding testing/fixtures/external-simulation-installation/gateway-binding.json \
  --output testing/fixtures/external-simulation-installation/gateway.json \
  --requirements testing/fixtures/external-simulation-installation/gateway-requirements.json \
  --provenance testing/fixtures/external-simulation-installation/gateway-provenance.json
```

Publish and install one exact committed revision:

```sh
PROFILE=testing/fixtures/external-simulation-installation/deployment.json
REVISION=$(git rev-parse HEAD)

cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
docker exec k3d-anonymous-simulation-server-0 \
  sysctl -w fs.inotify.max_user_instances=1024
cargo xtask image builder ensure
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output testing/fixtures/external-simulation-installation/deployment.lock.json
cargo xtask smoke profile-up --profile "$PROFILE"
```

The fixture raises the running node's nonpersistent inotify instance ceiling because
one workstation may host several independent k3d clusters. Fielded hosts set an
equivalent persistent ceiling through their normal node configuration.

Use the operator's persistent, authenticated headed Chrome profile on the
active X11 display. The acceptance command rejects HeadlessChrome and requires
at least one NVIDIA-backed WebGPU or WebGL path before it loads the App.

```sh
google-chrome-stable \
  --user-data-dir="$HOME/.config/veoveo-acceptance-chrome" \
  --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port=9222 \
  --ozone-platform=x11 \
  http://localhost:8782/console/
```

Then run:

```sh
cargo xtask smoke simulation-view-verify \
  --context k3d-anonymous-simulation \
  --public-base-url http://localhost:8782 \
  --chrome-cdp-url http://127.0.0.1:9222
```

The command drives the anonymous producer through the core Simulation View
contract. It exercises several cameras, capacity admission, live leases, and
the real MCP App in headed hardware-backed Chrome. The fixture has no UAV
runtime or UAV-owned rendering behavior.

Stop the fixture cluster after acceptance:

```sh
cargo xtask smoke profile-cluster-stop \
  --profile testing/fixtures/external-simulation-installation/deployment.json
```

The stopped cluster retains its deployment state for the next independent
run. Its renderer must not remain active while another local profile performs
GPU acceptance because separate k3d schedulers cannot coordinate allocation
of the same host GPU.
