// Headless behavioral evidence only; this does not qualify visual appearance or GPU rendering.
import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';

const templateDirectory = fileURLToPath(new URL('../../../../templates/python-mcp', import.meta.url));
const html = execFileSync('uv', ['run', '--directory', templateDirectory, '--locked', 'python', '-c',
  'from datasheet_mcp.app import APP_HTML; print(APP_HTML)'], {encoding: 'utf8', timeout: 30000});

test('Datasheet loads bounded report pages on request and rejects foreign continuation', {timeout: 45000}, async () => {
  const browser = await chromium.launch({channel: 'chrome', headless: true});
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(10000);
    const reads = [];
    const next = new URL('datasheet://reports');
    next.searchParams.set('cursor', 'reserved +/&?=#');
    let foreign = false;
    await page.exposeFunction('datasheetFixture', async request => {
      if (request.method === 'ui/initialize') return {hostContext: {theme: 'dark'}};
      assert.equal(request.method, 'resources/read');
      reads.push(request.params.uri);
      let value;
      if (request.params.uri === 'datasheet://reports') {
        value = {items: Array.from({length: 100}, (_, i) => ({task_id: i})), limit: 100,
          next_cursor: 'opaque', next_uri: foreign ? 'artifact://metadata/private' : next.href};
      } else {
        assert.equal(request.params.uri, next.href);
        value = {items: [{task_id: 100}], limit: 100, next_cursor: null, next_uri: null};
      }
      return {contents: [{uri: request.params.uri, text: JSON.stringify(value)}]};
    });
    await page.addInitScript(() => {
      window.addEventListener('message', async ({data}) => {
        if (!['ui/initialize', 'resources/read'].includes(data?.method) || data.id === undefined) return;
        try {window.postMessage({jsonrpc: '2.0', id: data.id, result: await window.datasheetFixture(data)}, '*');}
        catch (error) {window.postMessage({jsonrpc: '2.0', id: data.id, error: {message: error.message}}, '*');}
      });
    });
    await page.route('http://datasheet.test/**', route => route.fulfill({contentType: 'text/html', body: html}));
    await page.goto('http://datasheet.test/');
    await page.waitForFunction(() => document.querySelector('#status').textContent === 'ready');
    assert.deepEqual(reads, []);
    await page.getByRole('button', {name: 'Reports', exact: true}).click();
    await page.waitForFunction(() => !document.querySelector('#more-reports').disabled);
    assert.deepEqual(reads, ['datasheet://reports']);
    assert.equal(JSON.parse(await page.locator('#output').textContent()).length, 100);
    await page.getByRole('button', {name: 'More reports', exact: true}).click();
    await page.waitForFunction(() => !document.querySelector('#reports').disabled);
    assert.deepEqual(reads, ['datasheet://reports', next.href]);
    assert.deepEqual(JSON.parse(await page.locator('#output').textContent()), [{task_id: 100}]);
    assert.equal(await page.locator('#more-reports').isDisabled(), true);
    foreign = true;
    await page.getByRole('button', {name: 'Reports', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#output').textContent === 'Invalid report continuation');
    assert.equal(await page.locator('#more-reports').isDisabled(), true);
    assert.equal(reads.length, 3);
  } finally {
    await browser.close();
  }
});
