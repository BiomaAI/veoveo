import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, mkdtempSync, cpSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { ComplianceProfile, START_MARKER, END_MARKER } from "./compliance.mjs";
import { buildDocumentManifest, loadDocumentBundle } from "./documents.mjs";
const read = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"));
const catalog = read("requirements.json");
const schema = read("compliance-profile.schema.json");
const fixtures = read("../../mcp/contract/testdata/compliance-profiles.json");
for (const fixture of fixtures.valid) {
  test(`shared admitted profile: ${fixture.name}`, () => {
    const profile = new ComplianceProfile(fixture.profile, catalog, schema);
    assert.equal(profile.wire().compliance.length, catalog.requirements.length);
    assert.equal(Object.isFrozen(profile), true);
    const copy = profile.wire(); copy.compliance.length = 0;
    assert.equal(profile.wire().compliance.length, catalog.requirements.length);
  });
}
for (const fixture of fixtures.invalid) {
  test(`shared rejected profile: ${fixture.name}`, () => assert.throws(() => new ComplianceProfile(fixture.profile, catalog, schema)));
}
test("manual rendering matches exact shared Rust bytes and marker admission", () => {
  const profile = new ComplianceProfile(read("../../mcp/contract/testdata/compliance-example.json"), catalog, schema);
  const manual = readFileSync(new URL("../../mcp/contract/testdata/compliance-example.md", import.meta.url), "utf8");
  profile.checkManual(manual);
  for (const changed of [manual.replace(START_MARKER, ""), manual + END_MARKER,
    manual.replace("Catalog revision: 2", "Catalog revision: 1"), manual.replace("## Contract Compliance", "## Other")]) assert.throws(() => profile.checkManual(changed));
  profile.checkManual("Extra before\n" + manual + "\nExtra after\n");
});
test("packaged artifact loader has no repository lookup and checks profile/manual agreement", (t) => {
  const directory = mkdtempSync(join(tmpdir(), "veoveo-chart-package-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const filename of ["AGENTS.md", "DESIGN.md", "contract-compliance.json", "requirements.json", "compliance-profile.schema.json"]) cpSync(new URL(filename, import.meta.url), join(directory, filename));
  buildDocumentManifest(directory);
  assert.equal(loadDocumentBundle(directory).profile.server, "charts");
  const value = JSON.parse(readFileSync(join(directory, "contract-compliance.json"), "utf8"));
  value.compliance[0].note = "changed owned explanation";
  writeFileSync(join(directory, "contract-compliance.json"), JSON.stringify(value));
  assert.throws(() => loadDocumentBundle(directory));
  // A regenerated digest cannot make a stale manual agree with the changed profile.
  assert.throws(() => buildDocumentManifest(directory), /differs/);
});

test("applicability is compared against actual discovery in both directions", () => {
  for (const fixture of fixtures.valid) {
    const profile = new ComplianceProfile(fixture.profile, catalog, schema);
    const absent = profile.wire().compliance.some((item) => item.status === "not_applicable");
    profile.checkApplicability(!absent);
    assert.throws(() => profile.checkApplicability(absent));
  }
});

test("profile cannot be reconstructed by overriding public checks", () => {
  class UncheckedProfile extends ComplianceProfile {}
  assert.throws(() => new UncheckedProfile(fixtures.valid[0].profile, catalog, schema));
  assert.throws(() => { ComplianceProfile.prototype.wire = () => ({}); });
});

// Object fixtures cannot expose duplicate raw names after a normal JSON decoder.
test("raw profile duplicates reject at actual build and retained-load admission", (t) => {
  const directory = mkdtempSync(join(tmpdir(), "veoveo-chart-raw-profile-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const filename of ["AGENTS.md", "DESIGN.md", "contract-compliance.json", "requirements.json", "compliance-profile.schema.json"]) cpSync(new URL(filename, import.meta.url), join(directory, filename));
  const profile = read("contract-compliance.json");
  const canonical = JSON.stringify(profile);
  const first = JSON.stringify(profile.compliance[0]);
  const duplicates = [];
  for (const [key, prior] of [["contractRevision", 2], ["catalogRevision", 2]]) {
    for (const name of [JSON.stringify(key), `"\\u${key.charCodeAt(0).toString(16).padStart(4, "0")}${key.slice(1)}"`]) {
      duplicates.push(canonical.replace("{", `{${name}:${prior},`));
    }
  }
  for (const [key, prior] of [["id", "C02"], ["status", "met"], ["note", "Other valid explanation"]]) {
    for (const name of [JSON.stringify(key), `"\\u${key.charCodeAt(0).toString(16).padStart(4, "0")}${key.slice(1)}"`]) {
      duplicates.push(canonical.replace(first, first.replace("{", `{${name}:${JSON.stringify(prior)},`)));
    }
  }
  for (const raw of duplicates) {
    // Ordinary JSON.parse loses the earlier field and leaves a valid final profile.
    assert.deepEqual(JSON.parse(raw), profile);
    writeFileSync(join(directory, "contract-compliance.json"), canonical);
    buildDocumentManifest(directory);
    const manifest = JSON.parse(readFileSync(join(directory, "_documents.json"), "utf8"));
    writeFileSync(join(directory, "contract-compliance.json"), raw);
    assert.throws(() => buildDocumentManifest(directory), /duplicate profile JSON member/);
    manifest.profile = createHash("sha256").update(raw).digest("hex");
    writeFileSync(join(directory, "_documents.json"), JSON.stringify(manifest));
    assert.throws(() => loadDocumentBundle(directory), /duplicate profile JSON member/);
  }
  for (const raw of [`// comment\n${canonical}`, canonical.replace("}", ",}"), "", "{bad}"]) {
    writeFileSync(join(directory, "contract-compliance.json"), raw);
    assert.throws(() => buildDocumentManifest(directory), /invalid profile JSON/);
    assert.throws(() => loadDocumentBundle(directory), /invalid profile JSON/);
  }
  writeFileSync(join(directory, "contract-compliance.json"), Buffer.from([0xff]));
  assert.throws(() => buildDocumentManifest(directory));
  assert.throws(() => loadDocumentBundle(directory));
});


test("prior declaration keys have no alias in the current profile", () => {
  for (const [current, obsolete] of [["contractRevision", "contract_revision"], ["catalogRevision", "catalog_revision"]]) {
    const value = structuredClone(fixtures.valid[0].profile);
    value[obsolete] = value[current]; delete value[current];
    assert.throws(() => new ComplianceProfile(value, catalog, schema));
  }
});
