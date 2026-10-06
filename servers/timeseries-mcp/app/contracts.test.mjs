import result from "../testdata/app-output.json" with {type:"json"};
import test from "node:test";
import assert from "node:assert/strict";
import {admit} from "./contracts.js";
import bundle from "./generated/timeseries.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
test("Timeseries admitted result preserves absent/default artifact fields and null open metadata",()=>{

 assert.deepEqual(admit("forecast",result),result);
 const defaults=structuredClone(result);for(const key of ["mime_type","filename","release_state","compliance"])delete defaults.artifact[key];
 assert.deepEqual(admit("forecast",defaults),defaults);
 const open=structuredClone(result);open.artifact.metadata=null;
 assert.deepEqual(admit("forecast",open),open);
 for(const bad of [{...result,preview:[{series_id:"x",observed:[],forecast:[{step:0,mean:1,q10:"bad",q90:2}]}]},{...result,forecast:{...result.forecast,method:"unknown"}},{...result,artifact:{}}])assert.throws(()=>admit("forecast",bad));
});

test("Timeseries relationships reject schema-valid contradictions before chart state",()=>{
 const good=structuredClone(result);
 good.forecast={method:"naive_trend",horizon:1000,source_rows:2,series:[{series_id:"user label",observed_rows:2,forecast_rows:1000}]};
 good.preview=[{series_id:"user label",observed:[{value:1}],forecast:[{step:1,mean:2,q10:1,q90:3},{step:1000,mean:4,q10:3,q90:5}]}];
 assert.deepEqual(admit("forecast",good),good);
 for(const mutate of [
  value=>{value.result_uri="timeseries://artifact/00000000-0000-7000-8000-000000000002";},
  value=>{value.forecast.source_rows=3;},
  value=>{value.forecast.series[0].forecast_rows=999;},
  value=>{value.preview[0].series_id="another label";},
  value=>{value.preview[0].forecast[1].step=999;},
  value=>{value.preview[0].forecast[1].step=1;},
  value=>{value.preview[0].forecast[0].q10=3;},
  value=>{value.forecast.series.push(structuredClone(value.forecast.series[0]));value.preview.push(structuredClone(value.preview[0]));value.forecast.source_rows=4;},
 ]){const invalid=structuredClone(good);mutate(invalid);assert.throws(()=>admit("forecast",invalid));}
});

test("Timeseries Artifact address admission preserves generic aliases and its canonical result route",()=>{
 const id=result.artifact.artifact_id;
 for(const alias of [id.toUpperCase(),id.replaceAll("-",""),`urn:uuid:${id}`,`{${id}}`]){
  const value=structuredClone(result);value.artifact.artifact_id=alias;
  assert.deepEqual(admit("forecast",value),value);
 }
 for(const address of [`artifact://${id.replaceAll("-","")}`,`independent+fixture.v2://artifact/urn:uuid:${id}`,`reason://artifact/${id.toUpperCase()}`]){
  const value=structuredClone(result);value.artifact.artifact_uri=address;
  assert.deepEqual(admit("forecast",value),value);
 }
 for(const address of [`reason://artifact/${id}`,`timeseries://wrong/${id}`,`timeseries://artifact/${id}/extra`,`timeseries://artifact/${id}?`,`timeseries://artifact/${id}#fragment`,`timeseries://artifact/${id.replaceAll("-","")}`,`timeseries://artifact/${id.replace("7000","4000")}`,`timeseries://artifact/${id.replace("8000","c000")}`]){
  const value=structuredClone(result);value.result_uri=address;assert.throws(()=>admit("forecast",value),address);
 }
 for(const address of [`fixture://wrong/${id}`,`fixture://artifact/${id}?`,`fixture://artifact/${id}#fragment`,`fixture://artifact/${id}/extra`,`fixture://artifact/%30${id.substring(1)}`,`fixture://artifact/{${id}}`]){
  const value=structuredClone(result);value.artifact.artifact_uri=address;assert.throws(()=>admit("forecast",value),address);
 }
});


test("Timeseries rejects ordinary JSON counts rounded before cross-field admission",()=>{
 const input={...result,forecast:{method:"naive_trend",horizon:1,source_rows:0,
  series:[{series_id:"label",observed_rows:0,forecast_rows:1}]},
  preview:[{series_id:"label",observed:[{value:1}],forecast:[{step:1,mean:2,q10:1,q90:3}]}]};
 const raw=JSON.stringify(input).replace('"source_rows":0','"source_rows":9007199254740993')
  .replace('"observed_rows":0','"observed_rows":9007199254740992');
 const rounded=JSON.parse(raw);
 assert.equal(rounded.forecast.source_rows,rounded.forecast.series[0].observed_rows,
  "distinct u64 wire counts collapse during ordinary JSON parsing");
 assert.throws(()=>admit("forecast",rounded),/exact JSON integer range/);
 for(const mutate of [
  value=>{value.forecast.source_rows=9007199254740992;},
  value=>{value.forecast.series[0].observed_rows=9007199254740992;}
 ]){const bad=structuredClone(input);mutate(bad);assert.throws(()=>admit("forecast",bad),/exact JSON integer range/);}
 const safe=structuredClone(input);
 safe.forecast.source_rows=Number.MAX_SAFE_INTEGER;
 safe.forecast.series[0].observed_rows=Number.MAX_SAFE_INTEGER;
 assert.deepEqual(admit("forecast",safe),safe);
});


test("Timeseries schema-valid empty or zero-observation success rejects before chart effects",()=>{
 const schemaOnly=ownerContracts(bundle);
 const empty=structuredClone(result);
 empty.forecast.source_rows=0;empty.forecast.series=[];empty.preview=[];
 assert.doesNotThrow(()=>schemaOnly("forecast",empty));
 assert.throws(()=>admit("forecast",empty),/usable observations/);
 const zero=structuredClone(result);
 zero.forecast.source_rows=0;zero.forecast.series[0].observed_rows=0;zero.preview[0].observed=[];
 assert.doesNotThrow(()=>schemaOnly("forecast",zero));
 assert.throws(()=>admit("forecast",zero),/usable observations/);
});
