# Fork Installation Fixture

This installation selects a platform and a domain workload from the fork checkout.
It owns the complete `gateway.json`, gateway requirements, trust references and public
endpoints. The workload chart and image remain independently deployable components.

Validate, publish, and install one committed revision:

```sh
PROFILE=testing/fixtures/fork-installation/deployment.json
LOCK=output/deployments/fork-workload-fixture/deployment.lock.json
REVISION=$(git rev-parse HEAD)

cargo xtask smoke profile-validate --profile "$PROFILE"
cargo xtask smoke profile-cluster-up --profile "$PROFILE"
cargo xtask release images \
  --profile "$PROFILE" \
  --profile-revision "$REVISION" \
  --lock-output "$LOCK"
cargo xtask smoke profile-up --profile "$PROFILE" --lock "$LOCK" \
  --all-components --receipt-output output/development/installation.receipt.json
```

The locked deployment verifies every source revision, chart, values file, and image
digest before Helm. It never resolves a moving source expression during installation.
Source-chart lock digests use Veoveo's file-content encoding. Installation
locks are generated outputs. Each image keeps the revision and attested publication
digest from its build, independent of the current chart revision.
The fixture is intentionally contract-only: its declared synthetic product does not
qualify GPU rendering, NVENC, advancing H.264 media, or browser playback. Each real simulation implementation owns that hardware evidence; the first-party UAV showcase
provides the repository's NVIDIA reference.

Stop the local fixture with:

```sh
cargo xtask smoke profile-cluster-stop \
  --profile testing/fixtures/fork-installation/deployment.json
```
