import { compileGeneratedSchema } from "../../console/web/src/jsonSchema.ts";
import test from "node:test";
import assert from "node:assert/strict";
import schema from "./generated/speech.schema.json" with { type: "json" };
import type { DictationSnapshot } from "./generated/speech.ts";

test("generated Speech receipts accept browser and native IDs and reject malformed addresses", () => {
  const definition: object = {
    $schema: schema.$schema,
    $defs: schema.$defs,
    $ref: "#/$defs/DictationSnapshot",
  };
  const validator = compileGeneratedSchema(definition);
  for (const id of ["01983da0-0000-4000-8000-000000000001", "01983da0-0000-7000-8000-000000000001"]) {
    const receipt: DictationSnapshot = {
      id,
      result_uri: `speech://dictation/${id}`,
      status: "listening",
      next_sequence: 0,
      max_duration_seconds: 120,
      transcript: null,
    };
    assert.deepEqual(validator.parse(receipt), receipt);
    for (const result_uri of [
      `speech://transcript/${id}`,
      `${receipt.result_uri}?extra=1`,
      `${receipt.result_uri}#fragment`,
      "speech://dictation/00000000-0000-0000-0000-000000000000",
      "speech://dictation/01983da0-0000-5000-8000-000000000001",
    ]) {
      assert.equal(validator.safeParse({ ...receipt, result_uri }).success, false);
    }
    assert.equal(validator.safeParse({ ...receipt, id: "00000000-0000-0000-0000-000000000000" }).success, false);
  }
});

test("generated transcription outputs admit Artifact references and require a transcript address", () => {
  const definition: object = { $schema: schema.$schema, $defs: schema.$defs, $ref: "#/$defs/TranscriptionOutput" };
  const validator = compileGeneratedSchema(definition);
  const id = "01983da0-0000-7000-8000-000000000001";
  const metadata = { artifact_id: id, artifact_uri: `artifact://${id}`, byte_len: 1, created_at: "2026-09-29T00:00:00Z" };
  const output = { result_uri: `speech://transcript/${id}`, source_artifact_uri: `artifact://${id}`, transcript: metadata, captions: metadata, duration_seconds: 1.0 };
  assert.equal(validator.safeParse(output).success, true);
  assert.equal(validator.safeParse({ ...output, result_uri: `speech://dictation/${id}` }).success, false);
});
