// Image-owned document bytes and build-time validators. No request data enters here.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const DOCUMENTS = Object.freeze([
  Object.freeze({ id: "agents", title: "Agent work manual", file: "AGENTS.md" }),
  Object.freeze({ id: "design", title: "Domain design", file: "DESIGN.md" }),
]);
export const DOCS_URI = "charts://docs";
export const MANIFEST_FILE = "_documents.json";

function documentBytes(directory, entry) {
  const bytes = readFileSync(join(directory, entry.file));
  const body = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  if (!body.trim() || bytes.length + 1024 > 64 * 1024) {
    throw new Error(`document ${entry.id} must fit the 64 KiB resource budget`);
  }
  return { body, digest: createHash("sha256").update(bytes).digest("hex") };
}

export function buildDocumentManifest(directory) {
  const digests = Object.fromEntries(DOCUMENTS.map((entry) => [
    entry.id, documentBytes(directory, entry).digest,
  ]));
  writeFileSync(join(directory, MANIFEST_FILE), JSON.stringify(digests) + "\n");
}

export function loadDocuments(directory) {
  const manifest = JSON.parse(readFileSync(join(directory, MANIFEST_FILE), "utf8"));
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest) ||
      Object.keys(manifest).sort().join(",") !== DOCUMENTS.map(({ id }) => id).join(",")) {
    throw new Error("invalid document manifest");
  }
  return Object.freeze(DOCUMENTS.map((entry) => {
    const { body, digest } = documentBytes(directory, entry);
    if (manifest[entry.id] !== digest) {
      throw new Error(`document ${entry.id} differs from its build manifest`);
    }
    const uri = new URL(DOCS_URI);
    uri.pathname = entry.id;
    return Object.freeze({ ...entry, body, digest, uri: uri.href });
  }));
}

export function documentPage(documents, uri) {
  if (uri.hash || [...uri.searchParams.keys()].some((key) => key !== "cursor") ||
      uri.searchParams.getAll("cursor").length > 1) {
    throw new Error("invalid document page parameters");
  }
  const cursor = uri.searchParams.get("cursor");
  const position = cursor === null ? -1 : documents.findIndex(({ id }) => id === cursor);
  if (cursor !== null && position === -1) throw new Error("unknown document cursor");
  const remaining = documents.slice(position + 1);
  const page = remaining.slice(0, 32);
  return {
    items: page.map(({ id, title, uri: address }) => ({ id, title, uri: address })),
    ...(remaining.length > 32 ? { nextCursor: page.at(-1).id } : {}),
  };
}
