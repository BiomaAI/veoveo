// Headless behavioral checks only; this does not qualify graphics or visual appearance.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

const template = await readFile(new URL('../../../../mcp/apps-extension/src/workbench.html', import.meta.url), 'utf8');
const taskId = index => `0195dabe-7777-7abc-8def-${index.toString(16).padStart(12, '0')}`;
const cases = [
  {domain: 'artifact', title: 'Library', collection: 'artifact://index', label: 'Artifact index',
    other: 'artifact://docs', otherLabel: 'Documentation', otherValue: {items: [{uri: 'artifact://docs/design', title: 'Design'}]},
    knowledge: true, entry: index => ({uri: `artifact://metadata/${taskId(index)}`, title: `Output ${index}`}), cursor: `artifact-index-v1_${taskId(99)}`},
  {domain: 'duckdb-databases', title: 'Workbench', collection: 'duckdb://dbs', label: 'Databases',
    other: 'duckdb://usage', otherLabel: 'Usage ledger', otherValue: {items: [], limit: 100, next_cursor: null},
    entry: index => ({db_id: `db_${String(index).padStart(3, '0')}`, db_uri: `duckdb://db/db_${String(index).padStart(3, '0')}`}),
    cursor: Buffer.from(JSON.stringify({version: 1, collection: 'duckdb://dbs', after: 'db_099'})).toString('base64url')},
  {domain: 'time', title: 'Timeline', collection: 'time://events', label: 'Events',
    other: 'time://clock/current', otherLabel: 'Clock', otherValue: {clock: 'current'},
    knowledge: true, entry: index => ({uri: `time://events/event-${taskId(index)}`, title: `Event ${index}`}), cursor: 'page +two/&?=#'},
  {domain: 'duckdb', title: 'Workbench', collection: 'duckdb://usage', label: 'Usage',
    other: 'duckdb://dbs', otherLabel: 'Databases', otherValue: {items: [{db_id: 'metrics', db_uri: 'duckdb://db/metrics'}], limit: 100, next_cursor: null},
    entry: index => ({task_id: taskId(index), usage_uri: `duckdb://usage/task/${taskId(index)}`}),
    cursor: Buffer.from(JSON.stringify({version: 1, collection: 'duckdb://usage', after: taskId(99)})).toString('base64url')},
  {domain: 'media-usage', title: 'Studio', collection: 'media://usage', label: 'Usage ledger',
    other: 'media://models', otherLabel: 'Model catalog', otherValue: [{model_id: 'test/image'}],
    entry: index => ({task_id: taskId(index), usage_uri: `media://usage/task/${taskId(index)}`}),
    cursor: Buffer.from(JSON.stringify({version: 1, collection: 'media://usage', after: taskId(99)})).toString('base64url')},
  {domain: 'media-predictions', title: 'Studio', collection: 'media://predictions', label: 'Predictions',
    other: 'media://models', otherLabel: 'Model catalog', otherValue: [{model_id: 'test/image'}],
    entry: index => ({id: `prediction-${index}`, prediction_uri: `media://prediction/prediction-${index}`}),
    cursor: Buffer.from(JSON.stringify({version: 1, collection: 'media://predictions', after: 'prediction-99'})).toString('base64url')},
];

