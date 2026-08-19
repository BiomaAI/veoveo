# Bioma enterprise GitOps reference

Bioma is the executable reference for an enterprise-owned Veoveo installation.
The public endpoint at https://veoveo.bioma.ai reaches a GPU-enabled k3d cluster
through an installation-owned Cloudflare Tunnel. Flux reconciles the platform and an
independently packaged UAV MCP extension from Git and OCI artifacts.

| Property | Value |
|---|---|
| k3d cluster | veoveo-bioma |
| Kubernetes context | k3d-veoveo-bioma |
| Application namespace | veoveo |
| Flux namespace | flux-system |
| Loopback ingress | http://localhost:8781 |
| Public origin | https://veoveo.bioma.ai |
| Root Kustomization | bioma |

This example follows the neutral contract in
[Enterprise deployment](../../docs/ENTERPRISE_DEPLOYMENT.md). Bioma-specific
identity, origins, capacity, and provider selection live here. The build and
installation architecture does not contain Bioma-specific roles, scopes, or
release machinery.

## Ownership and layout

The repository separates the local platform fixture from application desired state:

~~~text
examples/bioma/
  platform/                     local cluster prerequisites
    flux/                       pinned Flux 2.9.4 installation
    registry/                   cluster-local loopback OCI registry address
  gitops/
    bootstrap.yaml              Git source and root Kustomization, applied once
    sources/                    exact platform and extension OCI charts
    releases/                   platform and extension Helm releases
    cloudflared.yaml            installation edge connector
  kustomization.yaml            root desired-state composition
  values.yaml                   public identity and platform values
  k3d-values.yaml               local capacity and storage values
  uav-sim-values.yaml           UAV extension values
  images.lock.yaml              production image digests
  gateway.json                  MCP catalog, OAuth, policy, and routes
  acceptance/                   owner-local compiled composition checks
  recording-producer-jwks.json  public producer key
  service-client-jwks.json      public machine-client key
~~~

The local platform fixture installs Flux and the registry address because this
cluster has no enterprise platform team. A fielded installation uses its existing
GitOps controller and secure OCI registry, then begins at the root Kustomization.
Veoveo application desired state never owns the controller that reconciles it.

The platform and UAV extension charts are separate OCI packages. Removing or
upgrading the UAV HelmRelease does not replace the core platform release. A customer
MCP server follows the same independent-release pattern after its image and chart are
published and its server contract is registered in the gateway control plane.

## Release publication

Production workloads use the repository and digest map in `images.lock.yaml`. The
platform and UAV OCI sources select immutable chart versions independently in
`gitops/sources/`. Those files and the lock, rather than a copied revision in this
manual, define the current deployment closure. Each selected image digest is the
workload's immutable identity. A release-input commit advances the coordinated runtime
closure before a qualified publication promotes those exact inputs.

Publish a new local release directly to the shared registry:

~~~bash
REVISION=$(git rev-parse HEAD)
CHART_VERSION=0.1.0-$(git rev-parse --short=12 HEAD)

cargo xtask image builder ensure
cargo xtask release images --group platform-full \
  --push-registry 127.0.0.1:5001 \
  --pull-registry k3d-veoveo-registry.localhost:5001 \
  --registry-transport insecure-http --revision "$REVISION"
cargo xtask release images --group showcase-uav-sim \
  --push-registry 127.0.0.1:5001 \
  --pull-registry k3d-veoveo-registry.localhost:5001 \
  --registry-transport insecure-http --revision "$REVISION"

cargo xtask release helm-charts \
  --revision "$REVISION" --version "$CHART_VERSION" \
  --registry localhost:5001/charts --plain-http
~~~

BuildKit pushes only missing layers and does not load release images into the host
Docker store. Record the manifest digest for every published image in
images.lock.yaml, then update the selected chart tags in `gitops/sources/` in one
release-input commit. The root Flux artifact carries those chart selections and all
generated values from one Git revision.

After pushing that parent commit, observe the exact rollout through the focused typed
harness. Pass every Deployment whose digest changed; do not list an unchanged simulator
for a control-plane-only update.

~~~bash
REVISION="$(git rev-parse HEAD)"

cargo xtask smoke gitops-converge \
  --context k3d-veoveo-bioma \
  --source flux-system/bioma \
  --root flux-system/bioma \
  --release flux-system/veoveo \
  --release flux-system/uav-sim \
  --revision "$REVISION" \
  --deployment veoveo/<changed-deployment> \
  --evidence-output output/development/gitops-convergence.json
