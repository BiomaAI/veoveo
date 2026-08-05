import assert from "node:assert/strict";
import test from "node:test";

import {
  attachConsoleSessionToRecordingRrd,
  authorizeConsoleRecordingRrdFetch,
  isConsoleRecordingRrdRequest,
  observeRecordingLiveResponseEnd,
} from "./recordingLiveFetch.ts";

const ORIGIN = "https://installation.example";
const RECORDING_ID = "019fab95-e208-7901-9db7-77c8444652db";
const SEGMENT_ID = "019faba1-3e9b-77d2-a3b5-b7cc97d0d238";
const LIVE_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}` +
  `/segments/${SEGMENT_ID}/live.rrd`;
const BLUEPRINT_URL =
  `${ORIGIN}/console/api/recordings/${RECORDING_ID}` +
  "/blueprints/1/data.rrd";

test("attaches the Console session only to the canonical same-origin live receiver", () => {
  const request = new Request(LIVE_URL, {
    credentials: "omit",
    headers: { Accept: "application/vnd.rerun.rrd" },
  });
  const authorized = attachConsoleSessionToRecordingRrd(request, ORIGIN);

  assert.equal(isConsoleRecordingRrdRequest(request, ORIGIN), true);
  assert.notEqual(authorized, request);
  assert.equal(authorized.url, LIVE_URL);
  assert.equal(authorized.credentials, "same-origin");
  assert.equal(authorized.headers.get("accept"), "application/vnd.rerun.rrd");
});

test("attaches the Console session to the canonical producer Blueprint receiver", () => {
  const request = new Request(BLUEPRINT_URL, { credentials: "omit" });
  const authorized = attachConsoleSessionToRecordingRrd(request, ORIGIN);
  assert.equal(isConsoleRecordingRrdRequest(request, ORIGIN), true);
  assert.equal(authorized.credentials, "same-origin");
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
    assert.equal(isConsoleRecordingRrdRequest(request, ORIGIN), false);
    assert.equal(attachConsoleSessionToRecordingRrd(request, ORIGIN), request);
  }
});

test("preserves an explicitly credentialed canonical request", () => {
  const request = new Request(LIVE_URL, { credentials: "include" });
  assert.equal(attachConsoleSessionToRecordingRrd(request, ORIGIN), request);
});

test("adapts a canonical fetch without reconstructing unrelated requests", async () => {
  const liveRequest = new Request(LIVE_URL, {
    credentials: "omit",
    headers: { Accept: "application/vnd.rerun.rrd" },
  });
  const [authorizedInput, authorizedInit] = authorizeConsoleRecordingRrdFetch(
    liveRequest,
    undefined,
    ORIGIN
  );
  assert.ok(authorizedInput instanceof Request);
  assert.notEqual(authorizedInput, liveRequest);
  assert.equal(authorizedInput.credentials, "same-origin");
  assert.equal(authorizedInit, undefined);

  const redapRequest = new Request(`${ORIGIN}/redap/query`, {
    method: "POST",
    body: "query",
  });
  await redapRequest.text();
  assert.equal(redapRequest.bodyUsed, true);
  const untouched = authorizeConsoleRecordingRrdFetch(
    redapRequest,
    undefined,
    ORIGIN
  );
  assert.equal(untouched[0], redapRequest);
  assert.equal(untouched[1], undefined);
});

test("reports natural live response completion without treating cancellation as rollover", async () => {
  let completions = 0;
  const complete = observeRecordingLiveResponseEnd(
    new Response(new Uint8Array([1, 2, 3])),
    () => {
      completions += 1;
    }
  );
  assert.deepEqual(new Uint8Array(await complete.arrayBuffer()), new Uint8Array([1, 2, 3]));
  assert.equal(completions, 1);

  const pending = observeRecordingLiveResponseEnd(
    new Response(
      new ReadableStream<Uint8Array>({
        pull() {},
      })
    ),
    () => {
      completions += 1;
    }
  );
  await pending.body?.cancel();
  assert.equal(completions, 1);
});

test("reports a failed live response as a reactive reconnect event", async () => {
  let completions = 0;
  const failed = observeRecordingLiveResponseEnd(
    new Response(
      new ReadableStream<Uint8Array>({
        pull(controller) {
          controller.error(new Error("connection lost"));
        },
      })
    ),
    () => {
      completions += 1;
    }
  );

  await assert.rejects(failed.arrayBuffer(), /connection lost/);
  assert.equal(completions, 1);
});
