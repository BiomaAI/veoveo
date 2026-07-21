import { readFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";

import { chromium } from "playwright";

const toolDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(toolDirectory, "../..");
const catalogPath = path.join(repositoryRoot, "docs/screenshots/catalog.json");
const screenshotRoot = path.dirname(catalogPath);

const args = process.argv.slice(2);
const listOnly = args.includes("--list");
const captureAll = args.includes("--all");
const idsArgument = valueFor("--ids");

const catalog = validateCatalog(JSON.parse(await readFile(catalogPath, "utf8")));

if (listOnly) {
  for (const shot of catalog.screenshots) {
    console.log(`${shot.id.padEnd(28)} ${shot.captureMode.padEnd(8)} ${shot.title}`);
  }
  process.exit(0);
}

const requestedIds = captureAll
  ? new Set(catalog.screenshots.map((shot) => shot.id))
  : new Set((idsArgument ?? "").split(",").map((id) => id.trim()).filter(Boolean));

if (requestedIds.size === 0) {
  throw new Error("select screenshots with --ids id,id or use --all; run --list for the catalog");
}

const selected = catalog.screenshots.filter((shot) => requestedIds.has(shot.id));
const missingIds = [...requestedIds].filter((id) => !selected.some((shot) => shot.id === id));
if (missingIds.length > 0) throw new Error(`unknown screenshot id(s): ${missingIds.join(", ")}`);

const cdpUrl = process.env.CHROME_CDP_URL ?? "http://127.0.0.1:9227";
const browser = await chromium.connectOverCDP(cdpUrl);

try {
  const context = browser.contexts()[0];
  if (!context) throw new Error(`Chrome at ${cdpUrl} has no browser context`);

  const consoleShots = selected.filter((shot) => shot.captureMode === "console");
  if (consoleShots.length > 0) await captureConsoleShots(context, consoleShots, catalog.defaults);

  for (const shot of selected.filter((candidate) => candidate.captureMode === "rerun")) {
    await captureRerunShot(context, shot, catalog.defaults);
  }
} finally {
  await browser.close();
}

function valueFor(flag) {
  const direct = args.find((argument) => argument.startsWith(`${flag}=`));
  if (direct) return direct.slice(flag.length + 1);
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function validateCatalog(value) {
  if (!value || value.schemaVersion !== 1 || !Array.isArray(value.screenshots)) {
    throw new Error("screenshot catalog must use schemaVersion 1 and contain screenshots[]");
  }
  const ids = new Set();
  for (const shot of value.screenshots) {
    for (const field of ["id", "title", "category", "purpose", "output", "captureMode", "recipe"]) {
      if (typeof shot[field] !== "string" || shot[field].length === 0) {
        throw new Error(`catalog screenshot is missing string field ${field}`);
      }
    }
    if (ids.has(shot.id)) throw new Error(`duplicate screenshot id ${shot.id}`);
    ids.add(shot.id);
    if (!new Set(["console", "rerun", "manual"]).has(shot.captureMode)) {
      throw new Error(`unsupported captureMode ${shot.captureMode} for ${shot.id}`);
    }
    const resolved = path.resolve(screenshotRoot, shot.output);
    if (!resolved.startsWith(`${screenshotRoot}${path.sep}`)) {
      throw new Error(`screenshot output escapes docs/screenshots: ${shot.output}`);
    }
  }
  const viewport = value.defaults?.viewport;
  if (!viewport || !Number.isInteger(viewport.width) || !Number.isInteger(viewport.height)) {
    throw new Error("catalog defaults must define an integer viewport");
  }
  return value;
}

async function captureConsoleShots(context, shots, defaults) {
  const existingConsolePage = context.pages().find((page) => {
    try {
      return new URL(page.url()).pathname.startsWith("/console/");
    } catch {
      return false;
    }
  });
  const inferredConsoleUrl = existingConsolePage
    ? new URL("/console/", existingConsolePage.url()).toString()
    : undefined;
  const consoleUrl = process.env.CONSOLE_URL ?? inferredConsoleUrl;
  if (!consoleUrl) {
    throw new Error("CONSOLE_URL is required when authenticated Chrome has no open /console/ tab");
  }

  const page = existingConsolePage ?? await context.newPage();
  const directUrl = process.env.CONSOLE_DIRECT_URL;
  let removeDirectRoute = async () => {};
  if (directUrl) removeDirectRoute = await installDirectConsoleRoute(page, consoleUrl, directUrl);

  try {
    await page.setViewportSize(defaults.viewport);
    for (const shot of shots) {
      if (shot.recipe === "recordings") {
        await page.emulateMedia({ colorScheme: "dark" });
        await assertHardwareCaptureBrowser(page, shot.id);
      }
      await loadConsoleBaseline(page, consoleUrl);
      await runConsoleRecipe(page, shot.recipe);
      await capturePage(page, shot);
    }
  } finally {
    await removeDirectRoute();
  }
}

async function installDirectConsoleRoute(page, consoleUrl, directUrl) {
  const publicOrigin = new URL(consoleUrl).origin;
  const directOrigin = new URL(directUrl).origin;
  const pattern = `${publicOrigin}/console/**`;
  const handler = async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.pathname === "/console/api/stream" || url.pathname.endsWith(".rrd")) {
      await route.continue();
      return;
    }
    const headers = {
      ...request.headers(),
      host: new URL(publicOrigin).host,
      "x-forwarded-host": new URL(publicOrigin).host,
      "x-forwarded-proto": new URL(publicOrigin).protocol.slice(0, -1),
    };
    try {
      const response = await route.fetch({
        url: `${directOrigin}${url.pathname}${url.search}`,
        headers,
        timeout: 120_000,
      });
      await route.fulfill({ response });
    } catch {
      await route.abort("failed").catch(() => {});
    }
  };
  await page.route(pattern, handler);
  return () => page.unroute(pattern, handler);
}

