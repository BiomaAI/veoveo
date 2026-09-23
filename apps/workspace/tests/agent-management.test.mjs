// UI behavior with explicit HTTP fixtures. This does not establish installed
// kernel provisioning or model execution; those require deployed acceptance.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";
import { hardware } from "./hardware.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const digest = value => `sha256:${value.repeat(64)}`;

test("managed authoring reviews authority, recovers lost creation and observes lifecycle changes", { timeout: 90_000 }, async () => {
  const streams = new Set();
  let sequence = 1;
  const server = await createServer({ root, configFile: `${root}vite.config.ts`, server: { port: 0, host: "127.0.0.1", strictPort: false }, plugins: [{ name: "managed-agent-event-fixture", configureServer(server) {
    server.middlewares.use((req, res, next) => {
      if (req.url !== "/workspace/api/agent-events") return next();
      res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
      streams.add(res); req.on("close", () => streams.delete(res));
      res.write(`event: change\nid: ${sequence}\ndata: {"sequence":${sequence}}\n\n`);
    });
  } }] });
  const browser = await chromium.connectOverCDP(process.env.VEOVEO_BROWSER_CDP ?? "http://127.0.0.1:9222");
  const context = await browser.newContext({ viewport: { width: 1440, height: 1100 } });
  context.setDefaultTimeout(8000);
  const page = await context.newPage();
  try {
    console.info("Managed UI hardware", JSON.stringify(await hardware(page)));
    await server.listen();
    const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
    const owner = crypto.randomUUID();
    const now = new Date().toISOString();
    const limits = { maxOutputTokens: 4096, maxCompletionCalls: 4, maxToolCalls: 8, deadlineSeconds: 120 };
    const template = { id: "reviewed-pilot", revision: digest("b"), name: "Reviewed pilot", models: ["approved"], parameters: [{ name: "session", label: "Session", shape: { kind: "identifier", maxLength: 40 } }], tools: ["time__resolve_time"], resourceSubscriptions: ["fixture://changes"], scopes: ["pilot:use"], roles: ["pilot"], membership: "contributor", storageGib: 2 };
    const authoring = { workContext: "operations", definitionLimit: 100, instanceLimit: 32, storageLimitGib: 128, models: [{ reference: { id: "approved", revision: digest("a") }, name: "Approved model", provider: "Explicit fixture", model: "no-model-execution", limits }], permissions: { readContent: true, create: true, edit: true, publish: true, control: true, archive: true, transfer: false, deploy: true, instanceControl: true, manageContext: true } };
    let definition; let content; const history = [];
    let instance; let operation; let loseCreateReply = true;
    const otherInstances = [];
    const archivedDefinitions = () => definition ? Array.from({ length: 4 }, (_, i) => ({ ...definition, id: `retired-pilot-${i + 1}`, name: `Retired pilot ${i + 1}`, status: "archived", disabled: true })) : [];
    const creates = []; const controls = []; const errors = [];
    const notify = () => { sequence++; for (const response of streams) response.write(`event: change\nid: ${sequence}\ndata: {"sequence":${sequence}}\n\n`); };
    page.on("pageerror", error => errors.push(error.message));
    await page.route("**/workspace/api/**", async route => {
      const request = route.request(); const path = new URL(request.url()).pathname.replace("/workspace/api/", "");
      if (path === "agent-events") return route.continue();
      const respond = (json, status = 200) => route.fulfill({ status, contentType: "application/json", json });
      const body = request.method() === "GET" ? undefined : request.postDataJSON();
      if (body) assert.equal(request.headers()["x-veoveo-csrf-token"], "fixture-csrf");
      if (path === "agent-authoring") return respond(authoring);
      if (path === "agent-templates") return respond([template]);
      if (path === "agent-capabilities") return respond([{ name: "time__resolve_time", title: "Resolve time", description: "Admitted tool" }, { name: "other__unselected", title: "Unadmitted tool", description: "Outside template" }]);
      if (path === "agent-definitions" && !body) return respond({ items: definition ? [...archivedDefinitions(), definition] : [], next: null });
      if (path === "agent-definitions" && body) {
        assert.equal(body.source.content.execution.kind, "managed");
        content = body.source.content;
        definition = { id: body.id, name: body.name, description: body.description, owner, workContext: "operations", revision: 1, status: "enabled", disabled: false, draftDigest: digest("c"), publishedDigest: null, audience: [], updatedAt: now };
        return respond(definition);
      }
      if (path === `agent-definitions/${definition?.id}`) return respond(definition);
      if (path.endsWith("/draft")) {
        if (body) { content = body.content; definition.revision++; definition.draftDigest = digest(definition.publishedDigest ? "f" : "d"); return respond(definition); }
        return respond({ definition: definition.id, revision: definition.revision, content });
      }
      if (path.endsWith("/revisions")) return respond({ items: history, next: null });
      if (path.endsWith("/validate")) { assert.deepEqual(content.execution.parameters, { session: "flight-one" }); return respond({ revision: definition.revision, digest: definition.draftDigest, findings: [] }); }
      if (path.endsWith("/publish")) {
        definition.revision++; definition.publishedDigest = body.digest; definition.audience = body.audience;
        history.unshift({ definition: definition.id, digest: body.digest, content: structuredClone(content), createdBy: owner, createdAt: now });
        return respond(definition);
      }
      if (path === "agent-instances" && !body) return respond({ items: instance ? [instance, ...otherInstances] : [], next: null });
      if (path === "agent-instances" && body) {
        assert.deepEqual(Object.keys(body).sort(), ["definition", "id", "name", "requestId", "revision"]);
        creates.push(body);
        if (!instance) {
          operation = { id: crypto.randomUUID(), instance: body.id, generation: 1, phase: "queued", message: null, updatedAt: now };
          instance = { id: body.id, name: body.name, definition: body.definition, owner, workContext: "operations", template: template.id, requestedRevision: body.revision, activeRevision: null, generation: 1, activeGeneration: 0, desired: "running", observed: "queued", clientId: "fixture-client", principal: crypto.randomUUID(), storageGib: 2, operation: operation.id, updatedAt: now };
          for (let n = 2; n <= 5; n++) otherInstances.push({ ...instance, id: `pilot-${n}`, name: `Field pilot ${n}`, clientId: `fixture-client-${n}`, principal: crypto.randomUUID(), operation: crypto.randomUUID(), desired: n === 5 ? "archived" : "running", observed: n === 5 ? "archived" : "ready", activeRevision: body.revision, activeGeneration: 1 });
        }
        if (loseCreateReply) { loseCreateReply = false; return route.abort("failed"); }
        assert.deepEqual(creates.at(-1), creates[0]);
        return respond(operation, 202);
      }
      if (path.startsWith("agent-operations/")) {
        const other = otherInstances.find(item => path === `agent-operations/${item.operation}`);
        return respond(other ? { id: other.operation, instance: other.id, generation: other.generation, phase: other.observed, message: null, updatedAt: now } : operation);
      }
      if (path === `agent-instances/${instance?.id}` && body) {
        assert.equal(body.expectedGeneration, instance.generation); controls.push(body);
        instance.generation++; instance.updatedAt = new Date().toISOString();
        if (body.change.kind === "state") instance.desired = body.change.desired;
        if (body.change.kind === "revision") instance.requestedRevision = body.change.revision;
        instance.observed = "draining";
        operation = { id: crypto.randomUUID(), instance: instance.id, generation: instance.generation, phase: "draining", message: "Waiting for the active episode to drain", updatedAt: instance.updatedAt };
        instance.operation = operation.id;
        return respond(operation, 202);
      }
      errors.push(`Unhandled ${request.method()} ${path}`); return route.fulfill({ status: 404 });
    });
    await page.goto(`${origin}/workspace/tests/managed-agents.html`);
    await page.getByRole("heading", { name: "Managed instances", exact: true }).waitFor();
    assert.equal(await page.getByRole("region", { name: "Agent definitions", exact: true }).isVisible(), false);
    await page.getByRole("button", { name: "Create definition", exact: true }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("combobox", { name: "Execution", exact: true }).selectOption("managed");
    await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Field pilot");
    await dialog.getByRole("textbox", { name: "Description", exact: true }).fill("Bounded managed acceptance");
    await dialog.getByRole("button", { name: "Create draft", exact: true }).click();
    await page.getByRole("textbox", { name: "Session", exact: true }).fill("flight-one");
    await page.getByRole("textbox", { name: "Instructions", exact: true }).fill("Keep ${PRIVATE_KEY} literal in these authored instructions.");
    await page.getByRole("button", { name: "Load capabilities", exact: true }).click();
    await page.getByRole("checkbox", { name: /Resolve time/ }).check();
    await page.getByRole("checkbox", { name: "fixture://changes", exact: true }).check();
    assert.equal(await page.getByText("Unadmitted tool", { exact: true }).count(), 0);
    const checkbox = await page.getByRole("checkbox", { name: /Resolve time/ }).boundingBox(); assert.ok(checkbox.width <= 20);
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    await page.getByRole("button", { name: "Review publication", exact: true }).click();
    await page.getByRole("button", { name: "Publish this revision", exact: true }).click();
    template.revision = digest("f"); notify();
    await page.getByRole("button", { name: "Use the approved template", exact: true }).click();
    assert.equal(await page.getByRole("textbox", { name: "Session", exact: true }).inputValue(), "flight-one", "template patch must retain the existing target");
    assert.ok(await page.getByRole("checkbox", { name: "fixture://changes", exact: true }).isChecked(), "template patch must retain resource subscriptions");
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    assert.equal(content.execution.templateRevision, template.revision);
    await page.getByRole("button", { name: "Review publication", exact: true }).click();
    await page.getByRole("button", { name: "Publish this revision", exact: true }).click();
    await page.getByRole("button", { name: "Deploy instance", exact: true }).click();
    await dialog.getByText("pilot:use", { exact: true }).waitFor();
    await dialog.getByText("flight-one", { exact: true }).waitFor();
    const evidence = fileURLToPath(new URL("../../../output/development/agent-management", import.meta.url));
    await mkdir(evidence, { recursive: true });
    await hardware(page);
    await dialog.screenshot({ path: `${evidence}/managed-authority-fixture.png` });
    await dialog.getByRole("textbox", { name: "Instance ID", exact: true }).fill("pilot-one");
    await dialog.getByRole("button", { name: "Deploy instance", exact: true }).click();
    await dialog.getByRole("button", { name: "Retry deployment", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
    const card = page.getByRole("article", { name: "Field pilot", exact: true });
    await card.getByText("running requested · queued", { exact: true }).waitFor();
    // Four independently controlled instances share one definition. Archived
    // definitions and instances stay out of the default views, but remain inspectable.
    assert.equal(await page.getByRole("article", { name: /^Field pilot/ }).count(), 4);
    assert.equal(await page.getByRole("button", { name: "Field pilot", exact: true }).count(), 4);
    await card.getByRole("button", { name: "Field pilot", exact: true }).click();
    const definitions = page.getByRole("region", { name: "Agent definitions", exact: true });
    assert.equal(await definitions.locator(".am-list > button").count(), 1);
    await definitions.getByRole("checkbox", { name: "Show archived definitions", exact: true }).check();
    assert.equal(await definitions.locator(".am-list > button").count(), 5);
    await definitions.getByRole("checkbox", { name: "Show archived definitions", exact: true }).uncheck();
    const instructions = definitions.getByRole("textbox", { name: "Instructions", exact: true });
    const savedInstructions = await instructions.inputValue();
    await instructions.fill("Unsaved instructions survive switching views.");
    await page.getByRole("button", { name: "Instances", exact: true }).click();
    await card.getByRole("button", { name: "Field pilot", exact: true }).click();
    assert.equal(await instructions.inputValue(), "Unsaved instructions survive switching views.");
    await instructions.fill(savedInstructions);
    await page.getByRole("button", { name: "Instances", exact: true }).click();
    instance.observed = "ready"; instance.activeRevision = instance.requestedRevision; instance.activeGeneration = 1; operation.phase = "ready"; instance.updatedAt = new Date().toISOString(); notify();
    await card.getByText("running requested · ready", { exact: true }).waitFor();
    assert.equal(streams.size, 1, "Definition and instance views share one event stream");
    await card.getByRole("button", { name: "Pause", exact: true }).click();
    await card.getByText("paused requested · draining", { exact: true }).waitFor();
    instance.observed = "paused"; operation.phase = "paused"; instance.updatedAt = new Date().toISOString(); notify();
    await card.getByText("paused requested · paused", { exact: true }).waitFor();
    await card.getByRole("button", { name: "Resume", exact: true }).click();
    await card.getByText("running requested · draining", { exact: true }).waitFor();
    history.unshift({ ...history[0], digest: digest("e"), content: { ...content, instructions: "Updated reviewed instructions." } });
    await card.getByText("Review a revision update", { exact: true }).click();
    await card.getByText("Changed: instructions.", { exact: true }).waitFor();
    // An open review receives a later publication without adopting it or changing
    // the operator's selection. The same contentless event refreshes both views.
    history.unshift({ ...history[0], digest: digest("9"), content: { ...content, instructions: "Newly published instructions." } });
    definition.publishedDigest = digest("9"); definition.revision++; notify();
    const revisions = card.getByRole("combobox", { name: "Published revision", exact: true });
    await revisions.locator(`option[value="${digest("9")}"]`).waitFor({ state: "attached" });
    assert.equal(await revisions.inputValue(), digest("e"), "publication preserves the reviewed selection");
    assert.equal(instance.requestedRevision, digest("f"), "publication does not adopt a revision");
    await revisions.selectOption(digest("9"));
    await card.getByRole("button", { name: "Apply reviewed revision", exact: true }).click();
    await page.waitForFunction(() => document.body.textContent.includes("1 active / 4 requested"));
    await card.getByRole("button", { name: "Stop current run", exact: true }).click();
    await card.getByRole("button", { name: "Confirm stop", exact: true }).click();
    await page.waitForFunction(() => document.body.textContent.includes("1 active / 5 requested"));
    authoring.permissions.instanceControl = false; notify();
    await card.getByRole("button", { name: "Stop current run", exact: true }).waitFor({ state: "hidden" });
    authoring.permissions.instanceControl = true; notify();
    await card.getByRole("button", { name: "Archive instance", exact: true }).click();
    await card.getByRole("button", { name: "Confirm archive", exact: true }).click();
    await card.waitFor({ state: "hidden" });
    await page.getByRole("checkbox", { name: "Show archived instances", exact: true }).check();
    await card.getByText("archived requested · draining", { exact: true }).waitFor();
    assert.equal(await page.getByRole("article", { name: /^Field pilot/ }).count(), 5);
    assert.deepEqual(controls.map(v => v.change.kind), ["state", "state", "revision", "stop", "state"]);
    assert.equal(creates.length, 2); assert.equal(instance.storageGib, 2);
    assert.equal(await card.getByRole("button", { name: "Resume", exact: true }).count(), 0);
    assert.equal(content.instructions, "Keep ${PRIVATE_KEY} literal in these authored instructions.");
    assert.deepEqual(errors, []);
    await hardware(page);
    await page.getByRole("region", { name: "Managed instances", exact: true }).screenshot({ path: `${evidence}/managed-lifecycle-fixture.png` });
  } finally {
    for (const response of streams) response.end();
    await context.close(); await browser.close(); await server.close();
  }
});
