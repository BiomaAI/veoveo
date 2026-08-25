import * as maplibregl from "maplibre-gl";
import workerSource from "embedded:maplibre-worker";

// The opaque-origin App sandbox cannot directly fetch its own blob URL.
// MapLibre recognizes the `.cjs` suffix, fetches this CSP-admitted data URL,
// and starts the resulting classic worker under `worker-src blob:`.
const workerUrl = `data:text/javascript;charset=utf-8,${encodeURIComponent(workerSource)}#maplibre-worker.cjs`;
maplibregl.setWorkerUrl(workerUrl);

const bridge = (() => {
  let nextId = 1;
  const pending = new Map();
  const handlers = new Map();
  const post = (message) => parent.postMessage(message, "*");
  addEventListener("message", (event) => {
    const message = event.data;
    if (!message || message.jsonrpc !== "2.0") return;
    if (message.id !== undefined && (message.result !== undefined || message.error !== undefined)) {
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(message.error.message || "host error"));
      else waiter.resolve(message.result);
      return;
    }
    const handler = handlers.get(message.method);
    if (handler) handler(message.params, message.id);
  });
  return {
    request(method, params) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        post({ jsonrpc: "2.0", id, method, params });
      });
    },
    notify: (method, params) => post({ jsonrpc: "2.0", method, params }),
    on: (method, handler) => handlers.set(method, handler),
    post,
  };
})();

const state = {
  access: {
    administration: false,
    dataset_read: false,
    feature_read: false,
    feature_write: false,
    feature_publish: false,
    basemap: null,
  },
  layers: [],
  publications: [],
  compositions: [],
  styles: new Map(),
  sources: [],
  datasets: {},
  activeReleases: [],
  acquisitions: [],
  profiles: [],
  entries: new Map(),
  selectedKey: null,
  selectedFeatureId: null,
  selectedCompositionId: "live",
  compositionChoiceInitialized: false,
  previewPage: 0,
  map: null,
  mapReady: false,
  basemapTheme: null,
  basemapStyleUrl: null,
  desiredBasemapTheme: "light",
  basemapSwitchPromise: null,
  renderedLayerIds: [],
  queryGeneration: 0,
  refreshTimer: null,
  refreshPromise: null,
  refreshQueued: false,
  closing: false,
  action: null,
  rawAdminKind: null,
  featureDraft: { geometry: null, properties: {} },
  drawing: null,
  geopackageManifest: null,
};

const MAX_AUTHORED_VIEW_FEATURES = 5000;
const MAX_SOURCE_VIEW_FEATURES = 5000;
const PREVIEW_PAGE_SIZE = 50;
const QUERY_CONCURRENCY = 4;
const TERMINAL_TASK_STATUSES = new Set(["completed", "failed", "cancelled"]);
const ACTIVE_ACQUISITION_STATUSES = new Set(["queued", "running", "cancel_requested"]);
const PALETTE = ["#287e8e", "#b8683b", "#5a7d3c", "#725e9c", "#b34f68", "#3f70a5", "#99712d", "#477369"];
const el = (id) => document.getElementById(id);

function node(tag, text, className) {
  const value = document.createElement(tag);
  if (text !== undefined) value.textContent = String(text);
  if (className) value.className = className;
  return value;
}

function setStatus(message, kind = "") {
  el("status-message").textContent = message;
  el("status").className = `statusbar ${kind}`;
}

