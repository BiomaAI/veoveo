import test from 'node:test';
import assert from 'node:assert/strict';
import {createBridge} from './bridge.js';
import {mapSubscriptionUris, readMapSnapshot, readCollection} from './resources.js';

function fixture(timeout = 200) {
  const sent = [];
  let receive;
  const host = {parent: {postMessage: message => sent.push(message)},
    addEventListener: (_, handler) => {receive = handler;},
    removeEventListener: () => {receive = undefined;}};
  const bridge = createBridge(host, timeout);
  return {bridge, sent, deliver: (data, source = host.parent) => receive?.({data, source})};
}

test('bridge rejects a lost request and accepts a fresh request after timeout', {timeout: 2000}, async () => {
  const f = fixture(30);
  try {
    await assert.rejects(f.bridge.request('resources/read', {uri:'map://workspace'}), /did not respond/);
    const retry = f.bridge.request('resources/read', {uri:'map://workspace'});
    f.deliver({jsonrpc:'2.0',id: f.sent[0].id,result:{stale:true}});
    f.deliver({jsonrpc:'2.0',id: f.sent[1].id,result:{featureRead:true}});
    assert.deepEqual(await retry, {featureRead:true});
  } finally {f.bridge.close();}
});

test('only parent responses settle requests; teardown rejects pending operations', async () => {
  const f = fixture();
  const result = f.bridge.request('tools/call', {name:'query_features'});
  f.deliver({jsonrpc:'2.0',id:f.sent[0].id,result:{}}, {});
  const rejected = assert.rejects(result, /connection has closed/);
  f.bridge.close();
  await rejected;
});

test('subscriptions acknowledge once, deliver MCP notifications, and report termination', async () => {
  const f = fixture();
  const updates = [];
  const errors = [];
  f.bridge.on('notifications/resources/updated', ({uri}) => updates.push(uri));
  const opened = f.bridge.request('subscriptions/listen', {}, {onError: error => errors.push(error.message)});
  const id = f.sent[0].id;
  f.deliver({jsonrpc:'2.0',method:'notifications/subscriptions/acknowledged',params:{_meta:{'io.modelcontextprotocol/subscriptionId':id}}});
  await opened;
  f.deliver({jsonrpc:'2.0',method:'notifications/resources/updated',params:{uri:'map://feature-layers'}});
  f.deliver({jsonrpc:'2.0',id,result:{}});
  assert.deepEqual(updates, ['map://feature-layers']);
  assert.match(errors[0], /Live updates ended/);
  f.bridge.close();
});

test('snapshot reads respect permissions, bound concurrency and fail instead of returning empty data', async () => {
  const access = {featureRead:true, datasetRead:true, administration:true};
  let active = 0;
  let max = 0;
  const read = async uri => {
    max = Math.max(max, ++active);
    await new Promise(resolve => setImmediate(resolve));
    --active;
    if (uri === 'map://feature-layers') throw Error('service unavailable');
    return ["map://mobility-profiles", "map://sources", "map://datasets", "map://acquisitions", "map://publications", "map://compositions"].includes(uri) ? {items: [], limit: 100, nextCursor: null} : [];
  };
  await assert.rejects(readMapSnapshot(access, read), /feature-layers: service unavailable/);
  assert.equal(max, 4);
  assert.ok(!mapSubscriptionUris(access).includes('map://sources'));
  assert.ok(!mapSubscriptionUris(access).includes('map://acquisitions'));
  const calls = [];
  const snapshot = await readMapSnapshot(access, async uri => {calls.push(uri);return {items:[{id:1}],limit:100,nextCursor:null};}, new Set(['map://sources']));
  assert.deepEqual(calls, ['map://sources']);
  assert.deepEqual(snapshot, {sources:[{id:1}]});
  assert.deepEqual(mapSubscriptionUris({featureRead:true}), ['map://feature-layers','map://publications','map://compositions']);
});


test('dataset refresh walks pages before publishing a flat release collection', async () => {
  const calls = [];
  const read = async uri => {
    calls.push(uri);
    if (uri === 'map://datasets') return {items:[{releaseId:'first'}], limit:100, nextCursor:'aabb'};
    if (uri === 'map://datasets?cursor=aabb') return {items:[{releaseId:'last'}], limit:100, nextCursor:null};
    return uri === 'map://sources' ? {items:[],limit:100,nextCursor:null} : [];
  };
  const snapshot = await readMapSnapshot({datasetRead:true}, read, new Set(['map://active-releases']));
  assert.deepEqual(snapshot.datasets, [{releaseId:'first'}, {releaseId:'last'}]);
  assert.deepEqual(new Set(calls), new Set(['map://sources', 'map://datasets', 'map://datasets?cursor=aabb', 'map://active-releases']));
  assert.equal(calls.length, 4);
});

