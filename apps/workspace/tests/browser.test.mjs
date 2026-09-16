// Local client acceptance with explicit HTTP fixtures. This never supplies
// installation credentials and is not evidence of deployed agent execution.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const root = fileURLToPath(new URL("../", import.meta.url));

async function hardware(page) {
  const proof = await page.evaluate(async () => {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2", { failIfMajorPerformanceCaveat: true }) ?? canvas.getContext("webgl", { failIfMajorPerformanceCaveat: true });
    const debug = gl?.getExtension("WEBGL_debug_renderer_info");
    const renderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : "";
    const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "high-performance" });
    const info = adapter?.info;
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
    return { userAgent: navigator.userAgent, webgl: renderer,
      webgpu: info ? { vendor: info.vendor, architecture: info.architecture, description: info.description, fallback: info.isFallbackAdapter } : null };
  });
  assert.doesNotMatch(proof.userAgent, /HeadlessChrome/);
  const software = /swiftshader|llvmpipe|software/i;
  const gl = !!proof.webgl && !software.test(proof.webgl);
  const gpu = proof.webgpu && !proof.webgpu.fallback && !!proof.webgpu.vendor && !software.test(JSON.stringify(proof.webgpu));
  assert.ok(gl || gpu, "A headed hardware WebGPU or WebGL context is required");
  return proof;
}

