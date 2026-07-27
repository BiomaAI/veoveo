# Anonymous Simulation Installation

This installation fixture composes the Veoveo platform source with an
independently built external simulation source. The platform selection
contains Artifact, Frames, the canonical simulation runtime, and Simulation
View. Only `simulation-view-renderer` receives an NVIDIA GPU.

The installation owns `gateway-binding.json`, package and OCI credentials,
trust roots, the pose-producer certificate, public media coordinates, and the
combined deployment lock. The extension source owns its server fragment,
application chart, image, and release manifest.

Because this acceptance fixture is checked into the Veoveo test tree, both
local source declarations resolve the surrounding Git repository. The
`external-simulation-extension-fixture` Bake group is only a repository-local
adapter to that subtree. `smoke external-simulation-fixture` separately copies
the subtree into an isolated checkout and exercises its native Bake graph,
private package lock, tests, and package build.

After the installation has issued its pose-producer TLS identity, locked the
exact platform and extension images, and exposed the declared WebRTC
signaling and media ports, run:

```sh
just simulation-view-verify \
  context=<installation-context> \
  public_base_url=https://<installation-host> \
  chrome_cdp_url=http://127.0.0.1:9227
```

The command drives the anonymous producer through the core Simulation View
contract. It exercises several cameras, capacity admission, live leases, and
the real MCP App in headed hardware-backed Chrome. The fixture has no UAV
runtime or UAV-owned rendering behavior.
