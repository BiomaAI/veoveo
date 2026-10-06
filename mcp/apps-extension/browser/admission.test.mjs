import tasks from "../testdata/final-tasks.json" with {type:"json"};
import test from "node:test";
import assert from "node:assert/strict";
import {toolEnvelope,successfulToolEnvelope,resourceJson,taskEnvelope,ownerContracts} from "./admission.js";

test("maintained MCP admission preserves opaque content/meta and rejects malformed envelopes",()=>{
 const result={content:[{type:"text",text:"open output"}],structuredContent:{provider:{unknown:null}},_meta:{"ai.veoveo/extension":{open:true}}};
 assert.deepEqual(toolEnvelope(result),result);
 assert.throws(()=>toolEnvelope({content:[{type:"image",data:42}]}));
 assert.throws(()=>taskEnvelope({taskId:42,status:"invented"},"requested"));
 const uri="domain://items";
 assert.deepEqual(resourceJson({contents:[{uri,text:'{"open":null}'}]},uri),{open:null});
 assert.throws(()=>resourceJson({contents:[{uri:"domain://other",text:"{}"}]},uri));
});

test("owner roots validate composed schemas before a caller publishes state",()=>{
 const parse=ownerContracts({$schema:"https://json-schema.org/draft/2020-12/schema",properties:{row:{$ref:"#/$defs/Row"}},$defs:{Row:{type:"object",required:["kind","body"],additionalProperties:false,properties:{kind:{enum:["known"]},body:{type:"object"}}}}});
 let retained={kind:"known",body:{provider:null}};
 for(const malformed of [{kind:"bad",body:{}},{kind:"known"},{kind:"known",body:[],extra:true}]){
  assert.throws(()=>{const admitted=parse("row",malformed);retained=admitted;});
  assert.deepEqual(retained,{kind:"known",body:{provider:null}});
 }
 assert.deepEqual(parse("row",retained),retained);
 assert.throws(()=>parse("unknown",{}));
});

test("actual final flat Task bytes admit seeds and status payloads before effects",()=>{
 assert.deepEqual(toolEnvelope(tasks.seed),tasks.seed);
 assert.deepEqual(taskEnvelope(tasks.detail,tasks.seed.taskId),tasks.detail);
 assert.deepEqual(taskEnvelope(tasks.complete,tasks.seed.taskId),tasks.complete);
 let published=tasks.detail;
 for(const bad of [{...tasks.detail,status:"invented"},{...tasks.complete,result:null},{...tasks.complete,result:undefined},{...tasks.detail,status:"input_required"},{...tasks.detail,status:"failed"},{...tasks.detail,taskId:42}]){
  assert.throws(()=>{published=taskEnvelope(bad,tasks.seed.taskId)});
  assert.deepEqual(published,tasks.detail);
 }
 for(const field of ["resultType","taskId","status","createdAt","lastUpdatedAt"]){
  const bad={...tasks.seed};delete bad[field];
  assert.throws(()=>toolEnvelope(bad));
 }
});

test("task identity and errored tool results reject before a consumer publishes",()=>{
 let retained={draft:"unchanged"};let effects=0;
 for(const value of [tasks.detail,tasks.complete]){
  assert.throws(()=>{retained=taskEnvelope(value,"different-task");effects++});
 }
 const errored={content:[{type:"text",text:"domain failure"}],isError:true,structuredContent:{input:{malicious:true}}};
 assert.deepEqual(toolEnvelope(errored),errored);
 assert.throws(()=>{retained=successfulToolEnvelope(errored).structuredContent;effects++},/domain failure/);
 assert.deepEqual(retained,{draft:"unchanged"});assert.equal(effects,0);
});
