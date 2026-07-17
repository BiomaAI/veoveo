# Bioma deployment example

This directory owns the `veoveo.bioma.ai` installation profile. The public
hostname reaches its k3d cluster through a remote-managed Cloudflare Tunnel.

| Installation | k3d cluster | Kubernetes context | Host projection |
|---|---|---|---|
| Bioma public edge | `veoveo-bioma` | `k3d-veoveo-bioma` | `http://localhost:8781` |

Every Bioma recipe passes its Kubernetes context explicitly. The active
kubectl context cannot redirect a recipe into another installation.

## Profile ownership

- `values.yaml` owns Bioma's public origins and gateway ConfigMap identity.
- `gateway.json` owns the Entra application, tenant mapping, and MCP profiles.
- `k3d.yaml` owns the cluster, loopback ingress, and pinned local OCI registry.
- `k3d-values.yaml` sizes persistent volumes and replicas for the local Bioma
  cluster and bootstraps the canonical UAV ENU frame without changing the
  server catalog.
- `uav-sim-values.yaml` binds the Isaac session to that frame, the typed nadir
  camera, and the Bioma recording tenant.
- `lan-values.yaml` enables canonical-host TLS at Traefik for direct LAN access.
- `cloudflare-tunnel.json` is the desired remote tunnel ingress configuration.
- `tunnel.yaml` runs the connector inside the Bioma cluster.

The Bioma cluster deploys the complete Veoveo installation: the gateway,
artifact and recording planes, console, and every hosted MCP server in
`gateway.json`.

Isaac Sim, View, and Perception each request one NVIDIA GPU allocation. The k3d
node device plugin publishes three time-sliced allocations from the physical
GPU, while all three pods retain the normal `nvidia.com/gpu: 1` request and the
`nvidia` runtime class. No release scales down another GPU service. Perception
compiles the bundled TrafficCamNet ONNX model into a GPU-specific TensorRT
engine on first startup and keeps that engine in its model-cache volume. View
retains its direct Google Maps Platform path. Isaac loads Google Photorealistic
3D Tiles through Cesium ion as a separate core world contract.

The public root redirects to the operations console:

```text
https://veoveo.bioma.ai/console/
```

The console requests the operator and administrator scopes, Map administration,
and the Map, Time, and View read scopes needed to inspect the complete admin MCP
profile. Sign out and authenticate again after a deployment changes that scope
set because an existing console session retains the scopes issued at login.

The MCP page expands each hosted server into the tools, resource patterns,
prompts, protocol capabilities, required scopes, and owned HTTP routes declared
by the active gateway control plane. The Cluster page reads the live Kubernetes
namespace through a dedicated read-only Role. It can list workloads, pods,
services, ingress, storage claims, policies, disruption budgets, and ConfigMaps;
it has no permission to read Secrets. Every inventory request also passes the
gateway's `AdminRead` policy before the console contacts Kubernetes. Audit
renders 25 events per page and can export the current filtered result as CSV.

The Map page starts with the installation-owned OpenStreetMap El Salvador
source declared in `k3d-values.yaml`. Catalog bootstrap is typed and idempotent:
restarting Map preserves an existing source and registers it only when absent.
The source is an acquisition authority, not an implicitly active release. Start
an acquisition in the console, inspect the staged release, then activate it to
move the validated projection into service.

The object-store hostname is an S3 API endpoint. Its bucket root returns
`AccessDenied` without credentials and is not a browser console.

## Cloudflare state

The remote-managed tunnel is named `veoveo-bioma-ai`. Its desired ingress maps
both public hostnames to Traefik inside the Bioma cluster:

```text
veoveo.bioma.ai         -> http://traefik.kube-system.svc.cluster.local:80
objects-veoveo.bioma.ai -> http://traefik.kube-system.svc.cluster.local:80
```

Both proxied DNS records must target that tunnel's
`<tunnel-id>.cfargotunnel.com` hostname. Cloudflare terminates public TLS. The
origin leg is cluster-internal HTTP, and Traefik receives the matching public
Host header.

The object hostname stays one label beneath `bioma.ai` because Cloudflare's
Universal SSL certificate covers `*.bioma.ai`. A nested
`objects.veoveo.bioma.ai` name requires a separately managed certificate and is
not part of this profile.

The `.env` file must define `CLOUDFLARE_ACCOUNT_ID`,
`CLOUDFLARE_API_TOKEN`, `CLOUDFLARED_TUNNEL_TOKEN`,
`CESIUM_ION_ACCESS_TOKEN`, and `GOOGLE_MAPS_API_KEY`. The Cesium token is stored
as `cesium-ion-access-token` in the least-privilege `veoveo-uav-sim-secrets`
Secret for Isaac. The Google key remains the direct View MCP credential. The
account token needs Tunnel:Edit and DNS:Edit for this account and zone. The
tunnel token is stored only in the `bioma-cloudflared` Kubernetes Secret.

