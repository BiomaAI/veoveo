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
 const request={dataset_id:"0195dabe-7777-7abc-8def-000000000001",recording_id:"0195dabe-7777-7abc-8def-000000000002",entity_paths:["/sensor"],component_ids:["Scalars:scalars"],timeline:"tick",sampling:{kind:"sample_grid",values:[2,4]},sparse_fill:"latest_at_global",maximum_entities:1,maximum_columns:1,maximum_samples:2,maximum_rows:2,maximum_bytes:1024,deadline_ms:1000,idempotency_key:"projection-1",units:{"Scalars:scalars":"metres"},coordinate_frame_refs:[]};
 const bytes=new TextEncoder().encode("arrow");
 const hash=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),b=>b.toString(16).padStart(2,"0")).join("");
 const handle={schema:"veoveo.ai/recording-projection-handle/v1",projection_id:"0195dabe-7777-7abc-8def-000000000003",dataset_id:request.dataset_id,recording_id:request.recording_id,result:{catalog_revision:"catalog-1",query_digest:"a".repeat(64),timeline:"tick",sample_grid:[2,4],units:request.units,coordinate_frame_refs:[],omitted_sample_count:1,row_count:1,arrow_schema_sha256:"b".repeat(64),byte_len:bytes.length,payload_sha256:hash},expires_at:"2026-10-01T00:00:00Z"};
 assert.deepEqual(projectionValue(handle,request),handle);
 let bridgeEffects=0;
 for(const args of [{...request,dataset_id:"0195dabe-7777-7abc-8def-000000000004"},{...request,recording_id:"0195dabe-7777-7abc-8def-000000000004"},{...request,timeline:"other"},{...request,sampling:{kind:"latest_at",at:2}},{...request,maximum_bytes:1},{...request,units:{}},{...request,recording_id:42}]){
  assert.throws(()=>{projectionValue(handle,args);bridgeEffects++});
 }
 assert.equal(bridgeEffects,0);
 await verifyProjectionBytes(handle,[bytes]);
 await assert.rejects(verifyProjectionBytes(handle,[bytes.slice(1)]));
 await assert.rejects(verifyProjectionBytes({...handle,result:{...handle.result,payload_sha256:"0".repeat(64)}},[bytes]));
});
