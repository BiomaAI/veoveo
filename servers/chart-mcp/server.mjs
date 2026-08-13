import { readFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { createMcpHandler } from "@modelcontextprotocol/server";
import { toNodeHandler } from "@modelcontextprotocol/node";

import { createServer } from "./flint-v2.mjs";
import {
  loadInternalTokenVerifier,
  requireInternalIdentity,
} from "./internal-auth.mjs";


// Well-known surface of `mcp/contract/DESIGN.md` (C18-C21). The launcher
// serves the crate documents baked into the image beside this file, the
// contract declaration parsed from the agent manual, and the administrative
// llms.txt projection. Shapes mirror `veoveo_mcp_contract::docs`.
const CONTRACT_REVISION = 3;
const SERVER_SLUG = "charts";
const SERVER_MOUNT = `/${SERVER_SLUG}`;
const MCP_PATH = `${SERVER_MOUNT}/mcp`;
const DOCS_URI = "charts://docs";
const CONTRACT_URI = "charts://contract";

function loadServerDocument(name) {
  const path = join(dirname(fileURLToPath(import.meta.url)), name);
  const body = readFileSync(path, "utf8");
  if (body.trim().length === 0) {
    throw new Error(`server document ${path} is empty`);
  }
  return body;
}

const SERVER_DOCS = [
  { id: "agents", title: "Agent work manual", body: loadServerDocument("AGENTS.md") },
  { id: "design", title: "Domain design", body: loadServerDocument("DESIGN.md") },
];

function docsIndexJson() {
  return JSON.stringify(SERVER_DOCS.map(({ id, title }) => ({ id, title })));
}

function llmsTxt() {
  const entries = SERVER_DOCS.map((doc) => `- [${doc.title}](${doc.id})`);
  return (
    `# ${SERVER_SLUG}\n\n` +
    `> Veoveo MCP server documents. Contract revision ${CONTRACT_REVISION}.\n\n` +
    `## Docs\n\n${entries.join("\n")}\n`
  );
}

// Mirrors `veoveo_mcp_contract::docs::parse_compliance`: `- Cnn: met` and
// `- Cnn: pending — note` lines inside the `## Contract Compliance` section.
function parseCompliance(manual) {
  let inSection = false;
  const items = [];
  for (const line of manual.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("## ")) {
      inSection = trimmed === "## Contract Compliance";
      continue;
    }
    if (!inSection || !trimmed.startsWith("- C")) {
      continue;
    }
    const separator = trimmed.indexOf(":");
    if (separator === -1) {
      continue;
    }
    const id = `C${trimmed.slice(3, separator).trim()}`;
    const rest = trimmed.slice(separator + 1).trim();
    let status;
    let remainder;
    if (rest.startsWith("met")) {
      status = "met";
      remainder = rest.slice("met".length);
    } else if (rest.startsWith("pending")) {
      status = "pending";
      remainder = rest.slice("pending".length);
    } else {
      continue;
    }
    const note = remainder.replace(/^[\s—-]+/u, "").trim();
    items.push(note.length > 0 ? { id, status, note } : { id, status });
  }
  if (items.length === 0) {
    throw new Error("AGENTS.md declares no Contract Compliance items");
  }
  return items;
}

function textResource(uri, mimeType, text) {
  return { contents: [{ uri: uri.href, mimeType, text }] };
}

function registerWellKnownResources(server) {
  if (typeof server.registerResource !== "function") {
    throw new Error(
      "upstream chart server exposes no registerResource; the well-known surface (contract C18, C19) cannot be served",
    );
  }
  server.registerResource(
    "docs",
    DOCS_URI,
    {
      title: "Server documents",
      description: "Index of the server documents baked into the image.",
      mimeType: "application/json",
    },
    async (uri) => textResource(uri, "application/json", docsIndexJson()),
  );
  for (const doc of SERVER_DOCS) {
    server.registerResource(
      doc.id,
      `${DOCS_URI}/${doc.id}`,
      {
        title: doc.title,
        description: "Server document baked into the image.",
        mimeType: "text/markdown",
      },
      async (uri) => textResource(uri, "text/markdown", doc.body),
    );
  }
  let declaration;
  server.registerResource(
    "contract",
    CONTRACT_URI,
    {
      title: "Contract declaration",
      description:
        "Machine-readable contract revision and compliance evidence.",
      mimeType: "application/json",
    },
    async (uri) =>
      textResource(uri, "application/json", JSON.stringify(declaration)),
  );
  declaration = contractDeclaration();
}

function contractDeclaration() {
  return {
    server: SERVER_SLUG,
    contract_revision: CONTRACT_REVISION,
    compliance: parseCompliance(SERVER_DOCS[0].body),
  };
}

