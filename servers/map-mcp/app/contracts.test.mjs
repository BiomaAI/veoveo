import tasks from "../../../mcp/apps-extension/testdata/final-tasks.json" with {type:"json"};
import {toolEnvelope,taskEnvelope,structuredResult} from "../../../mcp/apps-extension/browser/admission.js";
import test from "node:test";
import assert from "node:assert/strict";
import {admit,resourceValue,toolValue} from "./contracts.js";
test("Map all seven owner page roots admit required null and reject malformed items before publication",()=>{
 for(const uri of ["feature-layers","publications","compositions","sources","datasets","mobility-profiles","acquisitions"]){
  const good={items:[],limit:100,nextCursor:null};
  assert.deepEqual(resourceValue(`map://${uri}`,good),good);
  for (const obsolete of [{items:[],limit:100,next_cursor:null},{...good,next_cursor:null}]) {
   let retained=good;
   assert.throws(()=>{retained=resourceValue(`map://${uri}`,obsolete)});
   assert.deepEqual(retained,good);
  }

  assert.throws(()=>resourceValue(`map://${uri}`,{...good,items:[{malformed:true}]}));
  assert.throws(()=>resourceValue(`map://${uri}`,{...good,nextCursor:42}));
  const omitted={items:[],limit:100};
  assert.throws(()=>resourceValue(`map://${uri}`,omitted));
  assert.deepEqual(resourceValue(`map://${uri}`,{...good,nextCursor:"owner-cursor"}),{...good,nextCursor:"owner-cursor"});
 }
 assert.deepEqual(resourceValue("map://active-releases",[]),[]);
 const output={layerId:"feature-layer-00000000-0000-7000-8000-000000000001",features:[],projectionSequence:0};
 assert.deepEqual(toolValue("query_features",output,{layerId:"feature-layer-00000000-0000-7000-8000-000000000001"}),output);
 assert.throws(()=>toolValue("query_features",output,{layerId:"feature-layer-00000000-0000-7000-8000-000000000002"}));
});

test("GeoPackage inspection consumes final Task completion through the owner root",()=>{
 const output={sourceArtifactUri:"artifact://0195dabe-7777-7abc-8def-000000000005",manifest:{version:"1",applicationId:1196444487,userVersion:10300,featureTables:[],extensions:[],findings:[]}};
 assert.throws(()=>taskEnvelope(tasks.complete,"wrong-requested-id"));
 const seed=toolEnvelope(tasks.seed);assert.equal(seed.taskId,tasks.detail.taskId);
 const completed={...tasks.complete,result:{resultType:"complete",content:[],structuredContent:output}};
 assert.deepEqual(toolValue("inspect_geopackage",structuredResult(taskEnvelope(completed,tasks.seed.taskId).result)),output);
 let retained=output;
 for(const bad of [{...output,manifest:{...output.manifest,featureTables:[{}]}},{...output,manifest:{...output.manifest,findings:[{level:"invented",code:"bad",message:"bad"}]}},{manifest:output.manifest}]){
  assert.throws(()=>{retained=toolValue("inspect_geopackage",bad)});assert.deepEqual(retained,output);
 }
});


test("standalone asset guard distinguishes CSS loads from schema metadata", async()=>{
 const {assertLocalAssets}=await import("./asset-policy.js");
 assert.doesNotThrow(()=>assertLocalAssets('<style>.x{background:url(data:image/png;base64,AA)}</style><script>new URL("https://github.com/cfworker"); const schema={"$schema":"https://json-schema.org/draft/2020-12/schema"};</script>'));
 for(const html of ['<style>.x{background:URL("https://example.test/image")}</style>', '<div style="background:url(//example.test/image)"></div>', '<style>@import "local.css";</style>', '<script src="https://example.test/app.js"></script>', '<script src=https://example.test/app.js></script>', '<link href="https&#58;//example.test/app.css">', '<script src="hTtPs&#x3a;//example.test/app.js"></script>', '<div style="background:url(&quot;https://example.test/image&quot;)"></div>', '<link href="https://example.test/style.css">', '<iframe src="https://example.test/app"></iframe>']) assert.throws(()=>assertLocalAssets(html));
});
