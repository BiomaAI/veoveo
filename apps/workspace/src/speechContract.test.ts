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
      resultUri: `speech://dictation/${id}`,
      status: "listening",
      nextSequence: 0,
      maxDurationSeconds: 120,
      transcript: null,
    };
    assert.deepEqual(validator.parse(receipt), receipt);
    assert.equal(validator.safeParse({ ...receipt, result_uri: receipt.resultUri }).success, false);
    assert.equal(validator.safeParse({ ...receipt, next_sequence: receipt.nextSequence }).success, false);
    for (const resultUri of [
      `speech://transcript/${id}`,
      `${receipt.resultUri}?extra=1`,
      `${receipt.resultUri}#fragment`,
      "speech://dictation/00000000-0000-0000-0000-000000000000",
      "speech://dictation/01983da0-0000-5000-8000-000000000001",
    ]) {
      assert.equal(validator.safeParse({ ...receipt, resultUri }).success, false);
    }
    assert.equal(validator.safeParse({ ...receipt, id: "00000000-0000-0000-0000-000000000000" }).success, false);
  }
});

test("generated transcription outputs admit Artifact references and require a transcript address", () => {
  const definition: object = { $schema: schema.$schema, $defs: schema.$defs, $ref: "#/$defs/TranscriptionOutput" };
  const validator = compileGeneratedSchema(definition);
  const id = "01983da0-0000-7000-8000-000000000001";
  const metadata = { artifactId: id, artifactUri: `artifact://${id}`, byteLen: 1, createdAt: "2026-09-29T00:00:00Z" };
  const output = { resultUri: `speech://transcript/${id}`, sourceArtifactUri: `artifact://${id}`, transcript: metadata, captions: metadata, durationSeconds: 1.0 };
  assert.equal(validator.safeParse(output).success, true);
  assert.equal(validator.safeParse({ ...output, result_uri: output.resultUri }).success, false);
  assert.equal(validator.safeParse({ ...output, source_artifact_uri: output.sourceArtifactUri }).success, false);
  assert.equal(validator.safeParse({ ...output, resultUri: `speech://dictation/${id}` }).success, false);
});
