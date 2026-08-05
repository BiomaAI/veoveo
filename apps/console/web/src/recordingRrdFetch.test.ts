import assert from "node:assert/strict";
import test from "node:test";

import {
  authorizeConsoleRecordingRrdFetch,
  isConsoleRecordingRrdRequest,
} from "./recordingRrdFetch.ts";

const ORIGIN = "https://installation.example";
const RECORDING_ID = "019fab95-e208-7901-9db7-77c8444652db";
const BLUEPRINT_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}/blueprints/1/data.rrd`;
const LIVE_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}/segments/019faba1-3e9b-77d2-a3b5-b7cc97d0d238/live.rrd`;

test("attaches the Console session only to canonical same-origin RRD sources", () => {
  for (const url of [BLUEPRINT_URL, LIVE_URL]) {
    const request = new Request(url, { credentials: "omit" });
    const [authorized] = authorizeConsoleRecordingRrdFetch(request, undefined, ORIGIN);
    assert.equal(isConsoleRecordingRrdRequest(request, ORIGIN), true);
    assert.ok(authorized instanceof Request);
    assert.equal(authorized.credentials, "same-origin");
  }
});

test("does not alter noncanonical recording requests", () => {
  for (const request of [
    new Request(BLUEPRINT_URL, { method: "POST", credentials: "omit" }),
    new Request(`${BLUEPRINT_URL}?token=forbidden`, { credentials: "omit" }),
    new Request(BLUEPRINT_URL.replace(ORIGIN, "https://other.example"), {
      credentials: "omit",
    }),
    new Request(
      `${ORIGIN}/console/api/recordings/${RECORDING_ID}/segments/019faba1-3e9b-77d2-a3b5-b7cc97d0d238/live.data.rrd`,
      { credentials: "omit" }
    ),
  ]) {
    const adapted = authorizeConsoleRecordingRrdFetch(request, undefined, ORIGIN);
    assert.equal(isConsoleRecordingRrdRequest(request, ORIGIN), false);
    assert.equal(adapted[0], request);
  }
});
