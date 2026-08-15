import assert from "node:assert/strict";
import test from "node:test";
import { resolveAppLink } from "./apps/links.ts";
import type { AppDescriptor } from "./types.ts";

const app = {
  server: "view",
  resourceUri: "ui://view/preview.html",
  standalonePath: "/apps/view/preview.html",
  name: "view-preview",
  tools: [],
  resourceDependencies: [],
  agentMessageTargets: [],
} satisfies AppDescriptor;

test("app links resolve only exact cataloged resources", () => {
  assert.deepEqual(resolveAppLink(app.resourceUri, [app]), { kind: "app", app });
  assert.equal(resolveAppLink("ui://view/missing.html", [app]), undefined);
  assert.equal(resolveAppLink("ui://view/../preview.html", [app]), undefined);
});

test("platform links use a closed allowlist", () => {
  assert.deepEqual(resolveAppLink("veoveo-console://agents", []), {
    kind: "platform",
    view: "agents",
  });
  assert.deepEqual(resolveAppLink("veoveo-console://recordings", []), {
    kind: "platform",
    view: "recordings",
  });
  assert.equal(resolveAppLink("veoveo-console://cluster", []), undefined);
});
