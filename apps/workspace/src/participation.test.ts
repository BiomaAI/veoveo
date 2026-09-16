import test from "node:test";
import assert from "node:assert/strict";
import { addressedAgents, responseAgents } from "./participation.ts";
import type { ChatAgent } from "./generated/workspace.ts";
const agents: ChatAgent[] = ["Writer", "Reviewer", "Research Lead"].map((name, i) => ({ id: `${i}`, name, definition: name, provider: "Fixture", model: "Fixture", active: true }));

test("leading mentions address known agents while quoted and ordinary body text do not", () => {
  assert.deepEqual(addressedAgents("@Writer @Research Lead: Please discuss", ["0"], agents), ["0", "2"]);
  for (const text of ["> @Writer please", "```\n@Writer\n```", "Someone said @Writer", '"@Writer"']) assert.deepEqual(addressedAgents(text, [], agents), []);
  assert.deepEqual(addressedAgents("@Writerly", [], agents), []);
  assert.throws(() => addressedAgents("@Writer please", [], [...agents, { ...agents[0], id: "duplicate" }]), /Several agents/);
  assert.throws(() => addressedAgents("Hello", ["removed"], agents), /left this chat/);
});
test("default yields to explicit requests; automatic participation deduplicates and stays bounded", () => {
  assert.deepEqual(responseAgents({ mode: "on_request", agents: [] }, ["0"]), ["0"]);
  assert.deepEqual(responseAgents({ mode: "default", agents: ["0"] }, []), ["0"]);
  assert.deepEqual(responseAgents({ mode: "default", agents: ["0"] }, ["1"]), ["1"]);
  assert.deepEqual(responseAgents({ mode: "automatic", agents: ["0", "1"] }, ["1"]), ["1", "0"]);
  assert.throws(() => responseAgents({ mode: "automatic", agents: ["0", "1", "2", "3"] }, ["4"]), /exceed four/);
});
