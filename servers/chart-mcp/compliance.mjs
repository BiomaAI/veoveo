// Rust-generated catalog/schema plus owner profile admission. No Markdown authority.
import { parseTree } from "jsonc-parser";
import { Validator } from "@cfworker/json-schema";
import generatedCatalog from "./requirements.json" with { type: "json" };
import generatedSchema from "./compliance-profile.schema.json" with { type: "json" };
const CATALOG_BYTES = JSON.stringify(generatedCatalog);
const SCHEMA_BYTES = JSON.stringify(generatedSchema);
export const START_MARKER = "<!-- veoveo:contract-compliance:start -->";
export const END_MARKER = "<!-- veoveo:contract-compliance:end -->";
const CONTRACT_REVISION = 4;
const CATALOG_REVISION = 2;

export function decodeProfileJson(text) {
  const errors = [];
  const root = parseTree(text, errors, { disallowComments: true, allowTrailingComma: false, allowEmptyContent: false });
  if (!root || errors.length !== 0) throw new Error("invalid profile JSON");
  function inspect(node) {
    if (node.type === "object") {
      const names = new Set();
      for (const property of node.children ?? []) {
        const name = property.children[0].value;
        if (names.has(name)) throw new Error("duplicate profile JSON member");
        names.add(name);
      }
    }
    for (const child of node.children ?? []) inspect(child);
  }
  inspect(root);
  return JSON.parse(text);
}

export class ComplianceProfile {
  #value;
  #conditions;
  constructor(value, catalog, schema) {
    if (new.target !== ComplianceProfile) throw new Error("compliance profiles cannot bypass admission through subclassing");
    if (catalog.contractRevision !== CONTRACT_REVISION || catalog.catalogRevision !== CATALOG_REVISION ||
        !Array.isArray(catalog.requirements) || catalog.requirements.length === 0) throw new Error("unsupported requirement catalog");
    if (JSON.stringify(catalog) !== CATALOG_BYTES || JSON.stringify(schema) !== SCHEMA_BYTES) throw new Error("catalog/schema differs from the generated revision; upgrade the consumer");
    value = structuredClone(value);
    catalog = JSON.parse(CATALOG_BYTES);
    schema = JSON.parse(SCHEMA_BYTES);
    const metadata = new Map(catalog.requirements.map((entry) => [entry.id, entry]));
    if (metadata.size !== catalog.requirements.length) throw new Error("duplicate requirement catalog identities");
    if (!new Validator(schema).validate(value).valid) throw new Error("invalid compliance profile schema");
    const keys = Object.keys(value).sort().join(",");
    if (keys !== "catalogRevision,compliance,contractRevision,server" ||
        value.contractRevision !== CONTRACT_REVISION || value.catalogRevision !== CATALOG_REVISION ||
        typeof value.server !== "string" || !/^[a-z0-9_-]+$/.test(value.server) || !Array.isArray(value.compliance)) throw new Error("invalid compliance profile");
    // Consume the actual Rust-generated rule; JS trim has different membership.
    const noteWhitespace = new Set(catalog.noteWhitespace);
    const entries = new Map();
    for (const item of value.compliance) {
      if (item === null || typeof item !== "object" || Array.isArray(item) ||
          Object.keys(item).some((key) => !["id", "status", "note"].includes(key)) ||
          !metadata.has(item.id) || entries.has(item.id) || !["met", "pending", "not_applicable"].includes(item.status)) throw new Error("unknown, duplicate or invalid compliance entry");
      if (Object.hasOwn(item, "note") && (typeof item.note !== "string" || [...item.note].every(character => noteWhitespace.has(character)) || /[\r\n]/.test(item.note))) throw new Error("invalid requirement note");
      if (item.status !== "met" && !Object.hasOwn(item, "note")) throw new Error("requirement status needs a note");
      if (item.status === "not_applicable" && metadata.get(item.id).applicability !== "knowledge_source") throw new Error("requirement has no applicability condition");
      entries.set(item.id, Object.freeze({ ...item }));
    }
    if (entries.size !== metadata.size) throw new Error("incomplete compliance profile");
    this.#conditions = new Map(catalog.requirements.map((entry) => [entry.id, entry.applicability]));
    this.#value = Object.freeze({ server: value.server, contractRevision: CONTRACT_REVISION,
      catalogRevision: CATALOG_REVISION, compliance: Object.freeze([...metadata.keys()].sort().map((id) => entries.get(id))) });
    Object.freeze(this);
  }
  get server() { return this.#value.server; }
  wire() { return structuredClone(this.#value); }
  checkApplicability(knowledgeSource) {
    if (typeof knowledgeSource !== "boolean") throw new Error("applicability requires actual discovery state");
    for (const item of this.#value.compliance) {
      if (this.#conditions.get(item.id) === "knowledge_source" && (item.status === "not_applicable") === knowledgeSource) throw new Error("knowledge-source applicability contradicts discovery");
    }
  }
  render() {
    return `Contract revision: ${CONTRACT_REVISION}\nCatalog revision: ${CATALOG_REVISION}\n\n` +
      this.#value.compliance.map((item) => `- ${item.id}: ${item.status}${Object.hasOwn(item, "note") ? ` — ${item.note}` : ""}\n`).join("");
  }
  checkManual(manual) {
    if (manual.split(START_MARKER).length !== 2 || manual.split(END_MARKER).length !== 2 ||
        manual.indexOf(END_MARKER) < manual.indexOf(START_MARKER)) throw new Error("invalid compliance markers");
    const [before, rest] = manual.split(START_MARKER);
    const [body] = rest.split(END_MARKER);
    if (!before.endsWith("## Contract Compliance\n\n") || body !== `\n${this.render()}`) throw new Error("manual compliance section differs from profile");
  }
}

Object.freeze(ComplianceProfile.prototype);
Object.freeze(ComplianceProfile);
