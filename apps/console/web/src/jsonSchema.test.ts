import test from "node:test";
import assert from "node:assert/strict";
import { compileGeneratedSchema } from "./jsonSchema.ts";

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