test("headed Workspace supports shared authors, stable retries, ownership controls and reload", { timeout: 90_000 }, async () => {
  const server = await createServer({ root, configFile: `${root}vite.config.ts`, server: { port: 0, host: "127.0.0.1", strictPort: false } });
  const browser = await chromium.connectOverCDP(process.env.VEOVEO_BROWSER_CDP ?? "http://127.0.0.1:9222");
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  context.setDefaultTimeout(10_000);
  try {
    await server.listen();
    const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
    const alice = { id: crypto.randomUUID(), displayName: "Alice Chen" };
    const bob = { id: crypto.randomUUID(), displayName: "Bob Rivera" };
    const members = [{ id: crypto.randomUUID(), person: alice, active: true }, { id: crypto.randomUUID(), person: bob, active: true }];
    const chat = { id: crypto.randomUUID(), title: "Launch planning", owner: alice.id, archived: false, membersCanInvite: false, sequence: 3, revision: 0, updatedAt: new Date().toISOString() };
    const messages = [
      { id: crypto.randomUUID(), author: members[0].id, text: "Let's bring the launch plan together here.", replyTo: null, sequence: 2, createdAt: new Date().toISOString() },
      { id: crypto.randomUUID(), author: members[1].id, text: "I'll review the schedule and share the next steps.", replyTo: null, sequence: 3, createdAt: new Date().toISOString() },
    ];
    const agents = ["Writer", "Reviewer"].map(name => ({ id: crypto.randomUUID(), definition: name.toLowerCase(), name, provider: "Explicit browser fixture", model: "No model execution", active: true }));
    const runs = [];
    const runStarts = [];
    const operation = { id: crypto.randomUUID(), chatId: chat.id, runId: null, tool: "fixture_review", phase: "task", revision: 2, createdAt: new Date().toISOString() };
    const task = { id: "opaque-task-fixture", state: "input_required", message: "Review the requested count.", createdAt: operation.createdAt, updatedAt: operation.createdAt, ttlMs: 300000, pollIntervalMs: 5000 };
    let inputs = [{ id: "approval-1", digest: "a".repeat(64), kind: "form", message: "How many follow-ups should be prepared?", schema: { type: "object", properties: { count: { type: "integer", title: "Follow-ups", minimum: 1, maximum: 3 } }, required: ["count"] }, url: null }];
    const operationPosts = [];
    const artifact = "01a0a75d-3458-78f3-ac54-91f1cab1fea1";
    let result = null;
    let fileAllowed = false;
    let uncertain = true;
    const sends = [];
    const pages = await Promise.all([context.newPage(), context.newPage()]);
    const errors = [];
    let uploadAdmissions = 0;
    let appOperation;
    let appStarts = 0;
    const appCalls = [];
    const appDescriptor = { server: "fixture", resourceUri: "ui://fixture/task.html", standalonePath: "/apps/fixture/task.html", name: "Task workbench", description: "Explicit native bridge fixture", tools: [{ name: "start", inputSchema: { type: "object" } }], resourceDependencies: [], toolDependencies: [], agentMessageTargets: [] };
    const appCatalog = { apps: [appDescriptor], degradations: [] };
    const nativeAppTask = { resultType: "task", taskId: "opaque-app-fixture", status: "working", createdAt: new Date().toISOString(), lastUpdatedAt: new Date().toISOString(), ttlMs: 300000 };
    const appHtml = `<!doctype html><html><body><h1>Task bridge fixture</h1><button id="start">Start fixture task</button><button id="get">Read task</button><button id="cancel">Cancel task</button><output id="result"></output><script>
    let sequence = 0; const waiting = new Map(); let taskId;
    function rpc(method, params) { return new Promise((resolve, reject) => { const id = ++sequence; waiting.set(id, {resolve, reject}); parent.postMessage({jsonrpc:"2.0", id, method, params}, "*"); }); }
    addEventListener("message", event => { if (event.source !== parent) return; const pending = waiting.get(event.data.id); if (!pending) return; waiting.delete(event.data.id); event.data.error ? pending.reject(Error(event.data.error.message)) : pending.resolve(event.data.result); });
    function show(value) { document.querySelector("output").textContent = JSON.stringify(value); }
    document.querySelector("#start").onclick = async () => { try { const result = await rpc("tools/call", { name: "start", arguments: {} }); taskId = result.taskId; show(result); } catch (error) { show(error.message); } };
    document.querySelector("#get").onclick = async () => show(await rpc("tasks/get", { taskId }));
    document.querySelector("#cancel").onclick = async () => show(await rpc("tasks/cancel", { taskId }));
    rpc("ui/initialize", {protocolVersion:"2026-01-26"}).then(() => document.body.dataset.connected = "true");
    </script></body></html>`;
    for (const [index, page] of pages.entries()) {
      await hardware(page);
      const person = index === 0 ? alice : bob;
      let upload;
      page.on("request", request => { if (new URL(request.url()).pathname.startsWith("/console/")) errors.push("Workspace contacted a Console route"); });
      page.on("pageerror", error => errors.push(error.message));
      await page.route("**/workspace/api/**", async route => {
        const request = route.request();
        const path = new URL(request.url());
        let body;
        if (!["GET", "HEAD"].includes(request.method())) {
          assert.equal(request.headers()["x-veoveo-csrf-token"], "fixture-csrf");
          body = request.postData() && request.headers()["content-type"]?.includes("application/json") ? request.postDataJSON() : undefined;
        }
        const respond = json => route.fulfill({ status: 200, contentType: "application/json", headers: { "x-veoveo-csrf-token": "fixture-csrf" }, json });
        if (path.pathname.endsWith("/artifact-uploads/policy")) return respond({ allowed: true, explanation: "Fixture upload policy", actor: `fixture#${person.id}`, work_context: "default", destination_name: "Product team", access_description: "Owned by you", available_bytes: 1048576,
          policy: { max_object_bytes: 1048576, tenant_quota_bytes: 1048576, max_active_uploads_per_tenant: 8, part_bytes: 1024, max_part_bytes: 1024, max_parts: 1024, parallel_parts: 1, max_inflight_bytes: 1024, inactivity_seconds: 60, lifetime_seconds: 3600, part_timeout_seconds: 30, allowed_mime_types: ["text/plain"] } });
        if (path.pathname.includes("/artifact-uploads")) {
          if (request.method() === "POST" && path.pathname.endsWith("/artifact-uploads")) {
            uploadAdmissions++;
            upload = { upload_id: "01a0a75d-3458-78f3-ac54-91f1cab1fea2", state: "open", descriptor: body,
              layout: { part_bytes: 1024, max_parts: 1024, max_total_bytes: 1048576, parallel_parts: 1 }, accepted_bytes: 0, accepted_part_count: 0, parts: [], created_at: new Date().toISOString(), expires_at: new Date(Date.now() + 3600000).toISOString() };
          }
          assert.ok(upload);
          if (request.method() === "PUT") {
            const bytes = request.postDataBuffer();
            const part = { part_number: 1, byte_len: bytes.length, sha256: request.headers()["x-veoveo-part-sha256"] };
            assert.equal(bytes.toString(), "Workspace file acceptance");
            upload.parts = [part]; upload.accepted_bytes = bytes.length; upload.accepted_part_count = 1;
            return respond(part);
          }
          if (path.pathname.endsWith("/complete")) {
            assert.equal(body.byte_len, upload.accepted_bytes);
            upload.state = "completed";
            upload.receipt = { upload_id: upload.upload_id, artifact_id: "01a0a75d-3458-78f3-ac54-91f1cab1fea3", artifact_uri: "artifact://01a0a75d-3458-78f3-ac54-91f1cab1fea3", filename: upload.descriptor.filename, mime_type: "text/plain", byte_len: upload.accepted_bytes, sha256: upload.parts[0].sha256, created_at: new Date().toISOString() };
          }
          return respond(upload);
        }
        if (path.pathname === `/workspace/api/artifacts/${artifact}/preview`) {
          if (!fileAllowed) return route.fulfill({ status: 403 });
          const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64");
          return route.fulfill({ status: 200, contentType: "image/png", headers: { "content-length": String(png.length) }, body: request.method() === "HEAD" ? "" : png });
        }
        if (path.pathname === "/workspace/api/apps") return respond(appCatalog);
        if (path.pathname === "/workspace/api/apps/events") return route.fulfill({ contentType: "text/event-stream", body: `retry: 250\nevent: catalog\ndata: ${JSON.stringify(appCatalog)}\n\n` });
        if (path.pathname === "/workspace/api/apps/frame") return route.fulfill({ contentType: "text/html", body: appHtml });
        if (path.pathname.endsWith("/app-operations") && request.method() === "POST") {
          assert.equal(body.appUri, appDescriptor.resourceUri); assert.equal(body.tool, "start"); appStarts++;
          appOperation = { id: body.id, chatId: chat.id, runId: null, tool: "fixture__start", phase: "task", revision: 2, createdAt: new Date().toISOString() };
          return respond(appOperation);
        }
        if (path.pathname.includes("/app-operations/")) { assert.equal(body.appUri, appDescriptor.resourceUri); return respond({ operation: appOperation, native: nativeAppTask }); }
        if (path.pathname.includes("/app-tasks/")) {
          assert.equal(body.appUri, appDescriptor.resourceUri); assert.equal(body.taskId, nativeAppTask.taskId); appCalls.push(path.pathname);
          return respond(path.pathname.endsWith("/get") ? { ...nativeAppTask, resultType: "complete" } : { resultType: "complete" });
        }
        if (appOperation && path.pathname === `/workspace/api/operations/${appOperation.id}`) return respond({ operation: appOperation, task: { id: nativeAppTask.taskId, state: nativeAppTask.status, message: "App Task continues independently.", createdAt: nativeAppTask.createdAt, updatedAt: nativeAppTask.lastUpdatedAt, ttlMs: 300000, pollIntervalMs: 5000 }, inputs: [], result: null });
        if (path.pathname.endsWith("/session")) return respond({ person, principalId: `fixture#${person.id}`, tenantId: "test", tenantName: "Shared work", workContext: "default", workContextTitle: "Product team", canContribute: true });
        if (path.pathname.endsWith("/operations")) return respond({ items: index === 0 ? [...(appOperation ? [appOperation] : []), operation] : [], next: null });
        if (path.pathname.includes(`/operations/${operation.id}`)) {
          if (request.method() === "POST") {
            operationPosts.push(path.pathname);
            if (path.pathname.endsWith("/input")) { assert.equal(body.answers[0].content.count, 2); assert.equal(body.answers[0].id, "approval-1"); inputs = []; task.state = "working"; task.message = "Preparing follow-ups."; }
            if (path.pathname.endsWith("/cancel")) { task.message = "Cancellation requested."; }
            return route.fulfill({ status: 204 });
          }
          return respond({ operation, task, inputs, result });
        }
        if (path.pathname.endsWith("/events")) return route.fulfill({ contentType: "text/event-stream", body: `retry: 250\nevent: change\ndata: {"sequence":${chat.sequence}}\n\n` });
        if (path.pathname.endsWith("/activity")) return respond({ agents, runs });
        if (path.pathname.endsWith("/agents")) return respond(agents.map(agent => ({ id: agent.definition, name: agent.name, description: "Explicit browser fixture", provider: agent.provider, model: agent.model, tools: [] })));
        if (path.pathname.endsWith("/runs")) {
          runStarts.push(body);
          let run = runs.find(run => run.agent === body.agent && run.trigger === body.trigger);
          if (!run) { run = { ...body, id: crypto.randomUUID(), initiator: person.id, state: "running", text: body.agent === agents[0].id ? "I am drafting the launch notes." : "I am reviewing the schedule.", failure: null, sequence: ++chat.sequence, updatedSequence: chat.sequence, createdAt: new Date().toISOString() }; runs.push(run); }
          return respond(run);
        }
        if (path.pathname.endsWith("/cancel")) {
          const run = runs.find(run => path.pathname.includes(run.id));
          assert.ok(run); run.state = "cancelled"; run.updatedSequence = ++chat.sequence;
          return respond(run);
        }
        if (path.pathname.endsWith("/invitations")) return respond([]);
        if (path.pathname.endsWith("/chats")) return respond([chat]);
        if (path.pathname.endsWith("/messages")) {
          sends.push(body.id);
          let message = messages.find(message => message.id === body.id);
          if (!message) { message = { ...body, author: members[index].id, sequence: ++chat.sequence, createdAt: new Date().toISOString() }; messages.push(message); }
          if (uncertain) { uncertain = false; return route.abort("failed"); }
          return respond(message);
        }
        if (path.pathname.endsWith(`/chats/${chat.id}`)) {
          if (request.method() === "PUT") { Object.assign(chat, body, { revision: chat.revision + 1, sequence: chat.sequence + 1 }); delete chat.expectedRevision; return respond(chat); }
          return respond({ chat, members, messages: path.searchParams.has("after") ? messages.filter(message => message.sequence > Number(path.searchParams.get("after"))) : messages });
        }
        throw new Error(`Unexpected fixture route ${request.method()} ${path.pathname}`);
      });
      await page.goto(`${origin}/workspace/?chat=${chat.id}`);
      await page.getByRole("textbox", { name: "Message", exact: true }).waitFor();
      await page.getByText("Let's bring the launch plan together here.", { exact: true }).waitFor();
    }
    const [owner, member] = pages;
    await owner.getByRole("button", { name: "Uploads", exact: true }).click();
    await owner.locator("#upload-files").setInputFiles({ name: "workspace-acceptance.txt", mimeType: "text/plain", buffer: Buffer.from("Workspace file acceptance") });
    await owner.getByRole("button", { name: "Upload 1 file", exact: true }).click();
    await owner.getByText("Ready", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("link", { name: "Download", exact: true }).getAttribute("href"), "/workspace/api/artifacts/01a0a75d-3458-78f3-ac54-91f1cab1fea3/download");
    await owner.getByRole("button", { name: "Close uploads; transfers continue", exact: true }).last().click();
    assert.equal(await owner.locator(".message-author strong").allTextContents().then(names => names.join(",")), "Alice Chen,Bob Rivera");
    await owner.getByRole("checkbox", { name: "Writer", exact: true }).check();
    await owner.getByRole("checkbox", { name: "Reviewer", exact: true }).check();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("A message with an uncertain response");
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.getByRole("alert").waitFor();
    await owner.getByRole("button", { name: "Retry message", exact: true }).click();
    await owner.waitForFunction(() => document.querySelector("textarea").value === "");
    assert.equal(sends.length, 2);
    assert.equal(sends[0], sends[1]);
    assert.equal(messages.length, 3);
    await member.getByText("A message with an uncertain response", { exact: true }).waitFor();
    await owner.getByRole("button", { name: "Participants", exact: true }).click();
    await owner.getByText("Owner controls", { exact: true }).waitFor();
    await member.getByRole("button", { name: "Participants", exact: true }).click();
    assert.equal(await member.getByText("Owner controls", { exact: true }).count(), 0);
    await owner.getByRole("button", { name: "Stop Writer's response", exact: true }).waitFor();
    await owner.getByRole("button", { name: "Stop Reviewer's response", exact: true }).waitFor();
    await member.getByRole("textbox", { name: "Message", exact: true }).fill("I can keep writing while both agents respond.");
    await member.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.getByText("I can keep writing while both agents respond.", { exact: true }).waitFor();
    await owner.getByRole("button", { name: "Stop Writer's response", exact: true }).click();
    await owner.getByText("Response stopped", { exact: true }).waitFor();
    assert.equal(runs[1].state, "running");
    assert.equal(runStarts.length, 2);
    await owner.reload();
    await owner.getByText("A message with an uncertain response", { exact: true }).waitFor();
    assert.equal(sends.length, 3, "reload cannot execute another send");
    await owner.getByRole("button", { name: "Stop Reviewer's response", exact: true }).waitFor();
    assert.equal(runStarts.length, 2, "reload cannot dispatch another run");
    await owner.getByRole("button", { name: /^Uploads/ }).click();
    await owner.getByText("Ready", { exact: true }).waitFor();
    assert.equal(uploadAdmissions, 1, "reload restores a receipt without another upload admission");
    await owner.getByRole("button", { name: "Close uploads; transfers continue", exact: true }).last().click();
    await owner.getByRole("button", { name: "Participants", exact: true }).click();
    await owner.bringToFront();
    const proof = await hardware(owner);
    assert.deepEqual(errors, []);
    const output = new URL("../../../output/workspace-client-local.png", import.meta.url);
    await mkdir(new URL(".", output), { recursive: true });
    await owner.screenshot({ path: fileURLToPath(output) });
    console.log(JSON.stringify({ evidence: "local HTTP fixture, not installed execution", proof, distinctAuthors: 4, committedMessages: messages.length, sendAttempts: sends.length }));
    await owner.setViewportSize({ width: 390, height: 844 });
    await owner.getByRole("button", { name: "Close chat details" }).click();
    assert.equal(await owner.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await owner.bringToFront();
    await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-client-mobile.png", import.meta.url)) });
    await owner.setViewportSize({ width: 1440, height: 1000 });
    await owner.getByRole("button", { name: "Activity", exact: true }).click();
    console.log(JSON.stringify({ step: "open task input" }));
    operation.runId = runs[0].id;
    await owner.getByText("Needs your input", { exact: true }).waitFor();
    await owner.getByRole("spinbutton", { name: "Follow-ups" }).fill("2");
    console.log(JSON.stringify({ step: "submit task input" }));
    await owner.getByRole("button", { name: "Continue", exact: true }).click();
    await owner.getByText("Preparing follow-ups.", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("spinbutton", { name: "Follow-ups" }).count(), 0);
    await owner.reload();
    await owner.getByRole("button", { name: "Activity", exact: true }).click();
    await owner.getByText("Preparing follow-ups.", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 1, "reload does not resubmit input or work");
    console.log(JSON.stringify({ step: "cancel restored task" }));
    await owner.getByRole("button", { name: "Cancel task", exact: true }).click();
    await owner.getByText("Waiting for the server to confirm the outcome.", { exact: true }).waitFor();
    assert.equal(await owner.getByText("Cancelled", { exact: true }).count(), 0, "cancel acknowledgement is not terminal");
    assert.equal(runs[1].state, "running", "task cancellation does not stop the agent response");
    task.state = "cancelled"; task.message = "Cancelled by request.";
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await member.getByRole("button", { name: "Activity", exact: true }).click();
    await member.getByText("No activity yet", { exact: true }).waitFor();
    await owner.getByRole("button", { name: "My activity", exact: true }).click();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await owner.reload();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 2);
    await owner.bringToFront();
    await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-tasks-local.png", import.meta.url)) });
    await owner.getByRole("link", { name: chat.title, exact: true }).click();
    await owner.getByRole("region", { name: "Your activity in this chat", exact: true }).waitFor();
    await owner.getByText("Writer · Requested for you", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 2, "opening the originating chat restores the receipt without dispatch");
    task.state = "completed";
    result = { isError: false, text: [], resources: [{ uri: `media://artifact/${artifact}`, name: "Generated image", mimeType: "image/png" }], structured: null };
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByRole("button", { name: "Preview", exact: true }).click();
    await owner.getByText("This file is unavailable with your current access. A chat or Task link does not grant file access.", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("img", { name: "Generated image", exact: true }).count(), 0);
    fileAllowed = true;
    await owner.getByRole("button", { name: "Preview", exact: true }).click();
    await owner.getByRole("img", { name: "Generated image", exact: true }).waitFor();
    await owner.waitForFunction(() => document.querySelector(".task-image")?.naturalWidth === 1);
    assert.equal(await owner.getByRole("link", { name: "Download", exact: true }).getAttribute("href"), `/workspace/api/artifacts/${artifact}/download`);
    assert.equal(operationPosts.length, 2, "preview cannot invoke the original tool");
    await owner.getByRole("button", { name: "Close preview", exact: true }).click();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("Keep this unsent draft");
    await owner.getByRole("button", { name: "Apps", exact: true }).click();
    await owner.getByRole("button", { name: /Task workbench/ }).click();
    const frame = owner.frameLocator(".workspace-app-frame");
    await frame.locator('body[data-connected="true"]').waitFor();
    assert.equal(await owner.locator(".workspace-app-frame").getAttribute("sandbox"), "allow-scripts");
    await frame.getByRole("button", { name: "Start fixture task", exact: true }).click();
    await frame.getByText('"taskId":"opaque-app-fixture"', { exact: false }).waitFor();
    await frame.getByRole("button", { name: "Read task", exact: true }).click();
    await frame.getByText('"resultType":"complete"', { exact: false }).waitFor();
    await frame.getByRole("button", { name: "Cancel task", exact: true }).click();
    await frame.getByText('{"resultType":"complete"}', { exact: true }).waitFor();
    assert.equal(appStarts, 1); assert.deepEqual(appCalls, ["/workspace/api/app-tasks/get", "/workspace/api/app-tasks/cancel"]);
    await owner.getByRole("button", { name: "Back to chat", exact: true }).click();
    assert.equal(await owner.getByRole("textbox", { name: "Message", exact: true }).inputValue(), "Keep this unsent draft");
    await owner.goto(`${origin}/workspace/?chat=${chat.id}&panel=activity`);
    await owner.getByRole("article", { name: "Activity: fixture__start", exact: true }).getByText("App Task continues independently.", { exact: true }).waitFor().catch(async error => { console.log(JSON.stringify({ fixtureBody: await owner.locator("body").innerText(), errors })); throw error; });
    assert.equal(appStarts, 1, "closing the App and reloading recovers the journal without repeating tools/call");
    await owner.bringToFront(); await hardware(owner);
    assert.deepEqual(errors, []);

  } finally {
    await context.close(); await browser.close(); await server.close();
  }
});
