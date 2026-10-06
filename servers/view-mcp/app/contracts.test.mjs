import taskBytes from "../../../mcp/apps-extension/testdata/final-tasks.json" with {type:"json"};
import {taskEnvelope} from "../../../mcp/apps-extension/browser/admission.js";
import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("View layer rows reject malformed data before retaining a catalog",()=>{
 const good=[{layer_id:"fixture",label:"Fixture",source_kind:"google_photorealistic"}];
 assert.deepEqual(resourceValue("view://layers",good),good);
 let retained=good;
 for(const bad of [[{layer_id:"fixture",source_kind:"known"}],[{layer_id:42,label:"bad",source_kind:"known"}]]){
  assert.throws(()=>{retained=resourceValue("view://layers",bad)});assert.deepEqual(retained,good);
 }
 assert.throws(()=>resourceValue("map://layers",good));
});

test("View final Task details require the requested identity before capture effects",()=>{
 let captures=0;
 assert.throws(()=>{taskEnvelope(taskBytes.complete,"another-task");captures++});
 assert.equal(captures,0);
 assert.deepEqual(taskEnvelope(taskBytes.complete,taskBytes.seed.taskId),taskBytes.complete);
});
