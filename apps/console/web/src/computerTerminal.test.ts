import assert from "node:assert/strict";
import test from "node:test";
import {
  TerminalSession,
  type TerminalClock,
  type TerminalStatus,
} from "./computers/terminalSession.ts";

test("the default clock preserves the browser timer receiver", async (t) => {
  const timeout = globalThis.setTimeout;
  const clear = globalThis.clearTimeout;
  t.mock.method(globalThis, "setTimeout", function (this: unknown, ...args: Parameters<typeof setTimeout>) {
    assert.ok(this === undefined || this === globalThis, "illegal browser timer receiver");
    return Reflect.apply(timeout, globalThis, args) as ReturnType<typeof setTimeout>;
  });
  t.mock.method(globalThis, "clearTimeout", function (this: unknown, ...args: Parameters<typeof clearTimeout>) {
    assert.ok(this === undefined || this === globalThis, "illegal browser timer receiver");
    return Reflect.apply(clear, globalThis, args);
  });
  // Evaluate the real module with browser-like native APIs, independently of
  // the injected deterministic clock used by the protocol tests below.
  const specifier = "./computers/terminalSession.ts?browser-timer-receiver";
  const browser = (await import(specifier)) as typeof import("./computers/terminalSession.ts");
  const session = new browser.TerminalSession(
    { bufferedAmount: 0, send: () => {}, close: () => {} },
    { input: () => {}, write: (_bytes, done) => done() },
    () => {},
  );
  session.close();
});

function fixture() {
  let now = 0;
  const timers = new Map<ReturnType<typeof setTimeout>, { callback: () => void; at: number }>();
  let timerId = 0;
  const time: TerminalClock = {
    wall: () => Date.UTC(2026, 8, 10) + now,
    monotonic: () => now,
    timer: (callback, ms) => {
      const id = ++timerId as unknown as ReturnType<typeof setTimeout>;
      timers.set(id, { callback, at: now + ms });
      return id;
    },
    clear: (id) => {
      timers.delete(id);
    },
  };
  const sent: Array<string | Uint8Array> = [];
  const writes: Array<{ bytes: Uint8Array; done: () => void }> = [];
  const statuses: TerminalStatus[] = [];
  let closed = 0;
  let input = false;
  const wire = {
    bufferedAmount: 0,
    send: (data: string | Uint8Array) => {
      sent.push(data);
    },
    close: () => {
      closed++;
    },
  };
  const session = new TerminalSession(
    wire,
    {
      write: (bytes, done) => {
        writes.push({ bytes, done });
      },
      input: (enabled) => {
        input = enabled;
      },
    },
    (status) => statuses.push(status),
    time,
  );
  const ready = () =>
    session.receive(
      JSON.stringify({
        type: "ready",
        version: 2,
        expiresAt: new Date(time.wall() + 30_000).toISOString(),
      }),
    );
  const replay = () => session.receive('{"type":"replay_complete"}');
  const lease = (sequence: number) =>
    session.receive(
      JSON.stringify({
        type: "lease",
        sequence,
        expiresAt: new Date(time.wall() + 30_000).toISOString(),
      }),
    );
  const advance = (ms: number) => {
    now += ms;
    for (const [id, timer] of timers)
      if (timer.at <= now) {
        timers.delete(id);
        timer.callback();
      }
  };
  return {
    session,
    wire,
    sent,
    writes,
    statuses,
    ready,
    replay,
    lease,
    advance,
    input: () => input,
    closed: () => closed,
  };
}
test("terminal input and answerbacks wait for historical callback drain; live output does not starve readiness", () => {
  const f = fixture();
  f.session.input("ignored");
  f.ready();
  f.session.receive(new TextEncoder().encode("history").buffer);
  f.replay();
  f.session.input("historical answerback");
  f.session.receive(new TextEncoder().encode("live").buffer);
  assert.equal(f.input(), false);
  assert.equal(f.sent.length, 0);
  f.writes[0].done();
  assert.equal(f.input(), true);
  assert.equal(f.writes.length, 2);
  f.session.input("current input");
  assert.equal(new TextDecoder().decode(f.sent[0] as Uint8Array), "current input");
  f.session.close();
});
test("terminal deadline closes blocked rendering and late callbacks cannot restore input", () => {
  const f = fixture();
  f.ready();
  f.session.receive(new ArrayBuffer(10));
  f.replay();
  f.advance(29_000);
  assert.equal(f.closed(), 1);
  assert.equal(f.input(), false);
  f.writes[0].done();
  f.lease(1);
  f.session.input("never");
  assert.equal(f.input(), false);
  assert.equal(f.sent.length, 0);
  assert.equal(f.closed(), 1);
});
test("terminal renewable lease advances only through strictly increasing safe sequences", () => {
  const f = fixture();
  f.ready();
  f.replay();
  f.advance(20_000);
  f.lease(1);
  f.advance(10_000);
  assert.equal(f.closed(), 0);
  f.lease(1);
  assert.equal(f.closed(), 1);
  const unsafe = fixture();
  unsafe.ready();
  unsafe.lease(Number.MAX_SAFE_INTEGER + 1);
  assert.equal(unsafe.closed(), 1);
});
test("terminal rejects invalid ordering, versions and oversized frames", () => {
  for (const data of [
    new ArrayBuffer(1),
    '{"type":"replay_complete"}',
    '{"type":"ready","version":1,"expiresAt":"2026-09-10T00:00:30Z"}',
    " ".repeat(1025),
  ]) {
    const f = fixture();
    f.session.receive(data);
    assert.equal(f.closed(), 1);
  }
  const f = fixture();
  f.ready();
  f.session.receive(new ArrayBuffer(65_537));
  assert.equal(f.closed(), 1);
});
test("terminal bounds output backlog and never queues unsent input across congestion", () => {
  const output = fixture();
  output.ready();
  for (let i = 0; i < 5; i++) output.session.receive(new ArrayBuffer(65_536));
  assert.equal(output.closed(), 1);
  assert.equal(output.writes.length, 1);
  const input = fixture();
  input.ready();
  input.replay();
  input.wire.bufferedAmount = 131_072;
  input.session.input("x");
  assert.equal(input.closed(), 1);
  assert.equal(input.sent.length, 0);
  const paste = fixture();
  paste.ready();
  paste.replay();
  paste.session.input("é".repeat(40_000));
  assert.equal(paste.closed(), 1);
  assert.equal(paste.sent.length, 0);
});
test("terminal attachment establishment has a fixed timeout and resize cannot bypass replay", () => {
  const pending = fixture();
  pending.advance(10_000);
  assert.equal(pending.closed(), 1);
  const f = fixture();
  f.ready();
  f.session.resize(80, 24);
  assert.equal(f.sent.length, 0);
  f.replay();
  f.session.resize(100, 30);
  assert.deepEqual(JSON.parse(f.sent[0] as string), { type: "resize", cols: 100, rows: 30 });
  f.session.close();
  f.session.receive(new ArrayBuffer(1));
  assert.equal(f.writes.length, 0);
});
