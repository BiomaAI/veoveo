import type { LogChannel } from "@rerun-io/web-viewer";

export const RERUN_LIVE_FRAMES_CONTENT_TYPE =
  "application/vnd.veoveo.rerun-live-frames; version=1";
export const MAX_RERUN_LIVE_FRAME_BYTES = 16 * 1024 * 1024;
const MAGIC = new TextEncoder().encode("VVRL0001");

export interface RerunLiveFrameStats {
  frames: number;
  payloadBytes: number;
}

export class RerunLiveFrameDecoder {
  private magicOffset = 0;
  private readonly header = new Uint8Array(4);
  private headerOffset = 0;
  private payload: Uint8Array | undefined;
  private payloadOffset = 0;

  push(chunk: Uint8Array, onFrame: (frame: Uint8Array) => void): void {
    let offset = 0;
    while (offset < chunk.byteLength) {
      if (this.magicOffset < MAGIC.byteLength) {
        const available = Math.min(
          MAGIC.byteLength - this.magicOffset,
          chunk.byteLength - offset
        );
        for (let index = 0; index < available; index += 1) {
          if (chunk[offset + index] !== MAGIC[this.magicOffset + index]) {
            throw new Error("Live recording transport has an invalid protocol preface.");
          }
        }
        this.magicOffset += available;
        offset += available;
        continue;
      }

      if (!this.payload) {
        const available = Math.min(4 - this.headerOffset, chunk.byteLength - offset);
        this.header.set(chunk.subarray(offset, offset + available), this.headerOffset);
        this.headerOffset += available;
        offset += available;
        if (this.headerOffset < 4) continue;
        const length = new DataView(this.header.buffer).getUint32(0, false);
        if (length === 0 || length > MAX_RERUN_LIVE_FRAME_BYTES) {
          throw new Error(`Live recording frame length ${length} is invalid.`);
        }
        this.payload = new Uint8Array(length);
        this.payloadOffset = 0;
      }

      const available = Math.min(
        this.payload.byteLength - this.payloadOffset,
        chunk.byteLength - offset
      );
      this.payload.set(chunk.subarray(offset, offset + available), this.payloadOffset);
      this.payloadOffset += available;
      offset += available;
      if (this.payloadOffset === this.payload.byteLength) {
        onFrame(this.payload);
        this.payload = undefined;
        this.payloadOffset = 0;
        this.headerOffset = 0;
      }
    }
  }

  finish(): void {
    if (this.magicOffset !== MAGIC.byteLength) {
      throw new Error("Live recording transport ended before its protocol preface.");
    }
    if (this.headerOffset !== 0 || this.payload) {
      throw new Error("Live recording transport ended with a truncated RRD frame.");
    }
  }
}

export async function pumpRerunLiveFrames(
  url: string,
  channel: LogChannel,
  signal: AbortSignal,
  onStats?: (stats: RerunLiveFrameStats) => void
): Promise<RerunLiveFrameStats> {
  const response = await fetch(url, {
    credentials: "same-origin",
    headers: { Accept: RERUN_LIVE_FRAMES_CONTENT_TYPE },
    signal,
  });
  if (!response.ok) {
    throw new Error(`Live recording transport returned ${response.status}.`);
  }
  if (response.headers.get("content-type") !== RERUN_LIVE_FRAMES_CONTENT_TYPE) {
    throw new Error("Live recording transport returned an unexpected content type.");
  }
  if (!response.body) {
    throw new Error("Live recording transport returned no response body.");
  }
  const decoder = new RerunLiveFrameDecoder();
  const reader = response.body.getReader();
  const stats: RerunLiveFrameStats = { frames: 0, payloadBytes: 0 };
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      decoder.push(next.value, (frame) => {
        if (!channel.ready) {
          throw new Error("Rerun live channel closed while receiving recording data.");
        }
        channel.send_rrd(frame);
        stats.frames += 1;
        stats.payloadBytes += frame.byteLength;
        onStats?.({ ...stats });
      });
    }
    decoder.finish();
    return stats;
  } finally {
    reader.releaseLock();
  }
}