async function loadConsoleBaseline(page, consoleUrl) {
  const response = await page.goto(consoleUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
  if (!response?.ok()) throw new Error(`Console navigation returned ${response?.status() ?? "no response"}`);
  await page.waitForTimeout(500);
  const current = new URL(page.url());
  if (current.pathname.startsWith("/auth/")) {
    throw new Error("Chrome is not authenticated for this Console; complete login in the attached profile");
  }
  await page.getByText(/platform services healthy/i).waitFor({ timeout: 30_000 });
  await page.addStyleTag({
    content: `
      *, *::before, *::after {
        animation-duration: 0s !important;
        animation-delay: 0s !important;
        caret-color: transparent !important;
        transition-duration: 0s !important;
      }
    `,
  });
}

async function runConsoleRecipe(page, recipe) {
  switch (recipe) {
    case "overview":
      await navigate(page, "Overview");
      await page.getByText("Platform health", { exact: true }).waitFor();
      break;
    case "work":
      await navigate(page, "Work");
      await page.getByText("Tasks", { exact: true }).waitFor();
      break;
    case "artifacts":
      await navigate(page, "Artifacts");
      await page.getByRole("columnheader", { name: "Artifact", exact: true }).waitFor();
      break;
    case "artifact-reason":
      await navigate(page, "Artifacts");
      await openArtifact(page, /reason.*\.json|\.reason\.json|reasoning/i);
      await page.getByText("Preview", { exact: true }).waitFor();
      break;
    case "artifact-video":
      await navigate(page, "Artifacts");
      await openArtifact(page, /\.mp4|video\/mp4/i);
      await prepareVideoPreview(page);
      break;
    case "recordings":
      await navigate(page, "Recordings");
      await page.getByText(/playable|complete authorized recording history/i).first().waitFor({ timeout: 30_000 });
      await page.waitForTimeout(12_000);
      break;
    case "mcp-reason":
      await expandMcp(page, "reason");
      break;
    case "mcp-map":
      await expandMcp(page, "map");
      break;
    case "app-chart":
      await openApp(page, "chart-view");
      await populateChartApp(page);
      break;
    case "app-map":
      await openApp(page, "Map data");
      await frameWithText(page, "Authoritative sources");
      break;
    case "app-timeseries":
      await openApp(page, "Timeseries forecast view");
      await populateTimeseriesApp(page);
      break;
    case "access":
      await navigate(page, "Access");
      await page.getByText("Membership", { exact: true }).waitFor();
      break;
    case "audit":
      await navigate(page, "Audit");
      await page.getByText("Audit events", { exact: true }).waitFor();
      break;
    case "cluster":
      await navigate(page, "Cluster");
      await page.getByText("Workloads", { exact: true }).waitFor({ timeout: 30_000 });
      break;
    default:
      throw new Error(`no Console recipe implementation for ${recipe}`);
  }
}

async function navigate(page, label) {
  const button = page.getByRole("button", { name: label, exact: true });
  await button.waitFor({ timeout: 20_000 });
  await button.click();
  await page.waitForTimeout(350);
}

async function openArtifact(page, pattern) {
  await page.getByRole("columnheader", { name: "Artifact", exact: true }).waitFor();
  const rows = page.locator("tbody tr");
  const count = await rows.count();
  for (let index = 0; index < count; index += 1) {
    const row = rows.nth(index);
    if (pattern.test(await row.innerText())) {
      await row.click();
      await page.locator(".drawer").waitFor({ timeout: 20_000 });
      return;
    }
  }
  throw new Error(`no artifact row matched ${pattern}`);
}

async function prepareVideoPreview(page) {
  const video = page.locator(".drawer video");
  await video.waitFor({ timeout: 30_000 });
  await video.evaluate(async (element) => {
    element.muted = true;
    await new Promise((resolve) => {
      if (element.readyState >= 2) resolve();
      else {
        element.addEventListener("loadeddata", resolve, { once: true });
        window.setTimeout(resolve, 10_000);
      }
    });
    if (Number.isFinite(element.duration) && element.duration > 1) {
      element.currentTime = Math.min(element.duration / 2, 8);
    }
    await element.play().catch(() => {});
    await new Promise((resolve) => window.setTimeout(resolve, 700));
    element.pause();
  });
}

async function expandMcp(page, server) {
  await navigate(page, "MCP");
  await page.getByText("MCP servers", { exact: true }).waitFor();
  const rows = page.locator("tr.mcp-summary-row");
  const count = await rows.count();
  for (let index = 0; index < count; index += 1) {
    const row = rows.nth(index);
    const serverCell = (await row.locator("td").nth(1).innerText()).trim().split("\n")[0];
    if (serverCell !== server) continue;
    await row.scrollIntoViewIfNeeded();
    await row.click();
    await page.locator("tr.mcp-detail-row").filter({ hasText: "Tools" }).waitFor();
    return;
  }
  throw new Error(`MCP server ${server} is absent from the catalog`);
}

async function openApp(page, name) {
  await navigate(page, "Apps");
  const button = page.getByRole("button", { name, exact: true });
  await button.waitFor();
  await button.click();
  await page.locator("iframe.app-frame").waitFor({ timeout: 20_000 });
  await page.waitForTimeout(500);
}

async function populateChartApp(page) {
  const input = {
    data: {
      values: [
        { region: "North", missions: 18 },
        { region: "East", missions: 27 },
        { region: "South", missions: 22 },
        { region: "West", missions: 31 },
      ],
    },
    chart_spec: {
      chartType: "Bar Chart",
      encodings: { x: { field: "region" }, y: { field: "missions" } },
    },
    field_display_names: { region: "Operating region", missions: "Completed missions" },
    semantic_types: { missions: "Quantity" },
    options: { addTooltips: true },
  };
  const result = await callConsoleAppTool(
    page,
    "charts",
    "ui://charts/chart-view.html",
    "create_chart_view",
    input,
  );
  await notifyApp(page, input, result);
  await frameWithText(page, "Operating region");
}

async function populateTimeseriesApp(page) {
  const input = {
    source: { kind: "inline_csv", filename: "fleet-readiness.csv", options: { header: true } },
    mapping: { time_column: "date", value_column: "readiness", series_column: "vehicle" },
    horizon: 6,
    method: "naive_trend",
  };
  const result = {
    content: [{ type: "text", text: "Fleet readiness forecast display fixture" }],
    structuredContent: {
      forecast: {
        method: "naive_trend",
        horizon: 6,
        source_rows: 12,
        series: [
          { series_id: "uav-alpha", observed_rows: 6, forecast_rows: 6 },
          { series_id: "uav-bravo", observed_rows: 6, forecast_rows: 6 },
        ],
      },
      preview: [
        forecastSeries("uav-alpha", [71, 74, 76, 79, 82, 86], 89, 3.1),
        forecastSeries("uav-bravo", [63, 67, 69, 73, 77, 80], 84, 3.8),
      ],
    },
  };
  await notifyApp(page, input, result);
  await frameWithText(page, "shaded band is the 10–90% interval");
}

function forecastSeries(seriesId, observedValues, firstForecast, spread) {
  return {
    series_id: seriesId,
    observed: observedValues.map((value, index) => ({
      event_time: `2026-07-${String(index + 1).padStart(2, "0")}`,
      value,
    })),
    forecast: Array.from({ length: 6 }, (_, index) => {
      const mean = firstForecast + index * 3;
      return { step: index + 1, mean, q10: mean - spread, q90: mean + spread };
    }),
  };
}

async function callConsoleAppTool(page, server, appUri, tool, toolArguments) {
  const payload = await page.evaluate(async ({ server, appUri, tool, toolArguments }) => {
    const snapshot = await fetch("/console/api/snapshot", { headers: { Accept: "application/json" } });
    await snapshot.arrayBuffer();
    const csrf = snapshot.headers.get("x-veoveo-csrf-token");
    if (!csrf) return { status: 0, text: "Console snapshot returned no CSRF token" };
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), 30_000);
    try {
      const response = await fetch("/console/api/apps/call", {
        method: "POST",
        signal: controller.signal,
        headers: { "content-type": "application/json", "x-veoveo-csrf-token": csrf },
        body: JSON.stringify({ server, appUri, tool, arguments: toolArguments }),
      });
      return { status: response.status, text: await response.text() };
    } catch (error) {
      return { status: 0, text: error instanceof Error ? error.message : String(error) };
    } finally {
      window.clearTimeout(timer);
    }
  }, { server, appUri, tool, toolArguments });
  if (payload.status !== 200) throw new Error(`MCP App tool ${tool} returned ${payload.status}: ${payload.text}`);
  return JSON.parse(payload.text);
}

