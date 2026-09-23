import assert from "node:assert/strict";
import test from "node:test";
import { appForRoute, consoleAppRoute, resolveAppLink } from "./apps/links.ts";
import type { AppDescriptor } from "./types.ts";

const app = {
  server: "view",
  resourceUri: "ui://view/preview.html",
  standalonePath: "/apps/view/preview.html",
  name: "view-preview",
  tools: [],
  resourceDependencies: [],
  toolDependencies: [],
  agentMessageTargets: [],
} satisfies AppDescriptor;

test("Console app navigation and reload resolve extension-free catalog routes", () => {
  assert.equal(consoleAppRoute(app), "#/apps/view/preview");
  assert.equal(appForRoute("view/preview", [app]), app);
  assert.equal(appForRoute("view/preview.html", [app]), undefined);
  assert.equal(appForRoute("view/missing", [app]), undefined);
  assert.equal(appForRoute("view/preview", [app, { ...app, resourceUri: "ui://view/preview" }]), undefined);
  const nested = { ...app, resourceUri: "ui://datasheet/admin/workbench.html" };
  assert.equal(consoleAppRoute(nested), "#/apps/datasheet/admin/workbench");
  assert.equal(appForRoute("datasheet/admin/workbench", [nested]), nested);
});

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
