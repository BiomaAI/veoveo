import { CfWorkerJsonSchemaValidator } from "@modelcontextprotocol/client/validators/cf-worker";
import {
  CallToolResultSchema,
  ReadResourceResultSchema,
} from "@modelcontextprotocol/core";

import taskSchemas from "./generated/tasks.schema.json" with {type:"json"};
const provider = new CfWorkerJsonSchemaValidator();

/** Owner-selected schema properties name DTO roots, never transport wrappers. */
export function ownerContracts(bundle) {
  const validators = new Map();
  return (root, value) => {
    if (!Object.hasOwn(bundle.properties, root)) throw new Error("Unknown owner contract root");
    let validate = validators.get(root);
    if (!validate) {
      validate = provider.getValidator({
        $schema: bundle.$schema, $defs: bundle.$defs, ...bundle.properties[root],
      });
      validators.set(root, validate);
    }
    const result = validate(value);
    if (!result.valid) throw new Error(`Invalid ${root} response`);
    return result.data;
  };
}

const taskAdmit = ownerContracts(taskSchemas);
export function toolEnvelope(value) {
  if(value?.task)throw new Error("Unsupported legacy Task envelope");
  // The maintained protocol schemas preserve extension metadata and content.
  return (value?.resultType === "task" || Object.hasOwn(value ?? {}, "taskId")) ? taskAdmit("seed",value) : CallToolResultSchema.parse(value);
}
/** @param {unknown} value @param {string} requestedId @returns {import("./generated/tasks.js").TaskContracts["detail"]} */
export function taskEnvelope(value,requestedId) {
 const detail = /** @type {import("./generated/tasks.js").TaskContracts["detail"]} */ (taskAdmit("detail",value));
 if(detail.taskId!==requestedId) throw new Error("Task response differs from requested identity");
 if(detail.status === "completed") toolEnvelope(detail.result);
 return detail;
}
export function resourceEnvelope(value) { return ReadResourceResultSchema.parse(value); }
export function resourceJson(value, uri) {
  const contents = resourceEnvelope(value).contents;
  const item = contents.find(item => item.uri === uri && "text" in item && typeof item.text === "string");
  if (!item || !("text" in item)) throw new Error("Resource response does not match its requested address");
  return JSON.parse(item.text);
}
export function structuredResult(value) {
  const result = toolEnvelope(value);
  if (result.isError) {
    const text = result.content.find(item => item.type === "text");
    throw new Error(text?.text || "Tool failed");
  }
  if (!Object.hasOwn(result, "structuredContent")) throw new Error("Tool returned no structured result");
  return result.structuredContent;
}

export function successfulToolEnvelope(value){
 const result=toolEnvelope(value);
 if(result.isError)throw new Error(result.content?.find(item=>item.type==="text")?.text||"Tool failed");
 return result;
}