## Start Bioma

```bash
just k3d-node-build
just bioma-k3d-create
just bioma-build
just bioma-import
just bioma-resources
just bioma-uav-sim-publish
just bioma-platform-up
just bioma-tunnel-up
```

`bioma-resources` reads the required Veoveo, media-provider, Cesium, Google, and
Cloudflare values from the main worktree `.env`, applies Kubernetes Secrets over
stdin, and never writes their plaintext to a repository or temporary manifest.
`bioma-build` and `bioma-import` cover the platform images.
`bioma-uav-sim-publish` pushes the immutable UAV dependency base and thin
commit-addressed runtime and MCP images to the cluster-managed OCI registry.
OCI blob deduplication avoids another 20+ GB cluster import when only runtime
source changes. `bioma-platform-up` deploys the exact Git commit and waits for
Isaac tile and PX4 readiness without suspending View or Perception.

## Local-network recording producers

A producer on the same local network uses the public installation name and the
same OAuth resource as an Internet producer. Configure the LAN DNS resolver to
return the Traefik address for `veoveo.bioma.ai`, then install a certificate for
that name as the `bioma-lan-ingress-tls` Kubernetes TLS Secret. The certificate
must chain to a CA trusted by each producer. An ACME DNS-01 certificate or an
enterprise CA certificate works without exposing the LAN ingress to the
Internet.

Create or rotate the Secret from operator-controlled certificate files:

```bash
kubectl --context k3d-veoveo-bioma -n veoveo create secret tls \
  bioma-lan-ingress-tls \
  --cert=/secure/path/veoveo.bioma.ai.crt \
  --key=/secure/path/veoveo.bioma.ai.key \
  --dry-run=client -o yaml | \
kubectl --context k3d-veoveo-bioma apply -f -
```

Install the LAN overlay after the normal resources are present:

```bash
just bioma-platform-up-lan
```

The local DNS answer changes only the route. Discovery, token issuance, the
protected-resource identifier, certificate hostname, and ingest URL remain
`https://veoveo.bioma.ai`. The firewall exposes Traefik TCP/443 to the producer
subnets; it does not expose Recording Hub ports 9876 or 9878. Cloudflare Tunnel
can remain active because its origin leg continues to use Traefik HTTP port 80.

## Acceptance

Run the installation check after the release and tunnel are active:

```bash
just bioma-verify
```

The Bioma check requires every deployment, three allocatable NVIDIA GPU shares,
a healthy public edge, and the Bioma authorization-server key at the public
JWKS endpoint. Run the core UAV delivery proof separately after the simulation
is ready:

```bash
just bioma-uav-sim-verify
```

The UAV check requires Google Photorealistic 3D Tiles to be resident inside
Isaac, flies a PX4 mission, verifies the governed recording, processes the
camera stream through Perception, and confirms all three GPU deployments remain
available. Its default mission comes from
`showcase/uav-sim/scenarios/bioma-aerial.json`; editing that file requires no
Isaac build or deployment.

The Access page reports policy sets from the active gateway control-plane
revision. Policies are not independent CRUD records. Edit `gateway.json`,
validate the complete document, and activate it as one atomic revision.

## Entra application registration

`gateway.json` uses one single-tenant Microsoft Entra application as the
external OIDC provider. Its registration must match the control plane:

- Register `https://veoveo.bioma.ai/oauth/callback` as a Web redirect URI.
- Create the app roles `veoveo_operator` and `veoveo_admin`, then allow user or
  group assignment. The operations console requires `veoveo_admin`; operator
  access requires `veoveo_operator` or `veoveo_admin` according to the profile
  policy. Sign out and authenticate again after changing an assignment because
  existing tokens retain their original `roles` claim.
- Treat the app-role UUID as part of the role identity. When a role value
  changes, remove its assignments, disable and delete the old definition, then
  create the canonical value with a new UUID and migrate the assignments. Entra
  can continue issuing the retired value when a definition reuses its old UUID;
  Veoveo does not accept that value.
- Keep the tenant-specific v2 issuer, authorization endpoint, token endpoint,
  and JWKS URI on the same directory tenant.
- Grant only `openid`, `profile`, and `email`. Browser authorization uses code
  flow with PKCE.
- Put the client secret in `VEOVEO_IDP_OIDC_CLIENT_SECRET`.

Validate control-plane changes before deployment:

```bash
cargo run -p veoveo-mcp-gateway --bin gateway -- \
  validate --control-plane examples/bioma/gateway.json
```

## Cleanup

Delete the local installation with:

```bash
just bioma-k3d-delete
```

Deleting the Bioma cluster disconnects the tunnel but does not delete its
Cloudflare tunnel or DNS records.
