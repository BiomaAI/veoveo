import test from "node:test";
import assert from "node:assert/strict";
import { unstable_convertExternalMessages as convert, unstable_createExternalMessageConversionCache as cache } from "@assistant-ui/react";
import { toThreadMessage, mergeMessages, present, quote, type PresentedMessage } from "./conversation.ts";
import { parse } from "./api.ts";

test("assistant-ui preserves two humans and two concurrent agents across replay and updates", () => {
  const messages: PresentedMessage[] = [
    { id: "a", attachments: [], target: { kind: "message", id: "a" }, authorId: "alice", authorName: "Alice", kind: "human", mine: true, text: "Let's plan", createdAt: "2026-09-15T12:00:00Z" },
    { id: "b", attachments: [], target: { kind: "message", id: "b" }, authorId: "bob", authorName: "Bob", kind: "human", mine: false, text: "Check the schedule", createdAt: "2026-09-15T12:00:01Z" },
    { id: "c", attachments: [], target: { kind: "response", id: "c" }, authorId: "planner", authorName: "Planner", kind: "agent", mine: false, text: "Planning", createdAt: "2026-09-15T12:00:02Z", run: { id: "r1", state: "running", feedback: { phase: "responding", operations: 0 } } },
    { id: "d", attachments: [], target: { kind: "response", id: "d" }, authorId: "reviewer", authorName: "Reviewer", kind: "agent", mine: false, text: "Reviewing", createdAt: "2026-09-15T12:00:03Z", run: { id: "r2", state: "running", feedback: { phase: "responding", operations: 0 } } },
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
  updated[2] = { ...updated[2], text: "Plan ready", run: { id: "r1", state: "completed", feedback: { phase: "responding", operations: 0 } } };
  updated.push({ ...messages[0], id: "e", text: "One more constraint" });
  const next = convert(updated, toThreadMessage, false, {}, conversion);
  assert.equal(next.length, 5);
  assert.equal(next[3].metadata.custom.runId, "r2");
  assert.equal(next[3].role === "assistant" && next[3].status.type, "running");
  const replay = convert(updated, toThreadMessage, false, {}, cache());
  assert.deepEqual(replay.map(message => [message.id, message.content, message.metadata.custom]), next.map(message => [message.id, message.content, message.metadata.custom]));
});

test("message replay deduplicates stable identity and orders committed messages", () => {
  const message = { id: crypto.randomUUID(), author: crypto.randomUUID(), text: "First", replyTo: null, attachments: [], addressedAgents: [], responseAgents: [], sequence: 2, createdAt: "2026-09-15T12:00:00Z" };
  const later = { ...message, id: crypto.randomUUID(), sequence: 5, text: "Later" };
  const merged = mergeMessages([later], [message, later]);
  assert.deepEqual(merged.map(message => message.sequence), [2, 5]);
  assert.equal(parse("Message", message).text, "First");
  assert.throws(() => parse("Message", { ...message, author: "untrusted" }));
  assert.throws(() => parse("Message", { ...message, capabilityToken: "untrusted" }));
});

test("typed reply identities cannot collide and server quotes survive unloaded originals", () => {
  const id = crypto.randomUUID();
  const chat = { id, owner: id, title: "Shared", archived: false, membersCanInvite: false, participation: { mode: "on_request" as const, agents: [] }, sequence: 5, revision: 0, updatedAt: "2026-09-15T12:00:00Z" };
  const original = { id, author: id, text: "Human original", sequence: 2, createdAt: chat.updatedAt, attachments: [], addressedAgents: [], responseAgents: [] };
  const reply = { ...original, id: crypto.randomUUID(), sequence: 4, text: "Follow-up", replyTo: { kind: "response" as const, id }, replyContext: { authorName: "Reviewer", text: "Frozen response" } };
  const activity = { agents: [], runs: [{ id, agent: id, initiator: id, trigger: id, text: "Agent original", state: "completed" as const, feedback: { phase: "responding" as const, operations: 0 }, failure: null, sequence: 3, updatedSequence: 3, createdAt: chat.updatedAt }] };
  const source = present({ chat, members: [], messages: [original, reply] }, id, activity);
  assert.deepEqual(source.map(message => message.id), [`message:${id}`, `response:${id}`, `message:${reply.id}`]);
  assert.equal(quote(source[1]!, source)?.text, "Human original");
  assert.deepEqual(quote(source[2]!, [source[2]!]), reply.replyContext, "a frozen quote does not need its original history page");
  const unquoted = { ...source[2]!, replyContext: null };
  assert.equal(quote(unquoted, source)?.text, "Agent original", "same UUID in another namespace cannot select a human message");
});
