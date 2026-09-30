import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

test("referenced boolean schemas preserve impossible and open TypeScript values", { timeout: 30_000 }, () => {
  const schema = {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    title: "BooleanContract",
    type: "object",
    properties: {
      forbidden: { $ref: "#/$defs/NoIdentity" },
      nullable: { anyOf: [{ $ref: "#/$defs/NoIdentity" }, { type: "null" }] },
      open: { $ref: "#/$defs/OpenValue" },
    },
    $defs: { NoIdentity: false, OpenValue: true },
    additionalProperties: false,
  };
  const generated = spawnSync(process.execPath, [fileURLToPath(new URL("./client-types.mjs", import.meta.url))], {
    input: JSON.stringify(schema), encoding: "utf8", timeout: 10_000,
  });
  assert.equal(generated.status, 0, generated.stderr);
  const directory = mkdtempSync(join(tmpdir(), "veoveo-client-types-"));
  try {
    writeFileSync(join(directory, "contract.ts"), generated.stdout);
    writeFileSync(join(directory, "consumer.ts"), `
import type { BooleanContract } from "./contract";
const nullable: BooleanContract = { nullable: null };
const open: BooleanContract = { open: { arbitrary: [true, 42, null] } };
// @ts-expect-error: an impossible schema admits no identity.
const forbidden: BooleanContract = { forbidden: "invented" };
// @ts-expect-error: the nullable impossible branch admits only null.
const falseIdentity: BooleanContract = { nullable: "invented" };
void [nullable, open, forbidden, falseIdentity];
`);
    const checked = spawnSync(process.execPath, [
      fileURLToPath(new URL("../node_modules/typescript/bin/tsc", import.meta.url)),
      "--ignoreConfig", "--strict", "--noEmit", "--skipLibCheck", "--target", "es2022", join(directory, "consumer.ts"),
    ], { encoding: "utf8", timeout: 10_000 });
    assert.equal(checked.status, 0, checked.stdout + checked.stderr);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
