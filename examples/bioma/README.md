# Bioma enterprise GitOps reference

Bioma is the executable reference for an enterprise-owned Veoveo installation.
The public endpoint at https://veoveo.bioma.ai reaches a GPU-enabled k3d cluster
through an installation-owned Cloudflare Tunnel. Argo CD reconciles the platform
and an independently packaged UAV MCP extension from Git and OCI artifacts.

| Property | Value |
|---|---|
| k3d cluster | veoveo-bioma |
| Kubernetes context | k3d-veoveo-bioma |
| Application namespace | veoveo |
| Argo namespace | argocd |
| Loopback ingress | http://localhost:8781 |
| Public origin | https://veoveo.bioma.ai |
| Root Application | bioma |

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
    argocd/                     pinned Argo CD 3.4.5 installation
    registry/                   TLS adapter for the loopback OCI registry
  gitops/
    bootstrap.yaml              root Application, applied once
    project.yaml                installation reconciliation boundary
    applications/
      veoveo.yaml               platform OCI chart
      uav-sim.yaml              independent extension OCI chart
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

The local platform fixture installs Argo CD and the registry adapter because this
cluster has no enterprise platform team. A fielded installation uses its existing
GitOps controller and secure OCI registry, then begins at the root Application.
Veoveo application desired state never owns the controller that reconciles it.

The platform and UAV extension charts are separate OCI packages. Removing or
upgrading the UAV Application does not replace the core platform release. A customer
MCP server follows the same child-Application pattern after its image and chart are
published and its server contract is registered in the gateway control plane.

## Release publication

Production workloads use the repository and digest map in images.lock.yaml. The
platform Application selects chart 0.1.0-628ee1df1027, published from commit
628ee1df1027ae653adc5dc8c00c2d8600c00fbe. The Simulation View renderer
was published from commit 4a632d3dade497191bbb033acdddb36f1b314925. The UAV Application
selects chart 0.1.0-628ee1df1027. Its MCP and runtime images were published from
commit 628ee1df1027ae653adc5dc8c00c2d8600c00fbe. Each selected digest is the
workload's release identity. A release-input commit advances the coordinated runtime
closure.

Publish a new local release directly to the shared registry:

~~~bash
REVISION=$(git rev-parse HEAD)
CHART_VERSION=0.1.0-$(git rev-parse --short=12 HEAD)

cargo xtask image builder ensure
cargo xtask release images --group platform-full \
  --registry localhost:5001 --revision "$REVISION"
cargo xtask release images --group showcase-uav-sim \
  --registry localhost:5001 --revision "$REVISION"

cargo xtask release helm-charts \
  --revision "$REVISION" --version "$CHART_VERSION" \
  --registry localhost:5001/charts --plain-http
~~~

BuildKit pushes only missing layers and does not load release images into the host
Docker store. Record the manifest digest for every published image in
images.lock.yaml, then update both chart targetRevision fields in one release-input
commit. Record that commit's full SHA. A follow-up rollout commit must set the
`configuration` source targetRevision in both child Applications to the recorded SHA.
The parent Application then changes the chart and its values source in one child
Application update.

Never point a child Application's `configuration` source at a mutable branch. A mutable
values source can expose new image digests to the old chart before the parent updates
the chart revision. That ordering breaks the release boundary and can revive an old
replica policy during the transition. The full procedure and production registry
requirements are in the enterprise deployment guide.

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

The node must report seven allocatable GPU shares before application bootstrap.
The local time-slicing profile keeps the UAV simulator, Simulation View
renderer, View, Stream, Reason, the cuOpt executor, and the Rerun viewer MCP in separate
GPU-requesting workloads. Fielded installations use their measured exclusive,
MIG, or time-slicing placement instead of inheriting this development profile.
Each required workload still requests nvidia.com/gpu: 1 and the nvidia runtime
class. The shares make all seven render and GPU-compute workloads schedulable
together; they are not a CPU fallback.

The local Reason profile reserves 35% of the 24 GiB NVIDIA device for vLLM. This
bound preserves device-memory headroom for the six-frame multimodal pass while
Isaac Sim, Simulation View, cuOpt, Rerun, and the other GPU services remain
resident. Installations with different checkpoints, solver pools, or GPU capacity
size `reason.engine.gpuMemoryUtilization` and
`VEOVEO_CUOPT_POOL_GIB` against all seven concurrently resident workloads.
The development chart requests 4 GiB of host memory for the cuOpt executor and
8 GiB for the Simulation View renderer. Their higher memory limits remain available
for bursts without making the seven-workload placement unschedulable on the reference
64 GiB node.

