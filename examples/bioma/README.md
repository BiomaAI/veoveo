# Bioma deployment example

This directory owns the `veoveo.bioma.ai` installation profile. It is separate
from the SUMO development cluster and showcase. The public hostname reaches an
isolated k3d cluster through a remote-managed Cloudflare Tunnel.

| Installation | k3d cluster | Kubernetes context | Host projection |
|---|---|---|---|
| SUMO development | `veoveo-sumo` | `k3d-veoveo-sumo` | `http://localhost:8780` |
| Bioma public edge | `veoveo-bioma` | `k3d-veoveo-bioma` | `http://localhost:8781` |

Every repository recipe passes its Kubernetes context explicitly. Creating the
second cluster may change kubectl's current context, but it cannot redirect a
SUMO or Bioma recipe into the other installation.

## Profile ownership

- `values.yaml` owns Bioma's public origins and gateway ConfigMap identity.
- `gateway.json` owns the Entra application, tenant mapping, and MCP profiles.
- `k3d.yaml` owns the second cluster and its non-conflicting loopback port.
- `k3d-values.yaml` reduces the local proof to the core public edge. It does not
  alter the fielded Bioma values.
- `cloudflare-tunnel.json` is the desired remote tunnel ingress configuration.
- `tunnel.yaml` runs the connector inside the Bioma cluster.

The k3d proof deploys SurrealDB, RustFS, the gateway, the artifact service, and
the console BFF. Domain MCP workloads and Recording Hub remain absent from this
small profile. The Bioma gateway catalog stays canonical and can serve those
workloads when their deployment overlay is installed.

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
`CLOUDFLARE_API_TOKEN`, and `CLOUDFLARED_TUNNEL_TOKEN`. The account token needs
Tunnel:Edit and DNS:Edit for this account and zone. The tunnel token is stored
only in the `bioma-cloudflared` Kubernetes Secret.

## Start both clusters

Build the shared GPU-capable node once. Then start the SUMO installation:

```bash
just k3d-node-build
just sumo-k3d-create
just showcase-sumo-build
just showcase-sumo-import
just showcase-sumo-resources
just showcase-sumo-platform-up
just showcase-sumo-up
```

Start Bioma without stopping SUMO:

```bash
just bioma-k3d-create
just bioma-build
just bioma-import
just bioma-resources
just bioma-platform-up
just bioma-tunnel-up
```

`bioma-resources` reads the required Veoveo and Cloudflare values from `.env`,
applies Kubernetes Secrets over stdin, and never writes their plaintext to a
repository or temporary manifest.

## Acceptance

Run both checks while both clusters are active:

```bash
just showcase-sumo-verify
just bioma-verify
just clusters-status
```

The Bioma check requires an available SUMO MCP deployment in the first context,
an available gateway and tunnel connector in the second, a healthy public edge,
and the Bioma authorization-server key at the public JWKS endpoint.

## Entra application registration

`gateway.json` uses one single-tenant Microsoft Entra application as the
external OIDC provider. Its registration must match the control plane:

- Register `https://veoveo.bioma.ai/oauth/callback` as a Web redirect URI.
- Create the app roles `veoveo_operator` and `veoveo_admin`, allow user or group
  assignment, and assign at least one role to every user who can sign in.
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

Each cluster has an independent destructive command:

```bash
just bioma-k3d-delete
just sumo-k3d-delete
```

Deleting the Bioma cluster disconnects the tunnel but does not delete its
Cloudflare tunnel or DNS records.
