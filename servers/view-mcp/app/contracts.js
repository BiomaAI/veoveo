// @ts-check
import bundle from "./generated/view.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/view").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/view").AppContracts[K]}
 */
export function admit(root,value){return /** @type {import("./generated/view").AppContracts[K]} */ (validate(root,value));}

export const tools=/** @type {const} */ ({"create_scene_composition": "composition", "create_view": "view", "set_camera": "view", "capture_frame": "frame", "close_view": "closed"});
/** @template {keyof typeof tools} N @param {N} name @param {unknown} value @param {Record<string,unknown>} args @returns {import("./generated/view").AppContracts[(typeof tools)[N]]} */
export function toolValue(name,value,args={}) {
  const root=tools[name];
  if (!root) throw new Error("Unknown App tool result");
  const admitted=admit(root,value);
  if(root==="view") {
    const view=admit("view",value);
    if(args.viewId && view.viewId!==args.viewId) throw new Error("View result belongs to another view");
    if(args.compositionId && view.compositionId!==args.compositionId) throw new Error("View result belongs to another composition");
    if(view.viewUri!==new URL(encodeURIComponent(view.viewId),"view://view/").href) throw new Error("View address disagrees with identity");
  }
  if(root==="frame") {
    const frame=admit("frame",value);
    if(args.viewId && frame.viewId!==args.viewId) throw new Error("Frame belongs to another view");
    if(args.expectedRevision!==undefined && frame.viewRevision!==args.expectedRevision) throw new Error("Frame revision differs from requested view");
    if(args.compositionId && frame.compositionId!==args.compositionId) throw new Error("Frame belongs to another composition");
  }
  return /** @type {import("./generated/view").AppContracts[(typeof tools)[N]]} */ (admitted);
}

/** @template {keyof import("./generated/view").AppContracts} K @param {string} uri @param {unknown} value @param {K} [expectedRoot] @returns {import("./generated/view").AppContracts[K]} */
export function resourceValue(uri,value,expectedRoot) {
  const address=new URL(uri);
  const parts=address.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  /** @type {keyof import("./generated/view").AppContracts|undefined} */
  let root;
  if(address.protocol!=="view:") throw new Error("Wrong View resource owner");
  if(address.hostname==="layers") root="layers";
  else if(address.hostname==="view") root=parts.length===2&&parts[1]==="scene"?"scene":"view";
  if(!root) throw new Error("Unknown App resource contract");
  if(expectedRoot && root!==expectedRoot) throw new Error("Resource route differs from declared root");
  const admitted=admit(root,value);
  if(parts[0] && (root==="view"?admit("view",value).viewId:admit("scene",value).viewId)!==parts[0]) throw new Error("Resource belongs to another view");
  return /** @type {import("./generated/view").AppContracts[K]} */ (admitted);
}
