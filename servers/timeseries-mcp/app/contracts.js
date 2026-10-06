// @ts-check
import bundle from "./generated/timeseries.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
import {artifactIdentity,artifactAddress,forecastArtifactIdentity} from "./artifact.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/timeseries").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/timeseries").AppContracts[K]}
 */
export function admit(root,value){
 const admitted=validate(root,value);
 if(root==="forecast") validateForecast(/** @type {import("./generated/timeseries").AppContracts["forecast"]} */ (admitted));
 return /** @type {import("./generated/timeseries").AppContracts[K]} */ (admitted);
}
/** Rust sorts user labels by Unicode scalar value, without locale collation. @param {string} a @param {string} b */
function compareSeries(a,b){const left=Array.from(a),right=Array.from(b);for(let i=0;i<Math.min(left.length,right.length);i++){const delta=/** @type {number} */ (left[i].codePointAt(0))-/** @type {number} */ (right[i].codePointAt(0));if(delta)return delta;}return left.length-right.length;}
/** @param {import("./generated/timeseries").AppContracts["forecast"]} output */
function validateForecast(output){
 if(forecastArtifactIdentity(output.result_uri)!==artifactAddress(output.artifact.artifact_uri)||forecastArtifactIdentity(output.result_uri)!==artifactIdentity(output.artifact.artifact_id))throw new Error("Forecast Artifact identities differ");
 const summary=output.forecast;
 if(!Number.isSafeInteger(summary.source_rows)||summary.series.some(series=>!Number.isSafeInteger(series.observed_rows)))throw new Error("Forecast counts exceed this App’s exact JSON integer range");
 if(summary.source_rows===0||summary.series.length===0||summary.series.some(series=>series.observed_rows===0))throw new Error("Completed forecast must contain usable observations and series");
 if(summary.series.length!==output.preview.length)throw new Error("Forecast preview series differ from summary");
 let total=0n;
 for(let i=0;i<summary.series.length;i++){
  const series=summary.series[i],preview=output.preview[i];
  if(i>0&&compareSeries(summary.series[i-1].series_id,series.series_id)>=0)throw new Error("Forecast series must be unique and sorted");
  if(series.series_id!==preview.series_id||series.forecast_rows!==summary.horizon)throw new Error("Forecast preview identity or count differs");
  total+=BigInt(series.observed_rows);
  if(total>(1n<<64n)-1n)throw new Error("Forecast source count overflow");
  if(BigInt(preview.observed.length)>BigInt(series.observed_rows)||(series.observed_rows===0)!==(preview.observed.length===0))throw new Error("Forecast preview bounds differ from summary");
  for(const point of preview.observed)if(!Number.isFinite(point.value))throw new Error("Observed preview must be finite");
  let prior=0;
  for(const point of preview.forecast){
   if(point.step<=prior||point.step>summary.horizon||![point.mean,point.q10,point.q90].every(Number.isFinite)||point.q10>point.mean||point.mean>point.q90)throw new Error("Forecast preview points are inconsistent");
   prior=point.step;
  }
  if(prior!==summary.horizon)throw new Error("Forecast preview must retain its final step");
 }
 if(total!==BigInt(summary.source_rows))throw new Error("Forecast source count differs from series");
}