test('collection walks reject stale shapes, invalid bounds, cycles, and failed later pages', async () => {
  for (const page of [[], {}, {items:[],limit:100}, {items:[],limit:101,nextCursor:null},
    {items:Array(101).fill({}),limit:100,nextCursor:null},
    {items:[],limit:100,nextCursor:'bad cursor'}, {items:[],limit:100,nextCursor:'ab'}]) {
    await assert.rejects(readCollection('map://datasets', async () => page));
  }
  await assert.rejects(readCollection('map://datasets', async () => ({items:[{}],limit:100,nextCursor:'aabb'})), /did not advance/);
  await assert.rejects(readCollection('map://datasets', async uri => {
    if (uri === 'map://datasets') return {items:[{}],limit:100,nextCursor:'aabb'};
    throw Error('second page failed');
  }), /second page failed/);
  let calls = 0;
  await assert.rejects(readCollection('map://datasets', async () => ({items:[{}],limit:100,nextCursor:(++calls).toString(16).padStart(4,'0')}), {maxPages:2}), /exceeded 2 pages/);
  assert.equal(calls, 2);
});

test('collection refresh has a total deadline even if the host never answers', async () => {
  await assert.rejects(readCollection('map://datasets', () => new Promise(() => {}), {timeoutMs:10}), /time limit/);
});


test('administrators read acquisition pages without dataset permission', async () => {
  const calls = [];
  const snapshot = await readMapSnapshot({administration:true}, async uri => {
    calls.push(uri);
    return uri === 'map://acquisitions'
      ? {items:[{acquisitionId:'first'}],limit:100,nextCursor:'abcd'}
      : {items:[{acquisitionId:'last'}],limit:100,nextCursor:null};
  });
  assert.deepEqual(calls, ['map://acquisitions', 'map://acquisitions?cursor=abcd']);
  assert.deepEqual(snapshot, {acquisitions:[{acquisitionId:'first'}, {acquisitionId:'last'}]});
});


test('feature readers traverse every metadata index without dataset or admin access', async () => {
  const calls = [];
  const snapshot = await readMapSnapshot({featureRead:true}, async uri => {
    calls.push(uri);
    const root = uri.split('?')[0];
    return uri === root
      ? {items:[{id:root+'/first'}],limit:100,nextCursor:'aabb'}
      : {items:[{id:root+'/last'}],limit:100,nextCursor:null};
  });
  assert.equal(calls.length, 6);
  for (const [root, field] of [['feature-layers','layers'], ['publications','publications'], ['compositions','compositions']]) {
    assert.deepEqual(snapshot[field], [{id:`map://${root}/first`}, {id:`map://${root}/last`}]);
    assert.ok(calls.includes(`map://${root}?cursor=aabb`));
  }
});

test('collection continuation preserves its parent query through the URL builder', async () => {
  const root = 'map://publications?layerId=feature-layer-example';
  const calls = [];
  const items = await readCollection(root, async uri => {
    calls.push(uri);
    return {items: [{id: calls.length}], limit: 100, nextCursor: calls.length === 1 ? 'aabb' : null};
  });
  assert.deepEqual(calls, [root, root + '&cursor=aabb']);
  assert.deepEqual(items, [{id: 1}, {id: 2}]);
});


test('source refresh walks all pages and rejects a failed continuation', async () => {
  const calls = [];
  const read = async uri => {
    calls.push(uri);
    return uri === 'map://sources'
      ? {items:[{sourceId:'first'}],limit:100,nextCursor:'aabb'}
      : {items:[{sourceId:'last'}],limit:100,nextCursor:null};
  };
  const snapshot = await readMapSnapshot({datasetRead:true}, read, new Set(['map://sources']));
  assert.deepEqual(calls, ['map://sources', 'map://sources?cursor=aabb']);
  assert.deepEqual(snapshot, {sources:[{sourceId:'first'}, {sourceId:'last'}]});
  await assert.rejects(readMapSnapshot({datasetRead:true}, async uri => {
    if (uri !== 'map://sources') throw Error('source continuation failed');
    return read(uri);
  }, new Set(['map://sources'])), /source continuation failed/);
});


test('mobility profiles traverse every page before publishing a refresh', async () => {
  const calls = [];
  const read = async uri => {
    calls.push(uri);
    return uri === 'map://mobility-profiles'
      ? {items:[{family:'human',profile:{metadata:{version:1}}}],limit:100,nextCursor:'aabb'}
      : {items:[{family:'human',profile:{metadata:{version:101}}}],limit:100,nextCursor:null};
  };
  const snapshot = await readMapSnapshot({datasetRead:true}, read, new Set(['map://mobility-profiles']));
  assert.deepEqual(calls, ['map://mobility-profiles', 'map://mobility-profiles?cursor=aabb']);
  assert.deepEqual(snapshot.profiles.map(item => item.profile.metadata.version), [1,101]);
  await assert.rejects(readMapSnapshot({datasetRead:true}, async uri => {
    if (uri !== 'map://mobility-profiles') throw Error('profile continuation failed');
    return read(uri);
  }, new Set(['map://mobility-profiles'])), /profile continuation failed/);
});
