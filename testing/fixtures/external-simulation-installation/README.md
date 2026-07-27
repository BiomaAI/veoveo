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
