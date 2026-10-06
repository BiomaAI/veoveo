// @ts-check
import bundle from "./generated/stream.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/stream").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/stream").AppContracts[K]}
 */
export function admit(root,value){return /** @type {import("./generated/stream").AppContracts[K]} */ (validate(root,value));}

export const tools=/** @type {const} */ ({"start_live_session": "started", "stop_live_session": "stopped"});
/** @template {keyof typeof tools} N @param {N} name @param {unknown} value @param {Record<string,unknown>} args @returns {import("./generated/stream").AppContracts[(typeof tools)[N]]} */
export function toolValue(name,value,args={}) {
  const root=tools[name];
  if (!root) throw new Error("Unknown App tool result");
  const admitted=admit(root,value);
  if(root==="started") {
    const started=admit("started",value);
    if(started.result_uri!==new URL(encodeURIComponent(started.session_id),"stream://session/").href) throw new Error("Session address disagrees with identity");
    if(args.pipeline_id && started.pipeline_uri!==new URL(encodeURIComponent(String(args.pipeline_id)),"stream://pipeline/").href) throw new Error("Stream pipeline differs from request");
  } else {
    const stopped=admit("stopped",value);
    if(stopped.result_uri!==new URL(encodeURIComponent(String(args.session_id)),"stream://session/").href) throw new Error("Stopped result belongs to another session");
  }
  return /** @type {import("./generated/stream").AppContracts[(typeof tools)[N]]} */ (admitted);
}

/** @template {keyof import("./generated/stream").AppContracts} K @param {string} uri @param {unknown} value @param {K} [expectedRoot] @returns {import("./generated/stream").AppContracts[K]} */
export function resourceValue(uri,value,expectedRoot) {
  const address=new URL(uri);
  const parts=address.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  /** @type {keyof import("./generated/stream").AppContracts|undefined} */
  let root;
  if(address.protocol!=="stream:") throw new Error("Wrong Stream resource owner");
  root=(/** @type {const} */ ({pipelines:"pipelines",sessions:"sessions"}))[address.hostname];
  if(address.hostname==="session") root=parts.length===1?"session":(/** @type {const} */ ({results:"results",preview:"preview"}))[parts[1]];
  if(!root) throw new Error("Unknown App resource contract");
  if(expectedRoot && root!==expectedRoot) throw new Error("Resource route differs from declared root");
  const admitted=admit(root,value);
  if(address.hostname==="session" && (root==="session"?admit("session",value).session_id:root==="results"?admit("results",value).session_id:admit("preview",value).session_id)!==parts[0]) throw new Error("Resource belongs to another session");
  if(root==="session"){
    const session=admit("session",value);
    for(const [field,tail] of [["session_uri",""],["results_uri","/results"],["preview_uri","/preview"]]){
      if(session[field]!==new URL(encodeURIComponent(session.session_id)+tail,"stream://session/").href) throw new Error("Session addresses disagree");
    }
  }
  return /** @type {import("./generated/stream").AppContracts[K]} */ (admitted);
}
