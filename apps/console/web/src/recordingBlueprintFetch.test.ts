import assert from "node:assert/strict";
import test from "node:test";

import {
  authorizeConsoleRecordingBlueprintFetch,
  isConsoleRecordingBlueprintRequest,
} from "./recordingBlueprintFetch.ts";

const ORIGIN = "https://installation.example";
const RECORDING_ID = "019fab95-e208-7901-9db7-77c8444652db";
const BLUEPRINT_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}/blueprints/1/data.rrd`;

test("attaches the Console session only to a canonical same-origin Blueprint", () => {
  const request = new Request(BLUEPRINT_URL, { credentials: "omit" });
  const [authorized] = authorizeConsoleRecordingBlueprintFetch(request, undefined, ORIGIN);
  assert.equal(isConsoleRecordingBlueprintRequest(request, ORIGIN), true);
  assert.ok(authorized instanceof Request);
  assert.equal(authorized.credentials, "same-origin");
});

test("does not alter live, cross-origin, queried, or non-GET requests", () => {
  for (const request of [
    new Request(BLUEPRINT_URL, { method: "POST", credentials: "omit" }),
    new Request(`${BLUEPRINT_URL}?token=forbidden`, { credentials: "omit" }),
    new Request(BLUEPRINT_URL.replace(ORIGIN, "https://other.example"), {
      credentials: "omit",
    }),
    new Request(
      `${ORIGIN}/console/api/recordings/${RECORDING_ID}/segments/019faba1-3e9b-77d2-a3b5-b7cc97d0d238/live.rrd-frames`,
      { credentials: "omit" }
    ),
  ]) {
    const adapted = authorizeConsoleRecordingBlueprintFetch(request, undefined, ORIGIN);
    assert.equal(isConsoleRecordingBlueprintRequest(request, ORIGIN), false);
    assert.equal(adapted[0], request);
  }
});
