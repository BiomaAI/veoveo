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

test("headed Workspace supports shared authors, stable retries, ownership controls and reload", { timeout: 60_000 }, async () => {
  const server = await createServer({ root, configFile: `${root}vite.config.ts`, server: { port: 0, host: "127.0.0.1", strictPort: false } });
  const browser = await chromium.connectOverCDP(process.env.VEOVEO_BROWSER_CDP ?? "http://127.0.0.1:9222");
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
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
    let uncertain = true;
    const sends = [];
    const pages = await Promise.all([context.newPage(), context.newPage()]);
    const errors = [];
    for (const [index, page] of pages.entries()) {
      await hardware(page);
      const person = index === 0 ? alice : bob;
      page.on("pageerror", error => errors.push(error.message));
      await page.route("**/workspace/api/**", async route => {
        const request = route.request();
        const path = new URL(request.url());
        let body;
        if (request.method() !== "GET") {
          assert.equal(request.headers()["x-veoveo-csrf-token"], "fixture-csrf");
          body = request.postDataJSON();
        }
        const respond = json => route.fulfill({ status: 200, contentType: "application/json", headers: { "x-veoveo-csrf-token": "fixture-csrf" }, json });
        if (path.pathname.endsWith("/session")) return respond({ person, principalId: `fixture#${person.id}`, tenantId: "test", tenantName: "Shared work", workContext: "default", workContextTitle: "Product team", canContribute: true });
        if (path.pathname.endsWith("/events")) return route.fulfill({ contentType: "text/event-stream", body: `retry: 250\nevent: change\ndata: {"sequence":${chat.sequence}}\n\n` });
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
    assert.equal(await owner.locator(".message-author strong").allTextContents().then(names => names.join(",")), "Alice Chen,Bob Rivera");
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
    await owner.reload();
    await owner.getByText("A message with an uncertain response", { exact: true }).waitFor();
    assert.equal(sends.length, 2, "reload cannot execute another send");
    await owner.getByRole("button", { name: "Participants", exact: true }).click();
    const proof = await hardware(owner);
    assert.deepEqual(errors, []);
    const output = new URL("../../../output/workspace-client-local.png", import.meta.url);
    await mkdir(new URL(".", output), { recursive: true });
    await owner.screenshot({ path: fileURLToPath(output) });
    console.log(JSON.stringify({ evidence: "local HTTP fixture, not installed execution", proof, distinctAuthors: 2, committedMessages: messages.length, sendAttempts: sends.length }));
    await owner.setViewportSize({ width: 390, height: 844 });
    await owner.getByRole("button", { name: "Close chat details" }).click();
    assert.equal(await owner.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await hardware(owner);
    await owner.screenshot({ path: fileURLToPath(new URL("../../../output/workspace-client-mobile.png", import.meta.url)) });
  } finally {
    await context.close(); await browser.close(); await server.close();
  }
});
