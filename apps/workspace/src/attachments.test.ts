import test from "node:test";
import assert from "node:assert/strict";
import { attachmentReference } from "./attachments.ts";
import { parse } from "./api.ts";

test("attachments retain a typed identity and explicit label without accepting capabilities or external URLs", () => {
  const id = "01a0a75d-3458-78f3-ac54-91f1cab1fea1";
  const file = attachmentReference(`media://artifact/${id}`, " Project notes ");
  assert.deepEqual(file, { kind: "artifact", id, name: "Project notes" });
  for (const uri of [`https://example.org/${id}`, `artifact://${id}?token=private`, "javascript:alert(1)"]) {
    assert.throws(() => attachmentReference(uri, "Notes"));
  }
  for (const label of ["", "\n", "a\nb", "界".repeat(86)]) assert.throws(() => attachmentReference(`artifact://${id}`, label));
  const message = { id: crypto.randomUUID(), author: crypto.randomUUID(), text: "", attachments: [file], addressedAgents: [], responseAgents: [], sequence: 1, createdAt: new Date().toISOString() };
  assert.equal(parse("Message", message).attachments[0].id, id);
  assert.throws(() => parse("Message", { ...message, attachments: [{ ...file, downloadUrl: "https://example.org/private" }] }));
});
