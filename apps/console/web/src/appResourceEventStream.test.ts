import assert from "node:assert/strict";
import test from "node:test";

import {
  consumeServerSentEvents,
  openResourceEventStream,
  type ResourceEventStream,
  type ServerSentEvent,
} from "./apps/resourceEventStream.ts";

function byteStream(chunks: string[], leaveOpen = false): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      if (!leaveOpen) controller.close();
    },
  });
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setImmediate(resolve));
}

test("fetch SSE decoder preserves chunked names and multiline data", async () => {
  const events: ServerSentEvent[] = [];
  await consumeServerSentEvents(
    byteStream([
      ": keepalive\r\nevent: resource-",
      "updated\r\ndata: {\"uri\":\r\n",
      "data: \"fleet://plans\"}\r\n\r\n",
      "data: ordinary message\n\n",
    ]),
    (event) => events.push(event),
  );
  assert.deepEqual(events, [
    { type: "resource-updated", data: "{\"uri\":\n\"fleet://plans\"}" },
    { type: "message", data: "ordinary message" },
  ]);
});

test("fetch resource stream opens and forwards wake events", async () => {
  let opened = 0;
  const events: ServerSentEvent[] = [];
  const errors: Error[] = [];
  const fetchResource = async () =>
    new Response(
      byteStream([
        "event: subscribed\ndata: {}\n\n",
        "event: resource-updated\ndata: {\"uri\":\"fleet://plans\"}\n\n",
      ], true),
      { status: 200, headers: { "Content-Type": "text/event-stream" } },
    );
  const stream: ResourceEventStream = openResourceEventStream(
    "/console/api/apps/resource-events",
    {
      onOpen: () => { opened += 1; },
      onEvent: (event) => {
        events.push(event);
        if (event.type === "resource-updated") stream.close();
      },
      onInitialError: (error) => errors.push(error),
    },
    fetchResource,
  );
  await settle();

  assert.equal(opened, 1);
  assert.deepEqual(events, [
    { type: "subscribed", data: "{}" },
    { type: "resource-updated", data: "{\"uri\":\"fleet://plans\"}" },
  ]);
  assert.deepEqual(errors, []);
});

test("one fetch stream carries a bounded subscription batch", async () => {
  let fetches = 0;
  let opened = 0;
  let observedBody: unknown;
  const fetchResource = async (_input: RequestInfo | URL, init?: RequestInit) => {
    fetches += 1;
    observedBody = JSON.parse(String(init?.body));
    return new Response(byteStream(["event: subscribed\ndata: {}\n\n"], true), {
      status: 200,
      headers: { "Content-Type": "text/event-stream" },
    });
  };
  const stream = openResourceEventStream(
    "/console/api/apps/resource-events",
    {
      onOpen: () => { opened += 1; },
      onEvent: () => {},
      onInitialError: (error) => { throw error; },
    },
    async (input, init) => fetchResource(input, {
      ...init,
      body: JSON.stringify({
        subscriptions: [
          { subscriptionId: "one", uri: "fleet://plans" },
          { subscriptionId: "two", uri: "fleet://vehicles" },
        ],
      }),
    }),
  );
  await settle();

  assert.equal(fetches, 1);
  assert.equal(opened, 1);
  assert.deepEqual(observedBody, {
    subscriptions: [
      { subscriptionId: "one", uri: "fleet://plans" },
      { subscriptionId: "two", uri: "fleet://vehicles" },
    ],
  });
  stream.close();
});