for (const fixture of cases) {
  const config = {
    appId: `${fixture.domain}-pagination-test`, title: fixture.title, subtitle: 'Fixture', emptyMessage: 'No resources',
    resources: [{label: fixture.label, uri: fixture.collection}, {label: fixture.otherLabel, uri: fixture.other}],
    tools: [], streamResult: null,
  };
  const nextPage = new URL(fixture.collection);
  nextPage.searchParams.set('cursor', fixture.cursor);

  test(`${fixture.domain} Workbench follows cursor pages, refreshes the current page, and rejects stale responses`, {timeout: 45000}, async () => {
    const browser = await chromium.launch({channel: 'chrome', headless: true});
    try {
      const page = await browser.newPage();
      page.setDefaultTimeout(10000);
      const reads = [];
      let hold = false, release, arrived;
      const pending = new Promise(resolve => {arrived = resolve;});
      await page.exposeFunction('workbenchFixture', async request => {
        if (request.method === 'ui/initialize') return {hostContext: {theme: 'dark'}};
        if (request.method === 'subscriptions/listen') return {};
        assert.equal(request.method, 'resources/read');
        const uri = request.params.uri;
        reads.push(uri);
        let value;
        if (uri === fixture.collection) value = fixture.knowledge
          ? {items: Array.from({length: 100}, (_, i) => fixture.entry(i)), nextCursor: fixture.cursor}
          : {items: Array.from({length: 100}, (_, i) => fixture.entry(i)), limit: 100, next_cursor: fixture.cursor};
        else if (uri === nextPage.href) {
          if (hold) {hold = false; await new Promise(resolve => {release = resolve; arrived();});}
          value = fixture.knowledge ? {items: [fixture.entry(100)]} : {items: [fixture.entry(100)], limit: 100, next_cursor: null};
        } else if (uri === fixture.other) value = fixture.otherValue;
        else throw new Error(`Unexpected read ${uri}`);
        return {contents: [{uri, mimeType: 'application/json', text: JSON.stringify(value)}]};
      });
      await page.addInitScript(() => {
        window.addEventListener('message', async ({data}) => {
          if (!['ui/initialize', 'subscriptions/listen', 'resources/read'].includes(data?.method) || data.id === undefined) return;
          try {window.postMessage({jsonrpc: '2.0', id: data.id, result: await window.workbenchFixture(data)}, '*');}
          catch (error) {window.postMessage({jsonrpc: '2.0', id: data.id, error: {message: error.message}}, '*');}
        });
      });
      await page.route(`http://${fixture.domain}.test/**`, route => route.fulfill({contentType: 'text/html', body: template.replace('__VEOVEO_APP_CONFIG__', JSON.stringify(config))}));
      await page.goto(`http://${fixture.domain}.test/`);
      await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
      assert.deepEqual(reads, [fixture.collection], 'later pages are not prefetched');
      assert.equal(await page.locator('#page-previous').isDisabled(), true);
      await page.getByRole('button', {name: 'Next', exact: true}).click();
      await page.waitForFunction(() => document.querySelector('#page-previous').disabled === false);
      assert.equal(await page.locator('#page-number').textContent(), 'Page 2');
      assert.equal(await page.locator('#page-next').isDisabled(), true);
      assert.deepEqual(JSON.parse(await page.locator('#payload').textContent()).items, [fixture.entry(100)]);
      const before = reads.length;
      await page.getByRole('button', {name: 'Refresh', exact: true}).click();
      await page.waitForFunction(() => document.querySelector('#status').textContent === 'ready');
      assert.equal(reads[before], nextPage.href);
      await page.getByRole('button', {name: 'Previous', exact: true}).click();
      await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
      assert.equal(await page.locator('#page-number').textContent(), 'Page 1');
      hold = true;
      await page.getByRole('button', {name: 'Next', exact: true}).click();
      await pending;
      await page.getByRole('button', {name: fixture.otherLabel, exact: true}).click();
      await page.waitForFunction(expected => document.querySelector('#status').textContent === 'ready' && JSON.stringify(JSON.parse(document.querySelector('#payload').textContent)) === expected, JSON.stringify(fixture.otherValue));
      await page.evaluate(nextUri => {
        window.latePageReceived = false;
        window.addEventListener('message', ({data}) => {
          if (data?.result?.contents?.[0]?.uri === nextUri) window.latePageReceived = true;
        });
      }, nextPage.href);
      release();
      await page.waitForFunction(() => window.latePageReceived);
      assert.equal(await page.locator('#resource-title').textContent(), fixture.otherLabel);
      assert.equal(await page.locator('#pager').isHidden(), !Array.isArray(fixture.otherValue.items));
      assert.deepEqual(JSON.parse(await page.locator('#payload').textContent()), fixture.otherValue);
      await page.getByRole('button', {name: fixture.label, exact: true}).click();
      await page.waitForFunction(() => !document.querySelector('#pager').hidden && document.querySelector('#page-next').disabled === false && document.querySelector('#status').textContent === 'ready');
      assert.equal(await page.locator('#page-number').textContent(), 'Page 1');
    } finally {await browser.close();}
  });
}
