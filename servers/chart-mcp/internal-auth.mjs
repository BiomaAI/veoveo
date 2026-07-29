import {
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";

const INTERNAL_TOKEN_ISSUER = "veoveo-internal";

export function loadInternalTokenVerifier(rawJwks, audience) {
  if (typeof audience !== "string" || audience.length === 0) {
    throw new Error("internal token verifier requires an audience");
  }
  if (typeof rawJwks !== "string" || rawJwks.trim().length === 0) {
    throw new Error("VEOVEO_INTERNAL_TRUST_JWKS is required");
  }
  let bundle;
  try {
    bundle = JSON.parse(rawJwks);
  } catch (error) {
    throw new Error(`VEOVEO_INTERNAL_TRUST_JWKS is invalid JSON: ${String(error)}`);
  }
  if (!Array.isArray(bundle.keys) || bundle.keys.length === 0) {
    throw new Error("VEOVEO_INTERNAL_TRUST_JWKS must contain verification keys");
  }
  const keys = new Map();
  for (const jwk of bundle.keys) {
    if (
      jwk == null ||
      typeof jwk !== "object" ||
      typeof jwk.kid !== "string" ||
      jwk.kid.length === 0 ||
      jwk.kty !== "OKP" ||
      jwk.crv !== "Ed25519" ||
      (jwk.alg != null && jwk.alg !== "EdDSA") ||
      (jwk.use != null && jwk.use !== "sig") ||
      (jwk.key_ops != null &&
        (!Array.isArray(jwk.key_ops) ||
          jwk.key_ops.length === 0 ||
          jwk.key_ops.some((operation) => operation !== "verify"))) ||
      typeof jwk.x !== "string" ||
      Buffer.from(jwk.x, "base64url").length !== 32
    ) {
      throw new Error("internal trust JWKS contains an unsupported verification key");
    }
    if (keys.has(jwk.kid)) {
      throw new Error(`internal trust JWKS repeats kid ${jwk.kid}`);
    }
    keys.set(
      jwk.kid,
      createPublicKey({
        key: jwk,
        format: "jwk",
      }),
    );
  }
  return (token) => verifyInternalToken(token, keys, audience);
}

function verifyInternalToken(token, keys, audience) {
  const segments = token.split(".");
  if (segments.length !== 3 || segments.some((segment) => segment.length === 0)) {
    throw new Error("internal token is not a compact JWT");
  }
  let header;
  let claims;
  try {
    header = JSON.parse(Buffer.from(segments[0], "base64url").toString("utf8"));
    claims = JSON.parse(Buffer.from(segments[1], "base64url").toString("utf8"));
  } catch (error) {
    throw new Error(`internal token has invalid JSON: ${String(error)}`);
  }
  if (header.alg !== "EdDSA" || typeof header.kid !== "string") {
    throw new Error("internal token requires EdDSA and kid");
  }
  const key = keys.get(header.kid);
  if (
    key == null ||
    !verifySignature(
      null,
      Buffer.from(`${segments[0]}.${segments[1]}`),
      key,
      Buffer.from(segments[2], "base64url"),
    )
  ) {
    throw new Error("internal token signature is invalid");
  }
  const now = Math.floor(Date.now() / 1000);
  if (
    claims.iss !== INTERNAL_TOKEN_ISSUER ||
    claims.aud !== audience ||
    claims.server !== audience ||
    typeof claims.sub !== "string" ||
    claims.sub.length === 0 ||
    typeof claims.jti !== "string" ||
    claims.jti.length === 0 ||
    typeof claims.profile !== "string" ||
    claims.profile.length === 0 ||
    claims.actor == null ||
    typeof claims.actor !== "object" ||
    claims.actor.id !== claims.sub ||
    claims.authority == null ||
    typeof claims.authority !== "object" ||
    !Number.isInteger(claims.iat) ||
    !Number.isInteger(claims.nbf) ||
    !Number.isInteger(claims.exp) ||
    claims.iat > now ||
    claims.nbf > now ||
    claims.exp <= now
  ) {
    throw new Error("internal token claims are invalid");
  }
  return claims;
}

export function requireInternalIdentity(request, response, verifyToken) {
  const authorization = request.headers.authorization;
  if (
    typeof authorization !== "string" ||
    !authorization.startsWith("Bearer ") ||
    authorization.length === "Bearer ".length
  ) {
    response
      .writeHead(401, {
        "content-type": "text/plain; charset=utf-8",
        "www-authenticate": "Bearer",
      })
      .end("gateway internal identity required");
    return false;
  }
  try {
    verifyToken(authorization.slice("Bearer ".length));
    return true;
  } catch {
    response
      .writeHead(401, {
        "content-type": "text/plain; charset=utf-8",
        "www-authenticate": "Bearer error=\"invalid_token\"",
      })
      .end("gateway internal identity rejected");
    return false;
  }
}
