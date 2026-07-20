"""Gateway internal token verification for hosted Python MCP servers.

Mirrors the Rust `mcp-contract` internal-auth module: the gateway alone signs
short-lived Ed25519 (EdDSA) identity assertions; hosted servers receive a
public JWKS trust bundle, require a `kid`, and never hold the private key.
"""

from __future__ import annotations

import base64
import json
from datetime import datetime, timezone
from typing import Any, Awaitable, Callable

import jwt as pyjwt
from pydantic import ValidationError

from .contract.identity import GatewayInternalIdentity, InvocationAuthority, Principal

Scope = dict[str, Any]
Receive = Callable[[], Awaitable[dict[str, Any]]]
Send = Callable[[dict[str, Any]], Awaitable[None]]
AsgiApp = Callable[[Scope, Receive, Send], Awaitable[None]]

IDENTITY_SCOPE_KEY = "veoveo.internal_identity"
BEARER_SCOPE_KEY = "veoveo.forwarded_bearer"

_REQUIRED_CLAIMS = ["exp", "iss", "aud", "sub", "iat", "nbf", "jti"]


class InternalTokenError(Exception):
    pass


class GatewayInternalTrustBundle:
    """Validated Ed25519 verification keys, indexed by `kid`."""

    def __init__(self, jwks: dict[str, Any]) -> None:
        keys = jwks.get("keys")
        if not isinstance(keys, list) or not keys:
            raise InternalTokenError("internal trust JWKS contains no keys")
        self._keys: dict[str, pyjwt.PyJWK] = {}
        for jwk in keys:
            _validate_verification_jwk(jwk)
            key_id = _validate_key_id(jwk.get("kid"))
            if key_id in self._keys:
                raise InternalTokenError(
                    f"internal trust JWKS contains duplicate kid `{key_id}`"
                )
            self._keys[key_id] = pyjwt.PyJWK(jwk, algorithm="EdDSA")

    @classmethod
    def from_json(cls, value: str) -> "GatewayInternalTrustBundle":
        try:
            jwks = json.loads(value)
        except json.JSONDecodeError as error:
            raise InternalTokenError(f"invalid internal trust JWKS: {error}") from error
        return cls(jwks)

    def key(self, key_id: str) -> pyjwt.PyJWK:
        key = self._keys.get(key_id)
        if key is None:
            raise InternalTokenError(
                f"internal token references unknown kid `{key_id}`"
            )
        return key

    def key_ids(self) -> list[str]:
        return list(self._keys)


def _validate_key_id(value: Any) -> str:
    if (
        not isinstance(value, str)
        or not value
        or value.strip() != value
        or any(ch for ch in value if ch < " " or ch == "\x7f")
    ):
        raise InternalTokenError("internal signing key id is empty or invalid")
    return value


def _validate_verification_jwk(jwk: dict[str, Any]) -> None:
    if jwk.get("alg") != "EdDSA":
        raise InternalTokenError(
            "internal trust JWKS must contain Ed25519 verification keys"
        )
    use = jwk.get("use")
    if use is not None and use != "sig":
        raise InternalTokenError(
            "internal trust JWKS must contain Ed25519 verification keys"
        )
    key_ops = jwk.get("key_ops")
    if key_ops is not None and (not key_ops or any(op != "verify" for op in key_ops)):
        raise InternalTokenError(
            "internal trust JWKS must contain Ed25519 verification keys"
        )
    if jwk.get("kty") != "OKP" or jwk.get("crv") != "Ed25519":
        raise InternalTokenError(
            "internal trust JWKS must contain Ed25519 verification keys"
        )
    x = jwk.get("x")
    try:
        decoded = base64.urlsafe_b64decode(x + "=" * (-len(x) % 4)) if x else b""
    except (TypeError, ValueError):
        decoded = b""
    if len(decoded) != 32:
        raise InternalTokenError(
            "internal trust JWKS must contain Ed25519 verification keys"
        )


