import assert from "node:assert/strict";
import test from "node:test";

import { requestStandaloneBootstrap } from "./standaloneBootstrap.ts";
import type { AppDescriptor } from "./types.ts";

const descriptor: AppDescriptor = {
  server: "map",
  resourceUri: "ui://map/admin.html",
  standalonePath: "/apps/map/admin.html",
  name: "map-admin",
  title: "Map administration",
  tools: [],
  resourceDependencies: [],
  agentMessageTargets: [],
};

test("standalone bootstrap submits the browser path without reconstructing a ui URI", async () => {
  let observedInput: RequestInfo | URL | undefined;
  let observedInit: RequestInit | undefined;
  const result = await requestStandaloneBootstrap(
    "/apps/map/admin.html",
    async (input, init) => {
      observedInput = input;
      observedInit = init;
      return new Response(JSON.stringify(descriptor), {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "X-Veoveo-CSRF-Token": "csrf-token",
        },
      });
    },
  );

  assert.equal(observedInput, "/apps/map/admin.html");
  assert.equal(observedInit?.credentials, "same-origin");
  assert.deepEqual(observedInit?.headers, { Accept: "application/json" });
  assert.deepEqual(result, { app: descriptor, csrfToken: "csrf-token" });
});

test("standalone bootstrap rejects a response without CSRF settlement", async () => {
  await assert.rejects(
    requestStandaloneBootstrap(
      "/apps/map/admin.html",
      async () => new Response(JSON.stringify(descriptor), { status: 200 }),
    ),
    /CSRF/,
  );
});
