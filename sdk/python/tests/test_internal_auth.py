import base64
import json
import time
import uuid

import jwt as pyjwt
import pytest
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    NoEncryption,
    PrivateFormat,
    PublicFormat,
)

from veoveo_mcp.internal_auth import (
    GatewayInternalTokenVerifier,
    GatewayInternalTrustBundle,
    InternalTokenError,
    bearer_from_header,
)

KEY_ID = "veoveo-internal-1"
ISSUER = "veoveo-internal"


def _keypair():
    private = Ed25519PrivateKey.generate()
    public = private.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    x = base64.urlsafe_b64encode(public).rstrip(b"=").decode()
    jwks = {
        "keys": [
            {"kty": "OKP", "crv": "Ed25519", "alg": "EdDSA", "kid": KEY_ID, "x": x}
        ]
    }
    pem = private.private_bytes(
        Encoding.PEM, PrivateFormat.PKCS8, NoEncryption()
    ).decode()
    return pem, jwks


def _principal(subject: str = "conformance") -> dict:
    principal_id = f"https://conformance.veoveo.local#{subject}"
    return {
        "id": principal_id,
        "kind": "service",
        "issuer": "https://conformance.veoveo.local",
        "subject": subject,
        "tenant": "local",
        "groups": [],
        "roles": [],
        "scopes": ["operator:use"],
        "data_labels": [],
        "assurances": [],
    }


def _authority(
    actor: dict,
    *,
    mode: str = "automated",
    work_context: str = "operations",
) -> dict:
    provenance: dict = {"mode": mode}
    if mode in {"direct", "delegated"}:
        provenance["initiator"] = actor["id"]
    if mode == "delegated":
        provenance["delegation_id"] = "delegation-019f"
    return {
        "work_context": work_context,
        "tenant": actor["tenant"],
        "membership": "contributor",
        "policy_revision": "r1",
        "output_policy": {
            "owner": {"kind": "group", "id": work_context},
            "initial_grants": [
                {
                    "subject": {"kind": "group", "id": work_context},
                    "level": "read",
                }
            ],
            "data_labels": [],
        },
        "provenance": provenance,
    }


def _claims(server: str = "datasheet", **overrides) -> dict:
    now = int(time.time())
    actor = overrides.pop("actor", _principal())
    authority = overrides.pop("authority", _authority(actor))
    claims = {
        "iss": ISSUER,
        "sub": actor["id"],
        "aud": server,
        "exp": now + 300,
        "nbf": now - 5,
        "iat": now - 5,
        "jti": str(uuid.uuid4()),
        "profile": "operator",
        "server": server,
        "actor": actor,
        "authority": authority,
    }
    claims.update(overrides)
    return claims


def _token(pem: str, claims: dict, kid: str | None = KEY_ID) -> str:
    headers = {"kid": kid} if kid else {}
    return pyjwt.encode(claims, pem, algorithm="EdDSA", headers=headers)


def _verifier(jwks: dict, server: str = "datasheet") -> GatewayInternalTokenVerifier:
    return GatewayInternalTokenVerifier.for_server(
        ISSUER, server, GatewayInternalTrustBundle.from_json(json.dumps(jwks))
    )


def test_verifies_a_valid_gateway_assertion():
    pem, jwks = _keypair()
    identity = _verifier(jwks).verify(_token(pem, _claims()))
    assert identity.server == "datasheet"
    assert identity.profile == "operator"
    assert identity.actor.kind.value == "service"
    assert identity.actor.tenant == "local"
    assert "operator:use" in identity.actor.scopes
    assert identity.authority.work_context == "operations"
    assert identity.authority.provenance.mode == "automated"


@pytest.mark.parametrize("mode", ["direct", "delegated", "automated"])
def test_verifies_every_invocation_mode(mode: str):
    pem, jwks = _keypair()
    actor = _principal()
    claims = _claims(actor=actor, authority=_authority(actor, mode=mode))
    identity = _verifier(jwks).verify(_token(pem, claims))
    assert identity.authority.provenance.mode == mode


def test_rejects_wrong_audience_and_issuer():
    pem, jwks = _keypair()
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(server="media")))
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(iss="not-the-gateway")))


def test_rejects_missing_or_unknown_kid():
    pem, jwks = _keypair()
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(), kid=None))
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(), kid="other-key"))


def test_rejects_subject_principal_mismatch():
    pem, jwks = _keypair()
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(sub="someone-else")))


def test_rejects_expired_and_not_yet_valid_tokens():
    pem, jwks = _keypair()
    now = int(time.time())
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(exp=now - 10)))
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(pem, _claims(nbf=now + 60)))


def test_rejects_untrusted_signer():
    pem, jwks = _keypair()
    other_pem, _ = _keypair()
    with pytest.raises(InternalTokenError):
        _verifier(jwks).verify(_token(other_pem, _claims()))
    _verifier(jwks).verify(_token(pem, _claims()))


def test_trust_bundle_is_fail_closed():
    with pytest.raises(InternalTokenError):
        GatewayInternalTrustBundle.from_json(json.dumps({"keys": []}))
    with pytest.raises(InternalTokenError):
        GatewayInternalTrustBundle.from_json(
            json.dumps({"keys": [{"kty": "RSA", "alg": "RS256", "kid": "k"}]})
        )
    _, jwks = _keypair()
    doubled = {"keys": jwks["keys"] * 2}
    with pytest.raises(InternalTokenError):
        GatewayInternalTrustBundle.from_json(json.dumps(doubled))


def test_bearer_header_parsing_is_strict():
    assert bearer_from_header("Bearer abc.def.ghi") == "abc.def.ghi"
    with pytest.raises(InternalTokenError):
        bearer_from_header("Basic abc")
    with pytest.raises(InternalTokenError):
        bearer_from_header("Bearer ")
    with pytest.raises(InternalTokenError):
        bearer_from_header("Bearer two tokens")
