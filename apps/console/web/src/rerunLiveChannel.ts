import type { LogChannel } from "@rerun-io/web-viewer";

const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const LIVE_RRD_STREAM_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/live/rrd-stream$`
);
const LIVE_RRD_STREAM_CONTENT_TYPE =
  "application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=1";
export const MAX_RRD_FRAME_BYTES = 64 * 1024 * 1024;

export interface RerunLiveConnection {
  readonly done: Promise<void>;
  close(): void;
}

export interface RerunLiveConnectionEvents {
  onConnected?(): void;
  onFrame?(byteLength: number): void;
  onEnded?(): void;
}

export function validateConsoleRerunLiveRoute(
  route: string,
  consoleOrigin: string = window.location.origin
): string {
  const url = new URL(route, consoleOrigin);
  if (
    url.origin !== consoleOrigin ||
    url.search !== "" ||
    url.hash !== "" ||
    !LIVE_RRD_STREAM_PATH.test(url.pathname)
  ) {
    throw new Error("Live recording route is outside the governed Console boundary.");
  }
  return url.toString();
}

export function connectConsoleRerunLiveChannel(
  channel: LogChannel,
  route: string,
  events: RerunLiveConnectionEvents = {}
): RerunLiveConnection {
  const canonical = validateConsoleRerunLiveRoute(route);
  const abort = new AbortController();
  let closed = false;
  const done = followFramedRrdStream(channel, canonical, abort.signal, events).catch(
    (cause: unknown) => {
      if (closed && cause instanceof DOMException && cause.name === "AbortError") return;
      throw cause;
    }
  );
  return {
    done,
    close() {
      if (closed) return;
      closed = true;
      abort.abort();
    },
  };
}

async function followFramedRrdStream(
  channel: LogChannel,
  route: string,
  signal: AbortSignal,
  events: RerunLiveConnectionEvents
): Promise<void> {
  const response = await fetch(route, {
    method: "GET",
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    signal,
    headers: { accept: LIVE_RRD_STREAM_CONTENT_TYPE },
  });
  if (!response.ok) {
    throw new Error(`Live recording stream returned ${response.status}`);
  }
  if (response.headers.get("content-type") !== LIVE_RRD_STREAM_CONTENT_TYPE) {
    throw new Error("Live recording stream returned an unsupported media type.");
  }
  const reader = response.body?.getReader();
  if (!reader) {
    throw new Error("Live recording stream has no readable body.");
  }
  events.onConnected?.();
  const decoder = new FramedRrdDecoder((rrd) => {
    if (!channel.ready) {
      throw new Error("Rerun live channel closed before a complete RRD frame arrived.");
    }
    channel.send_rrd(rrd);
    events.onFrame?.(rrd.byteLength);
  });
  try {
    while (true) {
      const result = await reader.read();
      if (result.done) break;
      decoder.push(result.value);
    }
    decoder.finish();
    events.onEnded?.();
  } finally {
    reader.releaseLock();
  }
}

export class FramedRrdDecoder {
  private readonly header = new Uint8Array(4);
  private headerLength = 0;
  private expectedFrameLength: number | undefined;
  private frame: Uint8Array | undefined;
  private frameLength = 0;
  private readonly emit: (rrd: Uint8Array) => void;

  constructor(emit: (rrd: Uint8Array) => void) {
    this.emit = emit;
  }

  push(chunk: Uint8Array): void {
    if (chunk.byteLength === 0) return;
    let offset = 0;

    while (offset < chunk.byteLength) {
      if (this.expectedFrameLength === undefined) {
        const headerBytes = Math.min(4 - this.headerLength, chunk.byteLength - offset);
        this.header.set(chunk.subarray(offset, offset + headerBytes), this.headerLength);
        this.headerLength += headerBytes;
        offset += headerBytes;
        if (this.headerLength < 4) continue;
        this.expectedFrameLength = new DataView(this.header.buffer).getUint32(0, false);
        if (
          this.expectedFrameLength === 0 ||
          this.expectedFrameLength > MAX_RRD_FRAME_BYTES
        ) {
          throw new Error(
            `Live RRD frame length ${this.expectedFrameLength} is outside 1..=${MAX_RRD_FRAME_BYTES}.`
          );
        }
        this.frame = new Uint8Array(this.expectedFrameLength);
        this.frameLength = 0;
      }

      const frame = this.frame as Uint8Array;
      const frameBytes = Math.min(
        (this.expectedFrameLength as number) - this.frameLength,
        chunk.byteLength - offset
      );
      frame.set(chunk.subarray(offset, offset + frameBytes), this.frameLength);
      this.frameLength += frameBytes;
      offset += frameBytes;
      if (this.frameLength !== this.expectedFrameLength) continue;

      this.emit(frame);
      this.headerLength = 0;
      this.expectedFrameLength = undefined;
      this.frame = undefined;
      this.frameLength = 0;
    }
  }

  finish(): void {
    if (this.headerLength !== 0 || this.expectedFrameLength !== undefined) {
      throw new Error("Live recording stream ended inside an RRD frame.");
    }
  }
}
