// DOM nodes are declared by the owner App template.
/** @param {string} id @param {string} tag */
function node(id,tag){const value=document.getElementById(id);if(!value||value.tagName.toLowerCase()!==tag)throw new Error(`Missing App element ${id}`);return value;}
const nodes={
 "title": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("title","h1"))),
 "subtitle": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("subtitle","div"))),
 "controls": ()=> /** @type {HTMLFormElement} */ (/** @type {unknown} */ (node("controls","form"))),
 "horizon": ()=> /** @type {HTMLInputElement} */ (/** @type {unknown} */ (node("horizon","input"))),
 "rerun": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("rerun","button"))),
 "legend": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("legend","div"))),
 "chartwrap": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("chartwrap","div"))),
 "chart": ()=> /** @type {SVGSVGElement} */ (/** @type {unknown} */ (node("chart","svg"))),
 "tooltip": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("tooltip","div"))),
 "status": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("status","div"))),
 "error": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("error","div"))),
};
/** @template {string} K @param {K} id @returns {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */
export function element(id){const get=Object.hasOwn(nodes,id)?nodes[/** @type {keyof typeof nodes} */ (id)]:undefined;const value=get?get():document.getElementById(id);if(!value)throw new Error(`Missing App element ${id}`);return /** @type {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */ (value);}
/** @param {string} id @returns {HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement} */
export function input(id){const value=element(id);if(!(value instanceof HTMLInputElement||value instanceof HTMLSelectElement||value instanceof HTMLTextAreaElement))throw new Error(`Invalid App input ${id}`);return value;}