async function notifyApp(page, input, result) {
  await page.evaluate(({ input, result }) => {
    const frame = document.querySelector("iframe.app-frame");
    if (!(frame instanceof HTMLIFrameElement) || !frame.contentWindow) {
      throw new Error("MCP App frame is unavailable");
    }
    frame.contentWindow.postMessage({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-input",
      params: { arguments: input },
    }, "*");
    frame.contentWindow.postMessage({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-result",
      params: { result },
    }, "*");
  }, { input, result });
}

async function frameWithText(page, text) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    for (const frame of page.frames().slice(1)) {
      if (await frame.getByText(text, { exact: false }).count()) {
        await frame.getByText(text, { exact: false }).first().waitFor();
        return frame;
      }
    }
    await page.waitForTimeout(250);
  }
  throw new Error(`no MCP App frame rendered text: ${text}`);
}

async function capturePage(page, shot) {
  if (shot.captureMode === "console") {
    await page.getByRole("complementary").getByText("Live", { exact: true }).waitFor({ timeout: 30_000 });
  }
  await redactRenderedIdentity(page);
  await page.evaluate(async () => {
    await document.fonts.ready;
    window.scrollTo(0, 0);
  });
  await page.waitForTimeout(300);
  const output = path.resolve(screenshotRoot, shot.output);
  await mkdir(path.dirname(output), { recursive: true });
  await page.screenshot({ path: output, type: "png" });
  console.log(`captured ${shot.id} -> ${path.relative(repositoryRoot, output)}`);
}