~~~

The command requests reconciliation and watches the exact Git artifact, root apply,
Helm release inventories, rollout, and readiness. Re-running with the same output path
is rejected because convergence evidence is create-only. The full procedure and
production registry requirements are in the enterprise deployment guide.

## Create the local platform

Hardware GPU access is mandatory. Build the pinned K3s node image, create the shared
registry when it is absent, and create the Bioma cluster:

~~~bash
nvidia-smi
source deploy/local/k3d/versions.env
docker build \
  --build-arg K3S_VERSION="$K3S_VERSION" \
  --build-arg CUDA_VERSION="$CUDA_VERSION" \
  --build-arg NVIDIA_CONTAINER_TOOLKIT_VERSION="$NVIDIA_CONTAINER_TOOLKIT_VERSION" \
  --tag "$VEOVEO_K3D_NODE_IMAGE" \
  deploy/local/k3d/node

k3d registry create veoveo-registry.localhost   --port 127.0.0.1:5001   --image "$OCI_DISTRIBUTION_IMAGE"   --volume veoveo-registry:/var/lib/registry   --delete-enabled

k3d cluster create --config examples/bioma/k3d.yaml
kubectl --context k3d-veoveo-bioma apply   -f deploy/local/k3d/node/nvidia-device-plugin.yaml
kubectl --context k3d-veoveo-bioma -n kube-system rollout status   daemonset/nvidia-device-plugin --timeout=2m
kubectl --context k3d-veoveo-bioma get nodes   -o 'custom-columns=NAME:.metadata.name,GPU:.status.allocatable.nvidia\.com/gpu'
~~~

The node must report six allocatable GPU shares before application bootstrap.
The local time-slicing profile keeps the authoritative UAV simulator, View, Stream,
Reason, the cuOpt executor, and the Rerun viewer MCP in separate GPU-requesting
workloads. Fielded installations use their measured exclusive,
MIG, or time-slicing placement instead of inheriting this development profile.
Each required workload still requests nvidia.com/gpu: 1 and the nvidia runtime
class. The shares make all six render and GPU-compute workloads schedulable
together; they are not a CPU fallback.

The local Reason profile reserves 35% of the 24 GiB NVIDIA device for vLLM. This
bound preserves device-memory headroom for the six-frame multimodal pass while
the authoritative Isaac simulator, cuOpt, Rerun, and the other GPU services remain
resident. Installations with different checkpoints, solver pools, or GPU capacity
size `reason.engine.gpuMemoryUtilization` and
`VEOVEO_CUOPT_POOL_GIB` against all six concurrently resident workloads.
The development chart requests 4 GiB of host memory for the cuOpt executor. The
simulator's operator-camera products remain inside the simulator allocation. Higher
memory limits remain available
for bursts without making the six-workload placement unschedulable on the reference
64 GiB node.

The local fixture advertises simulator-owned live-view signaling through
`wss://veoveo.bioma.ai/uav-sim/signaling`. Its two preallocated viewer slots use
the bounded UDP media range `192.168.68.69:47998-47999`. The chart assigns those
slots to NodePorts `30998-30999`, and the k3d bindings admit each UDP port on the
host's network interfaces before forwarding it to the matching NodePort. The fixed
mapping is part of the
acceptance contract because a Kubernetes-assigned NodePort cannot satisfy a
predeclared browser media endpoint. An installation on a different network
must advertise its routable signaling origin and UDP media address instead. A
loopback media address is valid only when every browser runs on the cluster host.

Install the local platform fixture separately:

~~~bash
kubectl --context k3d-veoveo-bioma apply \
  --server-side --force-conflicts \
  -k examples/bioma/platform
kubectl --context k3d-veoveo-bioma -n flux-system wait \
  --for=condition=Available deployment --all --timeout=5m
~~~

The local OCI source explicitly admits the cluster-local HTTP registry. A fielded
installation uses its authenticated TLS registry and removes that local exception.

## Provision Secrets

The enterprise owns Secret creation. For this local reference, load the main
worktree .env and create the required Secret objects before the root Kustomization.
The following command reads values through the environment and sends the Secret
documents directly to Kubernetes over stdin:

~~~bash
set -a
source .env
set +a

