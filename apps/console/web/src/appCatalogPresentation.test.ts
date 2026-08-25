import assert from "node:assert/strict";
import test from "node:test";

import { appServerTitle, unavailableAppServers } from "./apps/catalogPresentation.ts";
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