async function redactRenderedIdentity(page) {
  await page.evaluate(() => {
    const principalUri = /https?:\/\/[^\s<]+#[A-Za-z0-9._~:/?#[\]@!$&'()*+,;=%-]+/g;
    const email = /\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi;
    const bearerQuery = /([?&](?:token|code|state)=)[^&\s]+/gi;
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const node of nodes) {
      const current = node.nodeValue ?? "";
      const redacted = current
        .replace(principalUri, "identity://redacted")
        .replace(email, "identity@example.invalid")
        .replace(bearerQuery, "$1redacted");
      if (redacted !== current) node.nodeValue = redacted;
    }
  });
}

async function captureRerunShot(context, shot, defaults) {
  if (shot.recipe === "rerun-uav") {
    await captureGovernedUavRecording(context, shot, defaults);
    return;
  }

  const environmentName = "RERUN_SUMO_URL_PATTERN";
  const fallback = "127.0.0.1:9092";
  const pattern = process.env[environmentName] ?? fallback;
  const page = context.pages().find((candidate) => candidate.url().includes(pattern));
  if (!page) {
    throw new Error(`${shot.id} needs an open Rerun tab matching ${pattern}; override with ${environmentName}`);
  }
  await page.emulateMedia({ colorScheme: "dark" });
  await assertHardwareCaptureBrowser(page, shot.id);
  const rerunTheme = await page.evaluate(() => {
    const memory = localStorage.getItem("egui_memory_ron") ?? "";
    return memory.match(/theme_preference:([A-Za-z]+)/)?.[1] ?? "unknown";
  });
  if (rerunTheme !== "Dark") {
    throw new Error(`${shot.id} requires Rerun's persisted theme to be Dark; received ${rerunTheme}`);
  }
  await page.setViewportSize(defaults.viewport);
  await page.bringToFront();
  await page.waitForTimeout(4_000);
  if (await page.getByText(/software rasterizer/i).count()) {
    throw new Error(`${shot.id} exposed Rerun's software rasterizer warning`);
  }
  const output = path.resolve(screenshotRoot, shot.output);
  await mkdir(path.dirname(output), { recursive: true });
  await page.screenshot({ path: output, type: "png" });
  console.log(`captured ${shot.id} -> ${path.relative(repositoryRoot, output)}`);
}

