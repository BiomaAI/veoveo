import {parse} from "parse5";

// Parse actual HTML attributes as the browser does, including unquoted values
// and character references. Script text and JSON Schema URL metadata are inert.
/** @param {string} html */
export function assertLocalAssets(html) {
  const css=[];
  function visit(node) {
    if (node.attrs) {
      for (const {name,value} of node.attrs) {
        const normalized=value.replace(/[\t\n\r]/g, "").trim();
        if ((name==="src"||name==="href") && /^(?:https?:|\/\/)/i.test(normalized)) {
          throw new Error("workspace app contains an external HTML asset reference");
        }
        if(name==="style") css.push(value);
      }
    }
    if(node.tagName==="style") css.push((node.childNodes||[]).map(child=>child.value||"").join(""));
    for(const child of node.childNodes||[]) visit(child);
    if(node.content) visit(node.content);
  }
  visit(parse(html));
  for (const source of css) {
    if (/@import\b|url\(\s*["']?\s*(?:https?:|\/\/)/i.test(source)) {
      throw new Error("workspace app contains an external CSS asset reference");
    }
  }
}
