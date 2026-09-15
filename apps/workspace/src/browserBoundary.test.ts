import test from "node:test";
import assert from "node:assert/strict";
import { browserApiRoot, browserLoginRoot, configureBrowserApplication } from "../../console/web/src/browserApp.ts";
import { browserJson } from "../../console/web/src/browserHttp.ts";
import { browserSession } from "../../console/web/src/csrf.ts";

test("shared browser components require explicit immutable application authority", async () => {
  assert.throws(() => browserApiRoot(), /not been initialized/);
  configureBrowserApplication("workspace");
  assert.equal(browserApiRoot(), "/workspace/api");
  assert.equal(browserLoginRoot(), "/workspace/auth/login");
  assert.throws(() => configureBrowserApplication("console"), /cannot change/);
  const original = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async (url, options) => {
    calls++;
    assert.equal(url, "/workspace/api/computers");
    assert.equal(options?.credentials, "same-origin");
    assert.equal(options?.redirect, "error");
    const headers = new Headers(options?.headers);
    assert.equal(headers.get("x-veoveo-csrf-token"), "workspace-csrf");
    assert.equal(headers.get("authorization"), null);
    return Response.json({ admitted: true }, { headers: { "x-veoveo-csrf-token": "rotated" } });
  };
  try {
    browserSession.csrfToken = undefined;
    await assert.rejects(browserJson("computers", {}), /session is not ready/);
    assert.equal(calls, 0);
    browserSession.csrfToken = "workspace-csrf";
    assert.deepEqual(await browserJson("computers", {}), { admitted: true });
    assert.equal(browserSession.csrfToken, "rotated");
    assert.equal(calls, 1);
  } finally { globalThis.fetch = original; browserSession.csrfToken = undefined; }
});
