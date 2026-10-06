import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("Stream collection admission preserves empty/default pages and rejects nested corrupt sessions",()=>{
 const good={sessions:[],limit:100};
 assert.deepEqual(resourceValue("stream://sessions",good),good);
 assert.throws(()=>resourceValue("stream://sessions",{...good,sessions:[{session_id:42}]}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,next_cursor:42}));
 assert.throws(()=>resourceValue("stream://sessions",{...good,extra:true}));
 assert.throws(()=>resourceValue("map://sessions",good));
});

test("Stream detection admission rejects schema-valid corrupt scalars before consumption",()=>{
 const good={schema:"veoveo.stream-live-results/v1",session_id:"01983da0-0000-7000-8000-000000000001",pipeline_id:"detector",frames:[{index:-1,observed_at:"2026-10-05T00:00:00Z",detections:[{class_id:1,label:"person",bounds:{x:0,y:0,width:1,height:1}}]}],processed_frames:1,dropped_result_frames:0};
 assert.deepEqual(admit("results",good),good);
 for(const mutate of [
  value=>{value.processed_frames=0;},
  value=>{value.frames[0].detections[0].class_id=65536;},
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
  assert.throws(()=>resourceValue(`stream://session/${id}/results`,{schema:"veoveo.stream-live-results/v1",session_id:id,pipeline_id:"detector",frames:[],processed_frames:0,dropped_result_frames:0}));
});
