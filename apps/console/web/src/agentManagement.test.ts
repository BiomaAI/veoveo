import assert from "node:assert/strict";
import test from "node:test";
import { AgentApi, AgentApiError } from "./agent-management/api.ts";
import { browserSession } from "./csrf.ts";

test("authoring keeps application profile, CSRF rotation and typed publication findings", async () => {
  const original = globalThis.fetch;
  browserSession.csrfToken = "initial";
  let calls = 0;
  const digest = `sha256:${"a".repeat(64)}`;
  globalThis.fetch = async (input, options) => {
    calls++;
    assert.equal(input, "/workspace/api/agent-definitions/researcher/publish");
    const headers = options?.headers as Record<string, string>;
    assert.equal(headers["X-Veoveo-CSRF-Token"], "initial");
    assert.equal(headers.Authorization, undefined);
    assert.equal(options?.credentials, "same-origin");
    return Response.json({ revision: 2, digest, findings: [{ code: "model_changed", field: "model", message: "Choose the current connection." }] }, { status: 422, headers: { "x-veoveo-csrf-token": "rotated" } });
  };
  try {
    await assert.rejects(new AgentApi("workspace").publish("researcher", { requestId: "01996a11-1111-7111-8111-111111111111", expectedRevision: 2, digest, audience: ["shared"] }), (error: unknown) => {
      assert.ok(error instanceof AgentApiError);
      assert.equal(error.validation?.findings[0].code, "model_changed");
      return true;
    });
    assert.equal(calls, 1);
    assert.equal(browserSession.csrfToken, "rotated");
    globalThis.fetch = async () => Response.json({ items: [{ id: "researcher", name: "Researcher", description: "Description", owner: "01996a11-1111-7111-8111-111111111111", workContext: "shared", revision: 1, status: "enabled", disabled: false, draftDigest: digest, publishedDigest: null, audience: [], updatedAt: "2026-09-19T12:00:00Z" }], next: null });
    const page = await new AgentApi("console").list();
    assert.equal(page.items[0].publishedDigest, null);
    assert.equal(page.next, null);
    browserSession.csrfToken = undefined;
    await assert.rejects(new AgentApi("workspace").status("researcher", "disable", { requestId: "01996a11-1111-7111-8111-111111111111", expectedRevision: 1 }), /Sign in/);
  } finally { globalThis.fetch = original; browserSession.csrfToken = undefined; }
});
