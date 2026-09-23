import workletUrl from "./pcm-worklet.js?url&no-inline";
import { dictation } from "./api.ts";

type Callbacks = { partial: (text: string) => void; failed: (message: string) => void; limit: () => void };

/** One bounded microphone owner. No chat APIs, durable audio or automatic retry. */
export class Capture {
  private readonly id = crypto.randomUUID();
  private readonly abort = new AbortController();
  private context?: AudioContext;
  private media?: MediaStream;
  private node?: AudioWorkletNode;
  private source?: MediaStreamAudioSourceNode;
  private sequence = 0;
  private queued = 0;
  private writes: Promise<void> = Promise.resolve();
  private admitted = false;
  private closed = false;
  private finishing = false;
  private timer?: ReturnType<typeof setTimeout>;
  private flushed?: () => void;
  constructor(private callbacks: Callbacks) {}

  async start() {
    if (!navigator.mediaDevices?.getUserMedia || !window.AudioWorkletNode) throw new Error("This browser cannot capture microphone audio.");
    this.context = new AudioContext({ sampleRate: 48000 });
    await this.context.resume();
    this.media = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true }, video: false });
    if (this.closed) { this.release(); return; }
    await this.context.audioWorklet.addModule(workletUrl);
    const snapshot = await dictation("", "POST", { id: this.id, sample_rate: this.context.sampleRate }, this.abort.signal);
    this.admitted = true;
    if (this.closed) { await this.cancel(); return; }
    if (snapshot.id !== this.id || snapshot.status !== "listening") throw new Error("The microphone session could not start.");
    this.source = this.context.createMediaStreamSource(this.media);
    this.node = new AudioWorkletNode(this.context, "speech-pcm", { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [1] });
    this.node.port.onmessage = (event: MessageEvent<unknown>) => {
      const message = event.data as { type?: string; bytes?: unknown };
      if (message.type === "finished") { this.flushed?.(); return; }
      if (message.type === "limit") { this.callbacks.limit(); return; }
      if (message.type !== "pcm" || !(message.bytes instanceof ArrayBuffer) || this.closed) return;
      const bytes = message.bytes;
      if (++this.queued > 4) { this.fail("Speech cannot keep up with the microphone connection. Your draft is preserved."); return; }
      const sequence = this.sequence++;
      this.writes = this.writes.then(async () => {
        if (this.closed) return;
        const snapshot = await dictation(`/${this.id}/chunks/${sequence}`, "PUT", bytes, this.abort.signal);
        if (snapshot.id !== this.id || snapshot.next_sequence !== sequence + 1 || snapshot.status === "failed") throw new Error("Dictation was interrupted. You can keep the available text.");
        if (snapshot.transcript) this.callbacks.partial(snapshot.transcript.text);
      }).catch(error => this.fail(error instanceof Error ? error.message : "Dictation was interrupted.")).finally(() => { this.queued--; });
    };
    this.source.connect(this.node); this.node.connect(this.context.destination);
    this.media.getAudioTracks().forEach(track => { track.onended = () => { if (!this.finishing && !this.closed) this.fail("The microphone disconnected. Your draft is preserved."); }; });
    this.timer = setTimeout(() => this.callbacks.limit(), 120_000);
  }

  async finish(): Promise<string> {
    if (this.closed || this.finishing) return "";
    this.finishing = true;
    clearTimeout(this.timer);
    // Stop microphone hardware immediately; the worklet flushes its short buffer.
    this.media?.getTracks().forEach(track => track.stop());
    if (this.node) await new Promise<void>(resolve => {
      const timeout = setTimeout(resolve, 500);
      this.flushed = () => { clearTimeout(timeout); resolve(); };
      this.node!.port.postMessage("finish");
    });
    this.release();
    await this.writes;
    if (this.closed) throw new Error("Dictation was interrupted. You can keep the available text.");
    try {
      const snapshot = await dictation(`/${this.id}/finish`, "POST", undefined, this.abort.signal);
      if (snapshot.id !== this.id || snapshot.status !== "completed") throw new Error("Dictation could not finish. You can keep the available text.");
      this.closed = true;
      return snapshot.transcript?.text ?? "";
    } catch (error) { await this.cancel(); throw error; }
  }

  async cancel() {
    this.closed = true; this.abort.abort(); this.release();
    if (this.admitted) { this.admitted = false; await dictation(`/${this.id}`, "DELETE").catch(() => {}); }
  }
  private fail(message: string) {
    if (this.closed) return;
    void this.cancel(); this.callbacks.failed(message);
  }
  private release() {
    clearTimeout(this.timer);
    this.media?.getTracks().forEach(track => track.stop());
    this.source?.disconnect(); this.node?.disconnect();
    const context = this.context; this.context = undefined;
    if (context && context.state !== "closed") void context.close().catch(() => {});
  }
}
