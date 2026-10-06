import {buildApp} from "../../../mcp/apps-extension/browser/build.mjs";
import {fileURLToPath} from "node:url";
const path=value=>fileURLToPath(new URL(value,import.meta.url));
await buildApp({entry:path("./main.js"),template:path("./app.template.html"),output:path("../composer.html")});
