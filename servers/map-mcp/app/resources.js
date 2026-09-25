const resources = [
  ["map://feature-layers", "layers", "feature_read"],
  ["map://publications", "publications", "feature_read"],
  ["map://compositions", "compositions", "feature_read"],
  ["map://sources", "sources", "dataset_read", false],
  ["map://datasets", "datasets", "dataset_read"],
  ["map://active-releases", "activeReleases", "dataset_read"],
  ["map://mobility-profiles", "profiles", "dataset_read"],
  ["map://acquisitions", "acquisitions", "administration", false],
];

export function mapSubscriptionUris(access) {
  return resources.filter(([, , permission, subscribable]) => access[permission] && subscribable !== false).map(([uri]) => uri);
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
      const [uri, field] = selected[next++];
      try { values[field] = await read(uri); }
      catch (error) { errors.push(`${uri}: ${error.message}`); }
    }
  }));
  if (errors.length) throw new Error(errors.join("; "));
  return values;
}