kubectl --context k3d-veoveo-bioma apply   -f examples/bioma/gitops/namespace.yaml

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-surreal-admin", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    username: env.VEOVEO_SURREAL_ADMIN_USERNAME,
    password: env.VEOVEO_SURREAL_ADMIN_PASSWORD
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-surreal-runtime", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    username: env.VEOVEO_SURREAL_RUNTIME_USERNAME,
    password: env.VEOVEO_SURREAL_RUNTIME_PASSWORD
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-installation-secrets", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "internal-signing-key-der-b64": env.VEOVEO_INTERNAL_SIGNING_KEY_DER_B64,
    "internal-signing-key-id": env.VEOVEO_INTERNAL_SIGNING_KEY_ID,
    "internal-trust-jwks": env.VEOVEO_INTERNAL_TRUST_JWKS,
    "oidc-client-secret": env.VEOVEO_IDP_OIDC_CLIENT_SECRET,
    "authorization-server-private-key-der-b64": env.VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64,
    "refresh-delivery-key-b64": env.VEOVEO_REFRESH_DELIVERY_KEY_B64,
    "console-session-key": env.VEOVEO_CONSOLE_SESSION_KEY,
    "recording-playback-token-key": env.VEOVEO_RECORDING_PLAYBACK_TOKEN_KEY,
    "object-store-access-key": env.VEOVEO_OBJECT_STORE_ACCESS_KEY,
    "object-store-secret-key": env.VEOVEO_OBJECT_STORE_SECRET_KEY,
    "media-provider-api-key": env.MEDIA_PROVIDER_API_KEY,
    "google-maps-api-key": env.GOOGLE_MAPS_API_KEY,
    "media-provider-webhook-secret": env.MEDIA_PROVIDER_WEBHOOK_SECRET
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-uav-sim-secrets", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "cesium-ion-access-token": env.CESIUM_ION_ACCESS_TOKEN
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-uav-sim-adapter", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "bearer-token": env.VEOVEO_UAV_SIM_ADAPTER_TOKEN
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-recording-producer", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "private-key.pem": env.VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "bioma-cloudflared", namespace: "veoveo"},
  type: "Opaque",
  stringData: {token: env.CLOUDFLARED_TUNNEL_TOKEN}
}' | kubectl --context k3d-veoveo-bioma apply -f -

~~~

A production installation projects the same keys from its secret manager. The UAV,
Cloudflare, and recording-producer credentials remain separate least-privilege
Secrets. The committed JWKS files contain public keys only. The reference installation mounts the
installation-owned machine-client JWKS with the gateway control plane, which keeps
local client assertions independent of an external JWKS endpoint.

## Connect Flux to Git

The Bioma repository is private. Give this installation one read-only GitHub deploy
key; Flux never needs permission to write the repository.

~~~bash
ssh-keygen -t ed25519 -N '' -C bioma-flux \
  -f /secure/path/bioma-flux
gh repo deploy-key add /secure/path/bioma-flux.pub \
  --repo BiomaAI/veoveo --title bioma-flux
flux --context k3d-veoveo-bioma --namespace flux-system \
  create secret git bioma-git-auth \
  --url ssh://git@github.com/BiomaAI/veoveo.git \
  --private-key-file /secure/path/bioma-flux
~~~

The private key remains in the cluster Secret and the installation's secret store. Do
not commit it. Production OCI credentials use a separate registry Secret when required.

## Bootstrap desired state

Apply only the Git source and root Kustomization:

~~~bash
kubectl --context k3d-veoveo-bioma apply   -f examples/bioma/gitops/bootstrap.yaml
~~~

Flux creates the namespace configuration, gateway and immutable UAV-world ConfigMaps,
Cloudflare connector, OCI sources, and the two Helm releases. Inspect reconciliation
through the standard Flux resources:

~~~bash
flux --context k3d-veoveo-bioma get sources git
flux --context k3d-veoveo-bioma get sources oci
flux --context k3d-veoveo-bioma get kustomizations
flux --context k3d-veoveo-bioma get helmreleases
kubectl --context k3d-veoveo-bioma -n veoveo get deployments,statefulsets,pods
~~~

The Git source and root Kustomization must be Ready at the same revision. Both
HelmReleases must be Ready with non-empty inventories. Do not operate concurrent Helm
releases for the same resources.

## Public edge

The remote-managed tunnel is named veoveo-bioma-ai. Its desired ingress sends the
installation's only public hostname to Traefik in the cluster:

~~~text
veoveo.bioma.ai -> http://traefik.kube-system.svc.cluster.local:80
~~~