The local fixture advertises Simulation View signaling through
`wss://veoveo.bioma.ai/simulation-view/signaling` and its bounded UDP media
range through `127.0.0.1:47998-48001`. The chart assigns those four slots to
the explicit NodePort range `30998-31001`, and the k3d bindings forward each
public port to its matching NodePort. The fixed mapping is part of the
acceptance contract because a Kubernetes-assigned NodePort cannot satisfy a
predeclared browser media endpoint. An installation on a different network
must advertise its routable signaling origin and UDP media address instead.

Install the local platform fixture separately:

~~~bash
kubectl --context k3d-veoveo-bioma apply \
  --server-side --force-conflicts \
  -k examples/bioma/platform
kubectl --context k3d-veoveo-bioma -n argocd wait   --for=condition=Available deployment --all --timeout=5m
~~~

The registry adapter gives stable Argo CD 3.4.5 an HTTPS endpoint for the
loopback HTTP registry. A normal enterprise registry does not need this adapter.

## Provision Secrets

The enterprise owns Secret creation. For this local reference, load the main
worktree .env and create the required Secret objects before the root Application.
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
    "simulation-view-renderer-control-token": env.SIMULATION_VIEW_RENDERER_CONTROL_TOKEN,
    "simulation-view-pose-control-token": env.SIMULATION_VIEW_POSE_CONTROL_TOKEN,
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

pose_tls_dir=$(mktemp -d)
trap 'rm -rf "$pose_tls_dir"' EXIT
umask 077

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 \
  -out "$pose_tls_dir/ca.key.pem"
openssl req -x509 -new -sha256 -days 30 \
  -key "$pose_tls_dir/ca.key.pem" \
  -subj '/CN=VeoVeo Reference Pose CA' \
  -out "$pose_tls_dir/ca.crt.pem"

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 \
  -out "$pose_tls_dir/server.key.pem"
openssl req -new -sha256 \
  -key "$pose_tls_dir/server.key.pem" \
  -subj '/CN=simulation-view-pose' \
  -addext 'subjectAltName=DNS:simulation-view-pose,DNS:simulation-view-pose.veoveo.svc,DNS:simulation-view-pose.veoveo.svc.cluster.local' \
  -addext 'extendedKeyUsage=serverAuth' \
  -out "$pose_tls_dir/server.csr.pem"
openssl x509 -req -sha256 -days 30 -copy_extensions copy \
  -in "$pose_tls_dir/server.csr.pem" \
  -CA "$pose_tls_dir/ca.crt.pem" \
  -CAkey "$pose_tls_dir/ca.key.pem" \
  -CAcreateserial \
  -out "$pose_tls_dir/server.crt.pem"

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 \
  -out "$pose_tls_dir/producer.key.pem"
openssl req -new -sha256 \
  -key "$pose_tls_dir/producer.key.pem" \
  -subj '/CN=uav-sim' \
  -addext 'subjectAltName=URI:spiffe://veoveo.local/simulation/uav-sim' \
  -addext 'extendedKeyUsage=clientAuth' \
  -out "$pose_tls_dir/producer.csr.pem"
openssl x509 -req -sha256 -days 30 -copy_extensions copy \
  -in "$pose_tls_dir/producer.csr.pem" \
  -CA "$pose_tls_dir/ca.crt.pem" \
  -CAkey "$pose_tls_dir/ca.key.pem" \
  -CAcreateserial \
  -out "$pose_tls_dir/producer.crt.pem"

openssl x509 -in "$pose_tls_dir/ca.crt.pem" -outform DER \
  -out "$pose_tls_dir/ca.crt.der"
openssl x509 -in "$pose_tls_dir/server.crt.pem" -outform DER \
  -out "$pose_tls_dir/server.crt.der"
openssl pkcs8 -topk8 -nocrypt -inform PEM -outform DER \
  -in "$pose_tls_dir/server.key.pem" \
  -out "$pose_tls_dir/server.key.der"

