// AudioWorklet isolates capture from UI rendering. It never monitors the microphone.
class PcmCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    // One second fits the 192,000-byte Speech frame ceiling at 48 kHz and
    // avoids making capture throughput depend on four HTTP round trips/second.
    this.buffer = new Float32Array(sampleRate);
    this.used = 0;
    this.total = 0;
    this.stopped = false;
    this.port.onmessage = event => {
      if (event.data === "finish") {
        this.flush(); this.stopped = true; this.port.postMessage({ type: "finished" });
      }
    };
  }
  flush() {
    if (!this.used) return;
    const bytes = new ArrayBuffer(this.used * 4);
    const view = new DataView(bytes);
    for (let i = 0; i < this.used; i++) view.setFloat32(i * 4, this.buffer[i], true);
    this.port.postMessage({ type: "pcm", bytes }, [bytes]);
    this.used = 0;
  }
  process(inputs) {
    if (this.stopped) return false;
    const samples = inputs[0]?.[0];
    if (!samples) return true;
    for (const sample of samples) {
      this.buffer[this.used++] = Math.max(-1, Math.min(1, sample));
      this.total++;
      if (this.used === this.buffer.length) this.flush();
      if (this.total >= sampleRate * 120) {
        this.flush(); this.stopped = true; this.port.postMessage({ type: "limit" }); return false;
      }
    }
    return true;
  }
}
registerProcessor("speech-pcm", PcmCapture);
