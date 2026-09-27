# Fork Installation Fixture

This installation selects a platform and a domain workload from the fork checkout.
It owns the complete `gateway.json`, gateway requirements, trust references and public
endpoints. The workload chart and image remain independently deployable components.

The public origin is `https://localhost:8783`; loopback HTTP ingress uses port 8782.
`installation-target.json` selects this control plane and the anonymous tenant. The
profile has no GPU workload or recording catalog, and its target does not claim either.
Before installation, provision `fork-ingress-tls` in namespace `veoveo` with a certificate
whose SAN includes `localhost`, and trust its issuer in the client environment. On Linux,
`SSL_CERT_FILE` can select a private CA bundle for the smoke process and its OAuth child.
Certificate and hostname verification remain enabled. The fixture needs its installation
Secrets from the profile; it never borrows the reference installation's credentials.

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
