// The same checked document/profile registration serves production and native fixtures.
import { ComplianceProfile } from "./compliance.mjs";
import { privateText, registerKnowledgeDocuments } from "./knowledge.mjs";
export function registerWellKnownResources(server, bundle) {
  if (!(bundle.profile instanceof ComplianceProfile) || bundle.profile.server !== "charts") throw new Error("well-known resources require the Charts admitted profile");
  bundle.profile.checkApplicability(true);
  registerKnowledgeDocuments(server, bundle.documents);
  const declaration = JSON.stringify(bundle.profile.wire());
  server.registerResource("contract", "charts://contract", {
    title: "Contract declaration", description: "Machine-readable contract revision and compliance declarations.", mimeType: "application/json",
  }, async (uri) => privateText(uri, "application/json", declaration));
}
