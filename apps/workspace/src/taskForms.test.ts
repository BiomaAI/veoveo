import test from "node:test";
import assert from "node:assert/strict";
import { formFields, readForm } from "./taskForms.ts";

test("MCP forms preserve typed answers, defaults, choices and numeric bounds", () => {
  const fields = formFields({ type: "object", required: ["count", "choice"], properties: {
    count: { type: "integer", minimum: 1, maximum: 3 },
    choice: { oneOf: [{ const: "save", title: "Save the result" }, { const: "review", title: "Review first" }] },
    enabled: { type: "boolean", default: true },
    tags: { type: "array", items: { enum: ["a", "b"] }, maxItems: 1 },
  }});
  assert.equal(fields[1]?.choices?.[0]?.label, "Save the result");
  const data = new FormData(); data.set("f-0", "2"); data.set("f-1", "save"); data.set("f-2", "on"); data.set("f-3", "a");
  assert.deepEqual(readForm(fields, data, "f"), { count: 2, choice: "save", enabled: true, tags: ["a"] });
  data.set("f-0", "9"); assert.throws(() => readForm(fields, data, "f"));
  data.set("f-0", "2.5"); assert.throws(() => readForm(fields, data, "f"));
  data.set("f-0", "2"); data.append("f-3", "b"); assert.throws(() => readForm(fields, data, "f"));
});

test("unsupported form shapes cannot silently submit empty answers", () => {
  assert.throws(() => formFields({ type: "object", properties: { nested: { type: "object" } } }));
  assert.throws(() => formFields({ type: "object", properties: { values: { type: "array", items: { type: "number" } } } }));
  const data = new FormData(); data.set("f-0", "");
  assert.throws(() => readForm(formFields({ type: "object", properties: { name: { type: "string" } }, required: ["name"] }), data, "f"));
});
