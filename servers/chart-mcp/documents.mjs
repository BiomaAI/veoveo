// Image-owned document bytes and build-time validators. No request data enters here.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ComplianceProfile, decodeProfileJson } from "./compliance.mjs";

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

const ARTIFACTS = Object.freeze({ agents: "AGENTS.md", design: "DESIGN.md",
  profile: "contract-compliance.json", catalog: "requirements.json", schema: "compliance-profile.schema.json" });

function artifactBundle(directory) {
  const data = Object.fromEntries(Object.entries(ARTIFACTS).map(([key, filename]) => [key, readFileSync(join(directory, filename))]));
  const decode = (key) => new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(data[key]);
  const profile = new ComplianceProfile(decodeProfileJson(decode("profile")), JSON.parse(decode("catalog")), JSON.parse(decode("schema")));
  if (profile.server !== "charts") throw new Error("compliance profile/server mismatch");
  profile.checkManual(decode("agents"));
  profile.checkApplicability(true);
  return { data, profile };
}

export function buildDocumentManifest(directory) {
  const { data } = artifactBundle(directory);
  const digests = Object.fromEntries(Object.entries(data).map(([key, bytes]) => [key, createHash("sha256").update(bytes).digest("hex")]));
  for (const entry of DOCUMENTS) documentBytes(directory, entry);
  writeFileSync(join(directory, MANIFEST_FILE), JSON.stringify(digests) + "\n");
}

export function loadDocumentBundle(directory) {
  const manifest = JSON.parse(readFileSync(join(directory, MANIFEST_FILE), "utf8"));
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest) ||
      Object.keys(manifest).sort().join(",") !== Object.keys(ARTIFACTS).sort().join(",")) throw new Error("invalid document manifest");
  const { data, profile } = artifactBundle(directory);
  for (const [key, bytes] of Object.entries(data)) {
    if (manifest[key] !== createHash("sha256").update(bytes).digest("hex")) throw new Error("packaged artifact differs from its build manifest");
  }
  const documents = Object.freeze(DOCUMENTS.map((entry) => {
    const body = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(data[entry.id]);
    if (!body.trim() || data[entry.id].length + 1024 > 64 * 1024) throw new Error(`document ${entry.id} must fit the 64 KiB resource budget`);
    const uri = new URL(DOCS_URI); uri.pathname = entry.id;
    return Object.freeze({ ...entry, body, digest: manifest[entry.id], uri: uri.href });
  }));
  return Object.freeze({ documents, profile });
}

export function loadDocuments(directory) { return loadDocumentBundle(directory).documents; }

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
