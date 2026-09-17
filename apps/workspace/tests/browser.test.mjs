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
    const chat = { id: crypto.randomUUID(), title: "Launch planning", owner: alice.id, archived: false, membersCanInvite: false, participation: { mode: "on_request", agents: [] }, sequence: 3, revision: 0, updatedAt: new Date().toISOString() };
    const messages = [
      { id: crypto.randomUUID(), author: members[0].id, text: "Let's bring the launch plan together here.", replyTo: null, attachments: [], addressedAgents: [], responseAgents: [], sequence: 2, createdAt: new Date().toISOString() },
      { id: crypto.randomUUID(), author: members[1].id, text: "I'll review the schedule and share the next steps.", replyTo: null, attachments: [], addressedAgents: [], responseAgents: [], sequence: 3, createdAt: new Date().toISOString() },
    ];
    const agents = ["Writer", "Reviewer"].map(name => ({ id: crypto.randomUUID(), definition: name.toLowerCase(), name, provider: "Explicit browser fixture", model: "No model execution", active: true }));
    const runs = [];
    const runStarts = [];
    const operation = { id: crypto.randomUUID(), chatId: chat.id, runId: null, agent: null, tool: "fixture_review", phase: "task", revision: 2, createdAt: new Date().toISOString() };
    const task = { id: "opaque-task-fixture", state: "input_required", message: "Review the requested count.", createdAt: operation.createdAt, updatedAt: operation.createdAt, ttlMs: 300000, pollIntervalMs: 5000 };
    let inputs = [
      { id: "approval-1", digest: "a".repeat(64), kind: "form", message: "How many follow-ups should be prepared?", schema: { type: "object", properties: { count: { type: "integer", title: "Follow-ups", minimum: 1, maximum: 3 } }, required: ["count"] }, url: null },
      { id: "notes", digest: "b".repeat(64), kind: "form", message: "Add a note for the follow-ups.", schema: { type: "object", properties: { note: { type: "string", title: "Follow-up note" } }, required: ["note"] }, url: null },
      { id: "complex", digest: "c".repeat(64), kind: "form", message: "Review the nested details.", schema: { type: "object", properties: { nested: { type: "object" } } }, url: null },
    ];
    let changeInput = true;
    const operationPosts = [];
    const artifact = "01a0a75d-3458-78f3-ac54-91f1cab1fea1";
    let result = null;
    let fileAllowed = false;
    let resultAllowed = true;
    let uncertain = true;
    const sends = [];
    let expireOwnerStream = false;
    let revokeOwner = false;
    let ownerRevoked = false;
    let messageAfterReconnect;
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
        if (index === 0 && path.pathname === `/workspace/api/chats/${chat.id}/events`) {
          if (ownerRevoked) return route.fulfill({ status: 403 });
          if (expireOwnerStream || revokeOwner) {
            expireOwnerStream = false; ownerRevoked = revokeOwner;
            return route.fulfill({ contentType: "text/event-stream", body: "retry: 250\nevent: expired\ndata: {}\n\n" });
          }
          if (messageAfterReconnect) {
            messages.push({ ...messageAfterReconnect, sequence: ++chat.sequence });
            messageAfterReconnect = undefined;
          }
        }
        if (index === 0 && ownerRevoked && path.pathname.startsWith(`/workspace/api/chats/${chat.id}`)) return route.fulfill({ status: 403 });
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
          if (!fileAllowed || index !== 0) return route.fulfill({ status: 403 });
          const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64");
          return route.fulfill({ status: 200, contentType: "image/png", headers: { "content-length": String(png.length) }, body: request.method() === "HEAD" ? "" : png });
        }
        if (path.pathname === "/workspace/api/artifacts/01a0a75d-3458-78f3-ac54-91f1cab1fea3/preview") {
          return route.fulfill(index === 0 ? { status: 200, contentType: "text/plain", headers: { "content-length": "25" }, body: "" } : { status: 403 });
        }
        if (path.pathname === "/workspace/api/apps") return respond(appCatalog);
        if (path.pathname === "/workspace/api/apps/events") return route.fulfill({ contentType: "text/event-stream", body: `retry: 250\nevent: catalog\ndata: ${JSON.stringify(appCatalog)}\n\n` });
        if (path.pathname === "/workspace/api/apps/frame") return route.fulfill({ contentType: "text/html", body: appHtml });
        if (path.pathname.endsWith("/app-operations") && request.method() === "POST") {
          assert.equal(body.appUri, appDescriptor.resourceUri); assert.equal(body.tool, "start"); appStarts++;
          appOperation = { id: body.id, chatId: chat.id, runId: null, agent: null, tool: "fixture__start", phase: "task", revision: 2, createdAt: new Date().toISOString() };
          return respond(appOperation);
        }
        if (path.pathname.includes("/app-operations/")) { assert.equal(body.appUri, appDescriptor.resourceUri); return respond({ operation: appOperation, native: nativeAppTask }); }
        if (path.pathname.includes("/app-tasks/")) {
          assert.equal(body.appUri, appDescriptor.resourceUri); assert.equal(body.taskId, nativeAppTask.taskId); appCalls.push(path.pathname);
          return respond(path.pathname.endsWith("/get") ? { ...nativeAppTask, resultType: "complete" } : { resultType: "complete" });
        }
        if (appOperation && path.pathname === `/workspace/api/operations/${appOperation.id}`) return respond({ operation: appOperation, task: { id: nativeAppTask.taskId, state: nativeAppTask.status, message: "App Task continues independently.", createdAt: nativeAppTask.createdAt, updatedAt: nativeAppTask.lastUpdatedAt, ttlMs: 300000, pollIntervalMs: 5000 }, inputs: [], result: null });
        if (path.pathname === "/workspace/api/events") {
          const owned = index === 0 && resultAllowed ? [...(appOperation ? [appOperation] : []), operation] : [];
          const events = [{ kind: "inventory", operations: owned.map(({ id, phase, revision }) => ({ id, phase, revision })), invitations: 0, limited: false }];
          for (const item of owned) if (item.phase === "task") events.push({ kind: "task", operation: item.id,
            state: item.id === operation.id ? task.state : nativeAppTask.status, updatedAt: task.updatedAt });
          events.push({ kind: "availability", liveTasks: true });
          return route.fulfill({ contentType: "text/event-stream", body: "retry: 1000\n" + events.map(event => `event: personal\ndata: ${JSON.stringify(event)}\n\n`).join("") });
        }
        if (path.pathname.endsWith("/session")) return respond({ person, principalId: `fixture#${person.id}`, tenantId: "test", tenantName: "Shared work", workContext: "default", workContextTitle: "Product team", canContribute: true });
        if (path.pathname.endsWith("/operations")) return respond({ items: index === 0 ? [...(appOperation ? [appOperation] : []), operation] : [], next: null });
        if (path.pathname.includes(`/operations/${operation.id}`)) {
          if (request.method() === "GET" && !resultAllowed) return route.fulfill({ status: 403 });
          if (request.method() === "POST") {
            operationPosts.push(path.pathname);
            if (path.pathname.endsWith("/input")) {
              assert.equal(body.revision, operation.revision);
              if (operation.phase === "input_required") {
                assert.deepEqual(body.answers.map(answer => ({ id: answer.id, digest: answer.digest, decision: answer.decision, content: answer.content })), [
                  { id: "sync-name", digest: "e".repeat(64), decision: "accept", content: { name: "A plan" } },
                  { id: "sync-count", digest: "f".repeat(64), decision: "accept", content: { count: 2 } },
                ], "synchronous continuation preserves its complete input batch");
                inputs = []; operation.phase = "completed"; operation.revision++;
                return route.fulfill({ status: 204 });
              }
              assert.equal(body.answers.length, 1, "one decision never answers a different request");
              const answer = body.answers[0];
              const input = inputs.find(input => input.id === answer.id);
              assert.ok(input); assert.equal(answer.digest, input.digest);
              if (answer.id === "approval-1" && changeInput) {
                changeInput = false; operation.revision++; input.digest = "d".repeat(64); input.schema.properties.count.minimum = 3;
                return route.fulfill({ status: 409 });
              }
              if (answer.id === "complex") { assert.equal(answer.decision, "decline"); assert.equal(answer.content, null); }
              else {
                assert.equal(answer.decision, "accept");
                assert.deepEqual(answer.content, answer.id === "approval-1" ? { count: 3 } : { note: "Keep this answer while the other request changes." });
              }
              inputs = inputs.filter(input => input.id !== answer.id); operation.revision++;
              if (!inputs.length) { task.state = "working"; task.message = "Preparing follow-ups."; }
              // The server accepted the decline, but its reply is lost. Recovery
              // must read status and preserve other drafts, without replaying it.
              if (answer.id === "complex") return route.abort("failed");
            }
            if (path.pathname.endsWith("/cancel")) { task.message = "Cancellation requested."; }
            return route.fulfill({ status: 204 });
          }
          return respond({ operation, task: operation.phase === "task" ? task : null, inputs, result });
        }
        if (path.pathname.endsWith("/events")) return route.fulfill({ contentType: "text/event-stream", body: `retry: 250\nevent: change\ndata: {"sequence":${chat.sequence}}\n\n` });
        if (path.pathname.endsWith("/activity")) return respond({ agents, runs });
        if (path.pathname.endsWith("/agents")) return respond(agents.map(agent => ({ id: agent.definition, name: agent.name, description: "Explicit browser fixture", provider: agent.provider, model: agent.model, tools: [] })));
        if (path.pathname.endsWith("/runs")) throw new Error("The browser must admit message and runs in one request");
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
          if (!message) {
            const targets = chat.participation.mode === "automatic" ? [...new Set([...body.addressedAgents, ...chat.participation.agents])]
              : chat.participation.mode === "default" && !body.addressedAgents.length ? chat.participation.agents : body.addressedAgents;
            let replyContext = null;
            assert.equal(body.replyContext, undefined, "the client never supplies a quote");
            if (body.replyTo) {
              const target = (body.replyTo.kind === "message" ? messages : runs).find(target => target.id === body.replyTo.id);
              assert.ok(target);
              const authorName = body.replyTo.kind === "message" ? members.find(member => member.id === target.author).person.displayName : agents.find(agent => agent.id === target.agent).name;
              if (body.replyTo.kind === "response") assert.ok(!["queued", "running"].includes(target.state));
              replyContext = { authorName, text: [...target.text].slice(0, 500).join("") };
            }
            message = { ...body, replyContext, responseAgents: targets, author: members[index].id, sequence: ++chat.sequence, createdAt: new Date().toISOString() }; messages.push(message);
            for (const agent of targets) {
              runStarts.push({ agent, trigger: body.id });
              runs.push({ agent, trigger: body.id, id: crypto.randomUUID(), initiator: person.id, state: "running", feedback: { phase: "responding", operations: 0 }, text: agent === agents[0].id ? "I am drafting the launch notes." : "I am reviewing the schedule.", failure: null, sequence: ++chat.sequence, updatedSequence: chat.sequence, createdAt: new Date().toISOString() });
            }
          } else { assert.equal(message.text, body.text); assert.deepEqual(message.replyTo, body.replyTo);
            assert.deepEqual(message.attachments, body.attachments); assert.deepEqual(message.addressedAgents, body.addressedAgents); }
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
    await member.locator(".message-text").getByText("A message with an uncertain response", { exact: true }).waitFor();
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
    assert.equal(runs.find(run => run.agent === agents[1].id).state, "running");
    assert.equal(runStarts.length, 2);
    await owner.reload();
    await owner.locator(".message-text").getByText("A message with an uncertain response", { exact: true }).waitFor();
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
    assert.equal(await owner.locator(".chat-header .actions button").evaluateAll(buttons => buttons.every(button => {
      const box = button.getBoundingClientRect(); return box.left >= 0 && box.right <= innerWidth;
    })), true, "all mobile chat controls remain reachable");
    await owner.bringToFront();
    await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-client-mobile.png", import.meta.url)) });
    await owner.setViewportSize({ width: 1440, height: 1000 });
    await owner.getByRole("button", { name: "Activity", exact: true }).click();
    console.log(JSON.stringify({ step: "open task input" }));
    operation.runId = crypto.randomUUID(); // Retained Task from a run outside the recent chat window.
    operation.agent = { id: agents[0].id, name: agents[0].name };
    await owner.getByText("Needs your input", { exact: true }).waitFor();
    await owner.getByRole("spinbutton", { name: "Follow-ups" }).fill("2");
    await owner.getByRole("textbox", { name: "Follow-up note" }).fill("Keep this answer while the other request changes.");
    console.log(JSON.stringify({ step: "independent task inputs and lost decline reply" }));
    await owner.getByRole("button", { name: "Decline request", exact: true }).click();
    await owner.getByText("Your answer could not be confirmed. The current request has been refreshed.", { exact: true }).waitFor();
    await owner.getByRole("button", { name: "Decline request", exact: true }).waitFor({ state: "detached" });
    assert.equal(await owner.getByRole("spinbutton", { name: "Follow-ups" }).inputValue(), "2");
    assert.equal(operationPosts.length, 1, "lost reply does not replay the decline");
    const countRequest = owner.getByRole("form", { name: "How many follow-ups should be prepared?", exact: true });
    await countRequest.getByRole("button", { name: "Continue", exact: true }).click();
    await owner.getByText("This input request changed. Review the current request below.", { exact: true }).waitFor();
    await owner.waitForFunction(() => document.querySelector('input[type="number"]')?.min === "3");
    assert.equal(await owner.getByRole("spinbutton", { name: "Follow-ups" }).inputValue(), "", "a changed request requires a fresh answer");
    assert.equal(await owner.getByRole("textbox", { name: "Follow-up note" }).inputValue(), "Keep this answer while the other request changes.", "an unchanged request retains its draft");
    await owner.getByRole("spinbutton", { name: "Follow-ups" }).fill("3");
    await countRequest.getByRole("button", { name: "Continue", exact: true }).click();
    await countRequest.waitFor({ state: "detached" });
    assert.equal(await owner.getByRole("textbox", { name: "Follow-up note" }).inputValue(), "Keep this answer while the other request changes.");
    await owner.getByRole("form", { name: "Add a note for the follow-ups.", exact: true }).getByRole("button", { name: "Continue", exact: true }).click();
    await owner.getByText("Preparing follow-ups.", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("spinbutton", { name: "Follow-ups" }).count(), 0);
    await owner.reload();
    await owner.getByRole("button", { name: "Activity", exact: true }).click();
    await owner.getByText("Preparing follow-ups.", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 4, "reload does not resubmit input or work");
    console.log(JSON.stringify({ step: "cancel restored task" }));
    await owner.getByRole("button", { name: "Cancel task", exact: true }).click();
    await owner.getByText("Waiting for the server to confirm the outcome.", { exact: true }).waitFor();
    assert.equal(await owner.getByText("Cancelled", { exact: true }).count(), 0, "cancel acknowledgement is not terminal");
    assert.equal(runs.find(run => run.agent === agents[1].id).state, "running", "task cancellation does not stop the agent response");
    task.state = "cancelled"; task.message = "Cancelled by request.";
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await member.getByRole("button", { name: "Activity", exact: true }).click();
    await member.getByText("No activity yet", { exact: true }).waitFor();
    await owner.getByRole("button", { name: /^My activity/ }).click();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await owner.locator(".task-origin").filter({ hasText: "Writer · Requested for you" }).waitFor();
    await owner.reload();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await owner.locator(".task-origin").filter({ hasText: "Writer · Requested for you" }).waitFor();
    assert.equal(operationPosts.length, 5);
    operation.phase = "input_required"; operation.revision++;
    inputs = [
      { id: "sync-name", digest: "e".repeat(64), kind: "form", message: "Name this plan.", schema: { type: "object", properties: { name: { type: "string", title: "Plan name" } }, required: ["name"] }, url: null },
      { id: "sync-count", digest: "f".repeat(64), kind: "form", message: "Choose its size.", schema: { type: "object", properties: { count: { type: "integer", title: "Plan size" } }, required: ["count"] }, url: null },
    ];
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByRole("textbox", { name: "Plan name" }).fill("A plan");
    await owner.getByRole("spinbutton", { name: "Plan size" }).fill("2");
    await owner.getByRole("button", { name: "Continue", exact: true }).click();
    await owner.getByText("Completed", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 6);
    operation.phase = "task";
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByText("Cancelled", { exact: true }).waitFor();
    await owner.bringToFront();
    await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-tasks-local.png", import.meta.url)) });
    await owner.getByRole("link", { name: chat.title, exact: true }).click();
    await owner.getByRole("region", { name: "Your activity in this chat", exact: true }).waitFor();
    await owner.getByText("Writer · Requested for you", { exact: true }).waitFor();
    assert.equal(operationPosts.length, 6, "opening the originating chat restores the receipt without dispatch");
    task.state = "completed";
    result = { isError: false, text: [], resources: [{ uri: `media://artifact/${artifact}`, name: "Generated image", mimeType: "image/png" }], images: [], omittedImages: 0, structured: null };
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByRole("button", { name: "Preview", exact: true }).click();
    await owner.getByText("This file is unavailable with your current access. A chat or Task link does not grant file access.", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("img", { name: "Generated image", exact: true }).count(), 0);
    fileAllowed = true;
    await owner.getByRole("button", { name: "Preview", exact: true }).click();
    await owner.getByRole("img", { name: "Generated image", exact: true }).waitFor();
    await owner.waitForFunction(() => document.querySelector(".task-image")?.naturalWidth === 1);
    assert.equal(await owner.getByRole("link", { name: "Download", exact: true }).getAttribute("href"), `/workspace/api/artifacts/${artifact}/download`);
    assert.equal(operationPosts.length, 6, "preview cannot invoke the original tool");
    await owner.getByRole("button", { name: "Close preview", exact: true }).click();
    result.images = [{ mimeType: "image/png", data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=" }];
    result.omittedImages = 1;
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByRole("img", { name: "Image result 1", exact: true }).scrollIntoViewIfNeeded();
    await owner.waitForFunction(() => document.querySelector('img[alt="Image result 1"]')?.naturalWidth === 1);
    await owner.getByText("1 image result is unavailable here.", { exact: false }).waitFor();
    await owner.goto(`${origin}/workspace/?chat=${chat.id}&panel=activity`);
    await owner.getByRole("img", { name: "Image result 1", exact: true }).scrollIntoViewIfNeeded();
    await owner.waitForFunction(() => document.querySelector('img[alt="Image result 1"]')?.naturalWidth === 1);
    assert.equal(operationPosts.length, 6, "restoring an inline image never resubmits the tool");
    resultAllowed = false;
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByText("This activity is no longer available with your access, or its retention period has ended.", { exact: true }).waitFor();
    assert.equal(await owner.getByRole("img", { name: "Image result 1", exact: true }).count(), 0, "denied refresh detaches previously visible image bytes");
    resultAllowed = true;
    await owner.getByRole("button", { name: "Refresh task", exact: true }).click();
    await owner.getByRole("img", { name: "Image result 1", exact: true }).waitFor();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("Keep this unsent draft");
    await owner.getByRole("button", { name: "Apps", exact: true }).click();
    await owner.getByRole("button", { name: /Task workbench/ }).click();
    const frame = owner.frameLocator(".workspace-app-frame");
    await frame.locator('body[data-connected="true"]').waitFor();
    assert.equal(await owner.locator(".workspace-app-frame").getAttribute("sandbox"), "allow-scripts");
    await frame.getByRole("button", { name: "Start fixture task", exact: true }).click();
    await frame.getByText('"taskId":"opaque-app-fixture"', { exact: false }).waitFor().catch(async error => { console.log(JSON.stringify({ appStartFailure: await frame.locator("body").innerText(), appStarts, appCalls, errors })); throw error; });
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
    await owner.getByRole("button", { name: "Participants", exact: true }).click();
    await owner.getByRole("combobox", { name: "Agent participation" }).selectOption("default");
    await owner.getByRole("radio", { name: "Writer", exact: true }).check();
    await owner.getByRole("button", { name: "Save participation" }).click();
    await owner.getByText("Will respond: Writer.", { exact: false }).waitFor();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("@Reviewer: Please check this message.");
    await owner.getByText("Will respond: Reviewer.", { exact: false }).waitFor();
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.waitForFunction(() => document.querySelector('textarea[aria-label="Message"]')?.value === "");
    assert.deepEqual(messages.at(-1).addressedAgents, [agents[1].id]);
    assert.deepEqual(messages.at(-1).responseAgents, [agents[1].id]);
    await owner.getByRole("combobox", { name: "Agent participation" }).selectOption("automatic");
    await owner.getByRole("group", { name: "Agent responses", exact: true }).getByRole("checkbox", { name: "Reviewer", exact: true }).check();
    await owner.getByRole("button", { name: "Save participation" }).click();
    await owner.getByText("Will respond: Writer, Reviewer.", { exact: false }).waitFor();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("Both agents can help with this.");
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.locator(".message-text").getByText("Both agents can help with this.", { exact: true }).waitFor();
    assert.deepEqual(messages.at(-1).responseAgents, agents.map(agent => agent.id));
    const admittedRuns = runs.length;
    await owner.reload();
    await owner.getByRole("button", { name: "Participants", exact: true }).click();
    assert.equal(await owner.getByRole("combobox", { name: "Agent participation" }).inputValue(), "automatic");
    assert.equal(runs.length, admittedRuns, "recovery must not resolve automatic participation again");
    await owner.getByRole("group", { name: "Ask an agent", exact: true }).getByRole("checkbox", { name: "Writer", exact: true }).check();
    agents[0].active = false;
    chat.participation = { mode: "automatic", agents: [agents[1].id] }; chat.revision++; chat.sequence++;
    await owner.getByRole("button", { name: "Clear unavailable agents", exact: true }).click();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("A human can continue after an agent leaves.");
    assert.equal(await owner.getByRole("button", { name: "Send message", exact: true }).isEnabled(), true);
    assert.equal(await owner.getByRole("button", { name: "Clear unavailable agents", exact: true }).count(), 0);
    assert.equal(runs.length, admittedRuns, "clearing a stale selection cannot dispatch work");
    await owner.bringToFront(); await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-participation-local.png", import.meta.url)) });
    assert.deepEqual(errors, []);

    await owner.getByRole("button", { name: "Close chat details", exact: true }).click();
    chat.participation = { mode: "on_request", agents: [] }; chat.revision++; chat.sequence++;
    const reviewer = runs.find(run => run.agent === agents[1].id);
    const response = owner.locator(`[data-message-id="response:${reviewer.id}"]`);
    assert.equal(await response.getByRole("button", { name: "Reply to Reviewer", exact: true }).isDisabled(), true);
    reviewer.state = "completed"; chat.sequence++;
    await owner.getByText("No agent response requested.", { exact: false }).waitFor();
    const original = messages[1];
    await owner.locator(`[data-message-id="message:${original.id}"]`).getByRole("button", { name: "Reply to Bob Rivera", exact: true }).click();
    await owner.getByRole("region", { name: "Reply context", exact: true }).getByText("Reply to Bob Rivera", { exact: true }).waitFor();
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("A reply with an interrupted confirmation.");
    uncertain = true;
    const beforeReply = sends.length;
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.getByRole("alert").waitFor();
    assert.equal(await owner.getByRole("button", { name: "Remove reply", exact: true }).isDisabled(), true);
    await owner.getByRole("button", { name: "Retry message", exact: true }).click();
    await owner.waitForFunction(() => document.querySelector('textarea[aria-label="Message"]')?.value === "");
    assert.equal(sends.length, beforeReply + 2); assert.equal(sends.at(-1), sends.at(-2));
    const humanReply = messages.at(-1);
    assert.deepEqual(humanReply.replyTo, { kind: "message", id: original.id });
    assert.deepEqual(humanReply.responseAgents, []);
    await response.getByRole("button", { name: "Reply to Reviewer", exact: true }).click();
    assert.equal(await owner.getByRole("group", { name: "Ask an agent", exact: true }).getByRole("checkbox", { name: "Reviewer", exact: true }).isChecked(), true);
    await owner.getByRole("textbox", { name: "Message", exact: true }).fill("Please expand on your review.");
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.waitForFunction(() => document.querySelector('textarea[aria-label="Message"]')?.value === "");
    const agentReply = messages.at(-1);
    assert.deepEqual(agentReply.replyTo, { kind: "response", id: reviewer.id });
    assert.deepEqual(agentReply.addressedAgents, [agents[1].id]);
    const afterReply = { sends: sends.length, runs: runs.length };
    await owner.reload();
    await owner.locator(`[data-message-id="message:${humanReply.id}"] .reply-quote`).getByText("Reply to Bob Rivera", { exact: true }).waitFor();
    await owner.locator(`[data-message-id="message:${agentReply.id}"] .reply-quote`).getByText("Reply to Reviewer", { exact: true }).waitFor();
    assert.deepEqual({ sends: sends.length, runs: runs.length }, afterReply, "reply recovery cannot send or dispatch again");
    await owner.bringToFront(); await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-replies-local.png", import.meta.url)) });
    assert.deepEqual(errors, []);

    // Explicit attachment publication never grants file access to another member.
    await owner.getByRole("button", { name: "Attach files", exact: true }).click();
    await owner.getByRole("region", { name: "Completed uploads", exact: true }).getByRole("button", { name: "workspace-acceptance.txt", exact: true }).click();
    await owner.getByRole("button", { name: "Attach files", exact: true }).click();
    await owner.getByRole("textbox", { name: "Artifact resource link", exact: true }).fill("javascript:alert(1)");
    await owner.getByRole("textbox", { name: "Name shown in chat", exact: true }).fill("Shared image <script>");
    await owner.getByRole("button", { name: "Add file link", exact: true }).click();
    await owner.getByText("Use an Artifact resource link, such as artifact:// followed by its file ID.", { exact: true }).waitFor();
    await owner.getByRole("textbox", { name: "Artifact resource link", exact: true }).fill(`artifact://${artifact}`);
    await owner.getByRole("button", { name: "Add file link", exact: true }).click();
    assert.equal(await owner.getByRole("list", { name: "Files to send", exact: true }).getByRole("listitem").count(), 2);
    assert.equal(await owner.getByRole("textbox", { name: "Message", exact: true }).inputValue(), "");
    uncertain = true;
    await owner.getByRole("button", { name: "Send message", exact: true }).click();
    await owner.getByRole("button", { name: "Retry message", exact: true }).waitFor();
    assert.equal(await owner.getByRole("button", { name: "Attach files", exact: true }).isDisabled(), true);
    assert.equal(await owner.getByRole("button", { name: "Remove attachment workspace-acceptance.txt", exact: true }).isDisabled(), true);
    const attached = messages.at(-1);
    assert.equal(attached.text, "");
    assert.deepEqual(attached.attachments.map(file => file.kind), ["artifact", "artifact"]);
    await owner.getByRole("button", { name: "Retry message", exact: true }).click();
    await owner.getByRole("list", { name: "Files to send", exact: true }).waitFor({ state: "detached" });
    assert.deepEqual(sends.slice(-2), [attached.id, attached.id]);
    const ownFiles = owner.locator(`[data-message-id="message:${attached.id}"]`).getByRole("region", { name: "Attachments", exact: true });
    const imageFile = ownFiles.locator(".task-resource").filter({ hasText: "Shared image <script>" });
    await imageFile.getByRole("button", { name: "Preview", exact: true }).click();
    await imageFile.locator("img").waitFor();
    await owner.waitForFunction(() => [...document.querySelectorAll('.message-attachments img')].some(image => image.complete && image.naturalWidth > 0));
    const ownUpload = ownFiles.locator(".task-resource").filter({ hasText: "workspace-acceptance.txt" });
    await ownUpload.getByRole("button", { name: "Preview", exact: true }).click();
    await ownUpload.getByText("This file is available to download. Inline preview supports raster images up to 20 MiB.", { exact: true }).waitFor();
    assert.equal(await ownUpload.getByRole("link", { name: "Download", exact: true }).getAttribute("href"), "/workspace/api/artifacts/01a0a75d-3458-78f3-ac54-91f1cab1fea3/download");
    await member.reload();
    const theirFiles = member.locator(`[data-message-id="message:${attached.id}"]`).getByRole("region", { name: "Attachments", exact: true });
    await theirFiles.locator(".task-resource").filter({ hasText: "Shared image <script>" }).getByRole("button", { name: "Preview", exact: true }).click();
    await theirFiles.getByText("This file is unavailable with your current access. A chat or Task link does not grant file access.", { exact: true }).waitFor();
    assert.equal(await theirFiles.locator("img").count(), 0);
    const afterFiles = { sends: sends.length, runs: runs.length, uploads: uploadAdmissions };
    await owner.reload();
    await owner.locator(`[data-message-id="message:${attached.id}"] .message-attachments`).getByText("Shared image <script>", { exact: true }).waitFor();
    assert.deepEqual({ sends: sends.length, runs: runs.length, uploads: uploadAdmissions }, afterFiles);
    assert.deepEqual(messages.at(-1).attachments, attached.attachments);
    await owner.bringToFront(); await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-attachments-local.png", import.meta.url)) });
    assert.deepEqual(errors, []);

    // A fresh read after an expired watch is insufficient: a later commit must
    // arrive through a newly admitted stream without focus, reload or mutation.
    expireOwnerStream = true;
    const recoveredMessage = { id: crypto.randomUUID(), author: members[1].id,
      text: "A later message after stream recovery.", replyTo: null, attachments: [],
      addressedAgents: [], responseAgents: [], createdAt: new Date().toISOString() };
    messageAfterReconnect = recoveredMessage;
    await owner.locator(".message-text").getByText(recoveredMessage.text, { exact: true }).waitFor();
    assert.deepEqual({ sends: sends.length, runs: runs.length, uploads: uploadAdmissions }, afterFiles);
    revokeOwner = true;
    await owner.getByText("This chat or action is no longer available with your access.", { exact: true }).waitFor();
    await owner.getByRole("textbox", { name: "Message", exact: true }).waitFor({ state: "detached" });
    assert.equal(await owner.locator(".message-text").count(), 0, "revocation clears retained conversation state");
    assert.deepEqual({ sends: sends.length, runs: runs.length, uploads: uploadAdmissions }, afterFiles);
    console.log(JSON.stringify({ evidence: "local HTTP fixture", expiredWatchReconnected: true, laterMessageDelivered: recoveredMessage.id, revokedHistoryCleared: true }));

  } finally {
    await context.close(); await browser.close(); await server.close();
  }
});
