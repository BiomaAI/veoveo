// Behavioral acceptance of session navigation and tool results; no rendering or GPU acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

const html = await readFile(new URL('../../../../servers/stream-mcp/assets/live.html', import.meta.url), 'utf8');
const browserHarness = await readFile(new URL('../../../../examples/bioma/acceptance/src/browser/browser.rs', import.meta.url), 'utf8');
const ensureSession = browserHarness.match(/const STREAM_APP_ENSURE_SESSION: &str = r#"([\s\S]*?)"#;/)[1];
const session = (number) => ({
  sessionId: `00000000-0000-7000-8000-${String(number).padStart(12, '0')}`,
  sessionUri: `stream://session/00000000-0000-7000-8000-${String(number).padStart(12, '0')}`,
  pipelineId:'fixture',pipelineUri:'stream://pipeline/fixture',lifecycle:'stopped',
  resultsUri:`stream://session/00000000-0000-7000-8000-${String(number).padStart(12,'0')}/results`,
  previewUri:`stream://session/00000000-0000-7000-8000-${String(number).padStart(12,'0')}/preview`,
  startedAt:'2026-10-05T00:00:00Z',receivedVideoFrames:0,processedFrames:0,
  ingress: {transport:'rtp_h264_udp',host:'fixture',port:9001,payloadType:96,clockRate:90000,caps:'application/x-rtp'},
  video: {codec: 'avc1.42e01f', width: 640, height: 480, frameRate: 30, expectedBitrateBps: 1000000},
});

test('Live Monitor and acceptance wait for initial session discovery before starting a pipeline', {timeout: 45000}, async () => {
  const browser = await chromium.launch({channel: 'chrome', headless: true});
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(10000);
    let releaseCatalog, releaseResults, startCalls = 0;
    const catalogGate = new Promise(resolve => {releaseCatalog = resolve;});
    const resultGate = new Promise(resolve => {releaseResults = resolve;});
    const active = {...session(1), lifecycle: 'running'};
    await page.exposeFunction('streamFixture', async (request) => {
      if (request.method === 'ui/initialize') return {hostContext: {theme: 'dark'}};
      if (request.method === 'tools/call') {
        startCalls += 1;
        throw new Error('pipeline already has an active live session');
      }
      const uri = request.params.uri;
      let value;
      if (uri === 'stream://pipelines') value = [{id:'fixture',uri:'stream://pipeline/fixture',title:'Fixture',description:'Fixture',profile:{kind:'pass_through'},supportsLiveInput:true,supportsRecordingReplay:true}];
      else if (uri === 'stream://sessions') {
        await catalogGate;
        value = {sessions: [active], limit: 100};
      } else if (uri.endsWith('/results')) {
        await resultGate;
        value = {schema:'veoveo.ai/stream-live-results/v2',sessionId:uri.split('/')[3],pipelineId:'fixture',frames:[],processedFrames:0,droppedResultFrames:0};
      } else if (uri.endsWith('/preview')) value = {schema:'veoveo.ai/stream-live-preview/v2',sessionId:active.sessionId,video:active.video,chunks:[],droppedChunks:0,receivedVideoFrames:0};
      else throw new Error(`Unexpected read: ${uri}`);
      return {contents: [{uri, mimeType: 'application/json', text: JSON.stringify(value)}]};
    });
    await page.addInitScript(() => {
      // This fixture checks controls and MCP calls; it supplies no rendering evidence.
      Object.defineProperty(navigator, 'mediaCapabilities', {value: {decodingInfo: async () => ({supported: true, smooth: true, powerEfficient: false})}});
      window.VideoDecoder = class {
        static async isConfigSupported() {return {supported: true};}
        configure() {}
        close() {}
      };
      window.addEventListener('message', async ({data}) => {
        if (!['ui/initialize', 'resources/read', 'tools/call'].includes(data?.method) || data.id === undefined) return;
        try {
          const result = await window.streamFixture(data);
          window.postMessage({jsonrpc: '2.0', id: data.id, result}, '*');
        } catch (error) {
          window.postMessage({jsonrpc: '2.0', id: data.id, error: {message: error.message}}, '*');
        }
      });
    });
    await page.route('http://stream.test/**', route => route.fulfill({contentType: 'text/html', body: html}));
    await page.goto('http://stream.test/');
    await page.waitForFunction(() => document.querySelector('#pipeline').options.length === 1);
    assert.equal(await page.locator('#start').isDisabled(), true);
    assert.equal(await page.evaluate(ensureSession), 'waiting');
    releaseCatalog();
    await page.waitForFunction(() => document.querySelector('#sessions').options.length === 1);
    assert.equal(await page.locator('#start').isDisabled(), true);
    assert.equal(await page.evaluate(ensureSession), 'waiting');
    releaseResults();
    await page.waitForFunction(() => document.querySelector('#status').textContent === 'running');
    assert.equal(await page.evaluate(ensureSession), 'available');
    assert.equal(startCalls, 0);
    assert.equal(await page.locator('#error').isVisible(), false);
    await page.evaluate(() => window.postMessage({jsonrpc: '2.0', id: 'teardown', method: 'ui/resource-teardown'}, '*'));
  } finally {
    await browser.close();
  }
});

