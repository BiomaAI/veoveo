import { randomUUID } from "node:crypto";
import { createServer as createHttpServer } from "node:http";

import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { isInitializeRequest } from "@modelcontextprotocol/sdk/types.js";

import { createServer } from "./dist/server.js";

const DEFAULT_MAX_BODY_BYTES = 32 * 1024 * 1024;

function parseArgs(argv) {
  const options = {
    host: "0.0.0.0",
    port: 8795,
    path: "/mcp",
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

async function readBody(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > DEFAULT_MAX_BODY_BYTES) {
      throw new Error("request body exceeds 32 MiB");
    }
    chunks.push(chunk);
  }
  if (chunks.length === 0) {
    return undefined;
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

const options = parseArgs(process.argv.slice(2));
const sessions = new Map();

async function closeSession(transport) {
  const sessionId = transport.sessionId;
  if (sessionId) {
    sessions.delete(sessionId);
  }
  await transport.close();
}

const httpServer = createHttpServer(async (request, response) => {
  try {
    const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);
    if (request.method === "GET" && (url.pathname === "/health" || url.pathname === "/")) {
      response
        .writeHead(200, { "content-type": "application/json" })
        .end(JSON.stringify({ name: "flint-chart-mcp", status: "ok", transport: "streamable_http" }));
      return;
    }
    if (url.pathname !== options.path) {
      jsonError(response, 404, `not found; MCP endpoint is ${options.path}`);
      return;
    }

    const sessionId = request.headers["mcp-session-id"];
    let transport = typeof sessionId === "string" ? sessions.get(sessionId) : undefined;
    let body;
    if (request.method === "POST") {
      body = await readBody(request);
      if (!transport && !sessionId && isInitializeRequest(body)) {
        const server = createServer({
          disableFileReference: options.disableFileReference,
        });
        transport = new StreamableHTTPServerTransport({
          sessionIdGenerator: () => randomUUID(),
          enableJsonResponse: false,
          enableDnsRebindingProtection: options.allowedHosts.length > 0,
          allowedHosts: options.allowedHosts,
          onsessioninitialized: (initializedSessionId) => {
            sessions.set(initializedSessionId, transport);
          },
        });
        transport.onclose = () => {
          if (transport.sessionId) {
            sessions.delete(transport.sessionId);
          }
        };
        await server.connect(transport);
      }
    }

    if (!transport) {
      jsonError(response, 400, "missing or invalid MCP session");
      return;
    }
    if (!["GET", "POST", "DELETE"].includes(request.method ?? "")) {
      response.writeHead(405, { allow: "GET, POST, DELETE" }).end();
      return;
    }
    await transport.handleRequest(request, response, body);
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
    `flint-chart-mcp listening on http://${options.host}:${options.port}${options.path} with sessionful Streamable HTTP\n`,
  );
});

async function shutdown() {
  await Promise.all([...sessions.values()].map(closeSession));
  httpServer.close(() => process.exit(0));
}

process.on("SIGINT", () => void shutdown());
process.on("SIGTERM", () => void shutdown());
