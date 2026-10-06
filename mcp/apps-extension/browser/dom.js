// DOM nodes are declared by the owner App template.
/** @param {string} id @param {string} tag */
function node(id,tag){const value=document.getElementById(id);if(!value||value.tagName.toLowerCase()!==tag)throw new Error(`Missing App element ${id}`);return value;}
const nodes={
 "title": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("title","h1"))),
 "subtitle": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("subtitle","p"))),
 "status": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("status","span"))),
 "refresh": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("refresh","button"))),
 "resources": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("resources","div"))),
 "tools": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("tools","div"))),
 "resource-title": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("resource-title","h2"))),
 "pager": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("pager","span"))),
 "page-previous": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("page-previous","button"))),
 "page-number": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("page-number","span"))),
 "page-next": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("page-next","button"))),
 "copy": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("copy","button"))),
 "payload": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("payload","pre"))),
};
/** @template {string} K @param {K} id @returns {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */
export function element(id){const get=Object.hasOwn(nodes,id)?nodes[/** @type {keyof typeof nodes} */ (id)]:undefined;const value=get?get():document.getElementById(id);if(!value)throw new Error(`Missing App element ${id}`);return /** @type {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */ (value);}
/** @param {string} id @returns {HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement} */
export function input(id){const value=element(id);if(!(value instanceof HTMLInputElement||value instanceof HTMLSelectElement||value instanceof HTMLTextAreaElement))throw new Error(`Invalid App input ${id}`);return value;}