function uuid() {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  return Array.from(crypto.getRandomValues(new Uint8Array(16)),
    (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function read(uri) {
  const result = await bridge.request("resources/read", { uri });
  const content = (result && result.contents || [])[0];
  if (!content || typeof content.text !== "string") throw new Error(`No JSON returned for ${uri}`);
  return JSON.parse(content.text);
}

function toolFailureText(result, name) {
  const content = Array.isArray(result && result.content) ? result.content : [];
  const text = content.find((entry) => entry && entry.type === "text" && typeof entry.text === "string");
  return text ? text.text : `${name} failed`;
}

async function toolRaw(name, args) {
  const result = await bridge.request("tools/call", { name, arguments: args });
  if (result && (result.isError || result.is_error)) throw new Error(toolFailureText(result, name));
  return result;
}

function structured(result) {
  return result && (result.structuredContent || result.structured_content) || result;
}

async function tool(name, args) {
  return structured(await toolRaw(name, args));
}

async function taskTool(name, args, onProgress = () => {}) {
  const created = await toolRaw(name, args);
  const seed = created && (created.task || created);
  const taskId = seed && (seed.taskId || seed.task_id);
  if (!taskId) return structured(created);
  const pollInterval = Math.min(Math.max(seed.pollIntervalMs || seed.poll_interval_ms || 500, 250), 5000);
  let snapshot = seed;
  let status = seed.status || "working";
  while (!TERMINAL_TASK_STATUSES.has(status)) {
    onProgress(status, seed.statusMessage || seed.status_message || "");
    await new Promise((resolve) => setTimeout(resolve, pollInterval));
    snapshot = await bridge.request("tasks/get", { taskId });
    status = snapshot.status || status;
    if (status === "input_required") throw new Error(`${name} requires additional input outside this workflow`);
  }
  const statusMessage = snapshot.statusMessage || snapshot.status_message || "";
  onProgress(status, statusMessage);
  if (status === "failed") throw new Error(statusMessage || `${name} task failed`);
  if (status === "cancelled") throw new Error(`${name} task was cancelled`);
  const result = snapshot.result || {};
  if (result.isError || result.is_error) throw new Error(toolFailureText(result, name));
  return structured(result);
}

function applyAccessVisibility(root = document) {
  root.querySelectorAll("[data-feature-write]").forEach((item) => { item.hidden = !state.access.feature_write; });
  root.querySelectorAll("[data-admin]").forEach((item) => { item.hidden = !state.access.administration; });
}

function renderAccess() {
  const permissions = [
    ["features", state.access.feature_read],
    ["datasets", state.access.dataset_read],
    ["write", state.access.feature_write],
    ["publish", state.access.feature_publish],
    ["admin", state.access.administration],
  ];
  el("access").replaceChildren(...permissions.map(([label, enabled]) =>
    node("span", label, `chip ${enabled ? "on" : ""}`)));
  const admitted = state.access.dataset_read || state.access.feature_read || state.access.administration;
  el("workspace").hidden = !admitted;
  el("permission-note").hidden = admitted;
  el("add-data").hidden = !state.access.feature_write && !state.access.administration;
  applyAccessVisibility();
}

function latestPublication(layerId) {
  return state.publications
    .filter((publication) => publication.layer_id === layerId)
    .sort((left, right) => right.layer_revision - left.layer_revision ||
      String(right.published_at || "").localeCompare(String(left.published_at || "")))[0];
}

function allDatasetReleases() {
  return Object.values(state.datasets || {}).flat().filter(Boolean);
}

function activeReleaseEntries() {
  if (!state.access.dataset_read) return [];
  const releases = allDatasetReleases();
  return state.activeReleases.flatMap((pointer) => {
    const release = releases.find((candidate) => candidate.release_id === pointer.release_id);
    if (!release) return [];
    const source = state.sources.find((candidate) => candidate.source_id === release.source_id);
    return [{ pointer, release, source }];
  });
}

function composition() {
  return state.compositions.find((candidate) => candidate.composition_id === state.selectedCompositionId);
}

async function rebuildEntries() {
  const previous = state.entries;
  const next = new Map();
  if (!state.compositionChoiceInitialized) {
    state.selectedCompositionId = state.compositions[0]?.composition_id || "live";
    state.compositionChoiceInitialized = true;
  }
  const selectedComposition = composition();
  const compositionLayers = new Map((selectedComposition?.current?.layers || [])
    .map((entry) => [entry.layer_id, entry]));

  for (const [index, layer] of state.layers.entries()) {
    const key = `authored:${layer.layer_id}`;
    const old = previous.get(key);
    const pinned = compositionLayers.get(layer.layer_id);
    const publication = pinned
      ? state.publications.find((candidate) => candidate.publication_id === pinned.publication_id)
      : latestPublication(layer.layer_id);
    let style = layer.style || null;
    const styleId = pinned?.style_revision_id || publication?.style_revision_id;
    if (styleId && (!style || style.style_revision_id !== styleId)) {
      if (!state.styles.has(styleId)) {
        try { state.styles.set(styleId, await read(`map://feature-style/${styleId}`)); } catch { state.styles.set(styleId, null); }
      }
      style = state.styles.get(styleId);
    }
    next.set(key, {
      key,
      kind: "authored",
      title: layer.title,
      subtitle: pinned ? `saved publication · r${publication?.layer_revision || "?"}`
        : publication?.layer_revision === layer.revision ? `published · r${layer.revision}` : `current head · r${layer.revision}`,
      layer,
      publicationId: pinned?.publication_id || null,
      publication,
      style,
      opacity: pinned?.opacity ?? old?.opacity ?? 1,
      enabled: selectedComposition ? Boolean(pinned?.visible) : old?.enabled ?? true,
      color: old?.color || PALETTE[index % PALETTE.length],
      features: old?.features || [],
      count: old?.count || 0,
      truncated: false,
      sourceId: null,
      renderLayerIds: [],
    });
  }

  for (const [index, item] of activeReleaseEntries().entries()) {
    const key = `source:${item.release.release_id}`;
    const old = previous.get(key);
    next.set(key, {
      key,
      kind: "source",
      title: item.source?.name || item.release.version_label || item.release.release_id,
      subtitle: `active source · ${item.release.version_label}`,
      source: item.source,
      release: item.release,
      pointer: item.pointer,
      enabled: old?.enabled ?? true,
      opacity: old?.opacity ?? 0.86,
      color: old?.color || PALETTE[(state.layers.length + index) % PALETTE.length],
      features: old?.features || [],
      count: old?.count || 0,
      truncated: false,
      sourceId: null,
      renderLayerIds: [],
    });
  }
  if (state.mapReady) clearInstalledEntries();
  state.entries = next;
  if (!state.selectedKey || !next.has(state.selectedKey)) {
    state.selectedKey = [...next.values()].find((entry) => entry.enabled)?.key || next.keys().next().value || null;
    state.selectedFeatureId = null;
  }
  renderCatalog();
  if (state.mapReady) {
    installEntries();
    await refreshViewport();
  }
  renderInspector();
  renderPreview();
}

function filteredEntries(kind) {
  const query = el("layer-search").value.trim().toLowerCase();
  return [...state.entries.values()].filter((entry) => entry.kind === kind &&
    (!query || `${entry.title} ${entry.subtitle}`.toLowerCase().includes(query)));
}

function renderCatalogList(hostId, entries, emptyText) {
  const host = el(hostId);
  host.replaceChildren();
  if (!entries.length) {
    host.append(node("div", emptyText, "empty"));
    return;
  }
  for (const entry of entries) {
    const row = node("button", undefined, `layer-row ${state.selectedKey === entry.key ? "selected" : ""}`);
    row.type = "button";
    row.dataset.catalogKey = entry.key;
    const visibility = document.createElement("input");
    visibility.type = "checkbox";
    visibility.checked = entry.enabled;
    visibility.setAttribute("aria-label", `Show ${entry.title}`);
    visibility.addEventListener("click", (event) => event.stopPropagation());
    visibility.addEventListener("change", () => setEntryVisibility(entry.key, visibility.checked));
    const swatch = node("span", undefined, "swatch");
    swatch.style.background = entry.color;
    const copy = node("span", undefined, "layer-copy");
    copy.append(node("span", entry.title, "layer-title"), node("span", entry.error ? "preview unavailable" : entry.subtitle, "layer-meta"));
    const count = node("span", `${entry.count}${entry.truncated ? "+" : ""}`, "layer-count");
    row.append(visibility, swatch, copy, count);
    row.addEventListener("click", () => selectEntry(entry.key));
    host.append(row);
  }
}

function renderCatalog() {
  const authored = filteredEntries("authored");
  const sources = filteredEntries("source");
  el("authored-count").textContent = String(authored.length);
  el("source-layer-count").textContent = String(sources.length);
  el("source-section").hidden = !state.access.dataset_read;
  renderCatalogList("authored-list", authored, "No authored feature layers.");
  renderCatalogList("source-layer-list", sources, "No active source releases.");
  const visible = [...state.entries.values()].filter((entry) => entry.enabled).length;
  el("visible-total").textContent = `${visible} visible`;
  renderCompositionOptions();
}

function renderCompositionOptions() {
  const select = el("composition-select");
  const prior = state.selectedCompositionId;
  const live = node("option", "Live layer heads");
  live.value = "live";
  select.replaceChildren(live, ...state.compositions.map((item) => {
    const option = node("option", `${item.title} · r${item.current.revision}`);
    option.value = item.composition_id;
    return option;
  }));
  state.selectedCompositionId = prior === "live" || state.compositions.some((item) => item.composition_id === prior)
    ? prior : (state.compositions[0]?.composition_id || "live");
  select.value = state.selectedCompositionId;
  el("composition-count").textContent = String(state.compositions.length);
  const selected = composition();
  el("map-context-label").textContent = selected ? selected.title : "Live workspace";
}

function setEntryVisibility(key, enabled) {
  const entry = state.entries.get(key);
  if (!entry) return;
  entry.enabled = enabled;
  for (const layerId of entry.renderLayerIds) {
    if (state.map.getLayer(layerId)) state.map.setLayoutProperty(layerId, "visibility", enabled ? "visible" : "none");
  }
  renderCatalog();
  scheduleViewportRefresh();
}

function selectEntry(key) {
  if (!state.entries.has(key)) return;
  state.selectedKey = key;
  state.selectedFeatureId = null;
  state.previewPage = 0;
  state.action = null;
  renderCatalog();
  renderInspector();
  renderPreview();
}

function selectedEntry() {
  return state.entries.get(state.selectedKey);
}

function selectFeature(key, featureId, zoom = false) {
  const entry = state.entries.get(key);
  if (!entry) return;
  state.selectedKey = key;
  state.selectedFeatureId = String(featureId);
  state.action = null;
  renderSelection();
  renderCatalog();
  renderInspector();
  renderPreview();
  if (zoom) {
    const feature = entry.features.find((candidate) => String(candidate.id) === state.selectedFeatureId);
    if (feature) fitGeometry(feature.geometry, 15);
  }
}

function rendererFingerprint() {
  const canvas = document.createElement("canvas");
  const gl = canvas.getContext("webgl2", { antialias: true, failIfMajorPerformanceCaveat: true });
  if (!gl) throw new Error("A hardware-backed WebGL2 context is required; WebGL2 initialization failed.");
  const debug = gl.getExtension("WEBGL_debug_renderer_info");
  if (!debug) throw new Error("Hardware WebGL2 cannot be proven because renderer diagnostics are unavailable.");
  const vendor = String(gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) || "");
  const renderer = String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) || "");
  const fingerprint = `${vendor} ${renderer}`.toLowerCase();
  if (!renderer || /swiftshader|llvmpipe|lavapipe|softpipe|software rasterizer|microsoft basic render|mesa offscreen/.test(fingerprint)) {
    throw new Error(`Software graphics are not accepted: ${renderer || "unknown renderer"}.`);
  }
  gl.getExtension("WEBGL_lose_context")?.loseContext();
  return { vendor, renderer };
}

