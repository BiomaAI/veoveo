import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("UAV sessions/cameras admit empty observations and reject malformed camera variants before effects",()=>{
 assert.deepEqual(resourceValue("uav-sim://sessions",[]),[]);
 assert.deepEqual(toolValue("list_live_cameras",[]),[]);
 for(const bad of [[{cameraId:"camera",streamPolicy:"unknown"}],[null]])assert.throws(()=>toolValue("list_live_cameras",bad));
 assert.throws(()=>resourceValue("uav-sim://sessions",[{session_id:"session",lifecycle:"unknown"}]));
});
