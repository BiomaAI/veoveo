/** @type {[string,string,string,boolean?,boolean?][]} */
const resources = [
  ["map://feature-layers", "layers", "featureRead", true, true],
  ["map://publications", "publications", "featureRead", true, true],
  ["map://compositions", "compositions", "featureRead", true, true],
  ["map://sources", "sources", "datasetRead", false, true],
  ["map://datasets", "datasets", "datasetRead", true, true],
  ["map://active-releases", "activeReleases", "datasetRead"],
  ["map://mobility-profiles", "profiles", "datasetRead", true, true],
  ["map://acquisitions", "acquisitions", "administration", false, true],
];

export function mapSubscriptionUris(access) {
  return resources.filter(([, , permission, subscribable]) => access[permission] && subscribable !== false).map(([uri]) => uri);
}

// Follow the declared page contract. A failed walk never publishes partial data.
export async function readCollection(uri, read, { maxPages = 100, timeoutMs = 60000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  const seen = new Set();
  const items = [];
  let nextUri = uri;
  for (let count = 0; count < maxPages; ++count) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw new Error("Collection refresh exceeded its time limit");
    let timer;
    let page;
    try {
      page = await Promise.race([
        read(nextUri),
        new Promise((_, reject) => {
          timer = setTimeout(() => reject(new Error("Collection refresh exceeded its time limit")), remaining);
        }),
      ]);
    } finally { clearTimeout(timer); }
    if (!page || !Array.isArray(page.items) || page.limit !== 100 || page.items.length > page.limit
        || !(page.nextCursor === null || (typeof page.nextCursor === "string"
          && /^[0-9a-f]{2,2048}$/.test(page.nextCursor) && page.nextCursor.length % 2 === 0))) {
      throw new Error("Invalid collection page; reload Map Explorer after the server upgrade");
    }
    items.push(...page.items);
    if (page.nextCursor === null) return items;
    if (!page.items.length || seen.has(page.nextCursor)) throw new Error("Collection cursor did not advance");
    seen.add(page.nextCursor);
    const next = new URL(uri);
    next.searchParams.set("cursor", page.nextCursor);
    nextUri = next.toString();
  }
  throw new Error(`Collection refresh exceeded ${maxPages} pages`);
}

// Publish a complete refresh to the view only after all requested reads succeed.
// Keep four host reads in flight and leave the previous view intact on failure.
export async function readMapSnapshot(access, read, changed) {
  // An active pointer may introduce a source and dataset absent from the prior view.
  if (changed?.has("map://active-releases")) {
    changed = new Set([...changed, "map://sources", "map://datasets"]);
  }
  const selected = resources.filter(([uri, , permission]) =>
    access[permission] && (!changed || changed.has(uri)));
  let next = 0;
  const values = {};
  const errors = [];
  await Promise.all(Array.from({ length: Math.min(4, selected.length) }, async () => {
    while (next < selected.length) {
      const [uri, field, , , paged] = selected[next++];
      try { values[field] = paged ? await readCollection(uri, read) : await read(uri); }
      catch (error) {
        errors.push(`${uri}: ${error.message}`);
        // Retire this worker even if the bridge is still expiring a timed-out read.
        return;
      }
    }
  }));
  if (errors.length) throw new Error(errors.join("; "));
  return values;
}
