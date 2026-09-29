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

## Workspace Models

The Workspace configuration in `values.yaml` admits Assistant and Reviewer in the
Bioma operations Work Context. It reuses the qualified Cloudflare Workers AI model
`@cf/moonshotai/kimi-k2.6` from the pilot model configuration. These are separate
per-chat contexts with explicit capability allowlists. They have no pilot identity,
simulation control or service credentials. The configured account identifier is
public configuration; the provider token remains in an installation-owned Secret.

[Provision Secrets](#provision-secrets) installs the model token in the application
and managed-kernel namespaces before the workloads start. Each namespace owns its
`veoveo-workspace-models` Secret.

The token needs model inference permission for the account configured in the model
URL. Do not put it in Helm values, Git or command-line literals. The gateway
receives the selected key through `VEOVEO_AGENT_MODEL_API_KEY`. The browser edge
uses the distinct Workspace OAuth client and requests operator use, Artifact upload
and time-read scopes. Work Context and resource policies still apply to every action.

`gateway.agents.models` admits the model connection. `agents.json` is the explicit
Assistant/Reviewer import source; gateway startup never reconciles it over Console
edits. The [catalog import procedure](../../platform/gateway/src/bin/gateway/agent_management/DESIGN.md#installation-import)
preserves existing chat participant IDs and records exact source digests.

The client entry is `/workspace/`. Agent responses can dispatch admitted native MCP
operations; Task input and results stay in the initiating person's private Activity.
The installation must pass real model and public Task acceptance after rollout.

Owner-local acceptance checks the shared server contract and validates the complete
typed installation catalog. Profiles, clients and policy belong to this installation;
they may differ from the disposable local fixture. Journey checks verify the selected
resource exposure, including private Computers and the separate recording publisher.

## Ownership and layout

Managed kernels run in the separate `veoveo-agents` namespace. The installation
copies the database-scoped runtime and approved model Secrets into that namespace;
the manager receives no credentials from the browser. `gateway.agents.templates`
binds the reviewed [UAV template](../../showcase/uav-sim/agents/DESIGN.md), its exact
immutable ConfigMap and kernel image. The manager's API and model egress entries
are installation addresses. Model entries pin the current IPv4 answers for
`api.cloudflare.com`; a destination change requires a reviewed configuration update.

The initial managed installation keeps the four existing pilot registrations while
qualifying new instance creation. Their explicit transfer removes those static
registrations and per-pilot Helm workloads. This checkpoint does not establish
that the retained pilots have been migrated.

The repository separates the local platform fixture from application desired state:

~~~text
examples/bioma/
  platform/                     local cluster prerequisites
    flux/                       pinned Flux 2.9.5 installation
    registry/                   cluster-local loopback OCI registry address
  gitops/
    bootstrap.yaml              Git source and root Kustomization, applied once
    sources/                    exact platform and UAV OCI charts
    releases/                   platform and UAV Helm releases
    cloudflared.yaml            installation edge connector
  kustomization.yaml            root desired-state composition
  values.yaml                   public identity and platform values
  k3d-values.yaml               local capacity and storage values
  uav-sim-values.yaml           UAV workload values
  images/veoveo.lock.yaml        platform image digests
  images/uav-sim.lock.yaml       UAV and pilot image digests
  gateway.json                  MCP catalog, OAuth, policy, and routes
  computers/                    private host, control and retained-template policy
  acceptance/                   owner-local compiled composition checks
  recording-producer-jwks.json  public producer key
  operator-client-jwks.json     public operator client key
  admin-client-jwks.json        public administrator client key
~~~

The local platform fixture installs Flux and the registry address because this
cluster has no enterprise platform team. A fielded installation uses its existing
GitOps controller and secure OCI registry, then begins at the root Kustomization.
Veoveo application desired state never owns the controller that reconciles it.

The platform and UAV charts are separate OCI packages. Removing or upgrading the
UAV HelmRelease does not replace the core platform release. A customer MCP server
built in the installation's fork deploys the same way: publish its image and chart,
then register it in the gateway control plane.

The UAV world bootstrap pairs `uav-sim-world.json` with its exact SHA-256 in
`uav-sim-values.yaml` at `world.bootstrap.contentSha256`. Update both in the same
commit. That digest restarts both the immutable simulator runtime and its MCP companion.
Drain active flights before changing it; the runtime and tile cache claims are preserved.

Generated Helm values ConfigMaps carry the `reconcile.fluxcd.io/watch: Enabled`
label. A values update therefore wakes the owning Helm controller immediately after
Flux applies it. Chart publication alone does not replace unchanged Pod templates.

## Release publication

<!-- TODO(foundations): Finish composed flight and installed workload acceptance.
Sensor health, landing, re-arming, takeoff, Map routing and mission completion pass.
Live Stream fails because the parsed H.264 access unit has no timestamp. Qualify that
path with uav-stream-verify before repeating flight, replay and Reason acceptance. -->

Service clients authenticate with separate installation-owned RSA keys. Only their
public JWKS belongs in this GitOps bundle. The private PEM files stay in the caller's
credential store with owner-only permissions and encrypted backup. The repository's
public conformance key is a disposable loopback fixture and cannot authenticate these
clients. Key rotation replaces the selected public key and the caller's private key as
one coordinated installation change; an operator key never authorizes the admin client.

For local operator acceptance, provide the private key by file path and its registered
public key ID. The command emits an access token on stdout; capture it directly in the
consumer's environment without displaying or writing it into an evidence file:

```sh
export VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE=/private/bioma/operator-service.pem
export VEOVEO_SERVICE_CLIENT_KEY_ID=bioma-operator-service-20260911
```

Use `conformance gateway-token-exchange` with the installation HTTPS token endpoint,
the `operator-service` client and its admitted resource/scopes. Administrator acceptance
uses its distinct private file and `bioma-admin-service-20260911` key ID. Existing caller
credentials must be provisioned by the installation owner; copying this public example
does not grant service access.

Computers capacity is selected by `computerCapacity: openshell-docker`. Its public
JSON files pin one provider identity and one command-capable `development` template.
This development template
has Python, Git and shell tools, an 8 GiB retained home, two CPUs and 2 GiB memory.
It grants no outbound network access. Admission allows two Computers per owner and
four for this installation. Console and Computers control each use one replica;
the private compute host owns a retained 100 GiB PVC. Host maintenance requires
explicit lifecycle coordination before its single replica is replaced.

Fresh installations enroll trust with this command:

```sh
cargo xtask release computers-trust --output /private/new-computers-trust --host-name computer-host.veoveo.svc.cluster.local
kubectl --context k3d-veoveo-bioma -n veoveo create secret generic bioma-computer-host-trust --from-file=/private/new-computers-trust/host
kubectl --context k3d-veoveo-bioma -n veoveo create secret generic bioma-computers-worker-trust --from-file=/private/new-computers-trust/worker
```

Set `execution.activeKeyId` and the matching key entry in `computers.json` to the
public identifier in `worker/command-key-id`, then recompute the configuration revision.
The reference UUID is installation configuration, not a credential. The command key
file contains exactly 32 random bytes and is mounted only in Computers workers.

The parent directory must already exist. Keep the operator CA keys outside the
cluster and protect the bundle with installation-owned encrypted backup. Leaf
certificates expire after 90 days. Inspect their `notAfter` dates and qualify rotation
as maintenance before expiry. Existing retained providers preserve their trust and JWT
identity across ordinary rollouts. Stop a running Computer through its authorized
lifecycle before replacing the private host. The disposable identifier-cut reset
enrolls fresh trust and selects only the current template after deleting all retained
homes and provider journals. Its installation has no template maintenance pairs.
The command refuses existing output, and these Secret commands refuse existing names.

Compute the admitted template fingerprint with the production encoder whenever a
template input changes:

```sh
cargo run --locked --offline -p veoveo-computers-runtime --example retained_template -- <digest-pinned-image> 2 2048 8192 256 examples/bioma/computers/policy.json
```

Update the exact policy in `computers.json`, the admitted fingerprint in both JSON
files, and the SHA-256 configuration revisions in `values.yaml` together. Existing
Computers keep their admitted template and retained identity. The cluster-local
development registry is explicit HTTP; this choice belongs to this k3d installation.
Restricted-network deployments must set the actual registry CIDR and port. The
provider and storage endpoints remain private and require separate worker mTLS keys.

The admin and operator profiles explicitly admit artifact uploads for their existing
human roles and machine clients with `artifact:upload` scope and contributor membership.
The Console requests that scope through its BFF. Existing sign-ins need a new login
to acquire a newly granted scope.

The reference policy admits objects up to 100 GiB under a 200 GiB tenant storage quota.
It starts with 16 MiB parts and at most four concurrent parts per file. Shared in-flight
payloads are limited to 128 MiB across replicas. These are installation limits, not
product limits. The policy does not certify that a consumer can load a file of that
size into memory. Deployed transfer evidence separately records the sizes exercised.

Uploads use `/artifacts/{profile}/uploads`; Console uses its same-origin
`/console/api/artifact-uploads` proxy. The whole file never becomes one HTTP request.
Its Gateway ConfigMap revision hashes the complete public bundle, including all JWKS
files, through the deployment contract's `veoveo.ai/gateway-activation/v1` encoding.


Production workloads use the repository and digest maps under `images/`. Each release
receives only the images consumed by its rendered objects. The
platform and UAV OCI sources select immutable chart manifest digests independently in
`gitops/sources/`. Chart metadata keeps the human-readable release version. Those
files and the image locks define the current deployment; this manual does not copy
the revision. A workload is identified by its selected image digest. A release-input
commit updates all of these inputs together, and a qualified publication then
promotes exactly those inputs.

Publish a new local release directly to the shared registry:

~~~bash
REVISION=$(git rev-parse HEAD)
CHART_VERSION=0.1.0-$(git rev-parse --short=12 HEAD)

cargo xtask image builder ensure
cargo xtask release images --group platform-full \
  --push-registry 127.0.0.1:5001 \
  --pull-registry k3d-veoveo-registry.localhost:5000 \
  --registry-transport insecure-http --revision "$REVISION"
cargo xtask release images --group showcase-uav-sim \
  --push-registry 127.0.0.1:5001 \
  --pull-registry k3d-veoveo-registry.localhost:5000 \
  --registry-transport insecure-http --revision "$REVISION"

cargo xtask release helm-charts \
  --revision "$REVISION" --version "$CHART_VERSION" \
  --registry localhost:5001/charts --plain-http
~~~

BuildKit pushes only missing layers and does not load release images into the host
Docker store. Record the manifest digest for every published image in
the owning release’s lock under `images/`, then update the selected chart manifest digests in `gitops/sources/`
in one release-input commit. Each source object's name ends in its complete manifest
digest without the `sha256:` prefix. Update its HelmRelease `chartRef.name` in
`gitops/releases/` to the same name. Kustomize generates immutable values ConfigMaps
with content suffixes and rewrites every Helm values reference through
`gitops/name-references.yaml`. The root Flux artifact carries those chart selections and
all generated values from one Git revision.

Update the runtime images selected outside Pod templates in the same commit.
`gateway.agents.templates[].workload.image` in `values.yaml` must select the published
`veoveo/agent-kernel` digest. Computers selects its guest image through both public
configuration files as described above. Check the complete consumed-image set before
pushing:

~~~bash
cargo test -p veoveo-xtask commands::helm::rollout_tests::
cargo xtask smoke helm-config
~~~

The HelmRelease update selects its chart and values together. A new values schema
therefore waits for its matching chart source. This also prevents an intermediate
upgrade that would otherwise combine new image pins with the previous chart.

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

The command passively observes the exact Git artifact, root apply, Helm release
inventories, rollout, and readiness. Add `--reconciliation request` to explicitly wake
the selected Flux controllers. For passive publication latency, start observation
before pushing the prepared commit and retain the publication timestamp; an observer
started after convergence measures only verification overhead. Reusing an evidence
path is rejected because records are create-only. The full procedure and production
registry requirements are in the enterprise deployment guide.

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

The node must report at least seven allocatable GPU shares before application bootstrap.
The local time-slicing profile keeps the UAV simulator, View, Stream,
Reason, Speech, the cuOpt executor, and the Rerun viewer MCP in separate GPU-requesting
workloads. Fielded installations use their measured exclusive,
MIG, or time-slicing placement instead of inheriting this development profile.
Each required workload still requests nvidia.com/gpu: 1 and the nvidia runtime
class. The shares make all seven render and GPU-compute workloads schedulable
together; they are not a CPU fallback.

The local Reason profile reserves 42% of the 24 GiB NVIDIA device for vLLM. This
bound preserves device-memory headroom for the six-frame multimodal pass while
the Isaac simulator, cuOpt, Rerun, and the other GPU services stay
resident. Installations with different checkpoints, solver pools, or GPU capacity
size `reason.engine.gpuMemoryUtilization` and
`VEOVEO_CUOPT_POOL_GIB` against all seven concurrently resident workloads.
The development chart requests 4 GiB of host memory for the cuOpt executor. The
simulator's operator-camera products run inside the simulator allocation. Higher
memory limits allow bursts without making the seven-workload placement unschedulable on
the reference 64 GiB node.

The local fixture advertises simulator-owned shared H.264 delivery through
`wss://veoveo.bioma.ai/uav-sim/live`. The existing HTTPS ingress upgrades authenticated
WebSocket connections and requires no public UDP range or per-viewer NodePort. Each
camera is rendered and encoded once inside the simulator, then its exact access units
fan out to browser viewers.

Install the local platform fixture separately:

~~~bash
kubectl --context k3d-veoveo-bioma apply \
  --server-side --field-manager=veoveo-flux-platform \
  -k examples/bioma/platform
kubectl --context k3d-veoveo-bioma -n flux-system wait \
  --for=condition=Available deployment --all --timeout=5m
~~~

The local OCI source explicitly admits the cluster-local HTTP registry. A fielded
installation uses its authenticated TLS registry and removes that local exception.

Controller images select exact release tags and immutable OCI index digests.
The Kustomize and Helm controllers enable `CancelHealthCheckOnNewRevision`.
A corrected source revision can interrupt an obsolete root health check, and the
resulting release update can interrupt Helm's old health check. Veoveo retries failed
upgrades in place after one minute. Health checks are still required, and a failed
release stays visible until its workloads recover. Automatic rollback could remove
new Computers services and repeatedly restart retained workloads when an unrelated
service fails. Rollback therefore requires an explicitly qualified installation
revision and storage transition. The isolated cancellation fixture keeps rollback
remediation enabled to exercise that controller path. Verify cancellation with the isolated
`cargo xtask smoke gitops-cancel-verify` scenario described in
[`testing/deployment-smoke/DESIGN.md`](../../testing/deployment-smoke/DESIGN.md).

## Provision Secrets

The enterprise owns Secret creation. For this local reference, load the main
worktree .env and create the required Secret objects before the root Kustomization.
Bootstrap the managed-kernel namespace with the chart's security labels and Helm
ownership metadata, allowing the release to adopt it after its Secrets exist.
The following command reads values through the environment and sends the Secret
documents directly to Kubernetes over stdin:

~~~bash
set -a
source .env
set +a

kubectl --context k3d-veoveo-bioma apply   -f examples/bioma/gitops/namespace.yaml

kubectl --context k3d-veoveo-bioma apply -f - <<'YAML'
apiVersion: v1
kind: Namespace
metadata:
  name: veoveo-agents
  labels:
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/enforce-version: v1.36
    veoveo.ai/agent-installation: veoveo
    app.kubernetes.io/managed-by: Helm
  annotations:
    helm.sh/resource-policy: keep
    meta.helm.sh/release-name: veoveo
    meta.helm.sh/release-namespace: veoveo
YAML

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-surreal-admin", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    username: env.VEOVEO_SURREAL_ADMIN_USERNAME,
    password: env.VEOVEO_SURREAL_ADMIN_PASSWORD
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

for bioma_namespace in veoveo veoveo-agents; do
  jq -n --arg namespace "$bioma_namespace" '{
    apiVersion: "v1", kind: "Secret",
    metadata: {name: "veoveo-surreal-runtime", namespace: $namespace},
    type: "Opaque",
    stringData: {
      username: env.VEOVEO_SURREAL_RUNTIME_USERNAME,
      password: env.VEOVEO_SURREAL_RUNTIME_PASSWORD
    }
  }' | kubectl --context k3d-veoveo-bioma apply -f -

  jq -n --arg namespace "$bioma_namespace" '{
    apiVersion: "v1", kind: "Secret",
    metadata: {name: "veoveo-workspace-models", namespace: $namespace},
    type: "Opaque",
    stringData: {"api-key": env.CLOUDFLARE_API_TOKEN}
  }' | kubectl --context k3d-veoveo-bioma apply -f -
