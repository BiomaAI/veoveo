import test from "node:test";
import assert from "node:assert/strict";
import { artifactId, canPreviewImage } from "./resources.ts";

test("Artifact links select a fixed governed route and reject external or ambiguous identities", () => {
  const id = "01a0a75d-3458-78f3-ac54-91f1cab1fea1";
  for (const uri of [`artifact://${id}`, `media://artifact/${id}`, `duckdb://artifact/${id}`]) assert.equal(artifactId(uri), id);
  for (const uri of [`https://artifact/${id}`, `media://artifact/${id}?token=x`, `media://artifact/${id}/../other`, `artifact://${id}#fragment`, "javascript:alert(1)", "artifact://not-an-id"]) assert.equal(artifactId(uri), undefined);
});

test("inline preview admits bounded raster files from authoritative response headers", () => {
  assert.equal(canPreviewImage("image/png", "1024"), true);
  assert.equal(canPreviewImage("image/webp; charset=binary", "2048"), true);
  for (const type of ["text/html", "image/svg+xml", "application/pdf", "video/mp4", null]) assert.equal(canPreviewImage(type, "1024"), false);
  for (const size of [null, "", "-1", "Infinity", "20971521"]) assert.equal(canPreviewImage("image/png", size), false);
});
