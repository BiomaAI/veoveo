import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { once } from "node:events";
import test from "node:test";
import { McpServer, createMcpHandler } from "@modelcontextprotocol/server";
import { toNodeHandler } from "@modelcontextprotocol/node";
import { buildDocumentManifest, loadDocuments, loadDocumentBundle, MANIFEST_FILE } from "./documents.mjs";
import { registerWellKnownResources } from "./well-known.mjs";
import { KNOWLEDGE_EXTENSION, OBSERVATION_KEY } from "./knowledge.mjs";
import { loadInternalTokenVerifier, requireInternalIdentity } from "./internal-auth.mjs";

test("packaged knowledge documents negotiate over authenticated stateless HTTP", { timeout: 15000 }, async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "veoveo-chart-docs-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const filename of ["AGENTS.md", "contract-compliance.json", "requirements.json", "compliance-profile.schema.json"]) {
    writeFileSync(join(directory, filename), readFileSync(new URL(filename, import.meta.url)));
  }
  writeFileSync(join(directory, "DESIGN.md"), "\ufeff# Design\n\nFixture.\n");
  assert.throws(() => loadDocuments(directory), /ENOENT/);
  buildDocumentManifest(directory);
  const documents = loadDocuments(directory);
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const verifier = loadInternalTokenVerifier(JSON.stringify({ keys: [{
    ...publicKey.export({ format: "jwk" }), kid: "docs-fixture", alg: "EdDSA", use: "sig",
  }] }), "charts");
  const token = (overrides = {}) => {
    const now = Math.floor(Date.now() / 1000);
    const claims = { iss: "veoveo-internal", aud: "charts", server: "charts", sub: "fixture",
      jti: "docs-read", profile: "operator", actor: { id: "fixture", kind: "service" },
      authority: { kind: "direct" }, iat: now, nbf: now, exp: now + 60, ...overrides };
    const message = [ { alg: "EdDSA", kid: "docs-fixture" }, claims ]
      .map((part) => Buffer.from(JSON.stringify(part)).toString("base64url")).join(".");
    return `${message}.${sign(null, Buffer.from(message), privateKey).toString("base64url")}`;
  };
  const handler = createMcpHandler(() => {
    const server = new McpServer({ name: "charts", version: "fixture" }, {
      capabilities: { extensions: { [KNOWLEDGE_EXTENSION]: {} }, resources: {} },
    });
    registerWellKnownResources(server, loadDocumentBundle(directory));
    return server;
  }, { legacy: "reject", responseMode: "json" });
  const handle = toNodeHandler(handler);
  const server = createServer((request, response) => {
    if (requireInternalIdentity(request, response, verifier)) void handle(request, response);
  });
  t.after(async () => {
    await handler.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const endpoint = `http://127.0.0.1:${server.address().port}/charts/mcp`;
  const request = async (method, params = {}, meta = {}, bearer = token(), capabilities = { extensions: { [KNOWLEDGE_EXTENSION]: {} } }, protocolHeader = "2026-07-28") => {
    const response = await fetch(endpoint, {
      method: "POST", signal: AbortSignal.timeout(3000),
      headers: { "Content-Type": "application/json", Accept: "application/json, text/event-stream",
        ...(protocolHeader ? { "MCP-Protocol-Version": protocolHeader } : {}),
        "Mcp-Method": method, ...(params.uri ? { "Mcp-Name": params.uri } : {}),
        ...(bearer ? { Authorization: `Bearer ${bearer}` } : {}) },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params: { ...params, _meta: {
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": capabilities, ...meta,
      } } }),
    });
    const body = await response.text();
    return { status: response.status, body: response.headers.get("Content-Type")?.includes("json") ? JSON.parse(body) : body };
  };
  const success = (response) => {
    assert.equal(response.status, 200, JSON.stringify(response.body));
    assert.ok(response.body.result, JSON.stringify(response.body));
    return response.body.result;
  };
  const negotiatedCapabilities = { extensions: { [KNOWLEDGE_EXTENSION]: {} } };
  for (const protocolHeader of [null, "2025-11-25"]) {
    const invalid = await request("server/discover", {}, {}, token(), negotiatedCapabilities, protocolHeader);
    assert.equal(invalid.status, 400);
    assert.equal(invalid.body.error.code, -32020);
  }
  const discovery = success(await request("server/discover"));
  assert.deepEqual(discovery.capabilities.extensions[KNOWLEDGE_EXTENSION], {});
  const templates = success(await request("resources/templates/list")).resourceTemplates;
  assert.equal(templates.find((entry) => entry._meta?.[KNOWLEDGE_EXTENSION])
    ._meta[KNOWLEDGE_EXTENSION].collection, "charts.docs");
  const page = success(await request("resources/read", { uri: "charts://docs" }));
  assert.deepEqual(JSON.parse(page.contents[0].text).items.map(({ id }) => id), ["agents", "design"]);
  const tail = success(await request("resources/read", { uri: "charts://docs?cursor=agents" }));
  assert.deepEqual(JSON.parse(tail.contents[0].text).items.map(({ id }) => id), ["design"]);
  assert.ok((await request("resources/read", { uri: "charts://docs?cursor=unknown" })).body.error);
  assert.ok((await request("resources/read", { uri: "charts://docs?cursor=agents&cursor=design" })).body.error);
  for (const doc of documents) {
    const full = success(await request("resources/read", { uri: doc.uri }, {
      [OBSERVATION_KEY]: { revision: "forged", modifiedBy: "caller" },
    }));
    assert.equal(full.contents[0].text, doc.body);
    assert.equal(full.ttlMs, 0);
    assert.equal(full.cacheScope, "private");
    const observation = full._meta[OBSERVATION_KEY];
    const digest = createHash("sha256").update(doc.body).digest("hex");
    assert.equal(observation.revision, digest);
    assert.equal(observation.contentSha256, digest);
    assert.equal(observation.modifiedBy, undefined);
    const condition = { [KNOWLEDGE_EXTENSION]: { ifNoneMatch: digest } };
    const conditional = success(await request("resources/read", { uri: doc.uri }, condition));
    assert.deepEqual(conditional.contents, []);
    assert.equal(conditional._meta[OBSERVATION_KEY].notModified, true);
    const plain = success(await request("resources/read", { uri: doc.uri }, condition, token(), {}));
    assert.equal(plain.contents[0].text, doc.body);
    assert.equal(plain._meta?.[OBSERVATION_KEY], undefined);
    for (const bearer of ["", token({ exp: 1 }), token({ aud: "map", server: "map" })]) {
      assert.equal((await request("resources/read", { uri: doc.uri }, condition, bearer)).status, 401);
    }
  }
  assert.ok((await request("resources/read", { uri: documents[0].uri }, {
    [KNOWLEDGE_EXTENSION]: { ifNoneMatch: documents[0].digest, extra: "unrecognized" },
  })).body.error);
  assert.ok((await request("resources/read", { uri: documents[0].uri }, {}, token(), {
    extensions: { [KNOWLEDGE_EXTENSION]: { unsupported: true } },
  })).body.error);
  const declarationResponse = success(await request("resources/read", { uri: "charts://contract" }, {}, token(), {}));
  assert.deepEqual(JSON.parse(declarationResponse.contents[0].text), loadDocumentBundle(directory).profile.wire());
  writeFileSync(join(directory, "DESIGN.md"), "tampered");
  assert.throws(() => loadDocuments(directory), /build manifest/);
  const manifest = JSON.parse(readFileSync(join(directory, MANIFEST_FILE), "utf8"));
  writeFileSync(join(directory, MANIFEST_FILE), JSON.stringify({ ...manifest, unexpected: "value" }));
  assert.throws(() => loadDocuments(directory), /invalid document manifest/);
});
