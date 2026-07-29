import assert from "node:assert/strict";
import test from "node:test";

import {
  attachConsoleSessionToLivePlayback,
  isConsoleLivePlaybackRequest,
} from "./recordingLiveFetch.ts";

const ORIGIN = "https://installation.example";
const RECORDING_ID = "019fab95-e208-7901-9db7-77c8444652db";
const SEGMENT_ID = "019faba1-3e9b-77d2-a3b5-b7cc97d0d238";
const LIVE_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}` +
  `/segments/${SEGMENT_ID}/live.rrd`;

test("attaches the Console session only to the canonical same-origin live receiver", () => {
  const request = new Request(LIVE_URL, {
    credentials: "omit",
    headers: { Accept: "application/vnd.rerun.rrd" },
  });
  const authorized = attachConsoleSessionToLivePlayback(request, ORIGIN);

  assert.equal(isConsoleLivePlaybackRequest(request, ORIGIN), true);
  assert.notEqual(authorized, request);
  assert.equal(authorized.url, LIVE_URL);
  assert.equal(authorized.credentials, "same-origin");
  assert.equal(authorized.headers.get("accept"), "application/vnd.rerun.rrd");
});

test("does not attach credentials outside the exact live playback boundary", () => {
  for (const request of [
    new Request(LIVE_URL, { method: "POST", credentials: "omit" }),
    new Request(`${LIVE_URL}?token=forbidden`, { credentials: "omit" }),
    new Request(LIVE_URL.replace(ORIGIN, "https://other.example"), {
      credentials: "omit",
    }),
    new Request(
      `${ORIGIN}/console/api/recordings/${RECORDING_ID}/playback-sessions/legacy/data.rrd`,
      { credentials: "omit" }
    ),
  ]) {
    assert.equal(isConsoleLivePlaybackRequest(request, ORIGIN), false);
    assert.equal(attachConsoleSessionToLivePlayback(request, ORIGIN), request);
  }
});

test("preserves an explicitly credentialed canonical request", () => {
  const request = new Request(LIVE_URL, { credentials: "include" });
  assert.equal(attachConsoleSessionToLivePlayback(request, ORIGIN), request);
});