async function captureGovernedUavRecording(context, shot, defaults) {
  const page = context.pages().find((candidate) => {
    try {
      return new URL(candidate.url()).pathname.startsWith("/console/");
    } catch {
      return false;
    }
  });
  if (!page) {
    throw new Error(`${shot.id} needs an authenticated Console page with a complete UAV recording`);
  }
  const consoleUrl = process.env.CONSOLE_URL ?? new URL("/console/", page.url()).toString();
  await page.setViewportSize(defaults.viewport);
  await page.emulateMedia({ colorScheme: "dark" });
  await assertHardwareCaptureBrowser(page, shot.id);

  if (await page.evaluate(() => Boolean(document.fullscreenElement))) {
    await page.evaluate(() => document.exitFullscreen());
  }
  await loadConsoleBaseline(page, consoleUrl);
  await runConsoleRecipe(page, "recordings");
  const viewer = page.locator(".rerun-web-viewer-host");
  await viewer.waitFor({ timeout: 30_000 });
  await viewer.evaluate(async (element) => element.requestFullscreen());
  await page.waitForFunction(() => Boolean(document.fullscreenElement));

  try {
    const { width, height } = defaults.viewport;
    // Pause before seeking. This keeps the decoder on the selected camera frame
    // while the complete archive remains open behind the unified timeline.
    await page.mouse.click(width * 0.039, height * 0.783);
    await page.waitForTimeout(500);
    // The UAV showcase camera is active near +3 minutes in its standard run.
    await page.mouse.click(width * 0.214, height * 0.874);
    await page.waitForTimeout(12_000);
    if (await page.getByText(/software rasterizer/i).count()) {
      throw new Error(`${shot.id} exposed Rerun's software rasterizer warning`);
    }
    const output = path.resolve(screenshotRoot, shot.output);
    await mkdir(path.dirname(output), { recursive: true });
    await page.screenshot({ path: output, type: "png" });
    console.log(`captured ${shot.id} -> ${path.relative(repositoryRoot, output)}`);
  } finally {
    if (await page.evaluate(() => Boolean(document.fullscreenElement))) {
      await page.evaluate(() => document.exitFullscreen());
    }
  }
}

async function assertHardwareCaptureBrowser(page, shotId) {
  const renderer = await page.evaluate(async () => {
    const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "high-performance" });
    return {
      userAgent: navigator.userAgent,
      vendor: adapter?.info?.vendor ?? "",
      architecture: adapter?.info?.architecture ?? "",
      device: adapter?.info?.device ?? "",
      description: adapter?.info?.description ?? "",
    };
  });
  if (renderer.userAgent.includes("HeadlessChrome")) {
    throw new Error(`${shotId} requires a headed browser; attached Chrome is headless`);
  }
  const fingerprint = [
    renderer.vendor,
    renderer.architecture,
    renderer.device,
    renderer.description,
  ].join(" ").toLowerCase();
  if (!renderer.vendor || /(swiftshader|llvmpipe|software)/.test(fingerprint)) {
    throw new Error(`${shotId} requires a hardware WebGPU adapter; received ${fingerprint || "none"}`);
  }
  console.log(
    `hardware renderer for ${shotId}: ${renderer.vendor} ${renderer.architecture}`.trim(),
  );
}
