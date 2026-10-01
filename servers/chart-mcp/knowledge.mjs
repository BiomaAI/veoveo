import { ProtocolError, ProtocolErrorCode, ResourceNotFoundError, ResourceTemplate } from "@modelcontextprotocol/server";
import { z } from "zod/v4";
import { DOCS_URI, documentPage } from "./documents.mjs";

export const KNOWLEDGE_EXTENSION = "ai.veoveo/knowledge-source";
export const OBSERVATION_KEY = "ai.veoveo/knowledge-observation";
const CLIENT_CAPABILITIES = "io.modelcontextprotocol/clientCapabilities";
const settingsSchema = z.strictObject({});
const conditionSchema = z.strictObject({
  ifNoneMatch: z.string().min(1).max(256).regex(/^[!-~]+$/u),
});
const collection = Object.freeze({
  collection: "charts.docs", entityKind: "document", enumerate: DOCS_URI,
  freshness: { immutable: true }, changeSignal: "immutable", access: "profile", indexing: "content",
});

export function privateText(uri, mimeType, text) {
  return { contents: [{ uri: uri.href, mimeType, text }], ttlMs: 0, cacheScope: "private" };
}

function member(doc, uri, context) {
  const settings = context.mcpReq.envelope?.[CLIENT_CAPABILITIES]?.extensions?.[KNOWLEDGE_EXTENSION];
  if (settings === undefined) return privateText(uri, "text/markdown", doc.body);
  if (!settingsSchema.safeParse(settings).success) {
    throw new ProtocolError(ProtocolErrorCode.InvalidParams, "unsupported knowledge-source settings");
  }
  const raw = context.mcpReq._meta?.[KNOWLEDGE_EXTENSION];
  const condition = raw === undefined ? undefined : conditionSchema.safeParse(raw);
  if (condition && !condition.success) throw new ProtocolError(ProtocolErrorCode.InvalidParams, "invalid knowledge read condition");
  const unchanged = condition?.data.ifNoneMatch === doc.digest;
  const result = privateText(uri, "text/markdown", doc.body);
  if (unchanged) result.contents = [];
  result._meta = { [OBSERVATION_KEY]: {
    collection: collection.collection, revision: doc.digest, contentSha256: doc.digest,
    observedAt: new Date().toISOString(), ...(unchanged ? { notModified: true } : {}),
  } };
  return result;
}

// The host authenticates every request before dispatching to these callbacks.
export function registerKnowledgeDocuments(server, documents) {
  const index = (uri) => {
    try {
      return privateText(uri, "application/json", JSON.stringify(documentPage(documents, uri)));
    } catch {
      throw new ProtocolError(ProtocolErrorCode.InvalidParams, "invalid document cursor or page parameters");
    }
  };
  server.registerResource("docs", DOCS_URI, {
    title: "Server documents", mimeType: "application/json",
  }, index);
  server.registerResource("docs-page", new ResourceTemplate(`${DOCS_URI}{?cursor}`, { list: undefined }), {
    title: "Document pages", mimeType: "application/json",
  }, index);
  server.registerResource("document", new ResourceTemplate(`${DOCS_URI}/{doc_id}`, { list: undefined }), {
    title: "Server document", mimeType: "text/markdown", _meta: { [KNOWLEDGE_EXTENSION]: collection },
  }, (uri, _variables, context) => {
    const doc = documents.find((entry) => entry.uri === uri.href);
    if (!doc) throw new ResourceNotFoundError("unknown server document");
    return member(doc, uri, context);
  });
  for (const doc of documents) {
    server.registerResource(doc.id, doc.uri, { title: doc.title, mimeType: "text/markdown" },
      (uri, context) => member(doc, uri, context));
  }
}
