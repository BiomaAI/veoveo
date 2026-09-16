import test from "node:test";
import assert from "node:assert/strict";
import { unstable_convertExternalMessages as convert, unstable_createExternalMessageConversionCache as cache } from "@assistant-ui/react";
import { toThreadMessage, mergeMessages, type PresentedMessage } from "./conversation.ts";
import { parse } from "./api.ts";

test("assistant-ui preserves two humans and two concurrent agents across replay and updates", () => {
  const messages: PresentedMessage[] = [
    { id: "a", authorId: "alice", authorName: "Alice", kind: "human", mine: true, text: "Let's plan", createdAt: "2026-09-15T12:00:00Z" },
    { id: "b", authorId: "bob", authorName: "Bob", kind: "human", mine: false, text: "Check the schedule", createdAt: "2026-09-15T12:00:01Z" },
    { id: "c", authorId: "planner", authorName: "Planner", kind: "agent", mine: false, text: "Planning", createdAt: "2026-09-15T12:00:02Z", run: { id: "r1", state: "running" } },
    { id: "d", authorId: "reviewer", authorName: "Reviewer", kind: "agent", mine: false, text: "Reviewing", createdAt: "2026-09-15T12:00:03Z", run: { id: "r2", state: "running" } },
  ];
  const conversion = cache();
  const first = convert(messages, toThreadMessage, false, {}, conversion);
  assert.deepEqual(first.map(message => message.id), ["a", "b", "c", "d"]);
  assert.deepEqual(first.map(message => message.metadata.custom.authorName), ["Alice", "Bob", "Planner", "Reviewer"]);
  assert.equal(first[2].role, "assistant");
  assert.equal(first[3].role, "assistant");
  if (first[2].role === "assistant" && first[3].role === "assistant") {
    assert.equal(first[2].status.type, "running");
    assert.equal(first[3].status.type, "running");
  }
  const updated = [...messages];
  updated[2] = { ...updated[2], text: "Plan ready", run: { id: "r1", state: "completed" } };
  updated.push({ ...messages[0], id: "e", text: "One more constraint" });
  const next = convert(updated, toThreadMessage, false, {}, conversion);
  assert.equal(next.length, 5);
  assert.equal(next[3].metadata.custom.runId, "r2");
  assert.equal(next[3].role === "assistant" && next[3].status.type, "running");
  const replay = convert(updated, toThreadMessage, false, {}, cache());
  assert.deepEqual(replay.map(message => [message.id, message.content, message.metadata.custom]), next.map(message => [message.id, message.content, message.metadata.custom]));
});

test("message replay deduplicates stable identity and orders committed messages", () => {
  const message = { id: crypto.randomUUID(), author: crypto.randomUUID(), text: "First", replyTo: null, addressedAgents: [], responseAgents: [], sequence: 2, createdAt: "2026-09-15T12:00:00Z" };
  const later = { ...message, id: crypto.randomUUID(), sequence: 5, text: "Later" };
  const merged = mergeMessages([later], [message, later]);
  assert.deepEqual(merged.map(message => message.sequence), [2, 5]);
  assert.equal(parse("Message", message).text, "First");
  assert.throws(() => parse("Message", { ...message, author: "untrusted" }));
  assert.throws(() => parse("Message", { ...message, capabilityToken: "untrusted" }));
});
