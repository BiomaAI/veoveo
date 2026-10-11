// Exact sampler behavior controls. Mock DOM/decoder results establish no GPU or visual acceptance.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const load = name => readFileSync(new URL(`../src/browser/browser/${name}.js`, import.meta.url), "utf8");
const progress = load("app_video_progress"), cadence = load("app_video_cadence"), decode = load("app_decode_identity");

function environment({ stall = false, changeIdentity = false, backwards = false, frameMs = 62.5 } = {}) {
  let now = 100, nextTimer = 0, tick = 0;
  const timers = new Map();
  const view = { id: "view-follow", dataset: { viewerInstanceId: "viewer", liveViewId: "live-current", streamProductId: "camera-atlas" } };
  const canvas = { dataset: { decodedFrames: "1", mediaTimeSeconds: "1", receiveToDisplayMs: "4", droppedFrames: "0", codec: "avc1.640034", sourceWidth: "3840", sourceHeight: "1440", frameRate: "16" }, closest: () => view };
  const context = {
    document: { querySelector: () => canvas, getElementById: () => ({ textContent: "NVIDIA NVENC · software H.264 decode" }) },
    performance: { timeOrigin: 123, now: () => now }, Date: { now: () => now },
    setTimeout: (callback, delay) => { const id = ++nextTimer; timers.set(id, { callback, delay }); return id; },
    clearTimeout: id => timers.delete(id),
    MutationObserver: class {
      constructor(callback) { this.callback = callback; this.active = true; }
      observe() {
        queueMicrotask(() => {
          if (!this.active) return;
          const timer = [...timers.values()].at(-1);
          if (stall || frameMs >= timer.delay) { now += timer.delay; timer.callback(); return; }
          now += frameMs; tick++;
          canvas.dataset.decodedFrames = String(Number(canvas.dataset.decodedFrames) + 1);
          canvas.dataset.mediaTimeSeconds = String(Number(canvas.dataset.mediaTimeSeconds) + (backwards && tick === 2 ? -1 : 0.25));
          if (changeIdentity && tick === 2) view.dataset.streamProductId = "other-product";
          this.callback();
        });
      }
      disconnect() { this.active = false; }
    },
  };
  return { context, canvas, timers };
}

test("isolated strict cadence samples actual canvas updates without private read or session globals", async () => {
  const { context, timers } = environment();
  const sample = await runInNewContext(cadence, context);
  assert.equal(sample.presentedFrameIntervals, 48);
  assert.equal(sample.decodedVideoFrames, 48);
  assert.equal(sample.intervalSeconds, 3);
  assert.equal(sample.deliverySamples, 48);
  assert.equal(timers.size, 0);
});

test("restart continuity admits several real slow frame changes without qualifying throughput", async () => {
  const { context } = environment({ frameMs: 1000 });
  const sample = await runInNewContext(progress, context);
  assert.equal(sample.decodedFrameChanges, 3);
  assert.equal(sample.last.decoded - sample.first.decoded, 3);
  assert.equal(sample.last.now - sample.first.now, 3000);
  assert.ok(sample.last.mediaTime > sample.first.mediaTime);
  assert.equal(sample.last.liveViewId, "live-current");
});

test("restart continuity refuses stalled frames, changed product and backwards capture time", async () => {
  for (const [options, error] of [[{ stall: true }, /deadline/], [{ changeIdentity: true }, /identity changed/], [{ backwards: true }, /backwards/]]) {
    const { context } = environment(options);
    await assert.rejects(runInNewContext(progress, context), error);
  }
});

test("strict cadence keeps its finite48-interval deadline for slow sources", async () => {
  const { context } = environment({ frameMs: 1000 });
  await assert.rejects(runInNewContext(cadence, context), /did not complete/);
});

test("decode assessment uses current admitted codec, coded dimensions and actual UAV policy", async () => {
  const { context } = environment();
  let configuration;
  context.navigator = { mediaCapabilities: { decodingInfo: async config => {
    configuration = JSON.parse(JSON.stringify(config));
    return { supported: true, smooth: true, powerEfficient: false };
  } } };
  const result = await runInNewContext(decode, context);
  assert.deepEqual(configuration, { type: "file", video: { contentType: 'video/mp4; codecs="avc1.640034"', width: 3840, height: 1440, bitrate: 12000000, framerate: 16 } });
  assert.equal(result.powerEfficient, false);
  assert.equal(result.label, "NVIDIA NVENC · software H.264 decode");
});

test("decode assessment refuses missing actual codec before MediaCapabilities", async () => {
  const { context, canvas } = environment();
  canvas.dataset.codec = "pending";
  context.navigator = { mediaCapabilities: { decodingInfo: () => assert.fail("unadmitted codec must not be assessed") } };
  await assert.rejects(runInNewContext(decode, context), /configuration is unavailable/);
});
