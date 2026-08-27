import assert from "node:assert/strict";
import test from "node:test";

import { interceptRecordingProjectionStreams } from "./apps/recordingProjectionStream.ts";
import type { JSONRPCMessage, Transport } from "./apps/protocol.ts";
import type { RecordingProjectionStream } from "./api.ts";
import type { AppDescriptor } from "./types.ts";

const RECORDING_ID = "019faa9f-acc8-7400-ba67-a9b022da1f63";
const PROJECTION_ID = "019faa9f-bb90-7f00-8b4a-f5c241881707";

const explorer: AppDescriptor = {
  server: "recording",
  resourceUri: "ui://recording/explorer.html",
  standalonePath: "/apps/recording/explorer.html",
  name: "Recording Explorer",
  tools: [{
    name: "create_recording_projection",
    inputSchema: { type: "object" },
  }],
  agentMessageTargets: [],
  resourceDependencies: [],
};

function innerTransport(sent: JSONRPCMessage[]): Transport {
  return {
    start: async () => {},
    send: async (message) => { sent.push(message); },
    close: async () => {},
  };
}

function nextMessage(port: MessagePort): Promise<Record<string, unknown>> {
  return new Promise((resolve) => {
    port.onmessage = (event) => resolve(event.data as Record<string, unknown>);
    port.start();
  });
}

test("Recording Explorer receives only a transferable bounded stream port", async () => {
  const sent: JSONRPCMessage[] = [];
  const inner = innerTransport(sent);
  let transferred: { message: Record<string, unknown>; port: MessagePort } | undefined;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array([1, 2, 3]));
      controller.close();
    },
  });
  const loader = async (): Promise<RecordingProjectionStream> => ({
    stream: body,
    byteLength: 3,
    sha256: "a".repeat(64),
  });
  interceptRecordingProjectionStreams(
    inner,
    explorer,
    (message, transfer) => {
      assert.equal(transfer.length, 1);
      assert.ok(transfer[0] instanceof MessagePort);
      transferred = { message, port: transfer[0] as MessagePort };
    },
    loader,
  );

  inner.onmessage?.({
    jsonrpc: "2.0",
    id: 7,
    method: "veoveo/recordings/projection-stream",
    params: { recordingId: RECORDING_ID, projectionId: PROJECTION_ID },
  });
  assert.ok(transferred);
  assert.deepEqual(Object.keys(transferred.message).sort(), ["id", "jsonrpc", "result"]);
  const result = transferred.message.result as Record<string, unknown>;
  assert.deepEqual(Object.keys(result).sort(), ["port", "transfer"]);
  assert.equal(result.transfer, "message_port");

  const payload = await nextMessage(transferred.port);
  assert.deepEqual(Object.keys(payload).sort(), ["byteLength", "sha256", "stream"]);
  assert.ok(payload.stream instanceof ReadableStream);
  assert.equal(payload.byteLength, 3);
  assert.equal(payload.sha256, "a".repeat(64));
  assert.equal(sent.length, 0);
});

test("projection bridge rejects every app except the exact Recording Explorer", async () => {
  const sent: JSONRPCMessage[] = [];
  const inner = innerTransport(sent);
  let loaded = false;
  interceptRecordingProjectionStreams(
    inner,
    { ...explorer, resourceUri: "ui://recording/other.html" },
    () => assert.fail("invalid app received a transferable port"),
    async () => {
      loaded = true;
      throw new Error("must not load");
    },
  );
  inner.onmessage?.({
    jsonrpc: "2.0",
    id: 8,
    method: "veoveo/recordings/projection-stream",
    params: { recordingId: RECORDING_ID, projectionId: PROJECTION_ID },
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(loaded, false);
  assert.equal(sent.length, 1);
  assert.ok("error" in sent[0]);
});

test("disposing a projection bridge aborts its active same-origin fetch", async () => {
  const inner = innerTransport([]);
  let aborted = false;
  const bridge = interceptRecordingProjectionStreams(
    inner,
    explorer,
    () => {},
    async (_recordingId, _projectionId, signal) => {
      await new Promise<void>((_resolve, reject) => {
        signal?.addEventListener("abort", () => {
          aborted = true;
          reject(new Error("aborted"));
        });
      });
      throw new Error("unreachable");
    },
  );
  inner.onmessage?.({
    jsonrpc: "2.0",
    id: 9,
    method: "veoveo/recordings/projection-stream",
    params: { recordingId: RECORDING_ID, projectionId: PROJECTION_ID },
  });
  bridge.dispose();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(aborted, true);
});
