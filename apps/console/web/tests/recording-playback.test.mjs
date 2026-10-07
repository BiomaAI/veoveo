// Behavioral acceptance of React receiver ownership. The renderer is a test double;
// headed hardware playback remains a separate installed acceptance requirement.
import test from 'node:test';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import {createServer} from 'vite';
import react from '@vitejs/plugin-react';
import producer from '../testdata/recording-playback.json' with {type:'json'};
import chronology from '../testdata/recording-chronology.json' with {type:'json'};

const recording = producer.live.recordingSegmentId;
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
  const manifest = structuredClone(producer.live);
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
      if(requests++>0)assert.equal(route.request().headers()['x-veoveo-recording-grant'],manifest.access.grantId);
      await route.fulfill({json:manifest});
    });
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/console/fixture`);
    const output=page.locator('output');
    await page.locator('output[data-receiver="live"][data-token="fixture-0"]').waitFor();
    const instance=await output.getAttribute('data-instance');
    const refusals=await page.evaluate(async ({producer,recording,chronology})=>{
      // page.evaluate bypasses Vite import rewriting; use the fixture's public base.
      const {recordingPlaybackValue}=await import('/console/src/recordingPlayback.ts');
      const live=structuredClone(producer.live),sealed=structuredClone(producer.sealed);
      recordingPlaybackValue(live,recording);recordingPlaybackValue(sealed,recording);
      let admittedEffects=0,rejected=0;
      const refuse=value=>{try{recordingPlaybackValue(value,recording);admittedEffects++;}catch{rejected++;}};
      for(const [pointer,old] of [['recordingSegmentId','recording_segment_id'],['datasetId','dataset_id'],['catalogRevision','catalog_revision']]){
        for(const mode of ['replacement','mixed','conflicting']){
          const value=structuredClone(live);value[old]=mode==='conflicting'?'retired-conflict':value[pointer];
          if(mode==='replacement')delete value[pointer];refuse(value);
        }
      }
      for(const [container,current,old] of [['access','redapToken','redap_token'],['archive','datasetId','dataset_id'],['archive','catalogRevision','catalog_revision']]){
        for(const mode of ['replacement','mixed','conflicting']){
          const value=structuredClone(sealed);value[container][old]=mode==='conflicting'?'retired-conflict':value[container][current];
          if(mode==='replacement')delete value[container][current];refuse(value);
        }
      }
      const old=structuredClone(live);old.schema='veoveo.ai/recording-playback/v10';refuse(old);
      for(const [path,replacement] of [
        [['recordingSegmentId'],'019fa000-0000-7000-8000-000000000004'],
        [['archive','datasetId'],'019fa000-0000-7000-8000-000000000004'],
        [['archive','recordingSegmentId'],'019fa000-0000-7000-8000-000000000004'],
        [['archive','catalogRevision'],'other-revision'],
        [['state'],'live'],
        [['endedAt'],'2026-10-02T00:00:00Z'],
      ]){
        const value=structuredClone(sealed);let target=value;
        for(const part of path.slice(0,-1))target=target[part];
        if(!Object.hasOwn(target,path.at(-1)))throw new Error('control targeted a missing current member');
        target[path.at(-1)]=replacement;refuse(value);
      }
      for(const [path,replacement] of [
        [['startedAt'],'not-a-date'], [['endedAt'],'not-a-date'], [['access','expiresAt'],'not-a-date'],
        [['startedAt'],'2026-02-30T00:00:00Z'], [['startedAt'],'2026-10-03'],
        [['applicationId'],' '], [['recordingKey'],'bad\nkey'], [['catalogRevision'],'x'.repeat(129)],
        [['access','redapToken'],' '], [['archive','rrdVersion'],'x'.repeat(129)],
        [['archive','optimizationProfile'],' '], [['archive','byteLen'],0], [['archive','layerCount'],0],
      ]) {
        const value=structuredClone(sealed);let target=value;
        for(const part of path.slice(0,-1))target=target[part];
        if(!Object.hasOwn(target,path.at(-1)))throw new Error('control targeted a missing current member');
        target[path.at(-1)]=replacement;refuse(value);
      }
      for(const field of ['historySeconds','videoPrerollSeconds']) {
        const value=structuredClone(live);value.live[field]=0;refuse(value);
      }
      for(const uri of producer.archiveRefusals) {
        const value=structuredClone(sealed);value.archive.uri=uri;refuse(value);
      }
      // Valid timezone offsets and millisecond precision come from the chrono/RFC3339 profile.
      const offset=structuredClone(sealed);offset.startedAt='2026-10-03T02:00:00+02:00';
      offset.endedAt='2026-10-03T02:01:00.001+02:00';recordingPlaybackValue(offset,recording);
      const {ChronoTimestamp}=await import('/console/src/chronoTimestamp.ts');
      const {compileGeneratedSchema}=await import('/console/src/jsonSchema.ts');
      const playbackSchema=(await import('/console/src/generated/recording-playback.schema.json')).default;
      const validate=compileGeneratedSchema(playbackSchema);
      if(chronology.length<27)throw new Error('current Rust chronology fixture is incomplete');
      if(!Array.isArray(playbackSchema.required) || !playbackSchema.required.includes('startedAt') || playbackSchema.required.includes('endedAt'))throw new Error('Playback required-list differs from owner Option admission');
      for(const name of ['live_ended_omitted','live_ended_null'])if(!chronology.some(row=>row.name===name))throw new Error('missing optional chronology companion: '+name);
      let chronologyAdmitted=0,chronologyRefused=0;
      for(const row of chronology){
        if(row.admitted && !validate.safeParse(row.producedWire).success)throw new Error('generated schema rejected actual Chrono producer: '+row.name);
        for(const value of [row.inputWire,row.producedWire].filter(value=>value!==null)){
          if(row.admitted && !validate.safeParse(value).success)throw new Error('schema refused current input before receiver: '+row.name);
          let admitted;
          try{admitted=recordingPlaybackValue(value,value.recordingSegmentId);}catch{
            if(row.admitted)throw new Error('current chronology refused: '+row.name);
            chronologyRefused++;continue;
          }
          if(!row.admitted)throw new Error('invalid chronology reached viewer admission: '+row.name);
          // Admission returns the original manifest and spelling, never a Date-normalized copy.
          if(admitted!==value)throw new Error('chronology replaced producer object');
          for(const [wire,expected] of [[value.startedAt,row.started],[value.endedAt,row.ended],[value.access.expiresAt,row.expires]]){
            if(expected===null){if(wire!=null)throw new Error('optional end changed: '+row.name);continue;}
            const instant=ChronoTimestamp.parse(wire);
            if(instant.wire!==wire || instant.wholeSecond!==BigInt(expected.wholeSecond) || instant.nanosecond!==expected.nanosecond)throw new Error('Chrono position differs: '+row.name);
          }
          if(value.endedAt!=null && ChronoTimestamp.parse(value.endedAt).compare(ChronoTimestamp.parse(value.startedAt))<0)throw new Error('Chrono ordering differs: '+row.name);
          chronologyAdmitted++;
        }
      }
      return {admittedEffects,rejected,chronologyAdmitted,chronologyRefused};
    },{producer,recording,chronology});
    assert.deepEqual(refusals,{admittedEffects:0,rejected:40+producer.archiveRefusals.length,chronologyAdmitted:2*chronology.filter(row=>row.admitted).length,chronologyRefused:chronology.filter(row=>!row.admitted).length});
    assert.deepEqual(await page.evaluate(()=>[window.viewerMounts,window.viewerUnmounts]),[1,0]);

    // Initial capture, publication gap and successor layer all keep the same
    // recording-scoped receiver. Rotating credentials force each refresh to settle.
    for(const [revision,layers,committed] of [[1,1,0],[2,1,1],[3,2,1]]) {
      manifest.catalogRevision=`r${revision}`;
      manifest.access.redapToken=`fixture-${revision}`;
      await page.evaluate(([layers,committed])=>window.renderRecording('live',layers,committed),[layers,committed]);
      await page.locator(`output[data-token="fixture-${revision}"]`).waitFor();
      assert.equal(await output.getAttribute('data-instance'),instance);
      assert.equal(await output.getAttribute('data-receiver'),'live');
      assert.deepEqual(await page.evaluate(()=>[window.viewerMounts,window.viewerUnmounts]),[1,0]);
    }
    Object.assign(manifest,structuredClone(producer.sealed));
    await page.evaluate(()=>window.renderRecording('sealed',2,2));
    await page.locator('output[data-receiver="archive"]').waitFor();
    assert.notEqual(await output.getAttribute('data-instance'),instance);
    assert.deepEqual(await page.evaluate(()=>[window.viewerMounts,window.viewerUnmounts]),[2,1]);
    assert.equal(requests,5);
  } finally {
    await browser?.close();await server.close();
  }
});