function css(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function fallbackStyle(theme) {
  return {
    version: 8,
    sources: {},
    layers: [{
      id: "workspace-background",
      type: "background",
      paint: { "background-color": theme === "dark" ? "#111820" : css("--map-water") },
    }],
  };
}

function basemapStyleUrl(theme) {
  const basemap = state.access?.basemap;
  return theme === "dark" ? basemap?.dark_style_url : basemap?.light_style_url;
}

function waitForMapEvent(eventName, timeoutMs = 10000) {
  return new Promise((resolve, reject) => {
    const onEvent = () => {
      clearTimeout(timeout);
      state.map.off(eventName, onEvent);
      resolve();
    };
    const timeout = setTimeout(() => {
      state.map.off(eventName, onEvent);
      reject(new Error(`MapLibre ${eventName} did not complete within ${timeoutMs / 1000} seconds.`));
    }, timeoutMs);
    state.map.on(eventName, onEvent);
  });
}

function cameraState() {
  const center = state.map.getCenter();
  return {
    center: [center.lng, center.lat],
    zoom: state.map.getZoom(),
    bearing: state.map.getBearing(),
    pitch: state.map.getPitch(),
  };
}

async function loadReplacementStyle(style, theme) {
  let ready = waitForMapEvent("style.load");
  state.map.setStyle(style);
  try {
    await ready;
    return true;
  } catch (error) {
    setStatus(`The ${theme} basemap is unavailable; governed layers remain interactive.`, "warn");
    ready = waitForMapEvent("style.load");
    state.map.setStyle(fallbackStyle(theme));
    await ready;
    return false;
  }
}

async function applyBasemapTheme(theme) {
  state.desiredBasemapTheme = theme;
  if (!state.map || !state.mapReady) return;
  const requestedUrl = basemapStyleUrl(theme) || null;
  if (state.basemapTheme === theme && state.basemapStyleUrl === requestedUrl) return;
  if (state.basemapSwitchPromise) return state.basemapSwitchPromise;
  state.basemapSwitchPromise = (async () => {
    while (!state.closing) {
      const nextTheme = state.desiredBasemapTheme;
      const nextUrl = basemapStyleUrl(nextTheme) || null;
      if (state.basemapTheme === nextTheme && state.basemapStyleUrl === nextUrl) break;
      const camera = cameraState();
      clearTimeout(state.refreshTimer);
      state.mapReady = false;
      state.queryGeneration += 1;
      clearInstalledEntries();
      el("map").dataset.basemapTheme = `loading-${nextTheme}`;
      const loaded = await loadReplacementStyle(nextUrl || fallbackStyle(nextTheme), nextTheme);
      if (state.closing) break;
      state.map.jumpTo(camera);
      state.basemapTheme = nextTheme;
      state.basemapStyleUrl = nextUrl;
      el("map").dataset.basemapTheme = nextTheme;
      el("map").dataset.basemapAvailable = String(loaded);
      state.mapReady = true;
      installUtilitySources();
      installEntries();
      renderSelection();
      updateViewportLabel();
      await refreshViewport();
    }
  })().catch((error) => {
    setStatus(`Basemap theme switch failed: ${error.message}`, "bad");
  }).finally(() => {
    state.basemapSwitchPromise = null;
    const requestedUrl = basemapStyleUrl(state.desiredBasemapTheme) || null;
    if (!state.closing && state.mapReady
      && (state.basemapTheme !== state.desiredBasemapTheme || state.basemapStyleUrl !== requestedUrl)) {
      void applyBasemapTheme(state.desiredBasemapTheme);
    }
  });
  return state.basemapSwitchPromise;
}

async function initializeMap() {
  if (state.map) return;
  let gpu;
  try {
    gpu = rendererFingerprint();
  } catch (error) {
    el("gpu-badge").hidden = true;
    el("map-failure").hidden = false;
    el("map-failure").textContent = error.message;
    throw error;
  }
  el("gpu-badge").textContent = `Hardware WebGL2 · ${gpu.renderer}`;
  const basemap = state.access.basemap;
  const theme = state.desiredBasemapTheme;
  const styleUrl = basemapStyleUrl(theme) || null;
  state.basemapTheme = theme;
  state.basemapStyleUrl = styleUrl;
  el("map").dataset.basemapTheme = theme;
  state.map = new maplibregl.Map({
    container: "map",
    center: [-89.2, 13.7],
    zoom: 6,
    bearing: 0,
    pitch: 0,
    attributionControl: false,
    style: styleUrl || fallbackStyle(theme),
  });
  state.map.addControl(new maplibregl.NavigationControl({ visualizePitch: true }), "top-right");
  state.map.addControl(new maplibregl.ScaleControl({ unit: "metric" }), "bottom-left");
  if (basemap) state.map.addControl(new maplibregl.AttributionControl({ compact: true }), "bottom-right");
  let basemapWarningShown = false;
  state.map.on("error", (event) => {
    const message = String(event?.error?.message || "");
    if (!basemapWarningShown && /style|sprite|glyph|tile|network|fetch|image/i.test(message)) {
      basemapWarningShown = true;
      setStatus("Basemap geography is unavailable; governed layers remain interactive.", "warn");
    }
  });
  try {
    await waitForMapEvent("load");
    el("map").dataset.basemapAvailable = String(Boolean(styleUrl));
  } catch (_error) {
    setStatus(`The ${theme} basemap is unavailable; governed layers remain interactive.`, "warn");
    const ready = waitForMapEvent("style.load");
    state.map.setStyle(fallbackStyle(theme));
    await ready;
    el("map").dataset.basemapAvailable = "false";
  }
  state.mapReady = true;
  installUtilitySources();
  installEntries();
  state.map.on("moveend", scheduleViewportRefresh);
  state.map.on("click", handleMapClick);
  updateViewportLabel();
  applyInitialView();
  await refreshViewport();
}

function installUtilitySources() {
  state.map.addSource("workspace-selection", { type: "geojson", data: emptyCollection() });
  state.map.addLayer({ id: "workspace-selection-fill", type: "fill", source: "workspace-selection", paint: { "fill-color": "#f2c14e", "fill-opacity": 0.2 } });
  state.map.addLayer({ id: "workspace-selection-line", type: "line", source: "workspace-selection", paint: { "line-color": "#f2c14e", "line-width": 5 } });
  state.map.addLayer({ id: "workspace-selection-circle", type: "circle", source: "workspace-selection", paint: { "circle-color": "#f2c14e", "circle-radius": 9, "circle-stroke-color": "#1b1b1b", "circle-stroke-width": 2 } });
  state.map.addSource("workspace-drawing", { type: "geojson", data: emptyCollection() });
  state.map.addLayer({ id: "workspace-drawing-fill", type: "fill", source: "workspace-drawing", paint: { "fill-color": "#f2c14e", "fill-opacity": 0.18 } });
  state.map.addLayer({ id: "workspace-drawing-line", type: "line", source: "workspace-drawing", paint: { "line-color": "#b47718", "line-width": 3, "line-dasharray": [1.5, 1.5] } });
  state.map.addLayer({ id: "workspace-drawing-circle", type: "circle", source: "workspace-drawing", paint: { "circle-color": "#f2c14e", "circle-radius": 6, "circle-stroke-color": "#1b1b1b", "circle-stroke-width": 1 } });
}

function emptyCollection() {
  return { type: "FeatureCollection", features: [] };
}

function clearInstalledEntries() {
  for (const entry of state.entries.values()) {
    for (const layerId of [...entry.renderLayerIds].reverse()) {
      if (state.map.getLayer(layerId)) state.map.removeLayer(layerId);
    }
    if (entry.sourceId && state.map.getSource(entry.sourceId)) state.map.removeSource(entry.sourceId);
    entry.renderLayerIds = [];
    entry.sourceId = null;
  }
  state.renderedLayerIds = [];
}

function styleRule(entry, geometry) {
  const rules = entry.style?.style?.rules || [];
  return rules.find((rule) => geometryKind(rule.geometry_type) === geometry)
    || rules.find((rule) => !rule.geometry_type) || {};
}

function geometryKind(value) {
  if (!value) return null;
  if (value === "Point" || value === "MultiPoint") return "point";
  if (value === "LineString" || value === "MultiLineString") return "line";
  if (value === "Polygon" || value === "MultiPolygon") return "polygon";
  return null;
}

function installEntries() {
  if (!state.mapReady) return;
  clearInstalledEntries();
  let index = 0;
  for (const entry of state.entries.values()) {
    const sourceId = `workspace-entry-${index}`;
    entry.sourceId = sourceId;
    state.map.addSource(sourceId, { type: "geojson", data: emptyCollection() });
    installEntryLayer(entry, index, "fill");
    installEntryLayer(entry, index, "line");
    installEntryLayer(entry, index, "circle");
    index += 1;
  }
  renderSelection();
}

function installEntryLayer(entry, index, type) {
  const id = `workspace-entry-${index}-${type}`;
  const geometry = type === "fill" ? "polygon" : type === "circle" ? "point" : "line";
  const rule = styleRule(entry, geometry);
  const paint = type === "fill" ? {
    "fill-color": rule.fill_color || entry.color,
    "fill-opacity": (rule.fill_opacity ?? 0.3) * entry.opacity,
  } : type === "line" ? {
    "line-color": rule.line_color || entry.color,
    "line-width": rule.line_width_px || (entry.kind === "source" ? 1.6 : 2.5),
    "line-opacity": entry.opacity,
  } : {
    "circle-color": rule.circle_color || entry.color,
    "circle-radius": rule.circle_radius_px || (entry.kind === "source" ? 4 : 5.5),
    "circle-opacity": entry.opacity,
    "circle-stroke-color": css("--panel"),
    "circle-stroke-width": 1,
  };
  const filterType = type === "fill" ? "Polygon" : type === "circle" ? "Point" : "LineString";
  state.map.addLayer({
    id,
    source: entry.sourceId,
    type,
    minzoom: rule.minimum_zoom ?? 0,
    maxzoom: rule.maximum_zoom ?? 24,
    filter: ["==", ["geometry-type"], filterType],
    paint,
    layout: { visibility: entry.enabled ? "visible" : "none" },
  }, "workspace-selection-fill");
  entry.renderLayerIds.push(id);
  state.renderedLayerIds.push(id);
}

function applyInitialView() {
  const selected = composition();
  if (selected) {
    jumpToComposition(selected);
    return;
  }
  const coverages = [...state.entries.values()]
    .filter((entry) => entry.kind === "source" && entry.enabled && entry.release?.coverage)
    .map((entry) => entry.release.coverage);
  if (coverages.length) fitBoundingBoxes(coverages);
}

function jumpToComposition(selected) {
  const view = selected.current.view;
  state.map.jumpTo({
    center: [view.center.longitude_deg, view.center.latitude_deg],
    zoom: view.zoom,
    bearing: view.bearing_deg,
    pitch: view.pitch_deg,
  });
}

function viewportBoundingBox() {
  const bounds = state.map.getBounds();
  const rawWest = bounds.getWest();
  const rawEast = bounds.getEast();
  const span = rawEast - rawWest;
  if (span >= 359.999) return { west: -180, south: Math.max(-90, bounds.getSouth()), east: 180, north: Math.min(90, bounds.getNorth()) };
  const wrap = (value) => ((value + 180) % 360 + 360) % 360 - 180;
  return {
    west: wrap(rawWest),
    south: Math.max(-90, bounds.getSouth()),
    east: wrap(rawEast),
    north: Math.min(90, bounds.getNorth()),
  };
}

function updateViewportLabel() {
  if (!state.mapReady) return;
  const bbox = viewportBoundingBox();
  el("viewport").textContent = `${bbox.west.toFixed(2)}, ${bbox.south.toFixed(2)} → ${bbox.east.toFixed(2)}, ${bbox.north.toFixed(2)} · z${state.map.getZoom().toFixed(1)}`;
}

function scheduleViewportRefresh() {
  updateViewportLabel();
  clearTimeout(state.refreshTimer);
  state.refreshTimer = setTimeout(() => { void refreshViewport(); }, 180);
}

function asAuthoredGeoFeature(feature, entry) {
  return {
    type: "Feature",
    id: feature.id,
    geometry: feature.geometry,
    properties: {
      ...feature.properties,
      _workspace_key: entry.key,
      _feature_id: feature.id,
      _title: feature.title || "",
      _semantic_type: feature.featureType || feature.semantic_type || "feature",
      _layer_id: feature.layer_id,
      _feature_revision: feature.feature_revision,
    },
  };
}

function asSourceGeoFeature(match, entry) {
  const feature = match.feature || match;
  return {
    type: "Feature",
    id: feature.feature_id,
    geometry: feature.geometry,
    properties: {
      ...feature.normalized_tags,
      _workspace_key: entry.key,
      _feature_id: feature.feature_id,
      _title: feature.normalized_tags?.name || feature.source_element_id || "",
      _semantic_type: feature.representation || feature.source_element_type || "feature",
      _source_id: feature.source_id,
      _release_id: feature.release_id,
      _source_element_id: feature.source_element_id,
    },
  };
}

async function queryAuthoredEntry(entry, bbox, generation) {
  const features = [];
  let cursor;
  do {
    const request = { layer_id: entry.layer.layer_id, bbox, limit: 1000 };
    if (entry.publicationId) request.publication_id = entry.publicationId;
    if (cursor) request.cursor = cursor;
    const output = await tool("query_features", request);
    if (generation !== state.queryGeneration) return null;
    features.push(...(output.features || []).map((feature) => asAuthoredGeoFeature(feature, entry)));
    cursor = output.next_cursor;
  } while (cursor && features.length < MAX_AUTHORED_VIEW_FEATURES);
  entry.truncated = Boolean(cursor);
  return features;
}

async function querySourceEntry(entry, bbox, generation) {
  const features = [];
  let cursor;
  do {
    const request = {
      release_id: entry.release.release_id,
      source_id: entry.release.source_id,
      spatial: { kind: "bounding_box", bounds: bbox },
      limit: 500,
    };
    if (cursor) request.cursor = cursor;
    const output = await tool("query_source_features", request);
    if (generation !== state.queryGeneration) return null;
    features.push(...(output.features || []).map((feature) => asSourceGeoFeature(feature, entry)));
    cursor = output.next_cursor;
  } while (cursor && features.length < MAX_SOURCE_VIEW_FEATURES);
  entry.truncated = Boolean(cursor);
  return features;
}

async function parallelLimit(items, limit, operation) {
  const results = new Array(items.length);
  let next = 0;
  const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      const index = next++;
      results[index] = await operation(items[index], index);
    }
  });
  await Promise.all(workers);
  return results;
}

