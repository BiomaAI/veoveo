import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_RERUN_LIVE_FRAME_BYTES,
  RerunLiveFrameDecoder,
} from "./rerunLiveChannel.ts";

function encoded(...frames: Uint8Array[]): Uint8Array {
  const byteLength = 8 + frames.reduce((sum, frame) => sum + 4 + frame.byteLength, 0);
  const bytes = new Uint8Array(byteLength);
  bytes.set(new TextEncoder().encode("VVRL0001"));
  let offset = 8;
  for (const frame of frames) {
    new DataView(bytes.buffer).setUint32(offset, frame.byteLength, false);
    offset += 4;
    bytes.set(frame, offset);
    offset += frame.byteLength;
  }
  return bytes;
}

test("decodes complete RRD payloads across every network byte boundary", () => {
  const stream = encoded(new Uint8Array([1, 2]), new Uint8Array([3, 4, 5]));
  const decoder = new RerunLiveFrameDecoder();
  const frames: Uint8Array[] = [];
  for (const byte of stream) decoder.push(new Uint8Array([byte]), (frame) => frames.push(frame));
  decoder.finish();
  assert.deepEqual(frames, [new Uint8Array([1, 2]), new Uint8Array([3, 4, 5])]);
});

test("rejects invalid, oversized, and truncated framing", () => {
  const invalidMagic = encoded(new Uint8Array([1]));
  invalidMagic[0] = 0;
  assert.throws(() => new RerunLiveFrameDecoder().push(invalidMagic, () => {}), /preface/);

  const oversized = encoded(new Uint8Array([1]));
  new DataView(oversized.buffer).setUint32(8, MAX_RERUN_LIVE_FRAME_BYTES + 1, false);
  assert.throws(() => new RerunLiveFrameDecoder().push(oversized, () => {}), /length/);

  const decoder = new RerunLiveFrameDecoder();
  const truncated = encoded(new Uint8Array([1, 2, 3])).slice(0, -1);
  decoder.push(truncated, () => {});
  assert.throws(() => decoder.finish(), /truncated/);
});
