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
  assert.match(source, /structured\(successfulToolEnvelope\(chartEnvelope\(params\?\.result\|\|params\)\)\)/);
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

test("Composer admits current MCP envelopes and refuses retired keys on calls and notifications", async () => {
  const {createContext, runInContext} = await import("node:vm");
  const {toolEnvelope, successfulToolEnvelope} = await import("../../mcp/apps-extension/browser/admission.js");
  // Load the owning code and its handlers, with startup excluded from this CPU
  // admission control. No renderer, network bridge or chart engine is started.
  const element = {addEventListener() {}};
  const context = createContext({toolEnvelope, successfulToolEnvelope,
    document: {getElementById: () => element, querySelectorAll: () => []},
    addEventListener() {}, clearTimeout, setTimeout});
  const beforeStartup = source.replace(/^import[^\n]*\n/, "").split("(async()=>{try{const initialized=")[0];
  runInContext(beforeStartup, context, {timeout: 1000});
  const current = {content: [], structuredContent: {chart_spec: {native_field: 1}, semantic_types: {}},
    _meta: {"fixture.extension": {open_key: true}}, "fixtureExtension": {open_key: true}};
  context.reply = current;
  runInContext("bridge.request = async () => reply", context);
  assert.deepEqual(await runInContext("tool('compile_chart', {})", context), toolEnvelope(current));
  assert.deepEqual(runInContext("structured(chartEnvelope(reply))", context), current.structuredContent);
  runInContext("listeners.get('ui/notifications/tool-result')({result: reply})", context);
  for (const old of [
    {content: [], structured_content: {input: {}}},
    {...current, structured_content: {input: {}}},
    {content: [], is_error: true},
    {...current, is_error: false}
  ]) {
    context.reply = old;
    await assert.rejects(runInContext("tool('compile_chart', {})", context), /Unsupported Chart/);
    assert.throws(() => runInContext("listeners.get('ui/notifications/tool-result')({result: reply})", context), /Unsupported Chart/);
  }
  context.reply = {content: [{type: "text", text: "declared failure"}], isError: true};
  await assert.rejects(runInContext("tool('compile_chart', {})", context), /declared failure/);
});
