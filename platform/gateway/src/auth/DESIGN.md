# Gateway Token And Session Authority

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| OAuth 2.0 / OpenID Connect | Registered clients, exact issuer/resource and Work Context selection; existing authorization-code, refresh, client-credentials and ID-JAG flows |
| JWT, RFC 7519 | Signed access tokens with the installation's qualified asymmetric algorithms; issuer, audience and time validation |
| RFC 7009 / RFC 9700 | Refresh-family revocation and rotation; current family checks also deny bound access tokens |
| Veoveo `session_family` claim | Repository-owned optional UUIDv7 refresh-family identity; it is signed metadata, not a bearer credential or standardized device authorization |
| Veoveo internal `request_context` | Verified source principal and token metadata carried into a signed upstream assertion; the source token bounds that assertion's lifetime |
| SurrealDB 3.2.4 | Shared refresh family, replay, JWT revocation and audit records; no process-local positive revocation cache |

An authorization-code exchange creates its refresh family before signing the access
token. When that client supports refresh, the token carries `session_family`. Each
rotation preserves the same family identity. Client-credentials and ID-JAG exchanges
do not acquire a browser session family.

After JWT validation and current Work Context resolution, gateway authentication
reads a bound family and checks its authorization server, profile, client, context,
principal identity, tenant, scope and lifetime. A missing, expired or revoked family
denies access. Database failure returns unavailable, and the read has a five-second
deadline. Individual JWT revocation remains enforced. The request middleware does
not extend a token's signed expiration.

The refresh family retains the upstream IdP principal. JWT verification records the
installation authorization-server issuer in `Principal.issuer` and preserves the
original `principal_id`. Family matching therefore binds the canonical principal ID,
subject, kind and tenant while checking the verified principal against the signed
token issuer. Equating the IdP issuer with the gateway token issuer would reject a
valid federated login.

The pure family predicate lives in `platform/policy/src/session.rs`. The gateway and
Computers use that same read-only projection of the stored record. Database identity
and read deadlines remain the caller's responsibility; display-name decoding is outside
the authority predicate. Refresh issuance and display projections retain their owning
gateway types.

Logout already revokes the refresh family before the Console clears its session.
The signed family claim extends that revocation to subsequent bound access-token
requests on every replica. Refresh-token replay has the same effect. This follows
the related-token revocation policy described in
[RFC 7009 section 2.1](https://www.rfc-editor.org/rfc/rfc7009.html#section-2.1).
Rotation and replay protections follow
[RFC 9700 section 4.14](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14).

Computer attachment renewal must require an explicit verified family binding and
recheck it under the attachment's own bounded lease. A normal access-token request
is not that lease. Logout does not authorize stopping the Computer or cancelling
accepted background execution.

Authenticated MCP and HTTP proxies propagate `GatewayRequestContext`. The source
principal is the JWT-verified principal before delegated actor derivation. The
shared contract checks internal consistency at issuance and verification. The
gateway creates request authority for each upstream request while reusing only the
transport pool. No bearer token is stored in the context. Computers must persist a
separate accepted authorization and re-evaluate current policy before dispatch or
grant renewal; the assertion alone cannot extend access after its source expires.

## Rollout And Qualification

The gateway owns this claim extension. Existing family records need no migration.
Deploy all gateway readers and issuers before admitting session-bound Computer
grants. Refresh gives an existing browser session the new signed binding; a token
without it cannot create a session-bound Computer grant. Such tokens retain only
their existing short-lived request authority and individual JWT revocation. No
family is inferred from the principal, token hash or browser cookie. Service tokens
remain explicitly unbound.

The selected release must show that all gateway replicas reject family revocation.
Rollback to readers that ignore this claim is incompatible while session-bound
Computer grants are admitted; drain/revoke those grants before an older deployment.
Installed acceptance must also verify Computer transport closure, whose thirty-second
bound is governed by the Computers runtime. This document does not claim that the
request middleware alone closes existing streams.

Local evidence includes signed claim validation and isolated real-store issue,
rotation, replay, logout, expiry and missing-record checks across independent clients.
Tests reject mismatched client, context, tenant, principal, profile, authorization
server and scope. They do not establish public ingress or installed attachment UX.
