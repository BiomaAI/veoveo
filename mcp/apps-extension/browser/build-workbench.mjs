import {buildApp} from "./build.mjs";
import {fileURLToPath} from "node:url";
const path=value=>fileURLToPath(new URL(value,import.meta.url));
await buildApp({entry:path("./workbench.js"),template:path("./workbench.template.html"),output:path("../src/workbench.html")});
