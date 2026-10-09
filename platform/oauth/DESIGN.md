# OAuth Client Assertions

## Standards And Protocols

| Standard | Profile |
| --- | --- |
| OAuth 2.0, RFC 6749 | Client credentials assertions; grant transport belongs to callers |
| RFC 7523 section 3.2 | RSA JWT client authentication with equal client `iss` and `sub` |
| JWT RFC 7519 and JWS RFC 7515 | RS256 only, public `kid`, fresh UUID `jti`, Unix timestamps |
| jsonwebtoken 11.0.0 | Workspace-pinned Rust crypto implementation and PEM admission |

The library signs assertions without HTTP, database or authorization dependencies.
Owners admit credential files and token endpoints. Worker assertions take an admitted
OAuth client identity and canonical HTTPS token endpoint, expire after 60 seconds,
and generate a new assertion identifier on every call. Secret results expose text
only through an explicit method and clear their owned text on drop.

An explicit diagnostic signing method preserves the utility owner's chosen audience,
expiration and assertion identifier. It shares the same RS256 signing implementation.
It provides no endpoint transport policy or permission to dispatch a diagnostic assertion.
The utility confines its public fixture key to loopback endpoints; installed owners
provide their own private key. Public key identifiers use one admitted type.
