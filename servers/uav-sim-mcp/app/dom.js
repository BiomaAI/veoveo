// DOM nodes are declared by the owner App template.
/** @param {string} id @param {string} tag */
function node(id,tag){const value=document.getElementById(id);if(!value||value.tagName.toLowerCase()!==tag)throw new Error(`Missing App element ${id}`);return value;}
const nodes={
 "app": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("app","div"))),
 "decode": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("decode","span"))),
 "status": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("status","span"))),
 "count": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("count","span"))),
 "choices": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("choices","section"))),
 "commands": ()=> /** @type {HTMLFormElement} */ (/** @type {unknown} */ (node("commands","form"))),
 "agent": ()=> /** @type {HTMLSelectElement} */ (/** @type {unknown} */ (node("agent","select"))),
 "command": ()=> /** @type {HTMLInputElement} */ (/** @type {unknown} */ (node("command","input"))),
 "command-result": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("command-result","output"))),
 "grid": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("grid","main"))),
 "error": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("error","div"))),
 "session": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("session","span"))),
};
/** @template {string} K @param {K} id @returns {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */
export function element(id){const get=Object.hasOwn(nodes,id)?nodes[/** @type {keyof typeof nodes} */ (id)]:undefined;const value=get?get():document.getElementById(id);if(!value)throw new Error(`Missing App element ${id}`);return /** @type {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */ (value);}
/** @param {string} id @returns {HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement} */
export function input(id){const value=element(id);if(!(value instanceof HTMLInputElement||value instanceof HTMLSelectElement||value instanceof HTMLTextAreaElement))throw new Error(`Invalid App input ${id}`);return value;}
