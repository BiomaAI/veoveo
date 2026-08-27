import type { AppDescriptor } from "../types.ts";
import {
  INVALID_PARAMS,
  isJsonRpcRequest,
  type Transport,
} from "./protocol.ts";

const METHOD = "veoveo/recordings/projection-stream";
const EXPLORER_URI = "ui://recording/explorer.html";
const UUID_V7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

interface ProjectionStream {
  stream: ReadableStream<Uint8Array>;
  byteLength: number;
  sha256: string;
}

export function interceptRecordingProjectionStreams(
  inner: Transport,
  app: AppDescriptor,
  postTransfer: (message: Record<string, unknown>, transfer: Transferable[]) => void,
  load: (
    recordingId: string,
    projectionId: string,
    signal?: AbortSignal,
  ) => Promise<ProjectionStream>,
): { transport: Transport; dispose: () => void } {
  const active = new Map<string, AbortController>();
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };
  inner.onclose = () => transport.onclose?.();
  inner.onerror = (error) => transport.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (!isJsonRpcRequest(message) || message.method !== METHOD) {
      transport.onmessage?.(message, extra);
      return;
    }
    const params = message.params ?? {};
    const keys = Object.keys(params).sort();
    const recordingId = params.recordingId;
    const projectionId = params.projectionId;
    if (
      app.server !== "recording" ||
      app.resourceUri !== EXPLORER_URI ||
      keys.join(",") !== "projectionId,recordingId" ||
      typeof recordingId !== "string" ||
      typeof projectionId !== "string" ||
      !UUID_V7.test(recordingId) ||
      !UUID_V7.test(projectionId)
    ) {
      void inner.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: INVALID_PARAMS, message: "recording projection stream request is not allowed" },
      });
      return;
    }
    const key = `${typeof message.id}:${message.id}`;
    if (active.has(key)) {
      void inner.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: INVALID_PARAMS, message: "recording projection stream request is already active" },
      });
      return;
    }
    const controller = new AbortController();
    active.set(key, controller);
    const channel = new MessageChannel();
    postTransfer(
      {
        jsonrpc: "2.0",
        id: message.id,
        result: { transfer: "message_port", port: channel.port2 },
      },
      [channel.port2],
    );
    load(recordingId, projectionId, controller.signal)
      .then(({ stream, byteLength, sha256 }) => {
        channel.port1.postMessage({ stream, byteLength, sha256 }, [stream as Transferable]);
      })
      .catch((error: unknown) => {
        channel.port1.postMessage({
          error: error instanceof Error ? error.message : String(error),
        });
      })
      .finally(() => {
        active.delete(key);
        channel.port1.close();
      });
  };
  return {
    transport,
    dispose: () => {
      for (const controller of active.values()) controller.abort();
      active.clear();
    },
  };
}