class GatewayInternalTokenVerifier:
    def __init__(
        self,
        issuer: str,
        audiences: list[str],
        trust_bundle: GatewayInternalTrustBundle,
    ) -> None:
        if not audiences:
            raise InternalTokenError("internal token verifier requires an audience")
        self.issuer = issuer
        self.audiences = audiences
        self.trust_bundle = trust_bundle

    @classmethod
    def for_server(
        cls, issuer: str, server_slug: str, trust_bundle: GatewayInternalTrustBundle
    ) -> "GatewayInternalTokenVerifier":
        return cls(issuer, [server_slug], trust_bundle)

    def verify(self, bearer_token: str) -> GatewayInternalIdentity:
        try:
            header = pyjwt.get_unverified_header(bearer_token)
        except pyjwt.InvalidTokenError as error:
            raise InternalTokenError(
                f"internal token JWT validation failed: {error}"
            ) from error
        if header.get("alg") != "EdDSA":
            raise InternalTokenError(
                f"internal token algorithm `{header.get('alg')}` is not EdDSA"
            )
        key_id = header.get("kid")
        if key_id is None:
            raise InternalTokenError("internal token or trust key is missing kid")
        key = self.trust_bundle.key(key_id)
        try:
            claims = pyjwt.decode(
                bearer_token,
                key,
                algorithms=["EdDSA"],
                issuer=self.issuer,
                audience=self.audiences,
                leeway=0,
                options={"require": _REQUIRED_CLAIMS},
            )
        except pyjwt.InvalidTokenError as error:
            raise InternalTokenError(
                f"internal token JWT validation failed: {error}"
            ) from error
        server = claims.get("server")
        if server not in self.audiences:
            raise InternalTokenError(
                f"internal token audience mismatch: got `{server}`"
            )
        try:
            actor = Principal.model_validate(claims["actor"])
            authority = InvocationAuthority.model_validate(claims["authority"])
        except (KeyError, ValidationError) as error:
            raise InternalTokenError(
                f"internal token identity is invalid: {error}"
            ) from error
        if claims["sub"] != actor.id:
            raise InternalTokenError(
                "internal token subject does not match embedded actor"
            )
        return GatewayInternalIdentity(
            issuer=claims["iss"],
            profile=claims["profile"],
            server=server,
            actor=actor,
            authority=authority,
            jwt_id=claims["jti"],
            issued_at=_timestamp(claims["iat"], "iat"),
            not_before=_timestamp(claims["nbf"], "nbf"),
            expires_at=_timestamp(claims["exp"], "exp"),
        )


def _timestamp(value: Any, claim: str) -> datetime:
    try:
        return datetime.fromtimestamp(int(value), tz=timezone.utc)
    except (TypeError, ValueError, OSError) as error:
        raise InternalTokenError(
            f"internal token claim `{claim}` has invalid timestamp `{value}`"
        ) from error


def bearer_from_header(header: str) -> str:
    scheme, _, token = header.partition(" ")
    if not scheme.lower() == "bearer":
        raise InternalTokenError("authorization scheme must be Bearer")
    if not token or any(ch.isspace() for ch in token):
        raise InternalTokenError("bearer token contains invalid whitespace")
    return token


class InternalAuthMiddleware:
    """Reject requests without a valid gateway assertion; stash the identity.

    Downstream layers read `IDENTITY_SCOPE_KEY` (the parsed
    `GatewayInternalIdentity`) and `BEARER_SCOPE_KEY` (the forwarded bearer)
    from the ASGI scope.
    """

    def __init__(
        self,
        app: AsgiApp,
        verifier: GatewayInternalTokenVerifier,
        logger: Callable[[str], None] | None = None,
    ) -> None:
        self.app = app
        self.verifier = verifier
        self.logger = logger

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return
        header = None
        for name, value in scope.get("headers", []):
            if name.decode("latin-1").lower() == "authorization":
                header = value.decode("latin-1")
                break
        try:
            if header is None:
                raise InternalTokenError("missing internal authorization")
            token = bearer_from_header(header)
            identity = self.verifier.verify(token)
        except InternalTokenError as error:
            if self.logger is not None:
                self.logger(f"rejected MCP request: {error}")
            body = b"invalid gateway authorization"
            await send(
                {
                    "type": "http.response.start",
                    "status": 401,
                    "headers": [
                        (b"content-type", b"text/plain; charset=utf-8"),
                        (b"content-length", str(len(body)).encode()),
                    ],
                }
            )
            await send({"type": "http.response.body", "body": body})
            return
        scope[IDENTITY_SCOPE_KEY] = identity
        scope[BEARER_SCOPE_KEY] = token
        await self.app(scope, receive, send)
