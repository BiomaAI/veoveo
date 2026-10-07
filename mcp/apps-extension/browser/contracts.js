// @ts-check
import {ownerContracts} from "./admission.js";
import schemas from "./generated/workbench.schema.json" with {type:"json"};
/** @typedef {import("./generated/workbench.js").WorkbenchContracts["config"]} Config */
const admit=ownerContracts(schemas);
/** @param {unknown} value @returns {Config} */
export function configValue(value){return /** @type {Config} */ (admit("config",value));}
/** @param {Config} config @param {string} toolName */
export function streamsProjection(config,toolName){return config.streamResult?.kind==="recordingProjection"&&config.streamResult.tool_name===toolName;}

/** @param {unknown} value @param {unknown} args */
export function projectionValue(value,args){
 const handle=/** @type {import("./generated/workbench.js").WorkbenchContracts["projection"]} */ (admit("projection",value));
 const request=/** @type {import("./generated/workbench.js").WorkbenchContracts["projection_request"]} */ (admit("projection_request",args));
 const samples=request.sampling.kind==="sample_grid"?request.sampling.values:request.sampling.kind==="latest_at"?[request.sampling.at]:[];
 const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
 const ordered=value=>Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b)));
 if(handle.datasetId!==request.datasetId||handle.recordingId!==request.recordingId||handle.result.timeline!==request.timeline||!equal(handle.result.sampleGrid,samples)||!equal(ordered(handle.result.units),ordered(request.units))||!equal(handle.result.coordinateFrameRefs,request.coordinateFrameRefs)||handle.result.rowCount>request.maximumRows||handle.result.rowCount>request.maximumSamples||handle.result.byteLen>request.maximumBytes)throw new Error("Projection result differs from its request");
 return handle;
}
/** @param {import("./generated/workbench.js").WorkbenchContracts["projection"]} handle @param {Uint8Array[]} chunks */
export async function verifyProjectionBytes(handle,chunks){
 const length=chunks.reduce((total,chunk)=>total+chunk.byteLength,0);
 if(length!==handle.result.byteLen)throw new Error("Projection stream length mismatch");
 const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
 const digest=new Uint8Array(await crypto.subtle.digest("SHA-256",bytes));
 const hex=Array.from(digest,byte=>byte.toString(16).padStart(2,"0")).join("");
 if(hex!==handle.result.payloadSha256)throw new Error("Projection stream digest mismatch");
}
