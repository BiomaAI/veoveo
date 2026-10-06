import result from "../testdata/app-output.json" with {type:"json"};
import test from "node:test";
import assert from "node:assert/strict";
import {admit} from "./contracts.js";
test("Timeseries admitted result preserves absent/default artifact fields and null open metadata",()=>{

 assert.deepEqual(admit("forecast",result),result);
 const defaults=structuredClone(result);for(const key of ["mime_type","filename","release_state","compliance"])delete defaults.artifact[key];
 assert.deepEqual(admit("forecast",defaults),defaults);
 const open=structuredClone(result);open.artifact.metadata=null;
 assert.deepEqual(admit("forecast",open),open);
 for(const bad of [{...result,preview:[{series_id:"x",observed:[],forecast:[{step:0,mean:1,q10:"bad",q90:2}]}]},{...result,forecast:{...result.forecast,method:"unknown"}},{...result,artifact:{}}])assert.throws(()=>admit("forecast",bad));
});
