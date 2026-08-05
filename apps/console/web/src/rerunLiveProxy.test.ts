import assert from "node:assert/strict";
import test from "node:test";

import { validateConsoleRerunLiveProxyRoute } from "./rerunLiveProxy.ts";

const origin = "https://installation.example";
const route =
  `${origin}/console/api/recordings/019fab95-e208-7901-9db7-77c8444652db/segments/019faba1-3e9b-77d2-a3b5-b7cc97d0d238/live/proxy`;

test("accepts only a canonical same-origin recording-scoped live proxy route", () => {
  assert.equal(validateConsoleRerunLiveProxyRoute(route, origin), route);
  for (const invalid of [
    `${route}?token=forbidden`,
    route.replace(origin, "https://other.example"),
    route.replace("/live/proxy", "/live.rrd"),
    `${origin}/console/api/recordings/not-a-recording/segments/not-a-segment/live/proxy`,
  ]) {
    assert.throws(() => validateConsoleRerunLiveProxyRoute(invalid, origin));
  }
});

