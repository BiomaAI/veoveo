import taskBytes from "../../../mcp/apps-extension/testdata/final-tasks.json" with {type:"json"};
import {taskEnvelope} from "../../../mcp/apps-extension/browser/admission.js";
import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("View layer rows reject malformed data before retaining a catalog",()=>{
 const good=[{layerId:"fixture",label:"Fixture",sourceKind:"google_photorealistic"}];
 assert.deepEqual(resourceValue("view://layers",good),good);
 let retained=good;
 for(const bad of [[{layerId:"fixture",sourceKind:"known"}],[{layerId:42,label:"bad",sourceKind:"known"}]]){
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

// Captured by the owning Rust constructors; synthetic frame bytes do not prove rendering.
const produced=(await import("./testdata/contracts.json",{with:{type:"json"}})).default;
function renamedFields(value,path=[]){
 const fields=[];
 if(value&&typeof value==="object")for(const [key,child] of Object.entries(value)){
  if(/[A-Z]/.test(key))fields.push([path,key,key.replace(/[A-Z]/g,c=>`_${c.toLowerCase()}`)]);
  fields.push(...renamedFields(child,[...path,key]));
 }
 return fields;
}
test("View browser retains only current checked producer records",()=>{
 for(const [root,value] of Object.entries(produced)){
  assert.deepEqual(admit(root,value),value);
  let retained=value;
  for(const [path,current,retired] of renamedFields(value))for(const mode of ["replacement","mixed","conflicting"]){
   const invalid=structuredClone(value);
   const object=path.reduce((parent,key)=>parent[key],invalid);
   object[retired]=mode==="conflicting"?"retired conflict":object[current];
   if(mode==="replacement")delete object[current];
   assert.throws(()=>{retained=admit(root,invalid)},`${root}/${[...path,retired].join("/")} ${mode}`);
   assert.deepEqual(retained,value);
  }
 }
 assert.deepEqual(toolValue("create_scene_composition",produced.composition),produced.composition);
 assert.deepEqual(toolValue("create_view",produced.view,{compositionId:produced.composition.compositionId}),produced.view);
 assert.deepEqual(toolValue("capture_frame",produced.frame,{viewId:produced.view.viewId,expectedRevision:produced.view.revision}),produced.frame);
 assert.deepEqual(resourceValue(produced.view.viewUri,produced.view),produced.view);
 assert.deepEqual(resourceValue(`${produced.view.viewUri}/scene`,produced.scene),produced.scene);
 assert.throws(()=>toolValue("capture_frame",produced.frame,{viewId:"other"}));
 assert.throws(()=>toolValue("create_view",produced.view,{compositionId:"other"}));
 assert.throws(()=>resourceValue("view://view/other/scene",produced.scene));
});