async function refreshViewport() {
  if (!state.mapReady) return;
  const generation = ++state.queryGeneration;
  const bbox = viewportBoundingBox();
  const visible = [...state.entries.values()].filter((entry) => entry.enabled);
  if (!visible.length) {
    setStatus("No layers are visible. Enable a layer in the catalog.", "warn");
    renderCatalog();
    renderPreview();
    return;
  }
  setStatus(`Loading ${visible.length} visible layer${visible.length === 1 ? "" : "s"} for this extent…`);
  try {
    const results = await parallelLimit(visible, QUERY_CONCURRENCY, async (entry) => {
      try {
        const features = entry.kind === "authored"
          ? await queryAuthoredEntry(entry, bbox, generation)
          : await querySourceEntry(entry, bbox, generation);
        return { features, error: null };
      } catch (error) {
        return { features: [], error };
      }
    });
    if (generation !== state.queryGeneration) return;
    results.forEach((result, index) => {
      const features = result.features;
      if (!features) return;
      const entry = visible[index];
      entry.features = features;
      entry.count = features.length;
      entry.error = result.error?.message || null;
      state.map.getSource(entry.sourceId)?.setData({ type: "FeatureCollection", features });
    });
    await waitForMapPaint();
    if (generation !== state.queryGeneration) return;
    const renderedFeatureCount = state.renderedLayerIds.length
      ? state.map.queryRenderedFeatures({ layers: state.renderedLayerIds }).length : 0;
    el("map").dataset.renderedFeatureCount = String(renderedFeatureCount);
    const expectedFeatureCount = results.reduce((sum, result) => sum + (result.features?.length || 0), 0);
    if (expectedFeatureCount && !renderedFeatureCount) {
      throw new Error("MapLibre completed the viewport update without painting any returned feature.");
    }
    renderCatalog();
    renderSelection();
    renderPreview();
    if (!state.action) renderInspector();
    const truncated = visible.some((entry) => entry.truncated);
    const failed = results.filter((result) => result.error).length;
    setStatus(`${expectedFeatureCount} feature${expectedFeatureCount === 1 ? "" : "s"} visible across ${visible.length - failed} layer${visible.length - failed === 1 ? "" : "s"}${failed ? ` · ${failed} layer preview${failed === 1 ? "" : "s"} unavailable` : ""}${truncated ? " · preview cap reached" : ""}.`, failed || truncated ? "warn" : "good");
  } catch (error) {
    if (generation === state.queryGeneration) setStatus(`Layer preview failed: ${error.message}`, "bad");
  }
}

function waitForMapPaint() {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      cleanup();
      reject(new Error("MapLibre did not finish painting the viewport within 5 seconds."));
    }, 5000);
    const idle = () => { cleanup(); resolve(); };
    const cleanup = () => {
      clearTimeout(timeout);
      state.map.off("idle", idle);
    };
    state.map.once("idle", idle);
    state.map.triggerRepaint();
  });
}

function handleMapClick(event) {
  if (state.drawing) {
    drawingClick([event.lngLat.lng, event.lngLat.lat]);
    return;
  }
  const features = state.renderedLayerIds.length
    ? state.map.queryRenderedFeatures(event.point, { layers: state.renderedLayerIds }) : [];
  const feature = features[0];
  if (!feature) return;
  const key = feature.properties?._workspace_key;
  const featureId = feature.properties?._feature_id || feature.id;
  if (key && featureId !== undefined) selectFeature(key, featureId);
}

function renderSelection() {
  if (!state.mapReady) return;
  const entry = selectedEntry();
  const feature = entry?.features.find((candidate) => String(candidate.id) === state.selectedFeatureId);
  state.map.getSource("workspace-selection")?.setData(feature
    ? { type: "FeatureCollection", features: [feature] } : emptyCollection());
}

function renderPreview() {
  const entry = selectedEntry();
  const features = entry?.features || [];
  el("preview-title").textContent = entry ? entry.title : "Data preview";
  el("preview-count").textContent = entry ? `${features.length}${entry.truncated ? "+" : ""} in map extent` : "Select a layer";
  const pages = Math.max(1, Math.ceil(features.length / PREVIEW_PAGE_SIZE));
  state.previewPage = Math.min(state.previewPage, pages - 1);
  el("preview-page").textContent = `${state.previewPage + 1} / ${pages}`;
  el("preview-prev").disabled = state.previewPage === 0;
  el("preview-next").disabled = state.previewPage >= pages - 1;
  const rows = el("preview-rows");
  rows.replaceChildren();
  for (const feature of features.slice(state.previewPage * PREVIEW_PAGE_SIZE, (state.previewPage + 1) * PREVIEW_PAGE_SIZE)) {
    const row = node("tr");
    if (String(feature.id) === state.selectedFeatureId) row.className = "selected";
    const properties = feature.properties || {};
    const publicProperties = Object.fromEntries(Object.entries(properties).filter(([key]) => !key.startsWith("_")));
    const cells = [
      properties._title || feature.id,
      properties._semantic_type || "feature",
      feature.geometry?.type || "",
      propertySummary(publicProperties),
    ];
    for (const value of cells) row.append(node("td", value));
    row.addEventListener("click", () => selectFeature(entry.key, feature.id));
    rows.append(row);
  }
}

function propertySummary(properties) {
  const entries = Object.entries(properties);
  if (!entries.length) return "—";
  return entries.slice(0, 4).map(([key, value]) => `${key}: ${displayValue(value)}`).join(" · ") + (entries.length > 4 ? " …" : "");
}

