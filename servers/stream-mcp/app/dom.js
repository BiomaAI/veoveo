// DOM nodes are declared by the owner App template.
/** @param {string} id @param {string} tag */
function node(id,tag){const value=document.getElementById(id);if(!value||value.tagName.toLowerCase()!==tag)throw new Error(`Missing App element ${id}`);return value;}
const nodes={
 "app": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("app","div"))),
 "status": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("status","span"))),
 "decode": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("decode","span"))),
 "freshness": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("freshness","span"))),
 "controls": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("controls","section"))),
 "pipeline": ()=> /** @type {HTMLSelectElement} */ (/** @type {unknown} */ (node("pipeline","select"))),
 "sessions": ()=> /** @type {HTMLSelectElement} */ (/** @type {unknown} */ (node("sessions","select"))),
 "start": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("start","button"))),
 "stop": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("stop","button"))),
 "page-newer": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("page-newer","button"))),
 "page-status": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("page-status","span"))),
 "page-older": ()=> /** @type {HTMLButtonElement} */ (/** @type {unknown} */ (node("page-older","button"))),
 "stage": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("stage","main"))),
 "video": ()=> /** @type {HTMLCanvasElement} */ (/** @type {unknown} */ (node("video","canvas"))),
 "overlay": ()=> /** @type {HTMLCanvasElement} */ (/** @type {unknown} */ (node("overlay","canvas"))),
 "empty": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("empty","div"))),
 "session": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("session","span"))),
 "ingress": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("ingress","span"))),
 "recording": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("recording","span"))),
 "detections": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("detections","section"))),
 "frames": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("frames","b"))),
 "objects": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("objects","b"))),
 "dropped": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("dropped","b"))),
 "error": ()=> /** @type {HTMLElement} */ (/** @type {unknown} */ (node("error","div"))),
};
/** @template {string} K @param {K} id @returns {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */
export function element(id){const get=Object.hasOwn(nodes,id)?nodes[/** @type {keyof typeof nodes} */ (id)]:undefined;const value=get?get():document.getElementById(id);if(!value)throw new Error(`Missing App element ${id}`);return /** @type {K extends keyof typeof nodes ? ReturnType<(typeof nodes)[K]> : HTMLElement} */ (value);}
/** @param {string} id @returns {HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement} */
export function input(id){const value=element(id);if(!(value instanceof HTMLInputElement||value instanceof HTMLSelectElement||value instanceof HTMLTextAreaElement))throw new Error(`Invalid App input ${id}`);return value;}
