import assert from "node:assert/strict";
import test from "node:test";

import {
  FramedRrdDecoder,
  MAX_RRD_FRAME_BYTES,
  validateConsoleRerunLiveRoute,
} from "./rerunLiveChannel.ts";

function frame(payload: Uint8Array): Uint8Array {
  const framed = new Uint8Array(4 + payload.byteLength);
  new DataView(framed.buffer).setUint32(0, payload.byteLength, false);
  framed.set(payload, 4);
  return framed;
}

test("decodes complete RRD payloads across arbitrary HTTP chunks", () => {
  const first = frame(Uint8Array.from([1, 2, 3]));
  const second = frame(Uint8Array.from([4, 5]));
  const stream = new Uint8Array(first.byteLength + second.byteLength);
  stream.set(first);
  stream.set(second, first.byteLength);
  const decoded: number[][] = [];
  const decoder = new FramedRrdDecoder((rrd) => decoded.push([...rrd]));
  for (const byte of stream) decoder.push(Uint8Array.of(byte));
  decoder.finish();
  assert.deepEqual(decoded, [
    [1, 2, 3],
    [4, 5],
  ]);
});

test("rejects zero, oversized, and truncated frames", () => {
  const zero = new FramedRrdDecoder(() => undefined);
  assert.throws(() => zero.push(new Uint8Array(4)), /outside/);

  const oversized = new Uint8Array(4);
  new DataView(oversized.buffer).setUint32(0, MAX_RRD_FRAME_BYTES + 1, false);
  assert.throws(
    () => new FramedRrdDecoder(() => undefined).push(oversized),
    /outside/
  );

  const truncated = new FramedRrdDecoder(() => undefined);
  truncated.push(frame(Uint8Array.from([1, 2, 3])).slice(0, 6));
  assert.throws(() => truncated.finish(), /inside an RRD frame/);
});

test("accepts only the recording-scoped same-origin stream route", () => {
  const origin = "https://console.example";
  const path =
    "/console/api/recordings/019fab95-e208-7901-9db7-77c8444652db/live/rrd-stream";
  assert.equal(validateConsoleRerunLiveRoute(path, origin), `${origin}${path}`);
  for (const invalid of [
    `${path}?token=forbidden`,
    path.replace("/rrd-stream", "/other-stream"),
    `https://other.example${path}`,
  ]) {
    assert.throws(() => validateConsoleRerunLiveRoute(invalid, origin), /governed/);
  }
});