function displayValue(value) {
  if (value === null) return "null";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function renderInspector() {
  if (state.action) return;
  el("close-action").hidden = true;
  const entry = selectedEntry();
  if (!entry) {
    el("inspector-title").textContent = "Inspector";
    el("inspector-body").innerHTML = '<div class="inspector-empty"><h3>Select a layer or feature</h3><div>The map stays visible while details and actions appear here.</div></div>';
    return;
  }
  const feature = entry.features.find((candidate) => String(candidate.id) === state.selectedFeatureId);
  if (feature) renderFeatureInspector(entry, feature);
  else renderEntryInspector(entry);
}

function appendDetail(host, label, value, monospace = false) {
  const section = node("div", undefined, "inspector-section");
  section.append(node("div", label, "eyebrow"), node("div", value ?? "—", monospace ? "mono" : "value"));
  host.append(section);
}

function renderEntryInspector(entry) {
  el("inspector-title").textContent = entry.kind === "authored" ? "Layer" : "Source release";
  const body = el("inspector-body");
  body.replaceChildren();
  const heading = node("div", undefined, "inspector-section");
  heading.append(node("div", entry.kind === "authored" ? "Authored feature layer" : "Active governed release", "eyebrow"), node("h2", entry.title), node("div", entry.subtitle, "muted"));
  const actions = node("div", undefined, "actions");
  const zoom = node("button", "Zoom to data");
  zoom.addEventListener("click", () => zoomToEntry(entry));
  const visibility = node("button", entry.enabled ? "Hide" : "Show");
  visibility.addEventListener("click", () => setEntryVisibility(entry.key, !entry.enabled));
  actions.append(zoom, visibility);
  if (entry.kind === "authored" && state.access.feature_write) {
    const add = node("button", "Draw feature");
    add.addEventListener("click", () => showAction("add-feature", entry.layer.layer_id));
    const importButton = node("button", "Import artifact");
    importButton.addEventListener("click", () => showAction("import-artifact", entry.layer.layer_id));
    actions.append(add, importButton);
  }
  if (entry.kind === "authored" && state.access.feature_publish) {
    const publish = node("button", "Publish current", "primary");
    publish.disabled = entry.publication?.layer_revision === entry.layer.revision;
    publish.addEventListener("click", () => void publishEntry(entry));
    actions.append(publish);
  }
  if (entry.kind === "source" && state.access.administration) {
    const acquire = node("button", "New acquisition");
    acquire.addEventListener("click", () => showAction("acquire-source", entry.release.source_id));
    actions.append(acquire);
  }
  heading.append(actions);
  body.append(heading);
  if (entry.error) appendDetail(body, "Preview unavailable", entry.error);
  if (entry.kind === "authored") {
    appendDetail(body, "Identity", entry.layer.layer_id, true);
    appendDetail(body, "Content class", entry.layer.content_class);
    appendDetail(body, "Layer revision", entry.layer.revision);
    appendDetail(body, "Publication", entry.publicationId || entry.publication?.publication_id || "Not published", true);
    appendDetail(body, "Visible records", `${entry.count}${entry.truncated ? "+" : ""}`);
    const advanced = document.createElement("details");
    advanced.innerHTML = `<summary>Advanced layer record</summary><pre class="mono"></pre>`;
    advanced.querySelector("pre").textContent = JSON.stringify(entry.layer, null, 2);
    body.append(advanced);
  } else {
    appendDetail(body, "Release", entry.release.release_id, true);
    appendDetail(body, "Source", entry.release.source_id, true);
    appendDetail(body, "Version", entry.release.version_label);
    appendDetail(body, "Coverage", formatBounds(entry.release.coverage));
    appendDetail(body, "Attribution", entry.release.license?.attribution || "—");
    appendDetail(body, "Visible records", `${entry.count}${entry.truncated ? "+" : ""}`);
  }
}

function renderFeatureInspector(entry, feature) {
  el("inspector-title").textContent = "Feature";
  const body = el("inspector-body");
  body.replaceChildren();
  const properties = feature.properties || {};
  const heading = node("div", undefined, "inspector-section");
  heading.append(node("div", entry.kind === "authored" ? "Authored feature" : "Source feature", "eyebrow"),
    node("h2", properties._title || properties._semantic_type || "Map feature"),
    node("div", properties._feature_id || feature.id, "mono"));
  const actions = node("div", undefined, "actions");
  const zoom = node("button", "Zoom to feature");
  zoom.addEventListener("click", () => fitGeometry(feature.geometry, 16));
  const clear = node("button", "Clear selection");
  clear.addEventListener("click", () => { state.selectedFeatureId = null; renderSelection(); renderInspector(); renderPreview(); });
  actions.append(zoom, clear);
  heading.append(actions);
  body.append(heading);
  appendDetail(body, "Layer", entry.title);
  appendDetail(body, "Semantic type", properties._semantic_type || "feature");
  appendDetail(body, "Geometry", feature.geometry?.type || "—");
  const publicProperties = Object.fromEntries(Object.entries(properties).filter(([key]) => !key.startsWith("_")));
  const propertySection = node("div", undefined, "inspector-section");
  propertySection.append(node("div", "Properties", "eyebrow"));
  if (Object.keys(publicProperties).length) {
    for (const [key, value] of Object.entries(publicProperties)) {
      const line = node("div");
      line.append(node("strong", `${key}: `), document.createTextNode(displayValue(value)));
      propertySection.append(line);
    }
  } else propertySection.append(node("div", "No properties", "muted"));
  body.append(propertySection);
  const advanced = document.createElement("details");
  advanced.innerHTML = `<summary>Advanced GeoJSON</summary><pre class="mono"></pre>`;
  advanced.querySelector("pre").textContent = JSON.stringify(feature, null, 2);
  body.append(advanced);
}

function mountTemplate(templateId, title) {
  state.action = templateId;
  el("inspector-title").textContent = title;
  el("close-action").hidden = false;
  const fragment = el(templateId).content.cloneNode(true);
  const body = el("inspector-body");
  body.replaceChildren(fragment);
  applyAccessVisibility(body);
  body.closest(".inspector").scrollTop = 0;
  return body;
}

function closeAction() {
  cancelDrawing();
  renderDrawingGeometry(null);
  state.action = null;
  state.rawAdminKind = null;
  renderInspector();
}

function showAction(action, preferredId = null) {
  cancelDrawing();
  renderDrawingGeometry(null);
  if (action === "picker") {
    const body = mountTemplate("action-picker-template", "Add data");
    bindActionLinks(body);
    return;
  }
  if (action === "create-layer") return bindCreateLayer(mountTemplate("create-layer-template", "New layer"));
  if (action === "add-feature") return bindAddFeature(mountTemplate("add-feature-template", "Draw feature"), preferredId);
  if (action === "import-artifact") return bindImportArtifact(mountTemplate("import-artifact-template", "Import artifact"), preferredId);
  if (action === "acquire-source") return bindAcquireSource(mountTemplate("acquire-source-template", "Acquire source"), preferredId);
  if (action === "save-view") return bindSaveView(mountTemplate("save-view-template", "Save view"));
  if (action === "manage-data") return bindManageData(mountTemplate("manage-data-template", "Governed data"));
  if (action === "register-source" || action === "register-profile") return bindRawAdmin(mountTemplate("raw-admin-template", "Advanced administration"), action);
}

function bindActionLinks(root) {
  root.querySelectorAll("[data-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.dataset.action;
      if (action === "close") closeAction(); else showAction(action);
    });
  });
}

function layerOptions(select, preferredId) {
  select.replaceChildren(...state.layers.map((layer) => {
    const option = node("option", `${layer.title} · r${layer.revision}`);
    option.value = layer.layer_id;
    return option;
  }));
  if (preferredId && state.layers.some((layer) => layer.layer_id === preferredId)) select.value = preferredId;
  else if (selectedEntry()?.kind === "authored") select.value = selectedEntry().layer.layer_id;
}

function bindCreateLayer(root) {
  bindActionLinks(root);
  el("create-layer-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void runAction(async () => {
      const schema = JSON.parse(el("new-layer-schema").value);
      const styleText = el("new-layer-style").value.trim();
      const request = {
        title: el("new-layer-title").value.trim(),
        content_class: el("new-layer-class").value,
        property_schema: schema,
      };
      const description = el("new-layer-description").value.trim();
      if (description) request.description = description;
      if (styleText) request.style = JSON.parse(styleText);
      const layer = await tool("create_feature_layer", request);
      await refreshAll();
      selectEntry(`authored:${layer.layer_id}`);
      setStatus(`Created ${layer.title}.`, "good");
    });
  });
}

function addPropertyRow(key = "", value = "") {
  const row = node("div", undefined, "property-row");
  const keyInput = document.createElement("input");
  keyInput.placeholder = "name";
  keyInput.value = key;
  keyInput.dataset.propertyKey = "";
  const valueInput = document.createElement("input");
  valueInput.placeholder = "value";
  valueInput.value = typeof value === "string" ? value : JSON.stringify(value);
  valueInput.dataset.propertyValue = "";
  const remove = node("button", "×", "icon quiet");
  remove.type = "button";
  remove.addEventListener("click", () => { row.remove(); syncAdvancedFeatureJson(); });
  for (const input of [keyInput, valueInput]) input.addEventListener("input", syncAdvancedFeatureJson);
  row.append(keyInput, valueInput, remove);
  el("property-rows").append(row);
}

function propertyFormValue() {
  const properties = {};
  el("property-rows").querySelectorAll(".property-row").forEach((row) => {
    const key = row.querySelector("[data-property-key]").value.trim();
    const raw = row.querySelector("[data-property-value]").value.trim();
    if (!key) return;
    try { properties[key] = JSON.parse(raw); } catch { properties[key] = raw; }
  });
  return properties;
}

function featureInput() {
  if (!state.featureDraft.geometry) throw new Error("Draw a point, line, or polygon on the map first.");
  const title = el("feature-title").value.trim();
  const semanticType = el("feature-semantic-type").value.trim();
  if (!title || !semanticType) throw new Error("Enter a title and semantic type.");
  return {
    geometry: state.featureDraft.geometry,
    properties: propertyFormValue(),
    semantic_type: semanticType,
    title,
    related_resources: [],
    evidence_resources: [],
  };
}

