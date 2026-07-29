import assert from "node:assert/strict";
import {
  generateKeyPairSync,
  sign,
} from "node:crypto";
import test from "node:test";

import { loadInternalTokenVerifier } from "./internal-auth.mjs";

function fixture() {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const kid = "chart-test";
  const jwk = publicKey.export({ format: "jwk" });
  const verifier = loadInternalTokenVerifier(
    JSON.stringify({
      keys: [{ ...jwk, kid, alg: "EdDSA", use: "sig", key_ops: ["verify"] }],
    }),
    "charts",
  );
  return {
    verifier,
    token(overrides = {}) {
      const now = Math.floor(Date.now() / 1000);
      const header = { alg: "EdDSA", kid, typ: "JWT" };
      const claims = {
        iss: "veoveo-internal",
        sub: "principal-1",
        aud: "charts",
        exp: now + 60,
        nbf: now,
        iat: now,
        jti: "token-1",
        profile: "operator",
        server: "charts",
        actor: { id: "principal-1", kind: "user" },
        authority: { kind: "direct" },
        ...overrides,
      };
      const signingInput = [
        Buffer.from(JSON.stringify(header)).toString("base64url"),
        Buffer.from(JSON.stringify(claims)).toString("base64url"),
      ].join(".");
      const signature = sign(null, Buffer.from(signingInput), privateKey);
      return `${signingInput}.${signature.toString("base64url")}`;
    },
  };
}

test("accepts the gateway Ed25519 identity for charts", () => {
  const { verifier, token } = fixture();
  assert.equal(verifier(token()).server, "charts");
});

test("rejects a token for another server", () => {
  const { verifier, token } = fixture();
  assert.throws(() => verifier(token({ aud: "map", server: "map" })));
});

test("rejects an expired token", () => {
  const { verifier, token } = fixture();
  assert.throws(() => verifier(token({ exp: 1 })));
});
