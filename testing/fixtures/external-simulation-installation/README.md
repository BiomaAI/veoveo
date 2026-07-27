# Anonymous Simulation Installation

This installation fixture composes the Veoveo platform source with an
independently built external simulation source. The platform selection
contains Artifact, Frames, the canonical simulation runtime, and Simulation
View. Only `simulation-view-renderer` receives an NVIDIA GPU.

The installation owns `gateway-binding.json`, package and OCI credentials,
trust roots, the pose-producer certificate, public media coordinates, and the
combined deployment lock. The extension source owns its server fragment,
application chart, image, and release manifest.