function syncAdvancedFeatureJson() {
  if (!el("feature-json-advanced")) return;
  const draft = {
    geometry: state.featureDraft.geometry,
    properties: propertyFormValue(),
    semantic_type: el("feature-semantic-type").value.trim(),
    title: el("feature-title").value.trim(),
    related_resources: [],
    evidence_resources: [],
  };
  el("feature-json-advanced").value = JSON.stringify(draft, null, 2);
}

function bindAddFeature(root, preferredId) {
  bindActionLinks(root);
  state.featureDraft = { geometry: null, properties: {} };
  layerOptions(el("feature-layer"), preferredId);
  addPropertyRow();
  for (const id of ["feature-title", "feature-semantic-type"]) el(id).addEventListener("input", syncAdvancedFeatureJson);
  root.querySelectorAll("[data-draw]").forEach((button) => button.addEventListener("click", () => {
    startDrawing(button.dataset.draw, (geometry) => {
      state.featureDraft.geometry = geometry;
      updateFeatureGeometryStatus();
      syncAdvancedFeatureJson();
    });
  }));
  el("add-property").addEventListener("click", () => addPropertyRow());
  el("apply-feature-json").addEventListener("click", () => void runAction(async () => {
    const input = JSON.parse(el("feature-json-advanced").value);
    state.featureDraft.geometry = input.geometry || null;
    el("feature-title").value = input.title || "";
    el("feature-semantic-type").value = input.semantic_type || input.featureType || "";
    el("property-rows").replaceChildren();
    for (const [key, value] of Object.entries(input.properties || {})) addPropertyRow(key, value);
    if (!Object.keys(input.properties || {}).length) addPropertyRow();
    updateFeatureGeometryStatus();
    renderDrawingGeometry(state.featureDraft.geometry);
  }));
  el("validate-feature").addEventListener("click", () => void validateFeature());
  el("add-feature-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void commitFeature();
  });
  syncAdvancedFeatureJson();
}

function selectedFormLayer(selectId) {
  const layer = state.layers.find((candidate) => candidate.layer_id === el(selectId).value);
  if (!layer) throw new Error("Choose an authored destination layer.");
  return layer;
}

function featureMutationRequest() {
  const layer = selectedFormLayer("feature-layer");
  return {
    layer_id: layer.layer_id,
    expected_layer_revision: layer.revision,
    mutations: [{ action: "create", feature: featureInput() }],
  };
}

async function validateFeature() {
  await runAction(async () => {
    const output = await tool("validate_feature_changes", featureMutationRequest());
    renderFindings(output.findings || [], output.valid ? "Feature is valid." : "Feature needs changes.");
    setStatus(output.valid ? "Feature validation passed." : "Feature validation found problems.", output.valid ? "good" : "warn");
  });
}

async function commitFeature() {
  await runAction(async () => {
    const request = featureMutationRequest();
    request.idempotency_key = uuid();
    const output = await tool("commit_feature_changes", request);
    const featureId = output.features?.[0]?.id;
    await refreshAll();
    selectEntry(`authored:${request.layer_id}`);
    if (featureId) selectFeature(`authored:${request.layer_id}`, featureId);
    setStatus("Feature committed to the authored layer.", "good");
  });
}

function renderFindings(findings, successText) {
  const host = el("feature-findings");
  host.replaceChildren();
  if (!findings.length) host.append(node("div", successText, "finding"));
  else for (const finding of findings) host.append(node("div", `${finding.code}: ${finding.message}`, "finding"));
}

function updateFeatureGeometryStatus() {
  if (!el("feature-geometry-status")) return;
  const geometry = state.featureDraft.geometry;
  el("feature-geometry-status").textContent = geometry
    ? `${geometry.type} ready · ${geometryCoordinateCount(geometry)} coordinate${geometryCoordinateCount(geometry) === 1 ? "" : "s"}`
    : "No geometry drawn yet.";
}

function geometryCoordinateCount(geometry) {
  let count = 0;
  const walk = (value) => {
    if (!Array.isArray(value)) return;
    if (value.length >= 2 && value.every((part) => typeof part === "number")) count += 1;
    else value.forEach(walk);
  };
  walk(geometry?.coordinates);
  return count;
}

function bindImportArtifact(root, preferredId) {
  bindActionLinks(root);
  layerOptions(el("import-layer"), preferredId);
  el("import-format").addEventListener("change", () => {
    el("geopackage-fields").hidden = el("import-format").value !== "geo_package";
  });
  el("inspect-geopackage").addEventListener("click", () => void inspectGeoPackage());
  el("geopackage-table").addEventListener("change", renderGeoPackageTable);
  el("import-artifact-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void importArtifact();
  });
}

async function inspectGeoPackage() {
  await runAction(async () => {
    const artifactId = el("import-artifact-id").value.trim();
    if (!artifactId) throw new Error("Enter an authorized artifact ID first.");
    el("import-progress").textContent = "Inspecting GeoPackage…";
    const output = await taskTool("inspect_geopackage", { source_artifact_id: artifactId }, (status, message) => {
      el("import-progress").textContent = `GeoPackage inspection ${status}${message ? ` · ${message}` : ""}`;
    });
    state.geopackageManifest = output.manifest;
    const tables = output.manifest?.feature_tables || [];
    el("geopackage-table").replaceChildren(...tables.map((table) => {
      const option = node("option", `${table.identifier || table.table} · ${table.feature_count} features`);
      option.value = table.table;
      return option;
    }));
    renderGeoPackageTable();
    setStatus(`GeoPackage inspected: ${tables.length} feature table${tables.length === 1 ? "" : "s"}.`, "good");
  });
}

function renderGeoPackageTable() {
  if (!state.geopackageManifest) return;
  const table = state.geopackageManifest.feature_tables.find((candidate) => candidate.table === el("geopackage-table").value);
  if (!table) return;
  el("geopackage-summary").textContent = `${table.geometry_type} · ${table.feature_count} features · ${table.crs_name || `SRS ${table.srs_id}`} · ${table.has_spatial_index ? "spatial index present" : "no declared spatial index"}${table.extent_wgs84 ? ` · ${formatBounds(table.extent_wgs84)}` : ""}`;
  const optional = (label) => {
    const option = node("option", label);
    option.value = "";
    return option;
  };
  for (const id of ["geopackage-identity", "geopackage-semantic", "geopackage-title"]) {
    el(id).replaceChildren(optional("Not mapped"), ...(table.fields || []).map((field) => {
      const option = node("option", `${field.name} · ${field.field_type}`);
      option.value = field.name;
      return option;
    }));
  }
}

async function importArtifact() {
  await runAction(async () => {
    const layer = selectedFormLayer("import-layer");
    const format = el("import-format").value;
    const defaultSemanticType = el("import-semantic-type").value.trim();
    if (!defaultSemanticType) throw new Error("Enter a default semantic type.");
    let source = { format, default_semantic_type: defaultSemanticType };
    if (format === "geo_package") {
      const table = el("geopackage-table").value;
      if (!table) throw new Error("Inspect the GeoPackage and choose one feature table.");
      source = { ...source, table };
      const mappings = [
        ["identity_column", "geopackage-identity"],
        ["semantic_type_column", "geopackage-semantic"],
        ["title_column", "geopackage-title"],
      ];
      for (const [field, id] of mappings) if (el(id).value) source[field] = el(id).value;
    }
    const request = {
      layer_id: layer.layer_id,
      expected_layer_revision: layer.revision,
      source_artifact_id: el("import-artifact-id").value.trim(),
      source,
      idempotency_key: uuid(),
    };
    el("import-progress").textContent = "Starting import task…";
    const output = await taskTool("import_feature_layer", request, (status, message) => {
      el("import-progress").textContent = `Import ${status}${message ? ` · ${message}` : ""}`;
    });
    await refreshAll();
    selectEntry(`authored:${layer.layer_id}`);
    setStatus(`Imported ${output.imported_feature_count || 0} features.`, "good");
  });
}

function bindAcquireSource(root, preferredId) {
  bindActionLinks(root);
  const select = el("acquire-source");
  select.replaceChildren(...state.sources.filter((source) => source.enabled).map((source) => {
    const option = node("option", source.name);
    option.value = source.source_id;
    return option;
  }));
  if (preferredId) select.value = preferredId;
  el("draw-coverage").addEventListener("click", () => startDrawing("bbox", (bounds) => setCoverage(bounds)));
  el("acquire-source-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void runAction(async () => {
      const sourceId = select.value;
      if (!sourceId) throw new Error("Choose an enabled governed source.");
      await tool("start_acquisition", {
        source_id: sourceId,
        requested_coverage: requestedCoverage(),
        idempotency_key: uuid(),
      });
      await refreshAll();
      setStatus("Source acquisition started. Updates will arrive reactively.", "good");
      closeAction();
    });
  });
}

function setCoverage(bounds) {
  for (const field of ["west", "south", "east", "north"]) el(`bbox-${field}`).value = bounds[field].toFixed(6);
  el("coverage-status").textContent = formatBounds(bounds);
}

