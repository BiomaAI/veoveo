import { build } from "esbuild";
import { readFile, writeFile } from "node:fs/promises";

/** Bundle maintained validators locally; opaque-origin Apps fetch no code. */
export async function buildApp({ entry, template, output }) {
  const result = await build({ entryPoints: [entry], bundle: true, format: "iife",
    minify: true, write: false, target: ["chrome120", "firefox121", "safari17.2"] });
  const source = result.outputFiles[0].text;
  if (source.toLowerCase().includes("</script")) throw new Error("App bundle is not script safe");
  const html = (await readFile(template, "utf8")).replace("/*__VEOVEO_APP_BUNDLE__*/", () => source);
  if (html.includes("/*__VEOVEO_APP_BUNDLE__*/")) throw new Error("App bundle marker remains");
  if (Buffer.byteLength(html) > 2 * 1024 * 1024) throw new Error("App exceeds host 2 MiB limit");
  // JSON Schema URLs are metadata. Only executable remote loads violate packaging.
  if (/<(?:script|link|iframe)\b[^>]*(?:src|href)\s*=\s*["']https?:/i.test(html)) {
    throw new Error("App references remote executable assets");
  }
  await writeFile(output, html);
}
