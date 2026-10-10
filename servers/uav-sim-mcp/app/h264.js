const ISAAC_SEI_UUID = new Uint8Array([0xaa,0x71,0xe4,0x8f,0x07,0x11,0x5d,0x80,0xa2,0x47,0xcd,0x31,0xca,0x6f,0xa4,0x9c]);
function parseJson(text, reviver = undefined) {
  try { return JSON.parse(text, reviver); }
  catch { throw new Error("Malformed capture metadata JSON"); }
}
let sourceTextSupported = false;
JSON.parse("1", (_key, _value, context) => { sourceTextSupported = context?.source === "1"; });
const CAPTURE_KEYS = ["publish_sim_time_ns", "timestamp_iso8601", "timestamp", "frame_num"];

/** The pinned Isaac RTSP metadata is a closed flat JSON object. */
export function captureMetadata(payload) {
  if (!payload.length || payload.length > 4096) throw new Error("Invalid capture metadata size");
  if (!sourceTextSupported) throw new Error("Capture timestamps require a browser with JSON source-text access; update your browser");
  const text = new TextDecoder("utf-8", {fatal:true}).decode(payload), seen = new Set();
  // JSON.parse does not expose duplicate keys. Scan bounded key tokens before parsing;
  // the subsequent closed scalar/ISO profile refuses nested or arbitrary string values.
  for (const match of text.matchAll(/("(?:[^"\\]|\\.)*")\s*:/g)) {
    const key = parseJson(match[1]);
    if (!CAPTURE_KEYS.includes(key) || seen.has(key)) throw new Error("Invalid capture metadata keys");
    seen.add(key);
  }
  if (seen.size !== 4) throw new Error("Missing capture metadata fields");
  const value = parseJson(text, (key, item, context) => {
    if (key === "publish_sim_time_ns" || key === "timestamp" || key === "frame_num") {
      if (typeof item !== "number" || !context || typeof context.source !== "string") throw new Error("Capture timestamps require a browser with JSON source-text access; update your browser");
      if (!/^(0|[1-9][0-9]{0,19})$/.test(context.source)) throw new Error("Invalid capture integer");
      const integer = BigInt(context.source);
      if (integer > 18446744073709551615n) throw new Error("Capture integer exceeds uint64");
      return integer;
    }
    return item;
  });
  if (!value || Array.isArray(value) || Object.keys(value).length !== 4 || CAPTURE_KEYS.some(key => !Object.hasOwn(value,key))) throw new Error("Invalid capture metadata object");
  if (typeof value.publish_sim_time_ns !== "bigint" || typeof value.timestamp !== "bigint" || typeof value.frame_num !== "bigint" || value.frame_num < 1n) throw new Error("Invalid capture metadata quantities");
  if (typeof value.timestamp_iso8601 !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(value.timestamp_iso8601) || !Number.isFinite(Date.parse(value.timestamp_iso8601))) throw new Error("Invalid capture anchor timestamp");
  return {simulationNs:value.publish_sim_time_ns,frameNumber:value.frame_num};
}

function seiCapture(nal) {
  if (nal.length > 65536) throw new Error("H.264 SEI exceeds metadata bound");
  const rbsp = [];
  let zeros = 0;
  for (let i=1;i<nal.length;i++) {
    const byte=nal[i];
    if (zeros>=2 && byte===3) {
      if (i+1>=nal.length || nal[i+1]>3) throw new Error("Malformed SEI emulation prevention");
      zeros=0;continue;
    }
    rbsp.push(byte);zeros=byte===0?zeros+1:0;
  }
  let offset=0, messages=0, capture=null;
  const number=()=>{let value=0;while(offset<rbsp.length){const byte=rbsp[offset++];value+=byte;if(byte!==255)return value;}throw new Error("Truncated SEI header");};
  while(offset<rbsp.length) {
    if(rbsp[offset]===128 && rbsp.slice(offset+1).every(byte=>byte===0))return capture;
    if(++messages>256)throw new Error("Too many SEI messages");
    const type=number(), size=number();
    if(size>rbsp.length-offset)throw new Error("Truncated SEI payload");
    const payload=rbsp.slice(offset,offset+size);offset+=size;
    if(type===5 && payload.length<16)throw new Error("Truncated SEI UUID");
    if(type!==5 || !ISAAC_SEI_UUID.every((byte,i)=>payload[i]===byte))continue;
    if(capture)throw new Error("Duplicate capture metadata");
    capture=captureMetadata(new Uint8Array(payload.slice(16)));
  }
  throw new Error("Missing SEI trailing bits");
}

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
  let codec = null, idr = false, pps = false, capture = null, picture = false;
  for (let i = 0; i < starts.length; i++) {
    const start = starts[i][1], end = i + 1 < starts.length ? starts[i + 1][0] : bytes.length;
    if (start >= end || (bytes[start] & 128)) throw new Error("Invalid H.264 NAL header");
    const type = bytes[start] & 31;
    if (type >= 1 && type <= 5) picture = true;
    if (type === 6) {
      if (bytes[start] & 96) throw new Error("Invalid SEI NAL reference bits");
      const observed = seiCapture(bytes.subarray(start,end));
      if (observed) { if (capture) throw new Error("Duplicate capture metadata"); capture = observed; }
    }
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
  if (!picture) throw new Error("H.264 access unit contains no coded picture");
  if (!capture) throw new Error("Camera frame lacks Isaac capture metadata; upgrade the simulator streaming profile");
  return {codec, key: idr, reentrant: idr && pps && codec !== null, capture};
}

/** One serial configuration operation and at most one pending reentrant frame. */
export class H264DecoderAdmission {
  constructor({configure, reset, decode, failure}) {
    this.configure = configure; this.reset = reset; this.decode = decode; this.failure = failure;
    this.codec = null; this.pending = null; this.busy = false; this.closed = false; this.needsKey = true; this.lastCapture = null;
  }
  push(bytes) {
    if (this.closed) return;
    try {
      const unit = accessUnit(bytes), capture = unit.capture;
      if (this.lastCapture && (capture.frameNumber <= this.lastCapture.frameNumber || capture.simulationNs < this.lastCapture.simulationNs)) throw new Error("Camera capture epoch changed; reconnect required");
      const micros = capture.simulationNs / 1000n;
      if (micros > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("Camera capture timestamp exceeds WebCodecs range");
      this.lastCapture = capture;
      const timestamp = Number(micros);
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
