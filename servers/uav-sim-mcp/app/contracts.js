// @ts-check
import bundle from "./generated/uav-sim.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/uav-sim").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/uav-sim").AppContracts[K]}
 */
export function admit(root,value){return /** @type {import("./generated/uav-sim").AppContracts[K]} */ (validate(root,value));}

export const tools=/** @type {const} */ ({"list_live_cameras": "cameras", "open_live_view": "connection", "renew_live_view": "connection", "close_live_view": "closed"});
/** @template {keyof typeof tools} N @param {N} name @param {unknown} value @param {Record<string,unknown>} args @returns {import("./generated/uav-sim").AppContracts[(typeof tools)[N]]} */
export function toolValue(name,value,args={}) {
  const root=tools[name];
  if (!root) throw new Error("Unknown App tool result");
  const admitted=admit(root,value);
  if(root==="connection") {
    const stream=admit("connection",value).stream;
    if(stream.sessionId!==args.sessionId || stream.viewerInstanceId!==args.viewerInstanceId || (args.liveViewId && stream.liveViewId!==args.liveViewId) || (args.cameraId && stream.cameraId!==args.cameraId)) throw new Error("Live viewer result belongs to another request");
    const region=stream.sourceRegion;
    if(region.cameraId!==stream.cameraId || region.widthPx!==stream.widthPx || region.heightPx!==stream.heightPx || region.xPx+region.widthPx>stream.codedWidthPx || region.yPx+region.heightPx>stream.codedHeightPx) throw new Error("Live camera region differs from product");
    const address=new URL(stream.resourceUri);
    const parts=address.pathname.split("/").filter(Boolean).map(decodeURIComponent);
    if(address.hostname!=="session" || parts[0]!==stream.sessionId || parts[1]!=="live-view" || parts[2]!==stream.liveViewId || parts.length!==3) throw new Error("Live resource belongs to another viewer");
  }
  return /** @type {import("./generated/uav-sim").AppContracts[(typeof tools)[N]]} */ (admitted);
}

/** @template {keyof import("./generated/uav-sim").AppContracts} K @param {string} uri @param {unknown} value @param {K} [expectedRoot] @returns {import("./generated/uav-sim").AppContracts[K]} */
export function resourceValue(uri,value,expectedRoot) {
  const address=new URL(uri);
  const parts=address.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  /** @type {keyof import("./generated/uav-sim").AppContracts|undefined} */
  let root;
  if(address.protocol!=="uav-sim:") throw new Error("Wrong resource owner");
  root=address.hostname==="sessions"?"sessions":undefined;
  if(!root) throw new Error("Unknown App resource contract");
  if(expectedRoot && root!==expectedRoot) throw new Error("Resource route differs from declared root");
  const admitted=admit(root,value);
  return /** @type {import("./generated/uav-sim").AppContracts[K]} */ (admitted);
}

/** A standard correlated host protocol error, preserving owner data without interpreting it. */
export class AppProtocolError extends Error {
  /** @param {number} code @param {string} message @param {unknown} data */
  constructor(code, message, data) { super(message); this.code=code; this.data=data; }
}

/** @param {unknown} left @param {unknown} right @returns {boolean} */
function sameRequest(left,right) {
  if(left===right)return true;
  if(!left||!right||typeof left!=="object"||typeof right!=="object"||Array.isArray(left)||Array.isArray(right))return false;
  const keys=Object.keys(left), other=Object.keys(right);
  return keys.length===other.length&&keys.every(key=>Object.hasOwn(right,key)&&sameRequest(left[key],right[key]));
}

/** Admit only owner proof for this complete failed operation; never infer from message text.
 * @param {unknown} cause @param {"open_live_view"|"renew_live_view"} name @param {Record<string,unknown>} request
 */
export function provesNonAdmission(cause,name,request) {
  if(!(cause instanceof AppProtocolError)||!Number.isSafeInteger(cause.code)||cause.code < -2147483648||cause.code > 2147483647)return false;
  try {
    const marker=admit("nonAdmission",cause.data);
    return marker.operation.type===(name==="open_live_view"?"open":"renew")&&sameRequest(marker.operation.request,request);
  } catch { return false; }
}
