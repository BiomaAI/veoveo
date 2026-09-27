# Recording catalog installation fixture

This disposable profile exercises the native Rerun Catalog SDK with tenant
`enterprise` and origin `https://localhost:8785`. It deploys Recording, Artifact
storage, SurrealDB and the gateway. Its producer sends text records, so this profile
contains no rendering, perception or simulation workload.

The shared Linux host needs at least 512 inotify instances and 524,288 watches per
user while both clusters run. Check `fs.inotify.max_user_instances` and
`fs.inotify.max_user_watches` before creating the fixture. Raise lower limits on the
host with `sudo sysctl -w fs.inotify.max_user_instances=512
fs.inotify.max_user_watches=524288`. These runtime settings reset on reboot unless
the host administrator persists them. Kubernetes container runtimes share these
limits; exhausting them prevents the CRI service from starting. The
[kind troubleshooting guide](https://github.com/kubernetes-sigs/kind/blob/main/site/content/docs/user/known-issues.md)
documents the same host requirement for container-based clusters.

The public JWKS files identify this fixture's machine clients. Provision their matching
private keys outside the repository, or replace the public keys with a newly generated
fixture identity before publication. Never copy credentials from another installation.
The operator key ID is `catalog-operator`; producer, hub and properties-publisher
clients use `catalog-producer`. The authorization server uses `catalog-authorization`.
These OAuth keys use RSA. The separate internal gateway signing key uses Ed25519;
provision its PKCS#8 DER as standard base64 and its public JWKS as an `OKP` key with
curve `Ed25519`, algorithm `EdDSA` and key ID `catalog-internal`. The playback token
key is 32 random bytes encoded in padded standard base64. Internal signing and
playback keys belong only in the installation Secret.

The operations Work Context admits the hub and properties publisher's local service
principals as contributors, alongside their OAuth clients. The hub constructs catalog
authority directly from its local service identity during startup.

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
