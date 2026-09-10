import assert from "node:assert/strict";
import test from "node:test";
import { acceptConsoleCsrfToken } from "./csrf.ts";
import { pairCli, pairingLocation } from "./computers/pairing.ts";

const computerId = "00000000-0000-4000-8000-000000000001";
const pairingId = "00000000-0000-4000-8000-000000000002";
const grantId = "00000000-0000-4000-8000-000000000003";
const base = `https://veoveo.test/console/computers/${computerId}/auth/connect`;
const location = { computerId, callbackPort: 49152, code: "ABC-2345" };
test("pairing accepts only one exact local callback port and stock comparison code", () => {
  assert.deepEqual(pairingLocation(new URL(`${base}?callback_port=49152&code=ABC-2345`)), location);
  for (const query of ["callback_port=22&code=ABC-2345", "callback_port=65536&code=ABC-2345", "callback_port=49152&code=ABC-1234",
    "callback_port=49152&code=ABC-2345&host=foreign", "callback_port=49152&code=ABC-2345&code=DEF-2345",
    "callback_port=49152&code=ABC-2345#injected", "callback_port=+49152&code=ABC-2345", "callback_port=049152&code=ABC-2345"]) {
    assert.equal(pairingLocation(new URL(`${base}?${query}`)), undefined);
  }
});
test("pairing keeps credentials in the one local callback and revokes failed delivery", async () => {
  const previous = globalThis.fetch;
  const token = `vcli1.${grantId}.${"a".repeat(64)}`;
  acceptConsoleCsrfToken("fixture-csrf");
  try {
    for (const failDelivery of [false, true]) {
      const calls: string[] = [];
      globalThis.fetch = async (input, init) => {
        const path = String(input);
        calls.push(path);
        assert.ok(!path.includes(token));
        if (path.startsWith("http://")) {
          assert.equal(path, "http://127.0.0.1:49152/callback");
          assert.equal(init?.credentials, "omit");
          assert.equal(init?.redirect, "error");
          if (init?.method === "OPTIONS") {
            assert.equal(calls.length, 1, "permission must resolve before access issuance");
            assert.equal(init.body, undefined);
            return new Response(null, { status: 204 });
          }
          assert.deepEqual(JSON.parse(String(init?.body)), { token, code: location.code });
          if (failDelivery) throw new Error("local connection unavailable");
          return Response.json({ ok: true });
        }
        assert.equal(init?.credentials, "same-origin");
        assert.equal(new Headers(init?.headers).get("x-veoveo-csrf-token"), "fixture-csrf");
        if (path.endsWith("/revoke")) return Response.json({ computerId, grantId, revoked: true });
        const expiresAt = new Date(Date.now() + 60000).toISOString();
        if (path.endsWith("/confirm")) return Response.json({ computerId, pairingId, grantId, expiresAt, callbackPort: 49152, token });
        return Response.json({ computerId, pairingId, expiresAt });
      };
      if (failDelivery) {
        await assert.rejects(pairCli(location, "Laptop"), /access was revoked/);
        assert.equal(calls.at(-1), `/console/api/computers/${computerId}/access/${grantId}/revoke`);
      } else {
        await pairCli(location, "Laptop");
        assert.equal(calls.length, 4);
      }
    }
  } finally { globalThis.fetch = previous; }
});

test("denied local permission or unreachable CLI issues no access", async () => {
  const previous = globalThis.fetch;
  try {
    for (const reject of [false, true]) {
      let calls = 0;
      globalThis.fetch = async (input, init) => {
        calls++;
        assert.equal(String(input), "http://127.0.0.1:49152/callback");
        assert.equal(init?.method, "OPTIONS");
        assert.equal(init.body, undefined);
        if (reject) throw new TypeError("permission denied");
        return new Response(null, { status: 403 });
      };
      await assert.rejects(pairCli(location, "Laptop"), /No access was issued/);
      assert.equal(calls, 1);
    }
  } finally { globalThis.fetch = previous; }
});
