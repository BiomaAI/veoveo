# Offline installation bundle

`images.lock.json` is the canonical runtime image set. External images are
pinned by registry digest. Veoveo images use exact release tags and are recorded
by image id when a bundle is created.

Create a release bundle on a connected build host:

```sh
deploy/offline/create-bundle.sh \
  --platform linux/amd64 \
  --output output/veoveo-offline-0.1.0.tar.gz
```

The default release path requires `syft` and emits one SPDX JSON SBOM per image.
`--skip-sbom` is an explicit non-release escape hatch. The archive includes the
image tar, checksums, resolved image identities, Compose configuration, Helm
chart, deployment contract, gateway configuration, and telemetry configuration.

On the offline host, verify and import into Docker, retaining the installation
payload and checksum/SBOM evidence under an empty destination directory:

```sh
deploy/offline/load-bundle.sh \
  --bundle output/veoveo-offline-0.1.0.tar.gz \
  --runtime docker \
  --install-dir /opt/veoveo
```

For Kubernetes nodes using containerd, use `--runtime containerd`. The loader
checks every file before import, verifies every image reference afterward, and
installs the payload at `/opt/veoveo`. Use `/opt/veoveo/compose.yaml` for a
single-host installation or install `/opt/veoveo/deploy/helm/veoveo` with
`/opt/veoveo/deploy/values.offline.yaml` so the kubelet uses
`imagePullPolicy: Never`. Bundle evidence remains in
`/opt/veoveo/bundle-evidence`. Secrets, TLS material, the internal OIDC
configuration, and site-specific gateway/telemetry configuration are supplied
inside the offline boundary and are never embedded in the bundle.

Create the gateway refresh-delivery key inside that boundary with
`openssl rand -base64 32`. For Compose, set the resulting base64 text as
`VEOVEO_REFRESH_DELIVERY_KEY_B64`; for Helm, store it under
`refresh-delivery-key-b64` in `global.existingSecret`. It must decode to exactly
32 bytes and must not reuse any signing or console session key.

The offline deployment keeps the same default five-second
`VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS` /
`gateway.refreshDeliveryWindowSeconds` behavior as a connected installation.
It lets concurrent stateless BFF requests receive the identical rotated
successor recovered from an authenticated encrypted envelope. The plaintext
successor is never persisted or emitted to logs, audit, outbox, or snapshots;
use after the window is delayed replay and revokes the refresh-token family. A
successor consumption clears its envelope atomically; otherwise a dedicated
one-minute GC pass physically removes expired ciphertext.

Offline Kubernetes installations must also preserve the chart's dedicated `/s`
Ingress log suppression. The default annotation targets ingress-nginx; replace
`ingress.publicShareAnnotations` with the installed controller's access-log
disable or redaction policy before exposing anyone-with-link artifact URLs.
