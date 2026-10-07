import result from "../testdata/app-output.json" with {type:"json"};
import test from "node:test";
import assert from "node:assert/strict";
import {admit} from "./contracts.js";
import bundle from "./generated/timeseries.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
test("Timeseries admitted result preserves absent/default artifact fields and null open metadata",()=>{

 assert.deepEqual(admit("forecast",result),result);
 const defaults=structuredClone(result);for(const key of ["mimeType","filename","releaseState","compliance"])delete defaults.artifact[key];
 assert.deepEqual(admit("forecast",defaults),defaults);
 const open=structuredClone(result);open.artifact.metadata=null;
 assert.deepEqual(admit("forecast",open),open);
 for(const bad of [{...result,preview:[{seriesId:"x",observed:[],forecast:[{step:0,mean:1,q10:"bad",q90:2}]}]},{...result,forecast:{...result.forecast,method:"unknown"}},{...result,artifact:{}}])assert.throws(()=>admit("forecast",bad));
});

test("Timeseries relationships reject schema-valid contradictions before chart state",()=>{
 const good=structuredClone(result);
 good.forecast={method:"naive_trend",horizon:1000,sourceRows:2,series:[{seriesId:"user label",observedRows:2,forecastRows:1000}]};
 good.preview=[{seriesId:"user label",observed:[{value:1}],forecast:[{step:1,mean:2,q10:1,q90:3},{step:1000,mean:4,q10:3,q90:5}]}];
 assert.deepEqual(admit("forecast",good),good);
 for(const mutate of [
  value=>{value.resultUri="timeseries://artifact/00000000-0000-7000-8000-000000000002";},
  value=>{value.forecast.sourceRows=3;},
  value=>{value.forecast.series[0].forecastRows=999;},
  value=>{value.preview[0].seriesId="another label";},
  value=>{value.preview[0].forecast[1].step=999;},
  value=>{value.preview[0].forecast[1].step=1;},
  value=>{value.preview[0].forecast[0].q10=3;},
  value=>{value.forecast.series.push(structuredClone(value.forecast.series[0]));value.preview.push(structuredClone(value.preview[0]));value.forecast.sourceRows=4;},
 ]){const invalid=structuredClone(good);mutate(invalid);assert.throws(()=>admit("forecast",invalid));}
});

test("Timeseries Artifact address admission preserves generic aliases and its canonical result route",()=>{
 const id=result.artifact.artifactId;
 for(const alias of [id.toUpperCase(),id.replaceAll("-",""),`urn:uuid:${id}`,`{${id}}`]){
  const value=structuredClone(result);value.artifact.artifactId=alias;
  assert.deepEqual(admit("forecast",value),value);
 }
 for(const address of [`artifact://${id.replaceAll("-","")}`,`independent+fixture.v2://artifact/urn:uuid:${id}`,`reason://artifact/${id.toUpperCase()}`]){
  const value=structuredClone(result);value.artifact.artifactUri=address;
  assert.deepEqual(admit("forecast",value),value);
 }
 for(const address of [`reason://artifact/${id}`,`timeseries://wrong/${id}`,`timeseries://artifact/${id}/extra`,`timeseries://artifact/${id}?`,`timeseries://artifact/${id}#fragment`,`timeseries://artifact/${id.replaceAll("-","")}`,`timeseries://artifact/${id.replace("7000","4000")}`,`timeseries://artifact/${id.replace("8000","c000")}`]){
  const value=structuredClone(result);value.resultUri=address;assert.throws(()=>admit("forecast",value),address);
 }
 for(const address of [`fixture://wrong/${id}`,`fixture://artifact/${id}?`,`fixture://artifact/${id}#fragment`,`fixture://artifact/${id}/extra`,`fixture://artifact/%30${id.substring(1)}`,`fixture://artifact/{${id}}`]){
  const value=structuredClone(result);value.artifact.artifactUri=address;assert.throws(()=>admit("forecast",value),address);
 }
});


test("Timeseries rejects ordinary JSON counts rounded before cross-field admission",()=>{
 const input={...result,forecast:{method:"naive_trend",horizon:1,sourceRows:0,
  series:[{seriesId:"label",observedRows:0,forecastRows:1}]},
  preview:[{seriesId:"label",observed:[{value:1}],forecast:[{step:1,mean:2,q10:1,q90:3}]}]};
 const raw=JSON.stringify(input).replace('"sourceRows":0','"sourceRows":9007199254740993')
  .replace('"observedRows":0','"observedRows":9007199254740992');
 const rounded=JSON.parse(raw);
 assert.equal(rounded.forecast.sourceRows,rounded.forecast.series[0].observedRows,
  "distinct u64 wire counts collapse during ordinary JSON parsing");
 assert.throws(()=>admit("forecast",rounded),/exact JSON integer range/);
 for(const mutate of [
  value=>{value.forecast.sourceRows=9007199254740992;},
  value=>{value.forecast.series[0].observedRows=9007199254740992;}
 ]){const bad=structuredClone(input);mutate(bad);assert.throws(()=>admit("forecast",bad),/exact JSON integer range/);}
 const safe=structuredClone(input);
 safe.forecast.sourceRows=Number.MAX_SAFE_INTEGER;
 safe.forecast.series[0].observedRows=Number.MAX_SAFE_INTEGER;
 assert.deepEqual(admit("forecast",safe),safe);
});


test("Timeseries schema-valid empty or zero-observation success rejects before chart effects",()=>{
 const schemaOnly=ownerContracts(bundle);
 const empty=structuredClone(result);
 empty.forecast.sourceRows=0;empty.forecast.series=[];empty.preview=[];
 assert.doesNotThrow(()=>schemaOnly("forecast",empty));
 assert.throws(()=>admit("forecast",empty),/usable observations/);
 const zero=structuredClone(result);
 zero.forecast.sourceRows=0;zero.forecast.series[0].observedRows=0;zero.preview[0].observed=[];
 assert.doesNotThrow(()=>schemaOnly("forecast",zero));
 assert.throws(()=>admit("forecast",zero),/usable observations/);
});

test("Timeseries current result refuses retired root and nested wire members",()=>{
 for(const [parent,canonical,retired] of [
  ["","resultUri","result_uri"], ["/forecast","sourceRows","source_rows"],
  ["/forecast/series/0","seriesId","series_id"],
  ["/forecast/series/0","observedRows","observed_rows"],
  ["/forecast/series/0","forecastRows","forecast_rows"],
  ["/preview/0","seriesId","series_id"],
  ["/artifact","artifactUri","artifact_uri"], ["/artifact","createdAt","created_at"],
 ])for(const mixed of [false,true]){
  const invalid=structuredClone(result);
  const object=parent.split("/").filter(Boolean).reduce((value,key)=>value[key],invalid);
  object[retired]=object[canonical];if(!mixed)delete object[canonical];
  assert.throws(()=>admit("forecast",invalid));
 }
 const previous=structuredClone(result);
 const invalid=structuredClone(result);invalid.preview[0].observed=Array.from({length:502},()=>({value:1}));
 assert.throws(()=>admit("forecast",invalid));
 assert.deepEqual(result,previous);
});
