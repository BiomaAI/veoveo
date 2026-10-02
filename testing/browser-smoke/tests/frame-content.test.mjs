// These are sampler behavior tests; they establish no rendering or GPU acceptance.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const source = readFileSync(new URL("../src/browser/app_video_state.js", import.meta.url), "utf8");

function sample(luma, available = true) {
  const pixels = new Uint8ClampedArray(64 * 36 * 4);
  for (let y = 0; y < 36; y++) {
    for (let x = 0; x < 64; x++) {
      const offset = (y * 64 + x) * 4;
      pixels.fill(luma(x, y), offset, offset + 3);
      pixels[offset + 3] = 255;
    }
  }
  const canvas = {
    width: 1280, height: 720,
    dataset: { decodedFrames: "16", mediaTimeSeconds: "1", frameRate: "16" },
    closest: () => ({ id: "view-follow", dataset: {} }),
  };
  const document = {
    querySelector: () => available ? canvas : null,
    createElement: () => ({ getContext: () => ({
      drawImage: (actual, x, y, width, height) => {
        assert.equal(actual, canvas);
        assert.deepEqual([x, y, width, height], [0, 0, 64, 36]);
      },
      getImageData: () => ({ data: pixels }),
    }) }),
    getElementById: () => null,
    body: { innerText: "" },
  };
  return JSON.parse(JSON.stringify(runInNewContext(source, {
    document, performance: { timeOrigin: 0, now: () => 1000 },
  })));
}

test("spatial cells retain their own contrast independently of the frame mean", () => {
  const frame = sample((x, y) => (Math.floor(y / 9) * 4 + Math.floor(x / 16)) * 12 + (x % 2) * 30);
  assert.equal(frame.pixelSampleError, "");
  assert.equal(frame.meanLuma, 105);
  assert.ok(Math.abs(frame.lumaStandardDeviation - Math.sqrt(3285)) < 1e-8);
  assert.equal(frame.lumaCells.length, 16);
  for (const [index, cell] of frame.lumaCells.entries()) {
    assert.equal(cell.minimum, index * 12);
    assert.equal(cell.maximum, index * 12 + 30);
    assert.ok(Math.abs(cell.standardDeviation - 15) < 1e-8);
  }
});

test("a uniformly dark frame has no local contrast", () => {
  const frame = sample(() => 20);
  assert.ok(Math.abs(frame.meanLuma - 20) < 1e-8);
  assert.ok(frame.lumaStandardDeviation < 1e-5);
  for (const cell of frame.lumaCells) {
    assert.equal(cell.minimum, 20);
    assert.equal(cell.maximum, 20);
    assert.ok(cell.standardDeviation < 1e-5);
  }
});

test("a small bright patch changes only its own region", () => {
  const frame = sample((x, y) => x < 8 && y < 5 ? 255 : 0);
  assert.ok(frame.lumaStandardDeviation > 5);
  assert.equal(frame.lumaCells.filter(cell => cell.standardDeviation >= 5).length, 1);
});

test("an unavailable canvas reports the sampling failure with a complete typed shape", () => {
  const frame = sample(() => 0, false);
  assert.match(frame.pixelSampleError, /dimensions are unavailable/);
  assert.equal(frame.lumaCells.length, 16);
});
