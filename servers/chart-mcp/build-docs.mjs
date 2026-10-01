import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { buildDocumentManifest } from "./documents.mjs";

buildDocumentManifest(dirname(fileURLToPath(import.meta.url)));
