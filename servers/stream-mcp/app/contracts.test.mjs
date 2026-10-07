import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("Stream collection admission preserves empty/default pages and rejects nested corrupt sessions",()=>{
 const good={sessions:[],limit:100};
 assert.deepEqual(resourceValue("stream://sessions",good),good);
 assert.throws(()=>resourceValue("stream://sessions",{...good,sessions:[{sessionId:42}]}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,nextCursor:42}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,extra:true}));
 assert.throws(()=>resourceValue("map://sessions",good));
});

test("Stream detection admission rejects schema-valid corrupt scalars before consumption",()=>{
 const good={schema:"veoveo.ai/stream-live-results/v2",sessionId:"01983da0-0000-7000-8000-000000000001",pipelineId:"detector",frames:[{index:-1,observedAt:"2026-10-05T00:00:00Z",detections:[{classId:1,label:"person",bounds:{x:0,y:0,width:1,height:1}}]}],processedFrames:1,droppedResultFrames:0};
 assert.deepEqual(admit("results",good),good);
 for(const mutate of [
  value=>{value.processedFrames=0;},
  value=>{value.frames[0].detections[0].classId=65536;},
  value=>{value.frames[0].detections[0].label=" ";},
  value=>{value.frames[0].detections[0].confidence=1.1;},
  value=>{value.frames[0].detections[0].bounds.width=0;},
 ]){const invalid=structuredClone(good);mutate(invalid);assert.throws(()=>admit("results",invalid));}
});

test("Stream resource routes reject wrong parents and unsupported URI components",()=>{
 const page={sessions:[],limit:100};
 for(const address of ["stream://sessions/extra","stream://sessions?","stream://sessions#fragment","stream://other/sessions","stream://sessions/../sessions"])
  assert.throws(()=>resourceValue(address,page));
 for(const id of ["01983da0-0000-4000-8000-000000000001","01983da0-0000-7000-c000-000000000001"])
  assert.throws(()=>resourceValue(`stream://session/${id}/results`,{schema:"veoveo.ai/stream-live-results/v2",sessionId:id,pipelineId:"detector",frames:[],processedFrames:0,droppedResultFrames:0}));
});

test("actual retained Stream producers pass browser admission and retired spellings refuse before retention", async()=>{
 const {readFile}=await import("node:fs/promises");
 const fixtures=JSON.parse(await readFile(new URL("./testdata/contracts.json",import.meta.url),"utf8"));
 function fields(value,path=[],found=[]){
  if(value&&typeof value==="object")for(const [key,child]of Object.entries(value)){
   if(/[A-Z]/u.test(key))found.push([path,key,key.replace(/[A-Z]/gu,c=>`_${c.toLowerCase()}`)]);
   fields(child,[...path,key],found);
  }
  return found;
 }
 for(const [root,good]of Object.entries(fixtures)){
  assert.deepEqual(admit(root,good),good);
  for(const [path,current,retired]of fields(good))for(const mode of ["replacement","mixed","conflicting"]){
   const bad=structuredClone(good);let object=bad;for(const step of path)object=object[step];
   object[retired]=mode==="conflicting"?"retired-conflict":object[current];
   if(mode==="replacement")delete object[current];
   assert.throws(()=>admit(root,bad),`${root}/${path.join("/")}/${retired} ${mode}`);
  }
 }
 const session=fixtures.session.sessionId;
 assert.deepEqual(resourceValue(`stream://session/${session}`,fixtures.session),fixtures.session);
 assert.deepEqual(resourceValue(`stream://session/${session}/results`,fixtures.results),fixtures.results);
 assert.deepEqual(resourceValue(`stream://session/${session}/preview`,fixtures.preview),fixtures.preview);
 assert.deepEqual(toolValue("start_live_session",fixtures.started,{pipelineId:"preview"}),fixtures.started);
 assert.deepEqual(toolValue("stop_live_session",fixtures.stopped,{sessionId:session}),fixtures.stopped);
 for(const [root,old]of [["results","veoveo.stream-live-results/v1"],["preview","veoveo.stream-live-preview/v1"]])
  assert.throws(()=>admit(root,{...fixtures[root],schema:old}));
});
