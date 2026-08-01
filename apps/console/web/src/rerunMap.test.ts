import assert from "node:assert/strict";
import test from "node:test";

import {
  mapProviderCompatibilityError,
  resolveRerunMapViewerOptions,
} from "./rerunMap.ts";

test("openStreetMap configuration never requests or returns a provider token", async () => {
  const requests: Array<string> = [];
  const fetcher = async (input: string | URL | Request) => {
    requests.push(input.toString());
    return Response.json({ provider: "openStreetMap" });
  };

  assert.deepEqual(await resolveRerunMapViewerOptions(fetcher as typeof fetch), {
    provider: "openStreetMap",
    options: {},
  });
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
    provider: "mapbox",
    options: { mapbox_access_token: "pk.installation-token" },
  });
  assert.equal(requests.length, 2);
  assert.match(requests[1], /^https:\/\/api\.mapbox\.com\/styles\/v1\/mapbox\/streets-v12\?/);
});

test("mapbox authentication failures are map-scoped and never echo the token", async () => {
  const token = "pk.invalid-token";
  let request = 0;
  const fetcher = async () => {
    request += 1;
    return request === 1
      ? Response.json({ provider: "mapbox", accessToken: token })
      : new Response("unauthorized", { status: 401 });
  };

  const setup = await resolveRerunMapViewerOptions(fetcher as typeof fetch);
  assert.equal(setup.provider, "mapbox");
  assert.deepEqual(setup.options, {});
  assert.match(setup.mapError ?? "", /invalid or expired/);
  assert.equal(setup.mapError?.includes(token), false);
});

test("missing installation token produces an explicit map-scoped diagnostic", async () => {
  const setup = await resolveRerunMapViewerOptions(
    (async () =>
      Response.json({
        provider: "mapbox",
        diagnostic: "Mapbox is selected, but no installation token is mounted",
      })) as typeof fetch,
  );
  assert.equal(setup.provider, "mapbox");
  assert.deepEqual(setup.options, {});
  assert.match(setup.mapError ?? "", /no installation token/);
});

test("explicit provider mismatch never silently substitutes a background", () => {
  assert.match(
    mapProviderCompatibilityError("mapbox", "openStreetMap") ?? "",
    /does not select a Mapbox background/,
  );
  assert.match(
    mapProviderCompatibilityError("openStreetMap", "mapbox") ?? "",
    /installation selects OpenStreetMap/,
  );
  assert.match(
    mapProviderCompatibilityError("mapbox", "mixed") ?? "",
    /mixes map-provider families/,
  );
  assert.equal(mapProviderCompatibilityError("mapbox", "mapbox"), undefined);
});
