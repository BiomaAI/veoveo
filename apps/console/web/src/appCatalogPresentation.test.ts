import assert from "node:assert/strict";
import test from "node:test";

import {
  appServerTitle,
  groupAppsByServer,
  namespacedAppTitle,
  unavailableAppServers,
} from "./apps/catalogPresentation.ts";
import type { AppCatalogDegradation, AppDescriptor } from "./types.ts";

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
