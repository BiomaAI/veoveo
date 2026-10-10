/** Admit one Annex B access unit without decoding or copying its coded payload. */
export function accessUnit(bytes) {
  if (!(bytes instanceof Uint8Array) || !bytes.length || bytes.length > 16 * 1024 * 1024) throw new Error("Invalid H.264 access unit size");
  const starts = [];
  for (let i = 0; i + 2 < bytes.length; i++) {
    if (bytes[i] !== 0 || bytes[i + 1] !== 0) continue;
    const length = bytes[i + 2] === 1 ? 3 : bytes[i + 2] === 0 && bytes[i + 3] === 1 ? 4 : 0;
    if (length) { if (starts.length >= 4096) throw new Error("Too many H.264 NAL units"); starts.push([i, i + length]); i += length - 1; }
  }
  if (!starts.length || bytes.subarray(0, starts[0][0]).some(value => value !== 0)) throw new Error("Invalid Annex B framing");
  let codec = null, idr = false, pps = false;
  for (let i = 0; i < starts.length; i++) {
    const start = starts[i][1], end = i + 1 < starts.length ? starts[i + 1][0] : bytes.length;
    if (start >= end || (bytes[start] & 128)) throw new Error("Invalid H.264 NAL header");
    const type = bytes[start] & 31;
    if (type === 7) {
      // SPS begins with profile_idc, constraint flags, and level_idc (RFC 6381).
      if (end - start < 5 || !bytes[start + 1] || !bytes[start + 3] || (bytes[start + 2] & 3)) throw new Error("Malformed H.264 SPS");
      const observed = `avc1.${Array.from(bytes.subarray(start + 1, start + 4), value => value.toString(16).padStart(2, "0")).join("")}`;
      if (codec !== null && codec !== observed) throw new Error("Conflicting H.264 SPS codecs");
      codec = observed;
    }
    if (type === 8) { if (end - start < 2) throw new Error("Truncated H.264 PPS"); pps = true; }
    if (type === 5) { if (end - start < 2) throw new Error("Truncated H.264 IDR"); idr = true; }
  }
  return {codec, key: idr, reentrant: idr && pps && codec !== null};
}

/** One serial configuration operation and at most one pending reentrant frame. */
export class H264DecoderAdmission {
  constructor({configure, reset, decode, failure}) {
    this.configure = configure; this.reset = reset; this.decode = decode; this.failure = failure;
    this.codec = null; this.pending = null; this.busy = false; this.closed = false; this.needsKey = true;
  }
  push(bytes, timestamp) {
    if (this.closed) return;
    try {
      const unit = accessUnit(bytes);
      if (this.busy) {
        // Only a complete reentrant frame repairs references lost while configuring.
        this.needsKey = !unit.reentrant;
        if (unit.reentrant || unit.codec) this.pending = {bytes, timestamp, unit};
        return;
      }
      if (this.codec && (!unit.codec || unit.codec === this.codec)) {
        if (this.needsKey && !unit.reentrant) return;
        if (unit.reentrant) this.needsKey = false;
        this.decode(bytes, timestamp, unit.key); return;
      }
      // A new SPS without a reentrant IDR invalidates the old decoder immediately.
      this.reset(); this.codec = null;
      if (!unit.reentrant) return;
      this.pending = {bytes, timestamp, unit}; this.needsKey = false;
      void this.drain();
    } catch (cause) { this.close(); this.failure(cause); }
  }
  async drain() {
    this.busy = true;
    try {
      while (this.pending && !this.closed) {
        const frame = this.pending; this.pending = null;
        this.reset(); this.codec = null;
        if (!frame.unit.reentrant) continue;
        const commit = await this.configure(frame.unit.codec);
        if (this.closed) return;
        if (this.pending && (!this.pending.unit.reentrant || this.pending.unit.codec !== frame.unit.codec)) continue;
        commit(); this.codec = frame.unit.codec;
        this.decode(frame.bytes, frame.timestamp, true);
        if (this.pending) {
          const next = this.pending; this.pending = null;
          this.decode(next.bytes, next.timestamp, true);
        }
      }
    } catch (cause) { if (!this.closed) { this.close(); this.failure(cause); } }
    finally { this.busy = false; }
  }
  close() { this.closed = true; this.pending = null; this.reset(); }
}
