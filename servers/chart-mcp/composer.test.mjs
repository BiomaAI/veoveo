import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(new URL("./composer.html", import.meta.url), "utf8");
const source = readFileSync(new URL("./app/main.js", import.meta.url), "utf8");
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
    assert.match(source, new RegExp(`tool\\(\\"${tool}\\"`));
  }
  assert.match(source, /await render\(\)/);
  assert.match(source, /structured\(successfulToolEnvelope\(params\?\.result\|\|params\)\)/);
  assert.match(html, /Inline data must be a non-empty JSON array/);
  assert.match(html, /ui\/notifications\/tool-input/);
});


test("pinned upstream package publishes render declarations without structured MCP result schemas", () => {
  const root=process.env.VEOVEO_CHART_UPSTREAM_SOURCE;
  assert.ok(root,"set VEOVEO_CHART_UPSTREAM_SOURCE to the isolated pinned flint-chart-mcp package fixture");
  const packageJson=JSON.parse(readFileSync(`${root}/package.json`,"utf8"));
  assert.equal(packageJson.name,"flint-chart-mcp");
  assert.equal(packageJson.version,"0.5.1");
  assert.deepEqual(Object.keys(packageJson.exports).sort(),[".","./render"]);
  const declaration=readFileSync(`${root}/dist/render/index.d.ts`,"utf8");
  assert.match(declaration,/interface RenderResult/);
  assert.match(declaration,/declare function validateInput/);
  assert.doesNotMatch(declaration,/outputSchema|JsonSchema|JSONSchema/);
  assert.match(readFileSync(new URL("./app/main.js",import.meta.url),"utf8"),/toolEnvelope/);
});