function createHostedServer(options) {
  const server = createServer(options);
  registerWellKnownResources(server);
  return server;
}

function parseArgs(argv) {
  const options = {
    host: "0.0.0.0",
    port: 8795,
    path: MCP_PATH,
    allowedHosts: [],
    disableFileReference: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${argument} requires a value`);
      }
      return argv[index];
    };
    switch (argument) {
      case "--host":
        options.host = value();
        break;
      case "--port":
        options.port = Number(value());
        break;
      case "--path":
        options.path = value();
        break;
      case "--allowed-hosts":
        options.allowedHosts = value()
          .split(",")
          .map((host) => host.trim())
          .filter(Boolean);
        break;
      case "--disable-file-reference":
        options.disableFileReference = true;
        break;
      default:
        throw new Error(`unsupported argument: ${argument}`);
    }
  }
  if (!Number.isInteger(options.port) || options.port < 1 || options.port > 65535) {
    throw new Error(`invalid port: ${options.port}`);
  }
  if (!options.path.startsWith("/")) {
    throw new Error("MCP path must begin with /");
  }
  if (options.path !== MCP_PATH) {
    throw new Error(`MCP path must be the canonical ${MCP_PATH}`);
  }
  return options;
}

function jsonError(response, status, message) {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(
    JSON.stringify({
      jsonrpc: "2.0",
      error: { code: status === 500 ? -32603 : -32000, message },
      id: null,
    }),
  );
}

const options = parseArgs(process.argv.slice(2));
const verifyInternalIdentity = loadInternalTokenVerifier(
  process.env.VEOVEO_INTERNAL_TRUST_JWKS,
  SERVER_SLUG,
);
const mcpHandler = createMcpHandler(
  () => createHostedServer({ disableFileReference: options.disableFileReference }),
  {
    legacy: "reject",
    responseMode: "json",
    onerror: (error) => process.stderr.write(`MCP request failed: ${String(error)}\n`),
  },
);
const handleMcpRequest = toNodeHandler(mcpHandler, {
  onerror: (error) => process.stderr.write(`MCP adapter failed: ${String(error)}\n`),
});

const httpServer = createHttpServer(async (request, response) => {
  try {
    const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);
    if (
      options.allowedHosts.length > 0 &&
      !options.allowedHosts.includes(request.headers.host)
    ) {
      response
        .writeHead(421, { "content-type": "text/plain; charset=utf-8" })
        .end("untrusted Host authority");
      return;
    }
    if (request.method === "GET" && url.pathname === `${SERVER_MOUNT}/healthz`) {
      response
        .writeHead(200, { "content-type": "application/json" })
        .end(JSON.stringify({ name: SERVER_SLUG, status: "ok", transport: "streamable_http" }));
      return;
    }
    if (
      (url.pathname === options.path ||
        url.pathname.startsWith(`${SERVER_MOUNT}/admin/`)) &&
      !requireInternalIdentity(request, response, verifyInternalIdentity)
    ) {
      return;
    }
    if (
      request.method === "GET" &&
      url.pathname === `${SERVER_MOUNT}/admin/docs/llms.txt`
    ) {
      response
        .writeHead(200, { "content-type": "text/plain; charset=utf-8" })
        .end(llmsTxt());
      return;
    }
    if (
      request.method === "GET" &&
      url.pathname.startsWith(`${SERVER_MOUNT}/admin/docs/`)
    ) {
      const doc = SERVER_DOCS.find(
        (candidate) =>
          url.pathname === `${SERVER_MOUNT}/admin/docs/${candidate.id}`,
      );
      if (doc) {
        response
          .writeHead(200, { "content-type": "text/markdown; charset=utf-8" })
          .end(doc.body);
      } else {
        response
          .writeHead(404, { "content-type": "text/plain; charset=utf-8" })
          .end("unknown server document");
      }
      return;
    }
    if (url.pathname !== options.path) {
      jsonError(response, 404, `not found; MCP endpoint is ${options.path}`);
      return;
    }

    await handleMcpRequest(request, response);
  } catch (error) {
    if (!response.headersSent) {
      jsonError(response, 500, error instanceof Error ? error.message : "internal error");
    } else {
      response.end();
    }
  }
});

httpServer.listen(options.port, options.host, () => {
  process.stderr.write(
    `flint-chart-mcp listening on http://${options.host}:${options.port}${options.path} with MCP 2026-07-28 stateless HTTP\n`,
  );
});

async function shutdown() {
  await mcpHandler.close();
  httpServer.close(() => process.exit(0));
}

process.on("SIGINT", () => void shutdown());
process.on("SIGTERM", () => void shutdown());
