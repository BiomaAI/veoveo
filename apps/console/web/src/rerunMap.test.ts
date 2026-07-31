import assert from "node:assert/strict";
import test from "node:test";

import { resolveRerunMapViewerOptions } from "./rerunMap.ts";

test("openStreetMap configuration never requests or returns a provider token", async () => {
  const requests: Array<string> = [];
  const fetcher = async (input: string | URL | Request) => {
    requests.push(input.toString());
    return Response.json({ provider: "openStreetMap" });
  };

  assert.deepEqual(await resolveRerunMapViewerOptions(fetcher as typeof fetch), {});
  assert.deepEqual(requests, ["/console/api/viewer/rerun/map-config"]);
});

test("mapbox configuration is validated and passed through only as a viewer option", async () => {
  const requests: Array<string> = [];
  const fetcher = async (input: string | URL | Request) => {
    requests.push(input.toString());
    if (requests.length === 1) {
      return Response.json({ provider: "mapbox", accessToken: "pk.installation-token" });
    }
    return new Response("{}", { status: 200 });
  };

  assert.deepEqual(await resolveRerunMapViewerOptions(fetcher as typeof fetch), {
    mapbox_access_token: "pk.installation-token",
  });
  assert.equal(requests.length, 2);
  assert.match(requests[1], /^https:\/\/api\.mapbox\.com\/styles\/v1\/mapbox\/streets-v12\?/);
});

test("mapbox authentication failures are explicit and never echo the token", async () => {
  const token = "pk.invalid-token";
  let request = 0;
  const fetcher = async () => {
    request += 1;
    return request === 1
      ? Response.json({ provider: "mapbox", accessToken: token })
      : new Response("unauthorized", { status: 401 });
  };

  await assert.rejects(
    resolveRerunMapViewerOptions(fetcher as typeof fetch),
    (error: Error) => error.message.includes("invalid or expired") && !error.message.includes(token),
  );
});
