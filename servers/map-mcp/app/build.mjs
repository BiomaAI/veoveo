import { build } from "esbuild";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const directory = dirname(fileURLToPath(import.meta.url));
const maplibre = resolve(directory, "node_modules/maplibre-gl/dist");
const workerBuild = await build({
  entryPoints: [resolve(maplibre, "maplibre-gl-worker.mjs")],
  bundle: true,
  format: "iife",
  minify: true,
  write: false,
  target: ["chrome120", "firefox121", "safari17.2"],
});
const workerSource = workerBuild.outputFiles[0].text;

const workerPlugin = {
  name: "embedded-maplibre-worker",
  setup(builder) {
    builder.onResolve({ filter: /^embedded:maplibre-worker$/ }, () => ({
      path: "maplibre-worker",
      namespace: "embedded-maplibre",
    }));
    builder.onLoad({ filter: /.*/, namespace: "embedded-maplibre" }, () => ({
      contents: `export default ${JSON.stringify(workerSource)};`,
      loader: "js",
    }));
  },
};

const appBuild = await build({
  entryPoints: [resolve(directory, "workspace.js")],
  bundle: true,
  format: "esm",
  minify: true,
  write: false,
  target: ["chrome120", "firefox121", "safari17.2"],
  plugins: [workerPlugin],
});
const [template, maplibreCss] = await Promise.all([
  readFile(resolve(directory, "workspace.template.html"), "utf8"),
  readFile(resolve(maplibre, "maplibre-gl.css"), "utf8"),
]);
// MapLibre's default attribution control carries a navigable external link in
// both its main and worker bundles even when attributionControl is disabled.
// Keep the bundled attribution label but make it local for the offline App.
const appSource = appBuild.outputFiles[0].text.replaceAll(
  'href="https://maplibre.org/"',
  'href="#maplibre"',
);
const html = template
  .replace("/*__MAPLIBRE_CSS__*/", () => maplibreCss)
  .replace("/*__WORKSPACE_JS__*/", () => appSource);
if (html.includes("/*__MAPLIBRE_CSS__*/") || html.includes("/*__WORKSPACE_JS__*/")) {
  throw new Error("workspace template placeholders were not replaced");
}
for (const fetchReference of [
  'src="http://',
  'src="https://',
  "src='http://",
  "src='https://",
  'href="http://',
  'href="https://',
  "href='http://",
  "href='https://",
  "url(http://",
  "url(https://",
  'url("http',
  "url('http",
  "@import",
]) {
  if (html.toLowerCase().includes(fetchReference)) {
    throw new Error(`workspace app contains external fetch reference ${fetchReference}`);
  }
}
const maximumBytes = 2 * 1024 * 1024;
const bytes = Buffer.byteLength(html);
if (bytes > maximumBytes) {
  throw new Error(`workspace app is ${bytes} bytes; host limit is ${maximumBytes}`);
}
await writeFile(resolve(directory, "../assets/workspace-app.html"), html);
console.log(`built workspace-app.html (${bytes} bytes)`);
