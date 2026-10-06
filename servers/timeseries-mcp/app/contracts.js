// @ts-check
import bundle from "./generated/timeseries.schema.json" with {type:"json"};
import {ownerContracts} from "../../../mcp/apps-extension/browser/admission.js";
const validate=ownerContracts(bundle);
/**
 * @template {keyof import("./generated/timeseries").AppContracts} K
 * @param {K} root
 * @param {unknown} value
 * @returns {import("./generated/timeseries").AppContracts[K]}
 */
export function admit(root,value){return /** @type {import("./generated/timeseries").AppContracts[K]} */ (validate(root,value));}
