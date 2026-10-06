// @ts-check
import bundle from "./generated/map.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/map").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/map").AppContracts[K]}
 */
export function admit(root,value){return /** @type {import("./generated/map").AppContracts[K]} */ (validate(root,value));}

export const tools=/** @type {const} */ ({"inspect_geopackage":"inspected","query_features": "features", "query_source_features": "source_features", "create_feature_layer": "layer", "validate_feature_changes": "validated", "commit_feature_changes": "committed", "start_acquisition": "acquisition", "cancel_acquisition": "acquisition", "create_map_composition": "composition", "update_map_composition": "composition", "register_source": "source", "register_mobility_profile": "profile", "publish_feature_layer": "publication", "import_feature_layer": "imported", "activate_release": "release", "rollback_release": "release", "quarantine_release": "release"});
/** @template {keyof typeof tools} N @param {N} name @param {unknown} value @param {Record<string,unknown>} args @returns {import("./generated/map").AppContracts[(typeof tools)[N]]} */
export function toolValue(name,value,args={}) {
  const root=tools[name];
  if (!root) throw new Error("Unknown App tool result");
  const admitted=admit(root,value);
  for(const key of ["layer_id","composition_id","acquisition_id","release_id"]){
    if(args[key] && admitted[key]!==undefined && admitted[key]!==args[key]) throw new Error("Map result belongs to another request");
  }
  if(name==="query_features" && admit("features",value).features.some(feature=>feature.layer_id!==args.layer_id)) throw new Error("Feature belongs to another layer");
  return /** @type {import("./generated/map").AppContracts[(typeof tools)[N]]} */ (admitted);
}

/** @template {keyof import("./generated/map").AppContracts} K @param {string} uri @param {unknown} value @param {K} [expectedRoot] @returns {import("./generated/map").AppContracts[K]} */
export function resourceValue(uri,value,expectedRoot) {
  const address=new URL(uri);
  const parts=address.pathname.split("/").filter(Boolean).map(decodeURIComponent);
  /** @type {keyof import("./generated/map").AppContracts|undefined} */
  let root;
  if(address.protocol!=="map:") throw new Error("Wrong resource owner");
  root=(/** @type {const} */ ({"workspace": "workspace", "feature-layers": "layers", "publications": "publications", "compositions": "compositions", "sources": "sources", "datasets": "datasets", "mobility-profiles": "profiles", "acquisitions": "acquisitions", "active-releases": "active_releases", "feature-style": "style"}))[address.hostname];
  if(!root) throw new Error("Unknown App resource contract");
  if(expectedRoot && root!==expectedRoot) throw new Error("Resource route differs from declared root");
  const admitted=admit(root,value);
  if(root==="style" && admit("style",value).style_revision_id!==parts[0]) throw new Error("Style belongs to another request");
  return /** @type {import("./generated/map").AppContracts[K]} */ (admitted);
}
