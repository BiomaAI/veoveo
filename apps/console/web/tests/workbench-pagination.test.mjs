// Headless behavioral checks only; this does not qualify graphics or visual appearance.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

const template = await readFile(new URL('../../../../mcp/apps-extension/src/workbench.html', import.meta.url), 'utf8');
const config = {
  appId: 'time-pagination-test', title: 'Timeline', subtitle: 'Fixture', emptyMessage: 'No resources',
  resources: [{label: 'Events', uri: 'time://events'}, {label: 'Clock', uri: 'time://clock/current'}],
  tools: [], streamResult: null,
};

test('Workbench follows explicit cursor pages, refreshes the current page, and rejects stale responses', {timeout: 45000}, async () => {
  const browser = await chromium.launch({channel: 'chrome', headless: true});
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(10000);
    const reads = [];
    let hold = false, release, arrived;
    const pending = new Promise(resolve => {arrived = resolve;});
    await page.exposeFunction('timeFixture', async request => {
      if (request.method === 'ui/initialize') return {hostContext: {theme: 'dark'}};
      if (request.method === 'subscriptions/listen') return {};
      assert.equal(request.method, 'resources/read');
      const uri = request.params.uri;
      reads.push(uri);
      let value;
      if (uri === 'time://events') value = {items: Array.from({length: 100}, (_, i) => ({event_id: i})), limit: 100, next_cursor: 'page-two'};
      else if (uri === 'time://events?cursor=page-two') {
        if (hold) {hold = false; await new Promise(resolve => {release = resolve; arrived();});}
        value = {items: [{event_id: 100}], limit: 100, next_cursor: null};
      } else if (uri === 'time://clock/current') value = {clock: 'current'};
      else throw new Error(`Unexpected read ${uri}`);
      return {contents: [{uri, mimeType: 'application/json', text: JSON.stringify(value)}]};
    });
    await page.addInitScript(() => {
      window.addEventListener('message', async ({data}) => {
        if (!['ui/initialize', 'subscriptions/listen', 'resources/read'].includes(data?.method) || data.id === undefined) return;
        try {window.postMessage({jsonrpc: '2.0', id: data.id, result: await window.timeFixture(data)}, '*');}
        catch (error) {window.postMessage({jsonrpc: '2.0', id: data.id, error: {message: error.message}}, '*');}
      });
    });
    await page.route('http://time.test/**', route => route.fulfill({contentType: 'text/html', body: template.replace('__VEOVEO_APP_CONFIG__', JSON.stringify(config))}));
    await page.goto('http://time.test/');
    await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
    assert.deepEqual(reads, ['time://events'], 'later pages are not prefetched');
    assert.equal(await page.locator('#page-previous').isDisabled(), true);
    await page.getByRole('button', {name: 'Next', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#page-previous').disabled === false);
    assert.equal(await page.locator('#page-number').textContent(), 'Page 2');
    assert.equal(await page.locator('#page-next').isDisabled(), true);
    assert.deepEqual(JSON.parse(await page.locator('#payload').textContent()).items, [{event_id: 100}]);
    const before = reads.length;
    await page.getByRole('button', {name: 'Refresh', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#status').textContent === 'ready');
    assert.equal(reads[before], 'time://events?cursor=page-two');
    await page.getByRole('button', {name: 'Previous', exact: true}).click();
    await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
    assert.equal(await page.locator('#page-number').textContent(), 'Page 1');
    hold = true;
    await page.getByRole('button', {name: 'Next', exact: true}).click();
    await pending;
    await page.getByRole('button', {name: 'Clock', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#payload').textContent.includes('"clock"'));
    await page.evaluate(() => {
      window.latePageReceived = false;
      window.addEventListener('message', ({data}) => {
        if (data?.result?.contents?.[0]?.uri === 'time://events?cursor=page-two') window.latePageReceived = true;
      });
    });
    release();
    await page.waitForFunction(() => window.latePageReceived);
    assert.equal(await page.locator('#resource-title').textContent(), 'Clock');
    assert.equal(await page.locator('#pager').isHidden(), true);
    assert.deepEqual(JSON.parse(await page.locator('#payload').textContent()), {clock: 'current'});
    await page.getByRole('button', {name: 'Events', exact: true}).click();
    await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
    assert.equal(await page.locator('#page-number').textContent(), 'Page 1');
  } finally {await browser.close();}
});
