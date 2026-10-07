import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("UAV sessions/cameras admit empty observations and reject malformed camera variants before effects",()=>{
 assert.deepEqual(resourceValue("uav-sim://sessions",[]),[]);
 assert.deepEqual(toolValue("list_live_cameras",[]),[]);
 for(const bad of [[{cameraId:"camera",streamPolicy:"unknown"}],[null]])assert.throws(()=>toolValue("list_live_cameras",bad));
 assert.throws(()=>resourceValue("uav-sim://sessions",[{sessionId:"session",lifecycle:"unknown"}]));
});

import fixture from "../../../showcase/uav-sim/runtime/tests/fixtures/adapter_outputs.json" with {type:"json"};
test("UAV App receives current session facts and refuses retired or mixed owner members",()=>{
 const state=fixture.state;
 const current={sessionId:state.sessionId,lifecycle:state.lifecycle,world:state.world,tileLifecycle:state.tiles.lifecycle,vehicleCount:state.vehicles.length,recordingCount:state.recordings.length,timing:state.timing,updatedAt:state.updatedAt};
 assert.deepEqual(resourceValue("uav-sim://sessions",[current]),[current]);
 for(const key of ["sessionId","tileLifecycle","vehicleCount","recordingCount","updatedAt"]){
  const retired=key.replace(/[A-Z]/g,value=>"_"+value.toLowerCase());
  for(const mixed of [false,true]){
   const changed=structuredClone(current);changed[retired]=changed[key];if(!mixed)delete changed[key];
   assert.throws(()=>resourceValue("uav-sim://sessions",[changed]));
  }
 }
 for(const key of ["revisionUri","specSha256","simulationFrameUri","georeferenceOrigin"]){
  const retired=key.replace(/[A-Z]/g,value=>"_"+value.toLowerCase());
  for(const mixed of [false,true]){
   const changed=structuredClone(current);changed.world[retired]=changed.world[key];if(!mixed)delete changed.world[key];
   assert.throws(()=>resourceValue("uav-sim://sessions",[changed]));
  }
 }
});