done

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

# The disposable Bioma fixture uses the owner-managed recording test key for
# three separately scoped OAuth client identities. Production installations
# should issue distinct keys while preserving these Secret names and keys.
jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-recording-hub", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "private-key.pem": env.VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

jq -n '{
  apiVersion: "v1", kind: "Secret",
  metadata: {name: "veoveo-recording-mcp-publisher", namespace: "veoveo"},
  type: "Opaque",
  stringData: {
    "private-key.pem": env.VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM
  }
}' | kubectl --context k3d-veoveo-bioma apply -f -

for secret in veoveo-recording-producer veoveo-recording-hub veoveo-recording-mcp-publisher; do
  kubectl --context k3d-veoveo-bioma -n veoveo get secret "$secret" \
    -o go-template='{{index .data "private-key.pem" | base64decode}}' \
    | openssl pkey -check -noout
done

~~~

A production installation projects the same keys from its secret manager. The UAV,
Cloudflare and recording credentials remain separate least-privilege Secret objects.
The disposable fixture reuses one owner test key across the three recording OAuth
clients, while production issues separate private keys. The committed JWKS files contain public keys only. The reference installation mounts the
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

A fresh installation needs its Reason checkpoint before the release can become Ready.
Complete [Provision the Reason checkpoint](#provision-the-reason-checkpoint) while
Helm waits for the workloads.

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

## Provision the Reason checkpoint

Reason uses [Qwen3-VL-4B-Instruct-FP8 at revision
`fefbb44cbcce8d1bb7e20b920b94f77432b3446d`](https://huggingface.co/Qwen/Qwen3-VL-4B-Instruct-FP8/tree/fefbb44cbcce8d1bb7e20b920b94f77432b3446d).
The checkpoint uses the `Qwen3VLForConditionalGeneration` adapter supported by Reason.
The installation selects FP8 to reduce checkpoint memory. The
[file manifest](reason-model.sha256) pins every upstream snapshot file. Its own SHA-256
is the model identity in `k3d-values.yaml`.

Download the snapshot on the host, or reuse a directory that passes the same manifest:

~~~bash
set -euo pipefail
bioma_model_revision=fefbb44cbcce8d1bb7e20b920b94f77432b3446d
bioma_model_name=qwen3-vl-4b-instruct-fp8-$bioma_model_revision
bioma_model_dir="$PWD/output/models/$bioma_model_name"
mkdir -p "$bioma_model_dir"
while read -r checksum filename; do
  curl --fail --location --proto '=https' --proto-redir '=https' \
    --max-time 600 --retry 2 --retry-max-time 620 \
    "https://huggingface.co/Qwen/Qwen3-VL-4B-Instruct-FP8/resolve/$bioma_model_revision/$filename" \
    --output "$bioma_model_dir/$filename"
done < examples/bioma/reason-model.sha256
cp examples/bioma/reason-model.sha256 "$bioma_model_dir/SHA256SUMS"
(cd "$bioma_model_dir" && sha256sum --check SHA256SUMS)
~~~

After Helm creates `reason-model-cache` and the Reason Deployment, use its pinned image
for a temporary file-transfer pod. This pod only copies checkpoint files. Reason loads
them in its separate GPU-requesting workload. The helper expires after 15 minutes.

~~~bash
bioma_reason_image=$(kubectl --context k3d-veoveo-bioma -n veoveo \
  get deployment reason-mcp -o jsonpath='{.spec.template.spec.containers[0].image}')
kubectl --context k3d-veoveo-bioma apply -f - <<YAML
apiVersion: v1
kind: Pod
metadata:
  name: reason-model-stage
  namespace: veoveo
  labels:
    app.kubernetes.io/managed-by: bioma-checkpoint-provisioning
spec:
  restartPolicy: Never
  activeDeadlineSeconds: 900
  automountServiceAccountToken: false
  securityContext:
    runAsNonRoot: true
    runAsUser: 10001
    runAsGroup: 10001
    fsGroup: 10001
    fsGroupChangePolicy: OnRootMismatch
    seccompProfile:
      type: RuntimeDefault
  containers:
    - name: transfer
      image: $bioma_reason_image
      command: [/bin/sh, -c, "sleep 900"]
      resources:
        requests: {cpu: 100m, memory: 128Mi}
        limits: {cpu: "1", memory: 512Mi}
      securityContext:
        allowPrivilegeEscalation: false
        readOnlyRootFilesystem: true
        capabilities:
          drop: [ALL]
      volumeMounts:
        - name: model-cache
          mountPath: /models
  volumes:
    - name: model-cache
      persistentVolumeClaim:
        claimName: reason-model-cache
YAML
kubectl --context k3d-veoveo-bioma -n veoveo wait \
  --for=condition=Ready pod/reason-model-stage --timeout=5m
kubectl --context k3d-veoveo-bioma -n veoveo exec reason-model-stage -- \
  mkdir /models/.checkpoint-staging
kubectl --context k3d-veoveo-bioma -n veoveo cp \
  "$bioma_model_dir/." reason-model-stage:/models/.checkpoint-staging
kubectl --context k3d-veoveo-bioma -n veoveo exec reason-model-stage -- \
  /bin/sh -c 'cd /models/.checkpoint-staging && sha256sum --check SHA256SUMS'
kubectl --context k3d-veoveo-bioma -n veoveo exec reason-model-stage -- \
  /bin/sh -c 'test ! -e "/models/$1" && mv /models/.checkpoint-staging "/models/$1"' \
  stage "$bioma_model_name"
kubectl --context k3d-veoveo-bioma -n veoveo delete pod reason-model-stage --wait=true --timeout=1m
kubectl --context k3d-veoveo-bioma -n veoveo rollout restart deployment/reason-mcp
kubectl --context k3d-veoveo-bioma -n veoveo rollout status deployment/reason-mcp --timeout=5m
~~~

Publish a different checkpoint with a new manifest, digest and directory. Never replace
files under a directory that a running Reason workload reads. A failed transfer leaves
`.checkpoint-staging` for inspection; remove only that incomplete directory before
retrying. A cluster reset deletes the model PVC, so repeat this step during each rebuild.

## Public edge

The remote-managed tunnel is named veoveo-bioma-ai. Its desired ingress sends the
installation's only public hostname to Traefik in the cluster:

~~~text
veoveo.bioma.ai -> http://traefik.kube-system.svc.cluster.local:80
~~~

The DNS record targets the tunnel hostname and Cloudflare terminates public TLS.
RustFS remains cluster-private. Artifact bytes reach clients only through the
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

A LAN recording producer still uses the public resource identity
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
cargo xtask smoke installation-verify --installation examples/bioma/installation-target.json
~~~

This gate uses the public machine-client contract to export a deterministic artifact
larger than 8 MiB through DuckDB. It then verifies full, HEAD, and ranged delivery at
the installation origin with redirect following disabled, exact content and SHA-256
checks, and no object-storage address in metadata or response headers.

Prepare the [aviation release](#prepare-the-aviation-release), then check Map admission
before the full GPU delivery proof:

~~~bash
cargo xtask smoke uav-route-verify \
  --installation examples/bioma/installation-target.json
~~~

This command creates a Map route Task at the scenario's takeoff altitude. It requires
the operator credentials and Map, and issues no flight commands. Full acceptance
rechecks admission before taking control of a vehicle.

With the UAV camera and Stream service running, qualify live inference separately:

~~~bash
cargo xtask smoke uav-stream-verify \
  --installation examples/bioma/installation-target.json
~~~

This command needs only operator credentials. It checks fresh results and the encoded
preview through the public Stream resources, then stops the session if it created it.
It leaves a reused session with its owner. It performs no landing, takeoff, mission,
recording replay or Reason work. Full flight checks the same live prerequisite before
vehicle control and again after mission completion.

Flight verification also requires the administrator client's private key through
`VEOVEO_ADMIN_SERVICE_CLIENT_PRIVATE_KEY_FILE` and its ID through
`VEOVEO_ADMIN_SERVICE_CLIENT_KEY_ID`. The target file selects `admin-service` and its
admitted scopes separately from the operator.

~~~bash
VEOVEO_CUOPT_EXECUTOR_IMAGE=veoveo/cuopt-executor:0.1.0 \
  cargo xtask smoke agent-pilot
cargo xtask smoke uav-showcase-up \
  --installation examples/bioma/installation-target.json
cargo xtask smoke uav-domain-verify \
  --installation examples/bioma/installation-target.json
cargo xtask smoke uav-showcase-verify \
  --installation examples/bioma/installation-target.json \
  --chrome-cdp-url http://127.0.0.1:9222
~~~

`uav-showcase-up` converges the immutable Frames world, starts the perpetual fleet
loop, and leaves its simulator-hosted camera live. The verification commands
exercise bounded missions and may take temporary ownership of individual vehicles.

The Pilot acceptance starts the real cuOpt executor on the host GPU, sends a typed
MILP through the gateway as a durable task, verifies the solution independently,
wakes the sleeping agent from task completion, and checks its durable decision
record.

The live reference keeps four PX4 vehicles on nested loops over Manhattan until an
explicit mission or direct flight command takes control of an individual vehicle. The
UAV acceptance requires Google Photorealistic 3D Tiles resident in Isaac, claims one
vehicle for a PX4 mission, verifies direct Stream results from newly arrived
camera frames, then runs reproducible Stream replay and Reason over acknowledged
recording parts before archive rollover. The other vehicles continue their loops while
the acceptance confirms that concurrent GPU deployments remain available. Its runtime inputs come from
showcase/uav-sim/scenarios/new-york-aerial.json. The acceptance client creates
the complete world through Frames MCP and binds the returned immutable revision
to the simulator before Isaac constructs its stage.

### Prepare the aviation release

The Map bootstrap registers the synthetic showcase source and mobility profile.
An administrator must acquire and activate its dataset after creating or resetting
the application store. Use the public Map MCP tools with the installation's
administrator identity and its declared `map:admin` and `map:dataset:read` scopes.

Read `map://active-releases` first. Reuse an active release for dataset
`dataset-019ffdb2-0598-7717-b916-e359c426f8cf` only when its source digest matches
`27de4541a7ef2f87b6425fa4375c5a246d14fd4c87463afb5bb0fb5760456269`.
That digest identifies the registered immutable
[synthetic fixture](../../showcase/uav-sim/map/README.md).

Call `map__start_acquisition` with:

~~~json
{
  "source_id": "source-019ffdb2-0596-7c91-ac83-0a45b82d7952",
  "requested_coverage": {"west": -74.06, "south": 40.68, "east": -73.95, "north": 40.80},
  "expected_source_digest_sha256": "27de4541a7ef2f87b6425fa4375c5a246d14fd4c87463afb5bb0fb5760456269",
  "idempotency_key": "new-york-showcase-27de4541a7ef2f87b6425fa4375c5a246d14fd4c87463afb5bb0fb5760456269"
}
~~~

Save the returned acquisition identity. Observe its `map://acquisition/{acquisition_id}`
resource until it reports `succeeded`, with a ten-minute observation deadline. A timeout
leaves the acquisition unresolved; inspect that identity before another mutation. A
failed acquisition carries diagnostics and must be resolved before activation.

Read the returned staged release through
`map://dataset/{dataset_id}/release/{release_id}` and check its source digest.
Call `map__activate_release` with that release ID, its current `record_version` as
`expected_record_version`, and the dataset's current active-pointer version as
`expected_active_pointer_version` (zero when it has no active pointer). Verify the
active release and run `uav-route-verify`. Synthetic data supplies planning-advisory
routes for this simulation.

## Cleanup

### Rebuild the disposable installation

The foundations identifier cut changes stored formats and migration checksums. Publish
the new image locks and chart digests through [Release publication](#release-publication)
before this reset. The reference installation has no data to preserve. Its node owns
SurrealDB, Artifact object storage, recording journals, Computers retained homes and
host journals, and agent memory. Delete the cluster and its node volumes together:

~~~bash
mapfile -t bioma_node_volumes < <(
  docker inspect k3d-veoveo-bioma-server-0 \
    --format '{{json .Mounts}}' | jq -r '.[] | select(.Type == "volume") | .Name'
)
k3d cluster delete veoveo-bioma
for volume in "${bioma_node_volumes[@]}"; do
  if docker volume inspect "$volume" >/dev/null 2>&1; then
    docker volume rm "$volume"
  fi
done
rm -rf -- "$(git rev-parse --git-common-dir)/veoveo-deployment"
~~~

These commands affect only this cluster's volumes. The shared registry and other
clusters keep their data. Recreate any separate local development SurrealDB database
before connecting the new binaries; an old migration ledger cannot accept the changed
checksums.

Follow [Create the local platform](#create-the-local-platform), skipping registry
creation when it already exists. Then [provision Secrets](#provision-secrets) from the
installation's external credential store, enroll Computers trust as described in
[Release publication](#release-publication), and [connect Flux to Git](#connect-flux-to-git).
Reconcile any changed public Computers key ID and configuration revision before
[bootstrapping desired state](#bootstrap-desired-state). This creates fresh application
volumes from the published locks. Once Frames and the public gateway are available,
publish the current world using the operator credentials described above:

~~~bash
cargo xtask smoke uav-world-publish \
  --installation examples/bioma/installation-target.json \
  --output examples/bioma/uav-sim-world.json
~~~

Set `world.bootstrap.contentSha256` in `uav-sim-values.yaml` to the command's
`contentSha256`, and commit both files together. Publish and select the chart/runtime
that mount and pin this document in both workloads. Keep the UAV HelmRelease suspended
in reviewed GitOps desired state while draining the simulator and companion. A root
reconciliation can remove an out-of-band suspension patch. Remove the suspension in
the activation commit that selects the new binding, chart and images together, so the
old release cannot resume between those updates. The new deployment uses the current
Frames publication and keeps the tile cache.
A reset never reuses a deleted revision from the previous database.

Prepare the [current aviation release](#prepare-the-aviation-release) and pass
`uav-route-verify` before the flight gate. Run GitOps convergence and installed verification against that revision, including
live MCP conformance and composed UAV acceptance, before accepting the reset.

### Remove the cluster

Delete the disposable cluster when the reference installation is no longer needed:

~~~bash
k3d cluster delete veoveo-bioma
~~~

Deleting the cluster disconnects the tunnel. It does not delete the remote Cloudflare
Tunnel, DNS records, or the shared registry volume.

A platform image edit updates `images/veoveo.lock.yaml`; a simulator, UAV MCP, pilot,
or pilot-forwarder edit updates `images/uav-sim.lock.yaml`. The generated values
ConfigMaps keep the `images.lock.yaml` data key while reading distinct files. A release
therefore receives a values event only when one of its own inputs changes. Rendered
chart tests require each lock to match the images actually consumed by that release.
Publish a changed chart with `cargo xtask release helm-charts --chart veoveo` or
`--chart uav-sim`, together with its required revision and version arguments. An
image-only digest update needs no chart publication.

Template IDs stay attached to retained Computers. The installed `development`
template keeps its original ID and fingerprint. The execution-capable image has the
new ID `development-20260910`; changing the default selects it only for new requests.
An explicit qualified maintenance operation changes an existing Computer's template.
Never rename the retained catalog entry merely to make the new default use its name.