function requestedCoverage() {
  const ranges = { west: [-180, 180], south: [-90, 90], east: [-180, 180], north: [-90, 90] };
  const coverage = {};
  for (const [field, [minimum, maximum]] of Object.entries(ranges)) {
    const value = Number(el(`bbox-${field}`).value);
    if (!Number.isFinite(value) || value < minimum || value > maximum) throw new Error(`${field} must be between ${minimum} and ${maximum}.`);
    coverage[field] = value;
  }
  if (coverage.south > coverage.north) throw new Error("South must not exceed north.");
  return coverage;
}

function bindSaveView(root) {
  bindActionLinks(root);
  const selected = composition();
  el("save-view-title").value = selected?.title || "";
  const { layers, omitted } = compositionLayersForSave();
  el("save-view-summary").textContent = `${layers.length} visible published layer${layers.length === 1 ? "" : "s"} will be saved${omitted.length ? ` · ${omitted.length} current layer${omitted.length === 1 ? " is" : "s are"} omitted until published` : ""}.`;
  el("save-view-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void runAction(async () => {
      const current = compositionLayersForSave();
      if (!current.layers.length) throw new Error("At least one visible authored layer must have a publication.");
      const center = state.map.getCenter();
      const request = {
        title: el("save-view-title").value.trim(),
        layers: current.layers,
        view: {
          center: { longitude_deg: center.lng, latitude_deg: center.lat },
          zoom: state.map.getZoom(),
          bearing_deg: state.map.getBearing(),
          pitch_deg: state.map.getPitch(),
        },
      };
      let output;
      if (selected) {
        output = await tool("update_map_composition", {
          composition_id: selected.composition_id,
          expected_revision: selected.current.revision,
          ...request,
        });
      } else output = await tool("create_map_composition", request);
      state.selectedCompositionId = output.composition_id;
      await refreshAll();
      setStatus(`Saved ${output.title}.`, "good");
      closeAction();
    });
  });
}

function compositionLayersForSave() {
  const layers = [];
  const omitted = [];
  for (const entry of state.entries.values()) {
    if (entry.kind !== "authored" || !entry.enabled) continue;
    const publication = entry.publicationId
      ? state.publications.find((candidate) => candidate.publication_id === entry.publicationId)
      : latestPublication(entry.layer.layer_id);
    if (!publication) {
      omitted.push(entry);
      continue;
    }
    const layer = {
      layer_id: entry.layer.layer_id,
      publication_id: publication.publication_id,
      visible: true,
      opacity: entry.opacity,
    };
    if (publication.style_revision_id) layer.style_revision_id = publication.style_revision_id;
    layers.push(layer);
  }
  return { layers, omitted };
}

function bindRawAdmin(root, kind) {
  bindActionLinks(root);
  state.rawAdminKind = kind;
  el("raw-admin-title").textContent = kind === "register-source" ? "Register source" : "Register mobility profile";
  el("raw-admin-json").value = kind === "register-source"
    ? JSON.stringify({ source_id: "", dataset_id: "", name: "", adapter_kind: "authority_vector" }, null, 2)
    : JSON.stringify({ family: "uas", metadata: { profile_id: "", name: "", version: 1 } }, null, 2);
  el("raw-admin-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void runAction(async () => {
      const record = JSON.parse(el("raw-admin-json").value);
      if (kind === "register-source") await tool("register_source", { source: record, idempotency_key: uuid() });
      else await tool("register_mobility_profile", { profile: record, idempotency_key: uuid() });
      await refreshAll();
      setStatus(kind === "register-source" ? "Source registered." : "Mobility profile registered.", "good");
      showAction("picker");
    });
  });
}

function bindManageData(root) {
  bindActionLinks(root);
  const acquisitions = el("manage-acquisitions");
  acquisitions.replaceChildren();
  if (!state.acquisitions.length) acquisitions.append(node("div", "No acquisition jobs.", "muted"));
  for (const job of state.acquisitions) {
    const record = node("div", undefined, "record");
    const head = node("div", undefined, "record-head");
    head.append(node("strong", job.acquisition_id), node("span", job.status, `chip ${job.status === "completed" ? "good" : ""}`));
    record.append(head, node("div", job.progress?.phase || "", "mono"));
    if (ACTIVE_ACQUISITION_STATUSES.has(job.status) && job.status !== "cancel_requested") {
      const actions = node("div", undefined, "actions");
      const cancel = node("button", "Cancel acquisition");
      cancel.addEventListener("click", () => void runAction(async () => {
        await tool("cancel_acquisition", { acquisition_id: job.acquisition_id });
        await refreshAll();
        showAction("manage-data");
        setStatus("Acquisition cancellation requested.", "good");
      }));
      actions.append(cancel);
      record.append(actions);
    }
    acquisitions.append(record);
  }

  const releases = allDatasetReleases().sort((left, right) => String(right.updated_at || "").localeCompare(String(left.updated_at || "")));
  const releaseHost = el("manage-releases");
  releaseHost.replaceChildren();
  if (!releases.length) releaseHost.append(node("div", "No dataset releases.", "muted"));
  for (const release of releases) {
    const record = node("div", undefined, "record");
    const head = node("div", undefined, "record-head");
    head.append(node("strong", release.version_label || release.release_id), node("span", release.state, `chip ${release.state === "active" ? "good" : ""}`));
    record.append(head, node("div", release.release_id, "mono"), node("div", formatBounds(release.coverage), "mono"));
    const pointer = state.activeReleases.find((candidate) => candidate.dataset_id === release.dataset_id);
    const request = {
      release_id: release.release_id,
      expected_record_version: release.record_version,
      expected_active_pointer_version: pointer ? pointer.record_version : 0,
    };
    const actions = node("div", undefined, "actions");
    const addAction = (label, name) => {
      const button = node("button", label, name === "activate_release" ? "primary" : "");
      button.addEventListener("click", () => void runAction(async () => {
        await tool(name, request);
        await refreshAll();
        showAction("manage-data");
        setStatus(`${label} completed for ${release.version_label}.`, "good");
      }));
      actions.append(button);
    };
    if (release.state === "staged" || release.state === "active") addAction(release.state === "active" ? "Reconcile" : "Activate", "activate_release");
    if (release.state !== "quarantined" && release.state !== "active") {
      addAction("Rollback", "rollback_release");
      addAction("Quarantine", "quarantine_release");
    }
    if (actions.childNodes.length) record.append(actions);
    releaseHost.append(record);
  }
}

async function publishEntry(entry) {
  await runAction(async () => {
    await tool("publish_feature_layer", {
      layer_id: entry.layer.layer_id,
      expected_layer_revision: entry.layer.revision,
      title: entry.layer.title,
    });
    await refreshAll();
    selectEntry(entry.key);
    setStatus(`Published ${entry.title}.`, "good");
  });
}

async function runAction(operation) {
  try {
    return await operation();
  } catch (error) {
    setStatus(error.message, "bad");
    return undefined;
  }
}

function startDrawing(kind, onComplete) {
  if (!state.mapReady) return;
  cancelDrawing();
  state.drawing = { kind, coordinates: [], onComplete };
  state.map.getCanvas().style.cursor = "crosshair";
  state.map.doubleClickZoom.disable();
  el("draw-tools").hidden = false;
  el("draw-instruction").textContent = kind === "point" ? "Click a location"
    : kind === "bbox" ? "Click two opposite corners"
      : `Click vertices for the ${kind}`;
  el("finish-drawing").hidden = kind === "point" || kind === "bbox";
  renderDrawingGeometry(null);
}

function drawingClick(position) {
  const drawing = state.drawing;
  if (!drawing) return;
  drawing.coordinates.push(position);
  renderDrawingGeometry(drawingGeometry(drawing, false));
  if (drawing.kind === "point" || (drawing.kind === "bbox" && drawing.coordinates.length === 2)) finishDrawing();
}

function drawingGeometry(drawing, final) {
  const coordinates = drawing.coordinates;
  if (!coordinates.length) return null;
  if (drawing.kind === "point") return { type: "Point", coordinates: coordinates[0] };
  if (drawing.kind === "line") {
    if (coordinates.length === 1) return { type: "Point", coordinates: coordinates[0] };
    return { type: "LineString", coordinates };
  }
  if (drawing.kind === "polygon") {
    if (coordinates.length < 3) return coordinates.length === 1
      ? { type: "Point", coordinates: coordinates[0] } : { type: "LineString", coordinates };
    return { type: "Polygon", coordinates: [[...coordinates, coordinates[0]]] };
  }
  if (drawing.kind === "bbox" && coordinates.length >= 2) {
    const [first, second] = coordinates;
    const west = Math.min(first[0], second[0]);
    const east = Math.max(first[0], second[0]);
    const south = Math.min(first[1], second[1]);
    const north = Math.max(first[1], second[1]);
    if (final) return { west, south, east, north };
    return { type: "Polygon", coordinates: [[[west, south], [east, south], [east, north], [west, north], [west, south]]] };
  }
  return null;
}

function finishDrawing() {
  const drawing = state.drawing;
  if (!drawing) return;
  const minimum = drawing.kind === "line" ? 2 : drawing.kind === "polygon" ? 3 : drawing.kind === "bbox" ? 2 : 1;
  if (drawing.coordinates.length < minimum) {
    setStatus(`${drawing.kind} requires at least ${minimum} map clicks.`, "warn");
    return;
  }
  const result = drawingGeometry(drawing, true);
  const display = drawing.kind === "bbox" ? drawingGeometry(drawing, false) : result;
  const callback = drawing.onComplete;
  endDrawingInteraction();
  renderDrawingGeometry(display);
  callback(result);
}

