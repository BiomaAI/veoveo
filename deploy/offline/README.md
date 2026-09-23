# Offline installation bundle

`images.lock.json` lists every runtime image in the bundle. External images are
pinned by registry digest. Veoveo images use exact release tags and are recorded
by image id when a bundle is created.

Create a release bundle on a connected build host:

```sh
deploy/offline/create-bundle.sh \
  --platform linux/amd64 \
  --output output/veoveo-offline-0.1.0.tar.gz
```

The default release path requires `syft` and emits one SPDX JSON SBOM per image.
`--skip-sbom` skips SBOMs for non-release bundles only. The archive includes the
image tar, checksums, resolved image identities, Helm
chart, deployment contract, gateway configuration, and telemetry configuration.

The Stream image contains the DeepStream runtime. TensorRT engines are approved per
site and supplied at deployment, so the generic bundle leaves them out. Place
the engine/model files and the completed `catalog.json`/nvinfer configuration
under the site-approved Kubernetes volumes before starting the offline
installation.
The connected bundle builder must authenticate to `nvcr.io` before building the
Stream image.

The bundle includes the derived `veoveo/cuopt-executor` image based on the
digest-pinned NVIDIA cuOpt 26.08 CUDA 13.3 runtime. Offline GPU nodes still need
a compatible NVIDIA driver, device plugin, and `nvidia` RuntimeClass.

On the offline host, verify the bundle and import it into Docker. The loader writes
the installation payload and the checksum and SBOM evidence into an empty destination
directory:

```sh
deploy/offline/load-bundle.sh \
  --bundle output/veoveo-offline-0.1.0.tar.gz \
  --runtime docker \
  --install-dir /opt/veoveo
```

For Kubernetes nodes using containerd, use `--runtime containerd`. The loader
checks every file before import, verifies every image reference afterward, and
installs the payload at `/opt/veoveo`. Install
`/opt/veoveo/deploy/helm/veoveo` with
`/opt/veoveo/deploy/values.offline.yaml` so the kubelet uses
`imagePullPolicy: Never`. The loader keeps bundle evidence in
`/opt/veoveo/bundle-evidence`. The site supplies secrets, TLS material, the internal
OIDC configuration, and its gateway and telemetry configuration on the offline side;
the bundle carries none of them.

Create the gateway refresh-delivery key on the offline side with
`openssl rand -base64 32`. Store it under `refresh-delivery-key-b64` in
`global.existingSecret`. It must decode to exactly
32 bytes and must not reuse any signing or console session key.

Create the recording-scoped Redap key independently with
`openssl rand -base64 32`. Store it under `recording-playback-token-key` in
`global.existingSecret`. It must decode to exactly 32 bytes and must not reuse
the refresh-delivery, gateway signing, object-store, or Console session keys.

Offline installations use the same five-second default for
`VEOVEO_REFRESH_DELIVERY_WINDOW_SECONDS` / `gateway.refreshDeliveryWindowSeconds` as
connected ones. Within that window, concurrent BFF requests that present the same
just-rotated refresh token all receive the same new token, decrypted from an encrypted
envelope. Reuse after the window counts as replay and revokes the refresh-token family. The
plaintext token never reaches storage, logs, audit, the outbox, or snapshots. Consuming
the new token deletes its envelope in the same transaction; a one-minute GC pass deletes
envelopes that expire unused.

Offline Kubernetes installations must also preserve the chart's dedicated `/s`
Ingress log suppression. The default annotation targets ingress-nginx; replace
`ingress.publicShareAnnotations` with the installed controller's access-log
disable or redaction policy before exposing anyone-with-link artifact URLs.