kubectl --context k3d-veoveo-bioma -n veoveo create secret generic \
  simulation-view-pose-tls \
  --from-file=tls.crt.der="$pose_tls_dir/server.crt.der" \
  --from-file=tls.key.der="$pose_tls_dir/server.key.der" \
  --from-file=ca.crt.der="$pose_tls_dir/ca.crt.der" \
  --dry-run=client -o yaml | \
  kubectl --context k3d-veoveo-bioma apply -f -

kubectl --context k3d-veoveo-bioma -n veoveo create secret generic \
  uav-sim-pose-producer-tls \
  --from-file=ca.crt="$pose_tls_dir/ca.crt.pem" \
  --from-file=tls.crt="$pose_tls_dir/producer.crt.pem" \
  --from-file=tls.key="$pose_tls_dir/producer.key.pem" \
  --dry-run=client -o yaml | \
  kubectl --context k3d-veoveo-bioma apply -f -
~~~

A production installation projects the same keys from its secret manager. The UAV,
Cloudflare, and recording-producer credentials remain separate least-privilege
Secrets. The local pose certificates above expire after 30 days and exist only for
the disposable reference cluster. Rotate them through the installation secret manager
in a fielded deployment. The committed JWKS files contain public keys only. Bioma mounts the
installation-owned machine-client JWKS with the gateway control plane, which keeps
local client assertions independent of an external JWKS endpoint.

## Connect Argo CD to Git and OCI

Argo repository credentials are platform Secrets. The Bioma repository is private, so
the local fixture uses a GitHub token. Replace this with a GitHub App, deploy key, or
enterprise repository credential in a fielded installation.

~~~bash
export GITHUB_TOKEN=$(gh auth token)

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {
    name: "bioma-git-repository",
    namespace: "argocd",
    labels: {"argocd.argoproj.io/secret-type": "repository"}
  },
  type: "Opaque",
  stringData: {
    type: "git",
    url: "https://github.com/BiomaAI/veoveo.git",
    username: "git",
    password: env.GITHUB_TOKEN
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {
    name: "bioma-chart-repository",
    namespace: "argocd",
    labels: {"argocd.argoproj.io/secret-type": "repository"}
  },
  type: "Opaque",
  stringData: {
    type: "helm",
    name: "bioma-charts",
    url: "charts-registry.argocd.svc.cluster.local/charts",
    enableOCI: "true",
    insecure: "true"
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -
~~~

The insecure flag accepts the local Traefik-generated certificate; chart transport is
still HTTPS. A production OCI repository uses its trusted certificate and omits that
setting.

## Bootstrap desired state

Apply only the root Application:

~~~bash
kubectl --context k3d-veoveo-bioma apply   -f examples/bioma/gitops/bootstrap.yaml
~~~

Argo creates the namespace configuration, gateway ConfigMap, AppProject, Cloudflare
connector, platform child Application, and UAV extension child Application. Inspect
reconciliation with standard Kubernetes or Argo CD commands:

~~~bash
kubectl --context k3d-veoveo-bioma -n argocd get applications
kubectl --context k3d-veoveo-bioma -n veoveo get deployments,statefulsets,pods
argocd app get bioma
argocd app get bioma-veoveo
argocd app get bioma-uav-sim
~~~

All three Applications must be Synced and Healthy. Helm remains the renderer, but Argo
owns the live application resources; do not operate concurrent Helm releases for them.

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

Add lan-values.yaml to the platform Application valueFiles list. The public issuer,
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
cargo xtask smoke uav-domain-verify \
  --context k3d-veoveo-bioma \
  --public-base-url https://veoveo.bioma.ai
cargo xtask smoke uav-showcase-verify \
  --context k3d-veoveo-bioma \
  --public-base-url https://veoveo.bioma.ai \
  --chrome-cdp-url http://127.0.0.1:9222
~~~

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

Delete the root Application and wait for its foreground finalizer to remove managed
application resources before deleting the cluster:

~~~bash
kubectl --context k3d-veoveo-bioma -n argocd delete application bioma
kubectl --context k3d-veoveo-bioma -n argocd wait   --for=delete application/bioma --timeout=10m
k3d cluster delete veoveo-bioma
~~~

Deleting the cluster disconnects the tunnel. It does not delete the remote Cloudflare
Tunnel, DNS records, or the shared registry volume.