The DNS record targets the tunnel hostname and Cloudflare terminates public TLS.
RustFS remains cluster-private. Artifact bytes reach clients only through governed
Gateway, Console BFF, or public-share paths on `veoveo.bioma.ai`.

The operations console is available at:

~~~text
https://veoveo.bioma.ai/console/
~~~

The complete Veoveo server catalog comes from gateway.json. The Map page begins with
the installation-owned OpenStreetMap El Salvador source in k3d-values.yaml. The
Cluster page uses a dedicated read-only Kubernetes Role and cannot read Secrets.
Audit uses bounded pages.

## Identity

gateway.json uses one single-tenant Microsoft Entra application as the external OIDC
provider:

- register https://veoveo.bioma.ai/oauth/callback as a Web redirect URI;
- create and assign the operator and administrator app roles;
- keep the tenant-specific v2 issuer, endpoints, and JWKS on one directory tenant;
- grant openid, profile, and email;
- store the client secret only in veoveo-installation-secrets.

Validate control-plane edits before committing:

~~~bash
cargo run -p veoveo-mcp-gateway --bin gateway --   validate --control-plane examples/bioma/gateway.json
~~~

Sign out and authenticate again after an app-role or requested-scope change because an
existing browser session retains the claims issued at login.

## LAN producers

A LAN recording producer still uses the canonical public resource identity
https://veoveo.bioma.ai. Configure internal DNS for the Traefik address and create the
TLS Secret referenced by lan-values.yaml:

~~~bash
kubectl --context k3d-veoveo-bioma -n veoveo create secret tls   bioma-lan-ingress-tls   --cert=/secure/path/veoveo.bioma.ai.crt   --key=/secure/path/veoveo.bioma.ai.key   --dry-run=client -o yaml | kubectl --context k3d-veoveo-bioma apply -f -
~~~

Add lan-values.yaml to the platform HelmRelease values ConfigMap. The public issuer,
protected-resource identifier, certificate hostname, and ingest URL remain unchanged.
Only the route differs.

## Acceptance

Verify the reconciled installation and public edge:

~~~bash
cargo xtask smoke bioma-verify
~~~

This gate uses the public machine-client contract to export a deterministic artifact
larger than 8 MiB through DuckDB. It then verifies full, HEAD, and ranged delivery at
the installation origin with redirect following disabled, exact content and SHA-256
checks, and no object-storage address in metadata or response headers.

Then run the full GPU delivery proof:

~~~bash
VEOVEO_CUOPT_EXECUTOR_IMAGE=veoveo/cuopt-executor:0.1.0 \
  cargo xtask smoke agent-pilot
cargo xtask smoke uav-showcase-up \
  --context k3d-veoveo-bioma \
  --public-base-url https://veoveo.bioma.ai
cargo xtask smoke uav-domain-verify \
  --context k3d-veoveo-bioma \
  --public-base-url https://veoveo.bioma.ai
cargo xtask smoke uav-showcase-verify \
  --context k3d-veoveo-bioma \
  --public-base-url https://veoveo.bioma.ai \
  --chrome-cdp-url http://127.0.0.1:9222
~~~

`uav-showcase-up` converges the immutable Frames world, starts the perpetual fleet
loop, and leaves its authoritative simulator-hosted camera live. The verification commands
exercise bounded missions and may take temporary ownership of individual vehicles.

The Pilot acceptance starts the real cuOpt executor on the host GPU, sends a typed
MILP through the gateway as a durable task, verifies the solution independently,
wakes the sleeping agent from task completion, and checks its durable decision
record.

The live reference keeps four PX4 vehicles on nested loops over Manhattan until an
explicit mission or direct flight command takes control of an individual vehicle. The
UAV acceptance requires Google Photorealistic 3D Tiles resident in Isaac, claims one
vehicle for a governed PX4 mission, verifies direct Stream results from newly arrived
camera frames, then runs reproducible Stream replay and Reason over acknowledged
recording parts before archive rollover. The other vehicles continue their loops while
the acceptance confirms that concurrent GPU deployments remain available. Its runtime inputs come from
showcase/uav-sim/scenarios/new-york-aerial.json. The acceptance client creates
the complete world through Frames MCP and binds the returned immutable revision
to the simulator before Isaac constructs its stage.

## Cleanup

Delete the disposable cluster when the reference installation is no longer needed:

~~~bash
k3d cluster delete veoveo-bioma
~~~

Deleting the cluster disconnects the tunnel. It does not delete the remote Cloudflare
Tunnel, DNS records, or the shared registry volume.