function undoDrawing() {
  if (!state.drawing) return;
  state.drawing.coordinates.pop();
  renderDrawingGeometry(drawingGeometry(state.drawing, false));
}

function cancelDrawing() {
  if (!state.drawing) return;
  endDrawingInteraction();
  renderDrawingGeometry(null);
}

function endDrawingInteraction() {
  state.drawing = null;
  if (state.map) {
    state.map.getCanvas().style.cursor = "";
    state.map.doubleClickZoom.enable();
  }
  el("draw-tools").hidden = true;
}

function renderDrawingGeometry(geometry) {
  if (!state.mapReady) return;
  state.map.getSource("workspace-drawing")?.setData(geometry
    ? { type: "FeatureCollection", features: [{ type: "Feature", geometry, properties: {} }] }
    : emptyCollection());
}

function formatBounds(bounds) {
  if (!bounds) return "—";
  return `${Number(bounds.west).toFixed(4)}, ${Number(bounds.south).toFixed(4)} → ${Number(bounds.east).toFixed(4)}, ${Number(bounds.north).toFixed(4)}`;
}

function coordinatesOf(geometry) {
  const positions = [];
  const walk = (value) => {
    if (!Array.isArray(value)) return;
    if (value.length >= 2 && typeof value[0] === "number" && typeof value[1] === "number") positions.push(value);
    else value.forEach(walk);
  };
  walk(geometry?.coordinates);
  return positions;
}

function fitGeometry(geometry, maximumZoom = 16) {
  const positions = coordinatesOf(geometry);
  if (!positions.length) return;
  if (positions.length === 1) {
    state.map.easeTo({ center: positions[0], zoom: Math.max(state.map.getZoom(), maximumZoom), duration: 350 });
    return;
  }
  const bounds = new maplibregl.LngLatBounds(positions[0], positions[0]);
  positions.slice(1).forEach((position) => bounds.extend(position));
  state.map.fitBounds(bounds, { padding: 70, maxZoom: maximumZoom, duration: 350 });
}

function fitBoundingBoxes(boxes) {
  const first = boxes[0];
  const bounds = new maplibregl.LngLatBounds([first.west, first.south], [first.east, first.north]);
  for (const box of boxes.slice(1)) {
    bounds.extend([box.west, box.south]);
    bounds.extend([box.east, box.north]);
  }
  state.map.fitBounds(bounds, { padding: 70, maxZoom: 13, duration: 0 });
}

function zoomToEntry(entry) {
  if (entry.features.length) {
    const positions = entry.features.flatMap((feature) => coordinatesOf(feature.geometry));
    if (positions.length) {
      const bounds = new maplibregl.LngLatBounds(positions[0], positions[0]);
      positions.slice(1).forEach((position) => bounds.extend(position));
      state.map.fitBounds(bounds, { padding: 70, maxZoom: 15, duration: 350 });
      return;
    }
  }
  if (entry.release?.coverage) fitBoundingBoxes([entry.release.coverage]);
  else setStatus("No visible geometry is available to fit yet.", "warn");
}

async function loadFeatureData() {
  if (!state.access.feature_read) return [];
  const results = await Promise.allSettled([
    read("map://feature-layers"),
    read("map://publications"),
    read("map://compositions"),
  ]);
  const labels = ["layers", "publications", "saved views"];
  const failures = [];
  results.forEach((result, index) => {
    if (result.status === "fulfilled") {
      if (index === 0) state.layers = result.value;
      if (index === 1) state.publications = result.value;
      if (index === 2) state.compositions = result.value;
    } else failures.push(`${labels[index]}: ${result.reason.message}`);
  });
  return failures;
}

async function loadDatasetData() {
  if (!state.access.dataset_read) return [];
  const resources = [
    ["map://sources", "sources"],
    ["map://datasets", "datasets"],
    ["map://active-releases", "activeReleases"],
    ["map://mobility-profiles", "profiles"],
  ];
  const results = await Promise.allSettled(resources.map(([uri]) => read(uri)));
  const failures = [];
  results.forEach((result, index) => {
    const [uri, field] = resources[index];
    if (result.status === "fulfilled") state[field] = result.value;
    else failures.push(`${uri}: ${result.reason.message}`);
  });
  return failures;
}

async function loadAdminData() {
  if (!state.access.administration) return [];
  try {
    state.acquisitions = await read("map://acquisitions");
    return [];
  } catch (error) {
    return [`map://acquisitions: ${error.message}`];
  }
}

async function refreshSnapshot() {
  setStatus("Refreshing authorized map resources…");
  const [featureFailures, datasetFailures, adminFailures] = await Promise.all([
    loadFeatureData(),
    loadDatasetData(),
    loadAdminData(),
  ]);
  await rebuildEntries();
  const failures = [...featureFailures, ...datasetFailures, ...adminFailures];
  if (failures.length) setStatus(`Workspace updated with ${failures.length} unavailable resource${failures.length === 1 ? "" : "s"}.`, "warn");
  else if (!state.mapReady) setStatus("Map resources loaded.", "good");
  reportSize();
}

async function refreshAll() {
  if (state.refreshPromise) {
    state.refreshQueued = true;
    return state.refreshPromise;
  }
  state.refreshPromise = refreshSnapshot();
  try {
    await state.refreshPromise;
  } finally {
    state.refreshPromise = null;
  }
  if (state.refreshQueued) {
    state.refreshQueued = false;
    return refreshAll();
  }
  return undefined;
}

async function applyCompositionSelection() {
  state.selectedCompositionId = el("composition-select").value;
  await rebuildEntries();
  const selected = composition();
  el("map-context-label").textContent = selected ? selected.title : "Live workspace";
  if (selected) jumpToComposition(selected);
  else applyInitialView();
}

function resetView() {
  const selected = composition();
  if (selected) jumpToComposition(selected);
  else applyInitialView();
}

function applyHostContext(context) {
  if (context && (context.theme === "dark" || context.theme === "light")) {
    document.documentElement.dataset.theme = context.theme;
    void applyBasemapTheme(context.theme);
  }
}

function reportSize() {
  bridge.notify("ui/notifications/size-changed", {
    height: Math.ceil(document.documentElement.getBoundingClientRect().height),
  });
}

el("refresh").addEventListener("click", () => void refreshAll());
el("add-data").addEventListener("click", () => showAction("picker"));
el("save-view").addEventListener("click", () => showAction("save-view"));
el("close-action").addEventListener("click", closeAction);
el("layer-search").addEventListener("input", renderCatalog);
el("composition-select").addEventListener("change", () => void applyCompositionSelection());
el("reset-view").addEventListener("click", resetView);
el("preview-prev").addEventListener("click", () => { state.previewPage -= 1; renderPreview(); });
el("preview-next").addEventListener("click", () => { state.previewPage += 1; renderPreview(); });
el("toggle-preview").addEventListener("click", () => {
  el("preview").classList.toggle("collapsed");
  el("toggle-preview").textContent = el("preview").classList.contains("collapsed") ? "⌃" : "⌄";
  state.map?.resize();
});
el("undo-drawing").addEventListener("click", undoDrawing);
el("finish-drawing").addEventListener("click", finishDrawing);
el("cancel-drawing").addEventListener("click", cancelDrawing);

bridge.on("ui/notifications/host-context-changed", (params) => applyHostContext(params && (params.hostContext || params)));
bridge.on("ui/notifications/tool-result", () => void refreshAll());
bridge.on("ui/notifications/resource-updated", (params) => {
  const uri = params && params.uri || params;
  if (typeof uri === "string" && uri.startsWith("map://")) void refreshAll();
});
bridge.on("ui/resource-teardown", (_params, id) => {
  state.closing = true;
  clearTimeout(state.refreshTimer);
  state.map?.remove();
  if (id !== undefined) bridge.post({ jsonrpc: "2.0", id, result: {} });
});

(async () => {
  try {
    const initialized = await bridge.request("ui/initialize", {
      protocolVersion: "2026-01-26",
      appInfo: { name: "map-workspace", version: "2.0.0" },
      appCapabilities: { availableDisplayModes: ["inline"] },
    });
    bridge.notify("ui/notifications/initialized", {});
    applyHostContext(initialized && initialized.hostContext);
    state.access = await read("map://workspace");
    renderAccess();
    await refreshAll();
    await initializeMap();
    const subscriptions = [];
    if (state.access.feature_read) subscriptions.push(
      "map://feature-layers", "map://publications", "map://compositions");
    if (state.access.dataset_read) subscriptions.push(
      "map://datasets", "map://active-releases", "map://mobility-profiles");
    void bridge.request("subscriptions/listen", { notifications: { resourceSubscriptions: subscriptions } })
      .catch((error) => { if (!state.closing) setStatus(`Live resource updates unavailable: ${error.message}`, "warn"); });
  } catch (error) {
    setStatus(`Workspace initialization failed: ${error.message}`, "bad");
  }
})();
