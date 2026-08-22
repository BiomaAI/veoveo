# Anonymous Simulation Installation

This fixture composes the platform source with an independently packaged simulation
MCP source. The extension owns its camera, shared encoded product, stream authorization,
and App contract. The platform supplies gateway, ingress, trust, and simulation runtime
support without installing a generic simulation renderer.

The installation owns `gateway-binding.json`, the composed gateway document and
provenance, OCI coordinates, trust material, public stream coordinate, and the
combined deployment lock. The extension owns its server fragment, chart, image, and
release manifest.

Both source declarations resolve the surrounding repository only because this is a
checked-in test fixture. `smoke external-simulation-fixture` copies the extension into
an isolated checkout and proves its authenticated private package lock, tests, package,
Bake graph, and Helm chart without a source-tree import.

Regenerate deterministic gateway outputs after changing a fragment, binding, or base:

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

Validate, publish, and install one committed revision:

```sh
PROFILE=testing/fixtures/external-simulation-installation/deployment.json
LOCK=testing/fixtures/external-simulation-installation/deployment.lock.json
REVISION=$(git rev-parse HEAD)

cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output "$LOCK"
cargo xtask smoke profile-up --profile "$PROFILE" --lock "$LOCK"
```

The locked deployment verifies every source revision, chart, values file, and image
digest before Helm. It never resolves a moving source expression during installation.
The fixture is intentionally contract-only: its declared synthetic product does not
qualify GPU rendering, NVENC, advancing H.264 media, or browser playback. Each real external
simulation implementation owns that hardware evidence; the first-party UAV showcase
provides the repository's NVIDIA reference.

Stop the local fixture with:

```sh
cargo xtask smoke profile-cluster-stop \
  --profile testing/fixtures/external-simulation-installation/deployment.json
```
