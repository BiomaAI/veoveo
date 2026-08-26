import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(new URL("./composer.html", import.meta.url), "utf8");
const server = readFileSync(new URL("./flint-v2.mjs", import.meta.url), "utf8");

test("Composer is direct-launch useful and uses only canonical Charts tools", () => {
  assert.match(server, /ui:\/\/charts\/composer\.html/);
  assert.doesNotMatch(server, /ui:\/\/flint-chart\/chart-view\.html/);
  for (const tool of [
    "render_chart",
    "compile_chart",
    "validate_chart",
    "list_chart_types",
    "list_themes",
  ]) {
    assert.match(html, new RegExp(`tool\\(\\"${tool}\\"`));
  }
  assert.match(html, /await render\(\)/);
  assert.match(html, /Inline data must be a non-empty JSON array/);
  assert.match(html, /ui\/notifications\/tool-input/);
});
