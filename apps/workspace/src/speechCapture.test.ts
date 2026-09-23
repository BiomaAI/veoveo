import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

type Message = { type: string; bytes?: ArrayBuffer };
type Processor = { process(inputs: Float32Array[][]): boolean; port: { onmessage(event: { data: string }): void } };

test("microphone batches fit the wire ceiling and Stop preserves the partial tail", () => {
  const messages: Message[] = [];
  let create: (new () => Processor) | undefined;
  runInNewContext(readFileSync(new URL("./speech/pcm-worklet.js", import.meta.url), "utf8"), {
    sampleRate: 48_000, Float32Array, ArrayBuffer, DataView, Math,
    AudioWorkletProcessor: class { port = { postMessage: (message: Message) => messages.push(message) }; },
    registerProcessor: (_name: string, processor: new () => Processor) => { create = processor; },
  });
  assert.ok(create);
  const processor = new create();
  const input = Float32Array.from({ length: 108_000 }, (_, i) => (i % 200 - 100) / 100);
  assert.equal(processor.process([[input]]), true);
  assert.deepEqual(messages.map(message => message.bytes?.byteLength), [192_000, 192_000]);
  processor.port.onmessage({ data: "finish" });
  assert.deepEqual(messages.map(message => message.type), ["pcm", "pcm", "pcm", "finished"]);
  assert.equal(messages[2].bytes?.byteLength, 48_000);
  const samples = messages.filter(message => message.type === "pcm").flatMap(message => {
    const view = new DataView(message.bytes!);
    return Array.from({ length: view.byteLength / 4 }, (_, i) => view.getFloat32(i * 4, true));
  });
  assert.deepEqual(samples, Array.from(input));
  assert.equal(processor.process([[input]]), false);
  assert.equal(messages.length, 4);
});
