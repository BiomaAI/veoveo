# Recording catalog installation fixture

This disposable profile exercises the native Rerun Catalog SDK with tenant
`enterprise` and origin `https://localhost:8785`. It deploys Recording, Artifact
storage, SurrealDB and the gateway. Its producer sends text records, so this profile
contains no rendering, perception or simulation workload.

The public JWKS files identify this fixture's machine clients. Provision their matching
private keys outside the repository, or replace the public keys with a newly generated
fixture identity before publication. Never copy credentials from another installation.
The operator key ID is `catalog-operator`; producer, hub and properties-publisher
clients use `catalog-producer`. The authorization server uses `catalog-authorization`.

Create the installation Secrets required by the rendered profile. Provision
`catalog-ingress-tls` with a certificate whose SAN includes `localhost`, and set
`SSL_CERT_FILE` to its private CA bundle in the smoke process and OAuth child.
Keep certificate and hostname verification enabled. The native SDK uses loopback HTTP
at port 8784; OAuth and grant issuance use HTTPS at port 8785.

Follow the [local deployment profile workflow](../../../docs/LOCAL_DEPLOYMENT_PROFILES.md)
with `deployment.json`, then send a `veoveo-catalog-fixture` recording through the
production Recording Forwarder. Read its dataset and recording IDs through Recording
MCP and run:

```sh
cargo xtask smoke recording-catalog-sdk \
  --installation testing/fixtures/catalog-installation/installation-target.json \
  --dataset-id <dataset-uuid> --recording-id <recording-uuid>
```

The scenario verifies rows and renews the catalog grant. Delete `veoveo-catalog` and
its node-owned volumes after acceptance. The registry is shared and keeps its data.
