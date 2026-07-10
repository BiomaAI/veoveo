# Bioma deployment example

This directory is one operator-owned Veoveo installation. It is not a Veoveo
service dependency and is not the canonical deployment topology.

The overlay replaces the generic gateway control plane with Bioma's Entra
configuration and adds a Cloudflare Tunnel in front of the canonical Compose
edge:

```sh
docker compose \
  -f compose.yaml \
  -f examples/bioma/compose.yaml \
  --profile tunnel \
  up --build -d
```

Populate the canonical installation secrets plus the values documented in
`.env.example`. Cloudflare and Entra credentials belong only to this example.
No Bioma hostname or credential is required by Veoveo itself.

## Entra application registration

`gateway.json` uses one single-tenant Microsoft Entra application as the
external OIDC provider. Its registration must match the control plane:

- Register `https://veoveo.bioma.ai/oauth/callback` as a **Web** redirect URI.
- Create the app roles `veoveo_operator` and `veoveo_admin`, allow user/group
  assignment, and assign at least one of them to every user who can sign in.
  Entra emits those assigned values in the ID token's `roles` claim; Veoveo's
  operator and admin policies require the corresponding value.
- Keep the tenant-specific v2 issuer, authorization endpoint, token endpoint,
  and JWKS URI on the same directory tenant. Veoveo maps the standard `oid`
  subject and `tid` tenant claims. A token from any other Entra tenant is
  rejected by the issuer and tenant mapping.
- Grant only the OIDC scopes used here: `openid`, `profile`, and `email`.
  Authorization code with PKCE is used for the browser flow; no implicit grant
  is required.
- Put the client secret in `VEOVEO_IDP_OIDC_CLIENT_SECRET`. Do not add it to
  `gateway.json` or the Compose overlay.

When adapting this example, replace both the Entra tenant UUID and client UUID,
then update the tenant mapping as one change. Validate the result before
deployment:

```sh
cargo run -p veoveo-mcp-gateway --bin gateway -- \
  validate --control-plane examples/bioma/gateway.json
```

The rest of `gateway.json` intentionally mirrors the canonical self-hosted
control plane. Bioma-specific differences are limited to Entra, tenant IDs,
public URLs, and the service-client JWKS locations.
