// Behavioral acceptance of React receiver ownership. The renderer is a test double;
// headed hardware playback remains a separate installed acceptance requirement.
import test from 'node:test';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import {createServer} from 'vite';
import react from '@vitejs/plugin-react';

const recording = '019fa000-0000-7000-8000-000000000001';
const entry = `import React from 'react';
import {createRoot} from 'react-dom/client';
import {RecordingsView} from '/src/views/Recordings.tsx';
const root=createRoot(document.querySelector('#root'));
window.renderRecording=(state='live',layerCount=0,committedLayerCount=0)=>root.render(
 React.createElement(RecordingsView,{snapshot:{recordings:[{
 id:'${recording}',application:'fixture',recordingKey:'rollover',state,layerCount,
 committedLayerCount,committedByteLength:committedLayerCount*128,
 startedAt:'2026-10-03T00:00:00Z',lastDataAt:'2026-10-03T00:00:00Z'
 }]},onRecordingSelect:()=>{}}));
window.renderRecording();`;
const viewer = `import React,{useState,useEffect} from 'react';
window.viewerMounts=0;window.viewerUnmounts=0;
export default function Viewer({source}) {
 const [instance]=useState(()=>++window.viewerMounts);
 useEffect(()=>()=>{++window.viewerUnmounts;},[]);
 return React.createElement('output',{
  'data-instance':instance,'data-receiver':source.receiver.kind,
  'data-token':source.redapToken},source.receiver.kind);
}`;

test('Recording catalog refresh preserves a live viewer through rollover and switches on completion', {timeout:60_000}, async () => {
  const manifest = {
    schema:'veoveo.ai/recording-playback/v10',dataset_id:'019fa000-0000-7000-8000-000000000002',
    recording_segment_id:recording,application_id:'fixture',recording_key:'rollover',state:'live',
    started_at:'2026-10-03T00:00:00Z',ended_at:null,catalog_revision:'r0',
    access:{grant_id:'019fa000-0000-7000-8000-000000000003',redap_token:'fixture-0',expires_at:'2099-10-03T00:00:00Z'},
    archive:null,live:{history_seconds:1,video_preroll_seconds:2,transport:'rerun_rrd_channel_v2'},blueprint:null,
  };
  const server = await createServer({root:fileURLToPath(new URL('../',import.meta.url)),configFile:false,base:'/console/',
    optimizeDeps:{include:['react','react-dom/client']},server:{host:'127.0.0.1',port:0},
    plugins:[react(),{name:'recording-fixture',enforce:'pre',
      resolveId(id){
        if(id.endsWith('/fixture-entry.js'))return '\0recording-fixture';
        if(id.endsWith('/components/GovernedRerunViewer'))return '\0viewer-fixture';
      },
      load(id){if(id==='\0recording-fixture')return entry;if(id==='\0viewer-fixture')return viewer;},
      configureServer(server){server.middlewares.use(async(req,res,next)=>{
        if(new URL(req.url,'http://fixture.test').pathname!=='/console/fixture')return next();
        res.setHeader('Content-Type','text/html');
        res.end(await server.transformIndexHtml('/console/fixture','<!doctype html><div id="root"></div><script type="module" src="/console/fixture-entry.js"></script>'));
      });},
    }]});
  let browser;
  try {
    await server.listen();
    browser=await chromium.launch({channel:'chrome',headless:true});
    const context=await browser.newContext();context.setDefaultTimeout(10_000);
    const page=await context.newPage();
    let requests=0;
    await page.route(`**/console/api/recordings/${recording}/playback`,async route=>{
      if(requests++>0)assert.equal(route.request().headers()['x-veoveo-recording-grant'],manifest.access.grant_id);
      await route.fulfill({json:manifest});
    });
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/console/fixture`);
    const output=page.locator('output');
    await page.locator('output[data-receiver="live"][data-token="fixture-0"]').waitFor();
    const instance=await output.getAttribute('data-instance');
    // Initial capture, publication gap and successor layer all keep the same
    // recording-scoped receiver. Rotating credentials force each refresh to settle.
    for(const [revision,layers,committed] of [[1,1,0],[2,1,1],[3,2,1]]) {
      manifest.catalog_revision=`r${revision}`;
      manifest.access.redap_token=`fixture-${revision}`;
      await page.evaluate(([layers,committed])=>window.renderRecording('live',layers,committed),[layers,committed]);
      await page.locator(`output[data-token="fixture-${revision}"]`).waitFor();
      assert.equal(await output.getAttribute('data-instance'),instance);
      assert.equal(await output.getAttribute('data-receiver'),'live');
      assert.deepEqual(await page.evaluate(()=>[window.viewerMounts,window.viewerUnmounts]),[1,0]);
    }
    manifest.state='sealed';manifest.ended_at='2026-10-03T00:01:00Z';manifest.live=null;
    manifest.archive={uri:'rerun://archive.example:443/dataset/fixture',catalog_revision:'r4',
      dataset_id:manifest.dataset_id,recording_segment_id:recording,rrd_version:'0.38.1',
      optimization_profile:'object-store',byte_len:256,layer_count:2};
    await page.evaluate(()=>window.renderRecording('sealed',2,2));
    await page.locator('output[data-receiver="archive"]').waitFor();
    assert.notEqual(await output.getAttribute('data-instance'),instance);
    assert.deepEqual(await page.evaluate(()=>[window.viewerMounts,window.viewerUnmounts]),[2,1]);
    assert.equal(requests,5);
  } finally {
    await browser?.close();await server.close();
  }
});
