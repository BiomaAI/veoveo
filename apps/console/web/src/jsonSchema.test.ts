import test from "node:test";
import assert from "node:assert/strict";
import { compileGeneratedSchema } from "./jsonSchema.ts";

test("composed closed objects validate unevaluated properties without changing values", () => {
  const validator = compileGeneratedSchema({
    $schema: "https://json-schema.org/draft/2020-12/schema",
    type: "object",
    allOf: [{ properties: { value: { type: "integer" } }, required: ["value"] }],
    unevaluatedProperties: false,
  });
  const input = { value: 1 };
  assert.equal(validator.parse(input), input);
  assert.equal(validator.safeParse({ value: "1" }).success, false);
  assert.equal(validator.safeParse({ value: 1, extra: true }).success, false);
});

test("generated boolean references preserve impossible, nullable and open values", () => {
  const schema = {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    type: "object",
    properties: {
      forbidden: { $ref: "#/$defs/Never" },
      nullable: { anyOf: [{ $ref: "#/$defs/Never" }, { type: "null" }] },
      open: { $ref: "#/$defs/Open" },
    },
    $defs: { Never: false, Open: true },
    additionalProperties: false,
  };
  const validator = compileGeneratedSchema(schema);
  assert.equal(validator.safeParse({ nullable: null, open: [1, "text", null] }).success, true);
  for (const forbidden of [null, false, 0, "", {}, []]) {
    assert.equal(validator.safeParse({ forbidden }).success, false);
  }
  assert.equal(validator.safeParse({ nullable: "identity" }).success, false);
  assert.equal(validator.safeParse({ undeclared: true }).success, false);
  assert.deepEqual(schema.$defs, { Never: false, Open: true });
});
