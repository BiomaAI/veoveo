import assert from "node:assert/strict";
import test from "node:test";

import {
  appServerTitle,
  groupAppsByServer,
  namespacedAppTitle,
  selectMountedApp,
  unavailableAppServers,
} from "./apps/catalogPresentation.ts";
import type { AppCatalogDegradation, AppDescriptor } from "./types.ts";

test("only incomplete discovery of the selected App preserves its mounted descriptor", () => {
  const mounted: AppDescriptor = {
    server: "map", resourceUri: "ui://map/workspace.html", standalonePath: "/apps/map/workspace",
    name: "Map Explorer", tools: [], resourceDependencies: [], toolDependencies: [], agentMessageTargets: [],
  };
  const changed = { ...mounted, title: "Updated App" };
  const pending: AppCatalogDegradation = { server: "map", surface: "resources", code: "discovery_pending" };
  const uri = mounted.resourceUri;
  assert.equal(selectMountedApp(uri, { apps: [], degradations: [pending] }, mounted), mounted);
  assert.equal(selectMountedApp(uri, { apps: [changed], degradations: [{ ...pending, surface: "tools" }] }, mounted), mounted);
  assert.equal(selectMountedApp(uri, { apps: [changed], degradations: [] }, mounted), changed);
  assert.equal(selectMountedApp(uri, { apps: [], degradations: [] }, mounted), undefined);
  assert.equal(selectMountedApp(uri, { apps: [], degradations: [{ ...pending, surface: "tools" }] }, mounted), undefined);
  assert.equal(selectMountedApp(uri, { apps: [], degradations: [{ ...pending, server: "media" }] }, mounted), undefined);
  assert.equal(selectMountedApp("ui://map/another.html", { apps: [], degradations: [pending] }, mounted), undefined);
  assert.equal(selectMountedApp(undefined, { apps: [], degradations: [pending] }, mounted), undefined);
  assert.equal(selectMountedApp(uri, undefined, mounted), undefined);
  assert.equal(selectMountedApp(uri, { apps: [], degradations: [pending] }, undefined), undefined);
});

test("pending discovery is distinct from failed services", () => {
  const degradations: AppCatalogDegradation[] = [
    { server: "map", surface: "resources", code: "discovery_pending" },
    { server: "media", surface: "resources", code: "discovery_pending" },
    { server: "media", surface: "tools", code: "upstream_unavailable" },
  ];
  assert.deepEqual(unavailableAppServers([], degradations), ["media"]);
  assert.deepEqual(groupAppsByServer([], degradations).map(({ server, discovering, unavailable }) => ({ server, discovering, unavailable })), [
    { server: "map", discovering: true, unavailable: false },
    { server: "media", discovering: false, unavailable: true },
  ]);
});

test("catalog represents only servers without a healthy App as unavailable", () => {
  const apps = [{ server: "map" }] as AppDescriptor[];
  const degradations = [
    { server: "map" },
    { server: "media" },
    { server: "media" },
  ] as AppCatalogDegradation[];

  assert.deepEqual(unavailableAppServers(apps, degradations), ["media"]);
  assert.equal(appServerTitle("uav-sim"), "UAV Sim");
});

test("catalog groups local App names under a deterministic server namespace", () => {
  const apps = [
    { server: "view", resourceUri: "ui://view/preview.html", title: "Preview" },
    { server: "charts", resourceUri: "ui://charts/composer.html", title: "Composer" },
    { server: "optimization", resourceUri: "ui://optimization/routes.html", title: "Route Planning" },
    { server: "optimization", resourceUri: "ui://optimization/models.html", title: "Mathematical Models" },
  ] as AppDescriptor[];
  const degradations = [{ server: "media" }] as AppCatalogDegradation[];

  assert.deepEqual(
    groupAppsByServer(apps, degradations).map((group) => ({
      server: group.server,
      apps: group.apps.map((app) => app.title),
      unavailable: group.unavailable,
    })),
    [
      { server: "charts", apps: ["Composer"], unavailable: false },
      { server: "media", apps: [], unavailable: true },
      {
        server: "optimization",
        apps: ["Mathematical Models", "Route Planning"],
        unavailable: false,
      },
      { server: "view", apps: ["Preview"], unavailable: false },
    ],
  );
  assert.equal(namespacedAppTitle(apps[0]), "View / Preview");
});

test("repeated resource pages cannot add duplicate navigation buttons", () => {
  const routes = { server: "optimization", resourceUri: "ui://optimization/routes.html", title: "Routes" } as AppDescriptor;
  const models = { server: "optimization", resourceUri: "ui://optimization/models.html", title: "Models" } as AppDescriptor;
  const titles = (apps: AppDescriptor[]) => groupAppsByServer(apps, [])[0].apps.map((app) => app.title);
  assert.deepEqual(titles([routes, routes, models, routes]), ["Models", "Routes"]);
  assert.deepEqual(titles([routes, models]), ["Models", "Routes"]);
});
