import assert from "node:assert/strict";
import test from "node:test";

import { validateConsoleRerunLiveProxyRoute } from "./rerunLiveProxy.ts";

const origin = "https://installation.example";
const route =
  `${origin}/console/api/recordings/019fab95-e208-7901-9db7-77c8444652db/live/proxy`;

test("accepts only a canonical same-origin recording-scoped live proxy route", () => {
  assert.equal(validateConsoleRerunLiveProxyRoute(route, origin), route);
  for (const invalid of [
    `${route}?token=forbidden`,
    route.replace(origin, "https://other.example"),
    route.replace("/live/proxy", "/live.rrd"),
    `${origin}/console/api/recordings/not-a-recording/live/proxy`,
  ]) {
    assert.throws(() => validateConsoleRerunLiveProxyRoute(invalid, origin));
  }
});
