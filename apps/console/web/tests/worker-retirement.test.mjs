// Behavioral acceptance only. Isolated loopback fixtures, no installation identity.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const root = fileURLToPath(new URL("../", import.meta.url));
const scriptPath = "/console/recording-live-proxy-sw.js";
const legacyScript = `
self.addEventListener('install', e => e.waitUntil(self.skipWaiting()));
self.addEventListener('activate', e => e.waitUntil(self.clients.claim()));
self.addEventListener('fetch', e => {
  if (new URL(e.request.url).pathname === '/console/api/session')
    e.respondWith(new Response('<!doctype html>obsolete response', { headers: {'content-type':'text/html'} }));
});`;

test("retired worker recovery preserves browser data and unrelated registrations", { timeout: 60_000 }, async () => {
  let legacy = true;
  const retirementScript = await readFile(`${root}public/recording-live-proxy-sw.js`, "utf8");
  const server = await createServer({
    root, configFile: false, base: "/console/",
    server: { host: "127.0.0.1", port: 0 },
    plugins: [{ name: "recovery-fixtures", configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const path = new URL(req.url, "http://fixture.test").pathname;
        res.setHeader("Cache-Control", "no-store");
        if (path === scriptPath || path === "/other/worker.js") {
          res.setHeader("Content-Type", "application/javascript");
          res.end(path === scriptPath ? (legacy ? legacyScript : retirementScript) : "self.addEventListener('install', e => e.waitUntil(self.skipWaiting()));");
        } else if (path === "/console/api/session") {
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify({ authenticated: true }));
        } else if (path === "/console/fixture") {
          res.setHeader("Content-Type", "text/html");
          res.end(`<!doctype html><output></output><script type="module">
            import {retireRecordingWorker} from '/console/src/retireRecordingWorker.ts';
            if (!location.search && !await retireRecordingWorker()) {
              const response = await fetch('/console/api/session');
              document.querySelector('output').textContent = JSON.stringify(await response.json());
            }
          </script>`);
        } else next();
      });
    } }],
  });
  // This test exercises navigation and Service Worker lifecycle, not rendering.
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  const context = await browser.newContext();
  context.setDefaultTimeout(10_000);
  try {
    await server.listen();
    const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
    const page = await context.newPage();
    await page.goto(`${origin}/console/fixture?install`);
    await page.evaluate(async script => {
      localStorage.setItem("upload-fixture", "retained");
      document.cookie = "session-fixture=retained; SameSite=Strict; path=/";
      await navigator.serviceWorker.register('/other/worker.js', {scope:'/other/'});
      await navigator.serviceWorker.register(script, {scope:'/console/'});
      await navigator.serviceWorker.ready;
    }, scriptPath);
    await page.reload();
    await page.waitForFunction(() => !!navigator.serviceWorker.controller);
    assert.match(await page.evaluate(async () => (await fetch('/console/api/session')).text()), /obsolete response/);

    // New application code retires an installed worker before parsing API JSON.
    // Keep serving the legacy script to exclude automatic worker update as the fix.
    await page.goto(`${origin}/console/fixture`);
    await page.waitForFunction(() => document.querySelector('output').textContent.includes('authenticated'));
    assert.equal(await page.evaluate(() => navigator.serviceWorker.controller), null);
    assert.equal(await page.evaluate(() => localStorage.getItem('upload-fixture')), 'retained');
    assert.match(await page.evaluate(() => document.cookie), /session-fixture=retained/);
    assert.equal(await page.evaluate(async () => !!await navigator.serviceWorker.getRegistration('/other/')), true);
    await page.reload();
    await page.waitForFunction(() => document.querySelector('output').textContent.includes('authenticated'));

    // An old application that cannot run the new bootstrap can still retire by
    // fetching the same worker URL. Existing controlled clients regain HTTP JSON.
    await page.goto(`${origin}/console/fixture?install`);
    await page.evaluate(async script => {
      await navigator.serviceWorker.register(script, {scope:'/console/'});
      await navigator.serviceWorker.ready;
    }, scriptPath);
    await page.reload();
    await page.waitForFunction(() => !!navigator.serviceWorker.controller);
    legacy = false;
    await page.evaluate(async () => (await navigator.serviceWorker.getRegistration('/console/')).update());
    await page.waitForFunction(async () => !await navigator.serviceWorker.getRegistration('/console/'));
    assert.deepEqual(await page.evaluate(async () => (await fetch('/console/api/session')).json()), {authenticated:true});
  } finally {
    await context.close();
    await browser.close();
    await server.close();
  }
});