test('Live Monitor navigates one bounded page and returns to the first page after starting a session', {timeout: 45000}, async () => {
  const browser = await chromium.launch({channel: 'chrome', headless: true});
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(10000);
    const reads = [];
    let started = false, stopped = false, holdOlder = false, releaseOlder, olderArrived, corruptPreview = false;
    const current = number => ({...session(number), lifecycle: number === 108 && !stopped ? 'running' : 'stopped'});
    const olderPending = new Promise(resolve => {olderArrived = resolve;});
    await page.exposeFunction('streamFixture', async (request) => {
      if (request.method === 'ui/initialize') return {hostContext: {theme: 'dark'}};
      if (request.method === 'tools/call') {
        if (request.params.name === 'start_live_session') {
          started = true;
          return {content:[],structuredContent:{sessionId:session(108).sessionId,resultUri:session(108).sessionUri,resultsUri:session(108).resultsUri,previewUri:session(108).previewUri,pipelineUri:'stream://pipeline/fixture',ingress:session(108).ingress,video:session(108).video,startedAt:'2026-10-05T00:00:00Z'}};
        }
        assert.equal(request.params.name, 'stop_live_session');
        assert.equal(request.params.arguments.sessionId, session(108).sessionId);
        stopped = true;
        return {content:[],structuredContent:{resultUri:session(108).sessionUri,lifecycle:'stopped',receivedVideoFrames:0,processedFrames:0,stoppedAt:'2026-10-05T00:00:01Z'}};
      }
      assert.equal(request.method, 'resources/read');
      const uri = request.params.uri;
      reads.push(uri);
      let value;
      if (uri === 'stream://pipelines') value = [{id:'fixture',uri:'stream://pipeline/fixture',title:'Fixture',description:'Fixture',profile:{kind:'pass_through'},supportsLiveInput:true,supportsRecordingReplay:true}];
      else if (uri === 'stream://sessions') {
        const first = started ? 108 : 107;
        value = {sessions: Array.from({length: 100}, (_, i) => current(first - i)), limit: 100, nextCursor: 'older-page'};
      } else if (uri === 'stream://sessions?cursor=older-page') {
        if (holdOlder) {
          holdOlder = false;
          await new Promise(resolve => {releaseOlder = resolve; olderArrived();});
        }
        value = {sessions: Array.from({length: 7}, (_, i) => session(7 - i)), limit: 100};
      } else if (uri === session(108).sessionUri) value = current(108);
      else if (uri.endsWith('/results')) value = {schema:'veoveo.ai/stream-live-results/v2',sessionId:uri.split('/')[3],pipelineId:'fixture',frames:[],processedFrames:0,droppedResultFrames:0};
      else if (uri.endsWith('/preview')) value = {schema:'veoveo.ai/stream-live-preview/v2',sessionId:uri.split('/')[3],video:session(1).video,chunks:[],droppedChunks:0,receivedVideoFrames:0};
      else throw new Error(`Unexpected read: ${uri}`);
      if(uri.endsWith('/preview') && corruptPreview === 'nested') value.chunks=[{sequence:0,timestampUs:'invalid',keyframe:true,dataBase64:'AA=='}];
      if(uri.endsWith('/preview') && corruptPreview === 'parent') value.sessionId=session(999).sessionId;
      return {contents: [{uri, mimeType: 'application/json', text: JSON.stringify(value)}]};
    });
    await page.addInitScript(() => {
      // Empty preview responses exercise navigation only; no video is decoded.
      Object.defineProperty(navigator, 'mediaCapabilities', {value: {decodingInfo: async () => ({supported: true, smooth: true, powerEfficient: false})}});
      window.VideoDecoder = class {
        static async isConfigSupported() {return {supported: true};}
        configure() {window.fixtureDecoderConfigurations=(window.fixtureDecoderConfigurations||0)+1;}
        close() {}
      };
      window.addEventListener('message', async ({data}) => {
        if (!['ui/initialize', 'resources/read', 'tools/call'].includes(data?.method) || data.id === undefined) return;
        try {
          const result = await window.streamFixture(data);
          window.postMessage({jsonrpc: '2.0', id: data.id, result}, '*');
        } catch (error) {
          window.postMessage({jsonrpc: '2.0', id: data.id, error: {message: error.message}}, '*');
        }
      });
    });
    await page.route('http://stream.test/**', route => route.fulfill({contentType: 'text/html', body: html}));
    await page.goto('http://stream.test/');
    await page.waitForFunction(() => document.querySelector('#sessions').options.length === 100);
    assert.equal(await page.locator('#page-newer').isDisabled(), true);
    assert.equal(reads.some(uri => uri.includes('?cursor=')), false, 'older pages must not be prefetched');
    await page.getByRole('button', {name: 'Older sessions'}).click();
    await page.waitForFunction(() => document.querySelector('#sessions').options.length === 7);
    assert.equal(await page.locator('#page-status').textContent(), 'Page 2');
    assert.equal(await page.locator('#page-older').isDisabled(), true);
    assert.equal(await page.locator('#sessions').inputValue(), session(7).sessionId);
    await page.getByRole('button', {name: 'Newer sessions'}).click();
    await page.waitForFunction(() => document.querySelector('#sessions').options.length === 100);
    assert.equal(await page.locator('#sessions').inputValue(), session(107).sessionId);
    await page.getByRole('button', {name: 'Older sessions'}).click();
    await page.waitForFunction(() => document.querySelector('#sessions').options.length === 7);
    holdOlder = true;
    await page.evaluate(() => window.postMessage({jsonrpc: '2.0', method: 'ui/notifications/tool-result'}, '*'));
    await olderPending;
    await page.getByRole('button', {name: 'Start live session'}).click();
    await page.waitForFunction(() => document.querySelector('#page-status').textContent === 'Page 1');
    const readBoundary = reads.length;
    releaseOlder();
    await page.waitForFunction(id => document.querySelector('#sessions').value === id, session(108).sessionId);
    assert.equal(await page.locator('#page-status').textContent(), 'Page 1');
    assert.equal(await page.locator('#page-newer').isDisabled(), true);
    assert.equal(await page.locator('#error').isVisible(), false);
    assert.equal(reads.slice(readBoundary).some(uri => uri.startsWith(`stream://session/${session(7).sessionId}/`)), false, 'a stale page response must not select or read its old session');
    assert.equal(reads.filter(uri => uri === session(108).sessionUri).length, 1, 'starting must read the returned result URI');
    await page.getByRole('button', {name: 'Stop', exact: true}).click();
    await page.waitForFunction(() => document.querySelector('#stop').disabled && document.querySelector('#session').textContent.includes('stopped'));
    assert.equal(reads.filter(uri => uri === session(108).sessionUri).length, 2, 'stopping must read the returned result URI');
    assert.equal(await page.locator('#error').isVisible(), false);
    const beforeDecoder=await page.evaluate(()=>window.fixtureDecoderConfigurations||0);
    const retainedSelection=await page.locator('#sessions').inputValue();
    for(const [kind,diagnostic] of [['nested','Invalid preview response'],['parent','Resource belongs to another session']]) {
      corruptPreview=kind;
      await page.evaluate(()=>window.postMessage({jsonrpc:'2.0',method:'ui/notifications/tool-result'},'*'));
      await page.waitForFunction(text=>document.querySelector('#error').textContent.includes(text),diagnostic);
      assert.equal(await page.locator('#sessions').inputValue(),retainedSelection);
      assert.equal(await page.evaluate(()=>window.fixtureDecoderConfigurations||0),beforeDecoder,'invalid preview must be rejected before decoder configuration');
    }
    corruptPreview=false;
    await page.evaluate(() => window.postMessage({jsonrpc: '2.0', id: 'teardown', method: 'ui/resource-teardown'}, '*'));
  } finally {
    await browser.close();
  }
});
