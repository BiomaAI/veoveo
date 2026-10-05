// Assembles dist/: the site in public/ plus the whitepaper, its figures and brand files from docs/.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const site = dirname(fileURLToPath(import.meta.url));
const docs = resolve(site, "../../docs");
const dist = join(site, "dist");

rmSync(dist, { recursive: true, force: true });
cpSync(join(site, "public"), dist, { recursive: true });

const paper = readFileSync(join(docs, "veoveo-whitepaper.html"), "utf8");
const figures = new Set(
  [...paper.matchAll(/(?:src|srcset|href)="((?:images|assets\/brand)\/[^"?#]+)[^"]*"|url\(["']?((?:images|assets\/brand)\/[^"')]+)/g)]
    .map((m) => m[1] ?? m[2]),
);
const target = join(dist, "whitepaper");
mkdirSync(join(target, "images"), { recursive: true });
mkdirSync(join(target, "assets", "brand"), { recursive: true });
writeFileSync(join(target, "index.html"), paper);
for (const figure of figures) cpSync(join(docs, figure), join(target, figure));
cpSync(join(docs, "veoveo-whitepaper.pdf"), join(target, "veoveo-whitepaper.pdf"));

// Three.js comes from the pinned npm package, so the page loads exactly the locked release.
const three = join(site, "node_modules", "three");
const vendor = join(dist, "vendor", "three");
mkdirSync(join(vendor, "addons"), { recursive: true });
for (const file of ["three.module.js", "three.core.js"]) cpSync(join(three, "build", file), join(vendor, file));
for (const file of ["loaders/GLTFLoader.js", "utils/BufferGeometryUtils.js", "utils/SkeletonUtils.js", "libs/meshopt_decoder.module.js"]) {
  cpSync(join(three, "examples", "jsm", file), join(vendor, "addons", file));
}

console.log(`built ${dist} with ${figures.size} whitepaper figures`);
