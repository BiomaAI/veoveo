import test from "node:test";
import assert from "node:assert/strict";
import fixture from "../testdata/projection-workbench.json" with {type:"json"};
import {configValue,streamsProjection,projectionValue,verifyProjectionBytes} from "./contracts.js";
test("serialized owner configuration selects recording projection streaming",()=>{
 const config=configValue(fixture);
 assert.equal(streamsProjection(config,"project"),true);
 assert.equal(streamsProjection(config,"other"),false);
 assert.equal(streamsProjection(configValue({...fixture,streamResult:null}),"project"),false);
});

test("Recording request agreement rejects before stream effect and qualifies exact bytes",async()=>{
 const request={datasetId:"0195dabe-7777-7abc-8def-000000000001",recordingId:"0195dabe-7777-7abc-8def-000000000002",entityPaths:["/sensor"],componentIds:["Scalars:scalars"],timeline:"tick",sampling:{kind:"sample_grid",values:[2,4]},sparseFill:"latest_at_global",maximumEntities:1,maximumColumns:1,maximumSamples:2,maximumRows:2,maximumBytes:1024,deadlineMs:1000,idempotencyKey:"projection-1",units:{"Scalars:scalars":"metres"},coordinateFrameRefs:[]};
 const bytes=new TextEncoder().encode("arrow");
 const hash=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),b=>b.toString(16).padStart(2,"0")).join("");
 const handle={schema:"veoveo.ai/recording-projection-handle/v2",projectionId:"0195dabe-7777-7abc-8def-000000000003",datasetId:request.datasetId,recordingId:request.recordingId,result:{catalogRevision:"catalog-1",queryDigest:"a".repeat(64),timeline:"tick",sampleGrid:[2,4],units:request.units,coordinateFrameRefs:[],omittedSampleCount:1,rowCount:1,arrowSchemaSha256:"b".repeat(64),byteLen:bytes.length,payloadSha256:hash},expiresAt:"2026-10-01T00:00:00Z"};
 assert.deepEqual(projectionValue(handle,request),handle);
 let bridgeEffects=0;
 for(const args of [{...request,datasetId:"0195dabe-7777-7abc-8def-000000000004"},{...request,recordingId:"0195dabe-7777-7abc-8def-000000000004"},{...request,timeline:"other"},{...request,sampling:{kind:"latest_at",at:2}},{...request,maximumBytes:1},{...request,units:{}},{...request,recordingId:42}]){
  assert.throws(()=>{projectionValue(handle,args);bridgeEffects++});
 }

 for(const [object,current,old] of [[request,"datasetId","dataset_id"],[request,"maximumBytes","maximum_bytes"],[handle,"projectionId","projection_id"]]){
  for(const mode of ["replacement","mixed","conflicting"]){
   const invalid=structuredClone(object);invalid[old]=mode==="conflicting"?"retired-conflict":invalid[current];
   if(mode==="replacement")delete invalid[current];
   assert.throws(()=>{projectionValue(object===handle?invalid:handle,object===request?invalid:request);bridgeEffects++});
  }
 }
 for(const mode of ["replacement","mixed","conflicting"]){
  const invalid=structuredClone(handle);invalid.result.payload_sha256=mode==="conflicting"?"0".repeat(64):invalid.result.payloadSha256;
  if(mode==="replacement")delete invalid.result.payloadSha256;
  assert.throws(()=>{projectionValue(invalid,request);bridgeEffects++});
 }
 assert.throws(()=>{projectionValue({...handle,schema:"veoveo.ai/recording-projection-handle/v1"},request);bridgeEffects++});
 assert.equal(bridgeEffects,0);
 await verifyProjectionBytes(handle,[bytes]);
 await assert.rejects(verifyProjectionBytes(handle,[bytes.slice(1)]));
 await assert.rejects(verifyProjectionBytes({...handle,result:{...handle.result,payloadSha256:"0".repeat(64)}},[bytes]));
});
