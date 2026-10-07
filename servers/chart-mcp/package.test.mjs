// Package loading only; this test does not render a chart or qualify GPU execution.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, symlinkSync, cpSync, readdirSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const source = dirname(fileURLToPath(import.meta.url));
test("npm artifact loads exact declaration and manual outside the repository", { timeout: 60_000 }, (t) => {
  const temporary = mkdtempSync(join(tmpdir(), "veoveo-chart-artifact-"));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  const upstream = process.env.VEOVEO_CHART_UPSTREAM_SOURCE;
  assert.ok(upstream, "set VEOVEO_CHART_UPSTREAM_SOURCE to the pinned upstream0.5.1 package fixture");
  const upstreamPackage = JSON.parse(readFileSync(join(upstream, "package.json"), "utf8"));
  assert.equal(upstreamPackage.name, "flint-chart-mcp");
  assert.equal(upstreamPackage.version, "0.5.1");
  const assembled = join(temporary, "assembled");
  mkdirSync(assembled);
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    if (entry.isFile()) cpSync(join(source, entry.name), join(assembled, entry.name));
  }
  for (const directory of ["dist", "assets"]) cpSync(join(upstream, directory), join(assembled, directory), { recursive: true });
  cpSync(join(source, "composer.html"), join(assembled, "assets/composer.html"));
  // This is the same upstream-assets + owner-metadata overlay performed by Docker.
  const packed = JSON.parse(execFileSync("npm", ["pack", "--ignore-scripts", "--json", "--pack-destination", temporary],
    { cwd: assembled, encoding: "utf8", timeout: 30_000 }));
  assert.equal(packed.length, 1);
  execFileSync("tar", ["-xzf", join(temporary, packed[0].filename), "-C", temporary], { timeout: 10_000 });
  const installed = join(temporary, "package");
  // The installed third-party dependency tree is shared, never owner source files.
  symlinkSync(join(source, "node_modules"), join(installed, "node_modules"), "dir");
  execFileSync(process.execPath, ["build-docs.mjs"], { cwd: installed, timeout: 10_000 });
  const script = `import { loadDocumentBundle } from './documents.mjs';
import { createServer } from 'flint-chart-mcp';
import * as rendering from 'flint-chart-mcp/render';
if (typeof createServer !== 'function' || Object.keys(rendering).length === 0) throw new Error('broken package exports');
const server = createServer({ disableFileReference: true });
await server.close();
const bundle = loadDocumentBundle(process.cwd());
console.log(JSON.stringify({profile: bundle.profile.wire(), manual: bundle.documents.find(d => d.id === 'agents').body}));`;
  const value = JSON.parse(execFileSync(process.execPath, ["--input-type=module", "-e", script],
    { cwd: installed, encoding: "utf8", timeout: 10_000 }));
  const metadata = JSON.parse(readFileSync(join(installed, "package.json"), "utf8"));
  assert.equal(metadata.bin["chart-mcp"], "server.mjs");
  assert.throws(() => execFileSync(process.execPath, [metadata.bin["chart-mcp"]],
    { cwd: installed, env: { ...process.env, VEOVEO_INTERNAL_TRUST_JWKS: "" }, timeout: 10_000, stdio: "pipe" }),
    error => error.stderr.toString().includes("VEOVEO_INTERNAL_TRUST_JWKS is required"));
  assert.deepEqual(value.profile, JSON.parse(readFileSync(join(source, "contract-compliance.json"), "utf8")));
  assert.equal(value.manual, readFileSync(join(source, "AGENTS.md"), "utf8"));
});
